//! The daemon's state and operations, independent of D-Bus.

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hidlab_devices::{DeviceProfile, PROFILES, profile_for};
use hidlab_features::settings::{SettingKind, SettingValue};
use hidlab_features::{FeatureMap, Memo, Reader};
use hidlab_features::{hires_wheel, smartshift};
use hidlab_hidpp::{Session, SessionConfig, get_feature, get_protocol_version};
use hidlab_transport::{Access, ButtonField, Hidraw, HidrawDevice, discover, power_supplies_for};

use crate::api::{
    ApplyRecord, BATTERY_EVENT_SCHEMA, BUTTON_EVENT_SCHEMA, Battery, BatteryEvent, ButtonEvent,
    CurrentValue, DESKTOP_SCHEMA, DEVICE_EVENT_SCHEMA, DesktopReply, DesktopSetting, DeviceEvent,
    DeviceInfo, MODE_EVENT_SCHEMA, ModeEvent, REAPPLY_SCHEMA, RESET_SCHEMA, ReapplyReply,
    ResetReply, SET_SCHEMA, STATUS_SCHEMA, SecondCheck, SetReply, Status, Trigger,
    WATCH_BUTTONS_MAX_S,
};
use crate::apply::{self, ApplyReport, ItemResult};
use crate::config::{Config, DeviceConfig, SmartShiftMode};
use crate::conflicts;
use crate::gnome;
use crate::paths::tilde;
use crate::state::State;
use crate::{debug, info, warn};

/// Software ID of daemon requests, different from the CLI's `0x0B` so that
/// `hid-lab lab watch` can tell them apart. Any non-zero value is valid.
pub const DAEMON_SW_ID: u8 = 0x0C;
const REQUEST_TIMEOUT: Duration = Duration::from_millis(1000);
/// Per-try timeout while waiting for a reconnected mouse to answer.
const PROBE_TIMEOUT: Duration = Duration::from_millis(250);
/// Ping byte for the readiness probe (any value; the device echoes it).
const PROBE_PING: u8 = 0xC5;

/// The connected device the daemon manages.
#[derive(Debug, Clone)]
pub struct Present {
    pub device: HidrawDevice,
    pub profile: &'static DeviceProfile,
}

/// What the daemon reuses between calls to save time on Bluetooth, where
/// each request takes about 15 ms (O-11).
#[derive(Default)]
struct ConnCache {
    /// Feature indices of the current connection. Never reused for another
    /// connection: a firmware update may move features.
    map: Option<FeatureMap>,
    /// Capability reads (DPI list, control list), kept across reconnects of
    /// the same model while the daemon runs; for another model they are
    /// dropped. A stale entry can only make a check refuse a value; every
    /// write still goes to the index resolved on this connection.
    memo: Memo,
    memo_profile: Option<&'static str>,
}

pub struct Engine {
    config_path: PathBuf,
    state_path: PathBuf,
    config: Config,
    config_error: Option<String>,
    state: State,
    state_error: Option<String>,
    present: Option<Present>,
    /// The last apply, and the event time it was measured from.
    last_apply: Option<(ApplyRecord, Instant)>,
    /// Reads reused between calls; see [`ConnCache`].
    cache: RefCell<ConnCache>,
    /// The battery last sent in `BatteryChanged`.
    last_battery: Option<Battery>,
    /// End of the `WatchButtons` lease.
    watch_until: Option<Instant>,
}

/// Raw bytes, at debug level only (requirements §17).
fn log_transcript(session: &Session<Hidraw>) {
    if crate::log::enabled(crate::log::Level::Debug) {
        for e in &session.transcript().entries {
            debug!("{:>7} us {:?} {:?} {}", e.t_us, e.dir, e.disposition, e.hex);
        }
    }
}

