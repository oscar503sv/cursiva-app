//! `hid-labd`: the hid-lab user daemon. Run by the systemd user unit
//! `hid-labd.service`; see docs/daemon.md.
//!
//! One thread runs a `poll` loop over the udev monitor, a wake-up socket fed
//! by the logind thread, and a read-only listener on the mouse; zbus serves
//! D-Bus calls on its own thread. Both share the [`Engine`] behind a mutex.

use std::io::Read as _;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::process::ExitCode;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use hidlab_daemon::api::{BUS_NAME, DeviceEvent, INTERFACE, OBJECT_PATH, Trigger};
use hidlab_daemon::engine::Engine;
use hidlab_daemon::schedule::{Action, READY_BUDGET, Scheduler};
use hidlab_daemon::service::Service;
use hidlab_daemon::sleep::{self, SleepEvent};
use hidlab_daemon::{debug, error, info, log, paths, warn};
use hidlab_devices::profile_for;
use hidlab_features::hires_wheel;
use hidlab_transport::{
    ButtonField, Hidraw, HotplugAction, HotplugEvent, HotplugMonitor, PowerSupplyMonitor,
    RawChannel,
};
use serde::Serialize;
use zbus::blocking::Connection;
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::names::BusName;

const HELP: &str = "\
hid-labd — hid-lab user daemon

Usage: hid-labd [--help | --version]

Applies ~/.config/hid-lab/config.toml to the mouse and serves the D-Bus API
dev.ozkkar.Cursiva.Daemon1 on the session bus. Normally started by the
systemd user unit hid-labd.service. Talk to it with `hid-lab status`,
`hid-lab set` and `hid-lab reapply`.

Environment: HIDLAB_LOG=error|warn|info|debug (default info; debug logs raw
HID++ bytes).
";

fn lock(engine: &Mutex<Engine>) -> MutexGuard<'_, Engine> {
    // A panic while holding the lock leaves the data usable: every field is
    // replaced as a whole.
    engine.lock().unwrap_or_else(|p| p.into_inner())
}

fn emit<T: Serialize>(conn: &Connection, signal: &str, value: &T) {
    let body = match serde_json::to_string(value) {
        Ok(b) => b,
        Err(e) => return error!("cannot encode {signal}: {e}"),
    };
    if let Err(e) = conn.emit_signal(
        None::<BusName<'_>>,
        OBJECT_PATH,
        INTERFACE,
        signal,
        &(body.as_str(),),
    ) {
        warn!("cannot emit {signal}: {e}");
    }
}

fn describe(event: &DeviceEvent) -> String {
    match &event.device {
        Some(d) => format!(
            "{} connected at {} ({}, access {})",
            d.name,
            d.node,
            d.hid_device.as_deref().unwrap_or("?"),
            d.access
        ),
        None => "mouse disconnected".into(),
    }
}

/// The main loop's state: udev, the logind wake-up socket, and a read-only
/// listener on the mouse for the ratchet event.
struct Daemon {
    engine: Arc<Mutex<Engine>>,
    conn: Connection,
    monitor: HotplugMonitor,
    /// Battery changes; `None` if udev refused a second monitor.
    batteries: Option<PowerSupplyMonitor>,
    /// `None` once the logind thread is gone (or never started).
    wake: Option<UnixStream>,
    sleep_rx: Receiver<SleepEvent>,
    sched: Scheduler,
    /// The mouse opened for listening only, with the 0x2121 feature index.
    listener: Option<(Hidraw, u8)>,
    /// Where the listener's input reports carry buttons, and which are held.
    buttons: Option<ButtonField>,
    held: u32,
}

fn readable(fd: RawFd) -> libc::pollfd {
    libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    }
}

impl Daemon {
    fn on_hotplug(&mut self, ev: &HotplugEvent) {
        let ours = ev
            .ids
            .is_some_and(|i| profile_for(i.bus, i.vendor_id, i.product_id).is_some());
        if !ours {
            return;
        }
        let event = {
            let mut e = lock(&self.engine);
            match ev.action {
                HotplugAction::Add => e.refresh_device(),
                HotplugAction::Remove => ev.hid_device.as_deref().and_then(|h| e.device_removed(h)),
                HotplugAction::Change | HotplugAction::Other => None,
            }
        };
        let Some(event) = event else { return };
        info!("{}", describe(&event));
        self.listener = None;
        if event.connected {
            self.sched.event(Trigger::Connect, Instant::now());
        } else {
            self.sched.removed();
        }
        emit(&self.conn, "DeviceChanged", &event);
    }

