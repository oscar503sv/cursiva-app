//! `hid-labd`: the hid-lab user daemon. Run by the systemd user unit
//! `hid-labd.service`; see docs/daemon.md.

use std::process::ExitCode;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use hidlab_daemon::api::{BUS_NAME, DeviceEvent, INTERFACE, OBJECT_PATH};
use hidlab_daemon::engine::Engine;
use hidlab_daemon::service::Service;
use hidlab_daemon::{error, info, log, paths, warn};
use hidlab_devices::profile_for;
use hidlab_transport::{HotplugAction, HotplugEvent, HotplugMonitor};
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

fn emit_device(conn: &Connection, event: &DeviceEvent) {
    let body = match serde_json::to_string(event) {
        Ok(b) => b,
        Err(e) => return error!("cannot encode DeviceChanged: {e}"),
    };
    if let Err(e) = conn.emit_signal(
        None::<BusName<'_>>,
        OBJECT_PATH,
        INTERFACE,
        "DeviceChanged",
        &(body.as_str(),),
    ) {
        warn!("cannot emit DeviceChanged: {e}");
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

fn handle(engine: &Mutex<Engine>, conn: &Connection, ev: &HotplugEvent) {
    let ours = ev
        .ids
        .is_some_and(|i| profile_for(i.bus, i.vendor_id, i.product_id).is_some());
    if !ours {
        return;
    }
    let event = {
        let mut e = lock(engine);
        match ev.action {
            HotplugAction::Add => e.refresh_device(),
            HotplugAction::Remove => ev.hid_device.as_deref().and_then(|h| e.device_removed(h)),
            HotplugAction::Change | HotplugAction::Other => None,
        }
    };
    if let Some(event) = event {
        info!("{}", describe(&event));
        if event.connected {
            // Phase 4c re-applies here automatically.
            info!("run `hid-lab reapply` to apply the configuration after a reconnect");
        }
        emit_device(conn, &event);
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
    let mut monitor = HotplugMonitor::new().map_err(|e| e.to_string())?;
    let engine = Arc::new(Mutex::new(Engine::new(config_path, state_path)));

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

    {
        let mut e = lock(&engine);
        e.refresh_device();
        let event = e.device_event();
        info!("{}", describe(&event));
        if event.connected {
            if let Err(err) = e.apply_all() {
                warn!("cannot apply the configuration: {err}");
            }
        }
    }

    loop {
        match monitor.next_event(Duration::from_secs(3600)) {
            Ok(Some(ev)) => handle(&engine, &conn, &ev),
            Ok(None) => {}
            Err(e) => return Err(format!("the udev monitor failed: {e}")),
        }
    }
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