fn ms_since(t: Instant) -> u64 {
    u64::try_from(t.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn access_label(a: Access) -> &'static str {
    match a {
        Access::Ok => "ok",
        Access::PermissionDenied => "permission_denied",
        Access::Missing => "missing",
        Access::Disconnected => "disconnected",
        Access::Error => "error",
    }
}

fn info_for(p: &Present) -> DeviceInfo {
    DeviceInfo {
        profile: p.profile.key.to_owned(),
        name: p.profile.name.to_owned(),
        node: p.device.node.display().to_string(),
        hid_device: p.device.hid_device.clone(),
        bus: p.device.uevent.bus.label(),
        vendor_id: p.device.uevent.vendor_id,
        product_id: p.device.uevent.product_id,
        access: access_label(p.device.access.status).to_owned(),
    }
}

/// Finds the first connected device that has a profile.
pub fn find_device() -> Result<Option<Present>, String> {
    let d = discover().map_err(|e| e.to_string())?;
    Ok(d.devices.into_iter().find_map(|device| {
        let u = &device.uevent;
        profile_for(u.bus, u.vendor_id, u.product_id).map(|profile| Present { device, profile })
    }))
}

impl Engine {
    /// Loads config and state. Problems are logged and kept for `status`;
    /// they never stop the daemon (requirements §15).
    pub fn new(config_path: PathBuf, state_path: PathBuf) -> Self {
        let (config, config_error) = match Config::load(&config_path) {
            Ok(c) => (c, None),
            Err(e) => {
                warn!("{e}\nNo settings will be applied until the file is fixed.");
                (Config::default(), Some(e.to_string()))
            }
        };
        let (state, state_error) = match State::load(&state_path) {
            Ok(s) => (s, None),
            Err(e) => {
                warn!("{e}\nStarting without a remembered SmartShift mode.");
                (State::default(), Some(e))
            }
        };
        Self {
            config_path,
            state_path,
            config,
            config_error,
            state,
            state_error,
            present: None,
            last_apply: None,
            cache: RefCell::default(),
            last_battery: None,
            watch_until: None,
        }
    }

    pub fn present(&self) -> Option<&Present> {
        self.present.as_ref()
    }

    /// Settings to apply to `profile`, in order. The SmartShift mode is the
    /// last one remembered in the state, else the config's initial mode.
    pub fn desired(&self, profile: &DeviceProfile) -> Vec<SettingValue> {
        let dev = self.config.device(profile);
        let mode = self
            .state
            .mode(profile)
            .or_else(|| dev.and_then(|d| d.initial_mode()));
        let rest = dev.map(|d| d.values()).unwrap_or_default();
        let mut out = Vec::with_capacity(rest.len() + 1);
        out.extend(
            rest.iter()
                .copied()
                .filter(|v| v.kind() == SettingKind::Dpi),
        );
        out.extend(mode.map(|m| SettingValue::SmartshiftMode(m.code())));
        out.extend(
            rest.iter()
                .copied()
                .filter(|v| v.kind() != SettingKind::Dpi),
        );
        out
    }

    /// Re-reads the config file. On error the last valid config stays.
    pub fn reload_config(&mut self) {
        match Config::load(&self.config_path) {
            Ok(c) => {
                if self.config_error.take().is_some() {
                    info!("{} is valid again", tilde(&self.config_path));
                }
                self.config = c;
            }
            Err(e) => {
                warn!("{e}\nKeeping the last valid configuration.");
                self.config_error = Some(e.to_string());
            }
        }
    }

    /// Looks for the device again. Returns an event if presence changed.
    pub fn refresh_device(&mut self) -> Option<DeviceEvent> {
        let found = match find_device() {
            Ok(f) => f,
            Err(e) => {
                warn!("cannot list hidraw devices: {e}");
                return None;
            }
        };
        let before = self
            .present
            .as_ref()
            .and_then(|p| p.device.hid_device.clone());
        let after = found.as_ref().and_then(|p| p.device.hid_device.clone());
        let changed = before != after || self.present.is_some() != found.is_some();
        self.present = found;
        if changed {
            let mut cache = self.cache.borrow_mut();
            cache.map = None;
            let key = self.present.as_ref().map(|p| p.profile.key);
            if key.is_some() && key != cache.memo_profile {
                cache.memo = Memo::default();
                cache.memo_profile = key;
            }
            drop(cache);
            self.last_battery = self.battery();
        }
        changed.then(|| self.device_event())
    }

    /// Forgets the device if `hid_device` is the one in use.
    pub fn device_removed(&mut self, hid_device: &str) -> Option<DeviceEvent> {
        let ours = self
            .present
            .as_ref()
            .is_some_and(|p| p.device.hid_device.as_deref() == Some(hid_device));
        if !ours {
            return None;
        }
        self.present = None;
        self.cache.borrow_mut().map = None;
        self.last_battery = None;
        Some(self.device_event())
    }

    pub fn device_event(&self) -> DeviceEvent {
        DeviceEvent {
            schema: DEVICE_EVENT_SCHEMA.into(),
            connected: self.present.is_some(),
            device: self.present.as_ref().map(info_for),
        }
    }

    /// Opens the device, resolves the profile's features and runs `f`.
    fn with_reader<T>(
        &self,
        f: impl FnOnce(&mut Reader<'_, Hidraw>, &'static DeviceProfile) -> T,
    ) -> Result<T, String> {
        let p = self
            .present
            .as_ref()
            .ok_or("the mouse is not connected; move it or turn it on")?;
        let channel = Hidraw::open(&p.device.node).map_err(|e| e.to_string())?;
        let config = SessionConfig {
            sw_id: DAEMON_SW_ID,
            ..SessionConfig::for_device(&p.device, None, REQUEST_TIMEOUT)
        };
        let mut session = Session::new(channel, config);
        let cached = self.cache.borrow().map.clone();
        let map = match cached {
            Some(m) => m,
            None => {
                let m = FeatureMap::resolve(&mut session, &apply::features_for(p.profile))
                    .map_err(|e| {
                        log_transcript(&session);
                        e.to_string()
                    })?;
                self.cache.borrow_mut().map = Some(m.clone());
                m
            }
        };
        let memo = std::mem::take(&mut self.cache.borrow_mut().memo);
        let mut reader = Reader::with_memo(&mut session, map, memo);
        let out = f(&mut reader, p.profile);
        self.cache.borrow_mut().memo = reader.into_memo();
        log_transcript(&session);
        Ok(out)
    }

    fn warn_conflicts() {
        let t = Instant::now();
        let found = conflicts::scan();
        debug!("conflict scan took {} ms", ms_since(t));
        for c in found {
            warn!("{}", c.advice());
        }
    }

    fn record(&mut self, trigger: Trigger, event_at: Instant, report: ApplyReport) -> ApplyRecord {
        for item in &report.items {
            match item.result {
                ItemResult::Unchanged | ItemResult::Written => {}
                _ => warn!(
                    "{} = {}: {:?}{}",
                    item.setting.name(),
                    item.target.display(),
                    item.result,
                    item.error
                        .as_deref()
                        .map(|e| format!(": {e}"))
                        .unwrap_or_default()
                ),
            }
        }
        let since_event_ms = ms_since(event_at);
        info!(
            "{trigger:?}: applied {} setting(s) in {} ms, {} ms after the event: {}",
            report.items.len(),
            report.elapsed_ms,
            since_event_ms,
            if report.ok {
                "verified"
            } else {
                "with problems"
            }
        );
        let rec = ApplyRecord {
            trigger,
            at_unix: now_unix(),
            report,
            since_event_ms,
            second_check: None,
        };
        self.last_apply = Some((rec.clone(), event_at));
        rec
    }

    pub fn last_apply(&self) -> Option<&ApplyRecord> {
        self.last_apply.as_ref().map(|(r, _)| r)
    }

    /// Applies the whole configuration to the connected device. `event_at`
    /// is when the event that caused it happened, for the measured time.
    pub fn apply_all(
        &mut self,
        trigger: Trigger,
        event_at: Instant,
    ) -> Result<ApplyRecord, String> {
        let profile = self
            .present
            .as_ref()
            .ok_or("the mouse is not connected; move it or turn it on")?
            .profile;
        let desired = self.desired(profile);
        Self::warn_conflicts();
        let report = self.with_reader(|r, p| apply::apply(r, p, &desired))?;
        Ok(self.record(trigger, event_at, report))
    }

    /// One short ping. `Ok` once a reconnected mouse answers HID++.
    pub fn probe_ready(&self) -> Result<(), String> {
        let p = self.present.as_ref().ok_or("the mouse is not connected")?;
        let channel = Hidraw::open(&p.device.node).map_err(|e| e.to_string())?;
        let config = SessionConfig {
            sw_id: DAEMON_SW_ID,
            ..SessionConfig::for_device(&p.device, None, PROBE_TIMEOUT)
        };
        let mut session = Session::new(channel, config);
        let result = get_protocol_version(&mut session, PROBE_PING);
        log_transcript(&session);
        result.map(|_| ()).map_err(|e| e.to_string())
    }

    /// Where the mouse placed 0x2121, whose ratchet event the daemon follows.
    pub fn ratchet_event_index(&self) -> Result<Option<u8>, String> {
        let p = self.present.as_ref().ok_or("the mouse is not connected")?;
        let channel = Hidraw::open(&p.device.node).map_err(|e| e.to_string())?;
        let config = SessionConfig {
            sw_id: DAEMON_SW_ID,
            ..SessionConfig::for_device(&p.device, None, REQUEST_TIMEOUT)
        };
        let mut session = Session::new(channel, config);
        let result = get_feature(&mut session, hires_wheel::FEATURE);
        log_transcript(&session);
        result
            .map(|f| f.map(|f| f.index))
            .map_err(|e| e.to_string())
    }

    /// The second verification: reads every wanted setting again and
    /// re-applies once if something overwrote it. Updates the last record
    /// if it is the apply measured from `event_at`.
    pub fn second_check(&mut self, event_at: Instant) -> Result<ApplyRecord, String> {
        let profile = self
            .present
            .as_ref()
            .ok_or("the mouse is not connected")?
            .profile;
        let desired = self.desired(profile);
        let kinds: Vec<SettingKind> = desired.iter().map(|v| v.kind()).collect();
        let current = self.with_reader(|r, _| apply::read_values(r, &kinds))?;
        let overwritten: Vec<CurrentValue> = desired
            .iter()
            .zip(&current)
            .filter(|&(want, got)| got.as_ref().ok() != Some(want))
            .map(|(want, got)| CurrentValue {
                setting: want.kind(),
                value: got.as_ref().ok().copied(),
                error: got.as_ref().err().cloned(),
            })
            .collect();
        let reapplied = if overwritten.is_empty() {
            None
        } else {
            for o in &overwritten {
                warn!(
                    "{} changed after it was applied and verified (read {}); \
                     re-applying once. Something else is configuring the mouse: \
                     check `hid-lab status` for other configurators",
                    o.setting.name(),
                    o.value
                        .map(|v| v.display())
                        .or_else(|| o.error.clone())
                        .unwrap_or_default()
                );
            }
            Some(self.with_reader(|r, p| apply::apply(r, p, &desired))?)
        };
        let check = SecondCheck {
            ok: reapplied.as_ref().is_none_or(|r| r.ok),
            since_event_ms: ms_since(event_at),
            overwritten,
            reapplied,
        };
        info!(
            "second check {} ms after the event: {}",
            check.since_event_ms,
            match (&check.reapplied, check.ok) {
                (None, _) => "nothing overwritten",
                (Some(_), true) => "overwritten, re-applied and verified",
                (Some(_), false) => "overwritten, re-apply failed",
            }
        );
        match &mut self.last_apply {
            Some((rec, at)) if *at == event_at => {
                rec.second_check = Some(check);
                Ok(rec.clone())
            }
            _ => Err("a newer apply replaced this one".into()),
        }
    }

    /// The mouse sent the ratchet event (mode button, or a mode write):
    /// reads the SmartShift mode and remembers it if it changed.
    pub fn on_ratchet_event(&mut self) -> Result<Option<ModeEvent>, String> {
        let profile = self
            .present
            .as_ref()
            .ok_or("the mouse is not connected")?
            .profile;
        let read = self.with_reader(|r, _| smartshift::read(r))?;
        let ss = read.map_err(|e| e.to_string())?;
        let mode = SmartShiftMode::from_code(ss.mode_code)
            .ok_or_else(|| format!("unknown SmartShift mode code {}", ss.mode_code))?;
        if self.state.mode(profile) == Some(mode) {
            return Ok(None);
        }
        self.state.set_mode(profile, mode);
        self.state.save(&self.state_path)?;
        self.state_error = None;
        let value = SettingValue::SmartshiftMode(mode.code());
        info!(
            "SmartShift mode changed on the mouse to {}; remembered",
            value.display()
        );
        Ok(Some(ModeEvent {
            schema: MODE_EVENT_SCHEMA.into(),
            mode: value,
        }))
    }

    /// `Reapply`: reloads the config file, then applies it.
    pub fn reapply(&mut self) -> Result<ReapplyReply, String> {
        self.reload_config();
        let rec = self.apply_all(Trigger::Reapply, Instant::now())?;
        Ok(ReapplyReply {
            schema: REAPPLY_SCHEMA.into(),
            config_error: self.config_error.clone(),
            report: rec.report,
        })
    }

    fn target_profile(&self) -> Result<&'static DeviceProfile, String> {
        if let Some(p) = &self.present {
            return Ok(p.profile);
        }
        match PROFILES {
            [only] => Ok(only),
            _ => Err("the mouse is not connected; move it or turn it on, then retry".into()),
        }
    }

    fn refuse_broken_config(&self) -> Result<(), String> {
        match &self.config_error {
            Some(e) => Err(format!(
                "the config file has an error, so it was not changed:\n{e}\n\
                 Fix the file, then run `hid-lab reapply`."
            )),
            None => Ok(()),
        }
    }

    /// `Set`: applies one setting if the mouse is connected and stores it,
    /// unless the mouse rejected the value. `smartshift-threshold on`
    /// turns automatic switching back on with the saved threshold.
    pub fn set(&mut self, setting: &str, value: &str) -> Result<SetReply, String> {
        let kind = parse_kind(setting)?;
        let target = if kind == SettingKind::SmartshiftThreshold && value == "on" {
            let profile = self.target_profile()?;
            self.config
                .device(profile)
                .and_then(|d| d.saved_threshold())
                .map(SettingValue::SmartshiftThreshold)
                .or_else(|| profile.default_value(kind))
                .ok_or("no threshold to turn back on; give a number, 1..=50")?
        } else {
            kind.parse_value(value)?
        };
        if !target.confirmed() {
            return Err(format!(
                "{setting} {} has not been confirmed on the mouse yet, so the daemon does not \
                 send it; it can only be tried with `hid-lab lab write`",
                target.display()
            ));
        }
        let profile = self.target_profile()?;
        if !profile.supports(kind) {
            return Err(format!("{} does not support {setting}", profile.name));
        }
        self.refuse_broken_config()?;

        let applied = if self.present.is_some() {
            let started = Instant::now();
            Self::warn_conflicts();
            let report = self.with_reader(|r, p| apply::apply(r, p, &[target]))?;
            Some(self.record(Trigger::Set, started, report).report)
        } else {
            None
        };
        let rejected = applied
            .as_ref()
            .and_then(|r| r.items.first())
            .is_some_and(|i| i.result == ItemResult::Rejected);
        if rejected {
            return Ok(SetReply {
                schema: SET_SCHEMA.into(),
                value: target,
                saved_to: None,
                applied,
                note: Some("the mouse cannot take this value; nothing was saved".into()),
            });
        }

        let saved_to = match target {
            SettingValue::SmartshiftMode(code) => {
                let mode = SmartShiftMode::from_code(code).ok_or("invalid SmartShift mode")?;
                self.state.set_mode(profile, mode);
                self.state.save(&self.state_path)?;
                self.state_error = None;
                &self.state_path
            }
            other => {
                self.config.device_mut(profile).set(other);
                self.config.save(&self.config_path)?;
                &self.config_path
            }
        };
        let saved_to = tilde(saved_to);
        info!("saved {setting} = {} to {saved_to}", target.display());
        Ok(SetReply {
            schema: SET_SCHEMA.into(),
            value: target,
            saved_to: Some(saved_to),
            note: applied
                .is_none()
                .then(|| "the mouse is not connected; saved for later".into()),
            applied,
        })
    }

    /// `Reset`: forgets `setting` (or `all`) in the config file and writes
    /// the profile's default to the mouse if it is connected. Resetting the
    /// SmartShift mode remembers ratchet.
    pub fn reset(&mut self, setting: &str) -> Result<ResetReply, String> {
        let profile = self.target_profile()?;
        let kinds: Vec<SettingKind> = if setting == "all" {
            profile.settings.to_vec()
        } else {
            let kind = parse_kind(setting)?;
            if !profile.supports(kind) {
                return Err(format!("{} does not support {setting}", profile.name));
            }
            vec![kind]
        };
        self.refuse_broken_config()?;
        let values: Vec<SettingValue> = kinds
            .iter()
            .filter_map(|&k| profile.default_value(k))
            .collect();

        let applied = if self.present.is_some() {
            let started = Instant::now();
            Self::warn_conflicts();
            let report = self.with_reader(|r, p| apply::apply(r, p, &values))?;
            Some(self.record(Trigger::Reset, started, report).report)
        } else {
            None
        };

        let mut saved_to = Vec::new();
        let dev = self.config.device_mut(profile);
        let before = dev.clone();
        for &k in &kinds {
            dev.remove(k);
        }
        if *dev != before {
            if *dev == DeviceConfig::default() {
                self.config.devices.remove(profile.key);
            }
            self.config.save(&self.config_path)?;
            saved_to.push(tilde(&self.config_path));
        }
        if let Some(SettingValue::SmartshiftMode(code)) = values
            .iter()
            .copied()
            .find(|v| v.kind() == SettingKind::SmartshiftMode)
        {
            let mode = SmartShiftMode::from_code(code).ok_or("invalid SmartShift mode")?;
            self.state.set_mode(profile, mode);
            self.state.save(&self.state_path)?;
            self.state_error = None;
            saved_to.push(tilde(&self.state_path));
        }
        info!("reset {setting} to the defaults");
        Ok(ResetReply {
            schema: RESET_SCHEMA.into(),
            values,
            saved_to,
            note: applied
                .is_none()
                .then(|| "the mouse is not connected; the defaults apply when it is".into()),
            applied,
        })
    }

    /// The battery of the connected mouse, from the kernel's power_supply.
    pub fn battery(&self) -> Option<Battery> {
        let hid = self.present.as_ref()?.device.hid_device.as_deref()?;
        let supplies = match power_supplies_for(hid) {
            Ok(s) => s,
            Err(e) => {
                debug!("cannot read the battery: {e}");
                return None;
            }
        };
        supplies.into_iter().next().map(|p| Battery {
            percent: p.capacity,
            level: p.capacity_level,
            status: p.status,
        })
    }

    /// A power_supply entry of `hid_device` changed. Returns an event if the
    /// managed mouse's battery reads differently from the last time.
    pub fn on_power_supply(&mut self, hid_device: Option<&str>) -> Option<BatteryEvent> {
        let ours = self.present.as_ref()?.device.hid_device.as_deref();
        if hid_device.is_none() || hid_device != ours {
            return None;
        }
        let battery = self.battery();
        if battery == self.last_battery {
            return None;
        }
        self.last_battery.clone_from(&battery);
        Some(BatteryEvent {
            schema: BATTERY_EVENT_SCHEMA.into(),
            battery,
        })
    }

    /// `WatchButtons`: sends `ButtonPressed` for `seconds` (at most
    /// [`WATCH_BUTTONS_MAX_S`]; 0 stops). Returns the seconds granted.
    pub fn watch_buttons(&mut self, seconds: u32) -> u32 {
        let granted = seconds.min(WATCH_BUTTONS_MAX_S);
        self.watch_until =
            (granted > 0).then(|| Instant::now() + Duration::from_secs(granted.into()));
        granted
    }

    pub fn watching_buttons(&self) -> bool {
        self.watch_until.is_some_and(|t| Instant::now() < t)
    }

    /// One event per button that changed between the bit sets `before` and
    /// `after` of `field`.
    pub fn button_events(&self, field: &ButtonField, before: u32, after: u32) -> Vec<ButtonEvent> {
        let profile = self.present.as_ref().map(|p| p.profile);
        (0..field.count.min(32))
            .filter(|i| (before ^ after) & (1 << i) != 0)
            .map(|i| {
                let number = field
                    .first_button
                    .saturating_add(u16::try_from(i).unwrap_or(u16::MAX));
                ButtonEvent {
                    schema: BUTTON_EVENT_SCHEMA.into(),
                    number: Some(number),
                    control: profile
                        .and_then(|p| p.input_button(number))
                        .map(str::to_owned),
                    pressed: after & (1 << i) != 0,
                }
            })
            .collect()
    }

    /// The mode button, seen through the ratchet event.
    pub fn mode_button_event() -> ButtonEvent {
        ButtonEvent {
            schema: BUTTON_EVENT_SCHEMA.into(),
            number: None,
            control: Some("mode".into()),
            pressed: true,
        }
    }

    /// `SetDesktop`: writes one GNOME pointer setting.
    pub fn set_desktop(&self, setting: &str, value: &str) -> Result<DesktopReply, String> {
        let s = DesktopSetting::from_name(setting).ok_or_else(|| {
            let names: Vec<&str> = DesktopSetting::ALL.iter().map(|s| s.name()).collect();
            format!(
                "unknown desktop setting `{setting}`; known: {}",
                names.join(", ")
            )
        })?;
        gnome::set(s, value)?;
        info!("GNOME {setting} = {value}");
        Ok(DesktopReply {
            schema: DESKTOP_SCHEMA.into(),
            setting: s,
            value: value.to_owned(),
            desktop: gnome::read(),
        })
    }

    /// `GetStatus`. Reads the current values from the mouse (reads only).
    pub fn status(&self) -> Status {
        let mut capabilities = None;
        let (desired, current) = match &self.present {
            Some(p) => {
                let desired = self.desired(p.profile);
                let current = match self.with_reader(apply::read_snapshots) {
                    Ok(snaps) => {
                        capabilities = apply::capabilities(p.profile, &snaps);
                        p.profile
                            .settings
                            .iter()
                            .zip(apply::values_of(p.profile, &snaps))
                            .map(|(&setting, v)| CurrentValue {
                                setting,
                                error: v.as_ref().err().cloned(),
                                value: v.ok(),
                            })
                            .collect()
                    }
                    Err(e) => p
                        .profile
                        .settings
                        .iter()
                        .map(|&setting| CurrentValue {
                            setting,
                            value: None,
                            error: Some(e.clone()),
                        })
                        .collect(),
                };
                (desired, current)
            }
            None => (
                self.target_profile()
                    .map(|p| self.desired(p))
                    .unwrap_or_default(),
                Vec::new(),
            ),
        };
        let saved_threshold = self
            .target_profile()
            .ok()
            .and_then(|p| self.config.device(p))
            .and_then(|d| d.saved_threshold());
        Status {
            schema: STATUS_SCHEMA.into(),
            daemon_version: env!("CARGO_PKG_VERSION").into(),
            config_file: tilde(&self.config_path),
            config_error: self.config_error.clone(),
            state_error: self.state_error.clone(),
            device: self.present.as_ref().map(info_for),
            desired,
            current,
            saved_threshold,
            battery: self.battery(),
            capabilities,
            desktop: gnome::read(),
            last_apply: self.last_apply().cloned(),
            conflicts: conflicts::scan(),
        }
    }
}