    fn on_power_supply(&mut self) {
        let Some(m) = &mut self.batteries else { return };
        let mut hid_devices = Vec::new();
        while let Some(ev) = m.try_next() {
            hid_devices.push(ev.hid_device);
        }
        hid_devices.dedup();
        for h in hid_devices {
            let event = lock(&self.engine).on_power_supply(h.as_deref());
            if let Some(ev) = event {
                if let Some(b) = &ev.battery {
                    debug!(
                        "battery {}% {}",
                        b.percent.map_or("?".into(), |p| p.to_string()),
                        b.status.as_deref().unwrap_or("?")
                    );
                }
                emit(&self.conn, "BatteryChanged", &ev);
            }
        }
    }

    fn on_wake(&mut self) {
        let mut buf = [0u8; 16];
        if let Some(w) = &mut self.wake {
            loop {
                match w.read(&mut buf) {
                    Ok(0) => {
                        // The thread ended; stop polling a socket at EOF.
                        warn!(
                            "the logind watcher stopped; resume is handled when the mouse reconnects"
                        );
                        self.wake = None;
                        break;
                    }
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                    Err(_) => break,
                }
            }
        }
        while let Ok(ev) = self.sleep_rx.try_recv() {
            match ev {
                SleepEvent::Suspending => info!("system going to sleep"),
                SleepEvent::Resumed => {
                    info!("system resumed");
                    if lock(&self.engine).present().is_some() {
                        self.sched.resumed(Instant::now());
                    }
                }
            }
        }
    }

    /// Drains the listener. Acted on: the 0x2121 ratchet event, and button
    /// changes while a `WatchButtons` lease lasts. Movement and answers to
    /// other requests are skipped.
    fn on_listener(&mut self) {
        let mut buf = [0u8; 64];
        let mut events = 0;
        let mut changes = Vec::new();
        loop {
            let Some((dev, index)) = self.listener.as_mut() else {
                return;
            };
            match dev.recv(&mut buf, Duration::ZERO) {
                Ok(Some(n)) => {
                    let held = self.buttons.as_ref().and_then(|f| f.pressed(&buf[..n]));
                    if let Some(mask) = held {
                        if mask != self.held {
                            changes.push((self.held, mask));
                            self.held = mask;
                        }
                    } else if let Ok(frame) = hidlab_hidpp::decode(&buf[..n]) {
                        if hires_wheel::ratchet_event(&frame, *index).is_some() {
                            events += 1;
                        }
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    debug!("listener closed: {e}");
                    self.listener = None;
                    return;
                }
            }
        }
        if !changes.is_empty() || events > 0 {
            let e = lock(&self.engine);
            if e.watching_buttons() {
                let mut out = Vec::new();
                if let Some(f) = &self.buttons {
                    for (before, after) in changes {
                        out.extend(e.button_events(f, before, after));
                    }
                }
                if events > 0 {
                    out.push(Engine::mode_button_event());
                }
                drop(e);
                for ev in &out {
                    emit(&self.conn, "ButtonPressed", ev);
                }
            }
        }
        if events == 0 {
            return;
        }
        let result = lock(&self.engine).on_ratchet_event();
        match result {
            Ok(Some(ev)) => emit(&self.conn, "ModeChanged", &ev),
            Ok(None) => {}
            Err(e) => warn!("cannot read the SmartShift mode after a wheel event: {e}"),
        }
    }

    fn open_listener(&mut self, e: &Engine) {
        let Some(p) = e.present() else { return };
        let index = match e.ratchet_event_index() {
            Ok(Some(i)) => i,
            Ok(None) => return,
            Err(err) => return warn!("cannot find 0x2121 to follow the mode button: {err}"),
        };
        self.buttons = p
            .device
            .descriptor
            .as_ref()
            .and_then(|d| d.button_fields.first().copied());
        self.held = 0;
        match Hidraw::open(&p.device.node) {
            Ok(h) => self.listener = Some((h, index)),
            Err(err) => warn!("cannot listen to the mouse: {err}"),
        }
    }

    fn run_due(&mut self) {
        let now = Instant::now();
        match self.sched.due(now) {
            None => {}
            Some(Action::Probe { trigger, event_at }) => {
                let engine = Arc::clone(&self.engine);
                let mut e = lock(&engine);
                if e.probe_ready().is_err() {
                    if !self.sched.probe_failed(Instant::now()) {
                        warn!(
                            "the mouse did not answer within {} s after {trigger:?}; \
                             settings were not applied.\n\
                             Try: move the mouse, then run `hid-lab reapply`",
                            READY_BUDGET.as_secs()
                        );
                    }
                    return;
                }
                if self.listener.is_none() {
                    self.open_listener(&e);
                }
                match e.apply_all(trigger, event_at) {
                    Ok(rec) => {
                        drop(e);
                        self.sched.applied(event_at, Instant::now());
                        emit(&self.conn, "Applied", &rec);
                    }
                    Err(err) => {
                        drop(e);
                        warn!("cannot apply the configuration after {trigger:?}: {err}");
                        self.sched.done();
                    }
                }
            }
            Some(Action::SecondCheck { event_at }) => {
                let result = lock(&self.engine).second_check(event_at);
                self.sched.done();
                match result {
                    Ok(rec) => emit(&self.conn, "Applied", &rec),
                    Err(err) => warn!("second check skipped: {err}"),
                }
            }
        }
    }

    fn run(&mut self) -> Result<(), String> {
        loop {
            let timeout = self.sched.next_deadline().map_or(-1, |d| {
                let ms = d
                    .saturating_duration_since(Instant::now())
                    .as_micros()
                    .div_ceil(1000);
                i32::try_from(ms).unwrap_or(i32::MAX)
            });
            // poll skips negative fds.
            let mut fds = vec![
                readable(self.monitor.as_raw_fd()),
                readable(self.wake.as_ref().map_or(-1, |w| w.as_raw_fd())),
                readable(self.batteries.as_ref().map_or(-1, |b| b.as_raw_fd())),
                readable(self.listener.as_ref().map_or(-1, |(h, _)| h.as_raw_fd())),
            ];
            // SAFETY: `fds` holds valid pollfds for fds owned by `self`, which
            // outlive the call; the length matches the vector.
            let rc = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout) };
            if rc < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(format!("poll failed: {err}"));
            }
            if fds[1].revents != 0 {
                self.on_wake();
            }
            if fds[0].revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return Err("the udev monitor socket failed".into());
            }
            if fds[0].revents != 0 {
                while let Some(ev) = self.monitor.try_next() {
                    self.on_hotplug(&ev);
                }
            }
            if fds[2].revents != 0 {
                self.on_power_supply();
            }
            if fds[3].revents != 0 {
                self.on_listener();
            }
            self.run_due();
        }
    }
}