fn parse_kind(setting: &str) -> Result<SettingKind, String> {
    SettingKind::from_name(setting).ok_or_else(|| {
        let names: Vec<&str> = SettingKind::ALL.iter().map(|k| k.name()).collect();
        format!("unknown setting `{setting}`; known: {}", names.join(", "))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use hidlab_devices::MX_MASTER_3S;
    use hidlab_features::reprog_controls::RemapControl;
    use std::fs;

    struct Dir(PathBuf);
    impl Dir {
        fn new(tag: &str) -> Self {
            let d = std::env::temp_dir().join(format!("hidlab-core-{tag}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&d);
            fs::create_dir_all(&d).unwrap();
            Self(d)
        }
        fn engine(&self) -> Engine {
            Engine::new(self.0.join("config.toml"), self.0.join("state.toml"))
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn remembered_mode_wins_over_initial_mode() {
        let d = Dir::new("mode");
        fs::write(
            d.0.join("config.toml"),
            "format_version = 1\n[device.\"mx-master-3s\"]\ndpi = 1600\n\
             smartshift = { mode = \"ratchet\", threshold = 30 }\n",
        )
        .unwrap();
        let mut c = d.engine();
        assert_eq!(
            c.desired(&MX_MASTER_3S),
            [
                SettingValue::Dpi(1600),
                SettingValue::SmartshiftMode(2),
                SettingValue::SmartshiftThreshold(30),
            ]
        );
        c.state.set_mode(&MX_MASTER_3S, SmartShiftMode::FreeSpin);
        assert_eq!(c.desired(&MX_MASTER_3S)[1], SettingValue::SmartshiftMode(1));
    }

    #[test]
    fn set_without_a_mouse_saves_for_later() {
        let d = Dir::new("set");
        let mut c = d.engine();
        let r = c.set("dpi", "1200").unwrap();
        assert!(r.applied.is_none() && r.note.is_some());
        c.set("smartshift-mode", "free-spin").unwrap();
        // Saved to the two files, and read back by a new daemon.
        let c2 = d.engine();
        assert_eq!(
            c2.desired(&MX_MASTER_3S),
            [SettingValue::Dpi(1200), SettingValue::SmartshiftMode(1)]
        );
        assert!(c.set("dpi", "fast").is_err());
        assert!(
            c.set("wheel-mode", "on")
                .unwrap_err()
                .contains("unknown setting")
        );
    }

    #[test]
    fn threshold_off_on_and_remaps_are_saved() {
        let d = Dir::new("auto");
        let mut c = d.engine();
        c.set("smartshift-threshold", "30").unwrap();
        c.set("smartshift-threshold", "off").unwrap();
        c.set("remap-gesture", "middle").unwrap();
        let c2 = d.engine();
        assert_eq!(
            c2.desired(&MX_MASTER_3S),
            [
                SettingValue::SmartshiftThreshold(255),
                SettingValue::ButtonRemap(RemapControl::Gesture, 0x52),
            ]
        );
        // `on` brings the saved threshold back.
        let r = c.set("smartshift-threshold", "on").unwrap();
        assert_eq!(r.value, SettingValue::SmartshiftThreshold(30));
        assert!(c.set("remap-left", "back").is_err());
    }

    #[test]
    fn reset_forgets_settings_and_remembers_ratchet() {
        let d = Dir::new("reset");
        let mut c = d.engine();
        c.set("dpi", "1600").unwrap();
        c.set("smartshift-mode", "free-spin").unwrap();
        c.set("remap-back", "middle").unwrap();
        let r = c.reset("dpi").unwrap();
        assert_eq!(r.values, [SettingValue::Dpi(1000)]);
        assert!(r.applied.is_none() && r.note.is_some());
        assert_eq!(
            d.engine().desired(&MX_MASTER_3S),
            [
                SettingValue::SmartshiftMode(1),
                SettingValue::ButtonRemap(RemapControl::Back, 0x52),
            ]
        );
        let r = c.reset("all").unwrap();
        assert_eq!(r.values.len(), MX_MASTER_3S.settings.len());
        assert_eq!(r.saved_to.len(), 2);
        // No empty `[device."mx-master-3s"]` table is left behind.
        assert!(
            !fs::read_to_string(d.0.join("config.toml"))
                .unwrap()
                .contains("mx-master-3s")
        );
        assert_eq!(
            d.engine().desired(&MX_MASTER_3S),
            [SettingValue::SmartshiftMode(2)]
        );
        assert!(c.reset("wheel-mode").is_err());
    }

    #[test]
    fn button_changes_become_events_while_watched() {
        let d = Dir::new("buttons");
        let mut c = d.engine();
        assert!(!c.watching_buttons());
        assert_eq!(c.watch_buttons(600), WATCH_BUTTONS_MAX_S);
        assert!(c.watching_buttons());
        assert_eq!(c.watch_buttons(0), 0);
        assert!(!c.watching_buttons());
        let f = ButtonField {
            report_id: Some(2),
            bit_offset: 0,
            count: 16,
            first_button: 1,
        };
        // Button 4 down, then button 4 up and 7 down. No device is present,
        // so no control names.
        let ev = c.button_events(&f, 0, 0b1000);
        assert_eq!((ev[0].number, ev[0].pressed), (Some(4), true));
        let ev = c.button_events(&f, 0b1000, 0b100_0000);
        assert_eq!(ev.len(), 2);
        assert_eq!((ev[0].number, ev[0].pressed), (Some(4), false));
        assert_eq!((ev[1].number, ev[1].pressed), (Some(7), true));
        assert!(ev.iter().all(|e| e.control.is_none()));
        assert_eq!(MX_MASTER_3S.input_button(4), Some("back"));
    }

    #[test]
    fn a_broken_config_is_kept_unchanged() {
        let d = Dir::new("broken");
        let file = d.0.join("config.toml");
        fs::write(
            &file,
            "format_version = 1\n[device.\"mx-master-3s\"]\ndpi = 900\n",
        )
        .unwrap();
        let mut c = d.engine();
        fs::write(&file, "format_version = 1\ndpi = \n").unwrap();
        c.reload_config();
        // The last valid config stays, and `set` refuses to overwrite the file.
        assert_eq!(c.desired(&MX_MASTER_3S), [SettingValue::Dpi(900)]);
        assert!(c.set("dpi", "1000").unwrap_err().contains("not changed"));
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "format_version = 1\ndpi = \n"
        );
    }
}