fn run() -> Result<(), String> {
    let config_path = paths::config_file()?;
    let state_path = paths::state_file()?;
    info!(
        "hid-labd {} starting; config {}",
        env!("CARGO_PKG_VERSION"),
        paths::tilde(&config_path)
    );

    // Listen before looking, so no connection is missed in between.
    let monitor = HotplugMonitor::new().map_err(|e| e.to_string())?;
    let batteries = match PowerSupplyMonitor::new() {
        Ok(m) => Some(m),
        Err(e) => {
            warn!("{e}\nBattery changes will show only on the next `hid-lab status`.");
            None
        }
    };
    let engine = Arc::new(Mutex::new(Engine::new(config_path, state_path)));
    // Find the mouse before taking the bus name: with D-Bus activation the
    // call that started the daemon is answered as soon as the name is owned.
    lock(&engine).refresh_device();

    let conn = zbus::blocking::connection::Builder::session()
        .and_then(|b| {
            b.serve_at(
                OBJECT_PATH,
                Service {
                    engine: Arc::clone(&engine),
                },
            )
        })
        .and_then(|b| b.build())
        .map_err(|e| format!("cannot connect to the D-Bus session bus: {e}"))?;
    match conn.request_name_with_flags(BUS_NAME, RequestNameFlags::DoNotQueue.into()) {
        Ok(RequestNameReply::PrimaryOwner) => {}
        Ok(_) | Err(zbus::Error::NameTaken) => {
            return Err(format!(
                "another hid-labd already owns {BUS_NAME} on the session bus.\n\
                 Check with: systemctl --user status hid-labd.service"
            ));
        }
        Err(e) => return Err(format!("cannot own {BUS_NAME}: {e}")),
    }
    info!("serving {BUS_NAME} at {OBJECT_PATH}");

    let (wake, wake_tx) =
        UnixStream::pair().map_err(|e| format!("cannot create a socket pair: {e}"))?;
    wake.set_nonblocking(true)
        .map_err(|e| format!("cannot set the wake socket non-blocking: {e}"))?;
    let (sleep_tx, sleep_rx) = mpsc::channel();
    let wake = match sleep::spawn(sleep_tx, wake_tx) {
        Ok(()) => Some(wake),
        Err(e) => {
            warn!("{e}\nResume is still handled when the mouse reconnects after waking.");
            None
        }
    };

    let mut daemon = Daemon {
        engine: Arc::clone(&engine),
        conn,
        monitor,
        batteries,
        wake,
        sleep_rx,
        sched: Scheduler::default(),
        listener: None,
        buttons: None,
        held: 0,
    };
    {
        let e = lock(&engine);
        let event = e.device_event();
        info!("{}", describe(&event));
        if event.connected {
            daemon.sched.event(Trigger::Start, Instant::now());
        }
    }
    daemon.run()
}

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        None => {}
        Some("-h" | "--help") => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Some("-V" | "--version") => {
            println!("hid-labd {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Some(other) => {
            eprintln!("error: unknown argument `{other}`\n\n{HELP}");
            return ExitCode::from(2);
        }
    }
    if let Some(w) = log::init() {
        warn!("{w}");
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("{e}");
            ExitCode::FAILURE
        }
    }
}
