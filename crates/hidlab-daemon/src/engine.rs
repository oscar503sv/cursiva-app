//! The daemon's state and operations, independent of D-Bus.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use hidlab_devices::{DeviceProfile, PROFILES, profile_for};
use hidlab_features::settings::{SettingKind, SettingValue};
use hidlab_features::{FeatureMap, Reader};
use hidlab_hidpp::{Session, SessionConfig};
use hidlab_transport::{Access, Hidraw, HidrawDevice, discover};

use crate::api::{
    CurrentValue, DEVICE_EVENT_SCHEMA, DeviceEvent, DeviceInfo, REAPPLY_SCHEMA, ReapplyReply,
    SET_SCHEMA, STATUS_SCHEMA, SetReply, Status,
};
use crate::apply::{self, ApplyReport, ItemResult};
use crate::config::{Config, SmartShiftMode};
use crate::conflicts;
use crate::paths::tilde;
use crate::state::State;
use crate::{debug, info, warn};

/// Software ID of daemon requests, different from the CLI's `0x0B` so that
/// `hid-lab lab watch` can tell them apart. Any non-zero value is valid.
pub const DAEMON_SW_ID: u8 = 0x0C;
const REQUEST_TIMEOUT: Duration = Duration::from_millis(1000);

/// The connected device the daemon manages.
#[derive(Debug, Clone)]
pub struct Present {
    pub device: HidrawDevice,
    pub profile: &'static DeviceProfile,
}

pub struct Engine {
    config_path: PathBuf,
    state_path: PathBuf,
    config: Config,
    config_error: Option<String>,
    state: State,
    state_error: Option<String>,
    present: Option<Present>,
    last_apply: Option<(ApplyReport, u64)>,
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
        let map = FeatureMap::resolve(&mut session, &apply::features_for(p.profile))
            .map_err(|e| e.to_string())?;
        let out = f(&mut Reader::new(&mut session, map), p.profile);
        if crate::log::enabled(crate::log::Level::Debug) {
            for e in &session.transcript().entries {
                debug!("{:?} {:?} {}", e.dir, e.disposition, e.hex);
            }
        }
        Ok(out)
    }

    fn warn_conflicts() {
        for c in conflicts::scan() {
            warn!("{}", c.advice());
        }
    }

    fn record(&mut self, report: &ApplyReport) {
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
        info!(
            "applied {} setting(s) in {} ms: {}",
            report.items.len(),
            report.elapsed_ms,
            if report.ok {
                "verified"
            } else {
                "with problems"
            }
        );
        self.last_apply = Some((report.clone(), now_unix()));
    }

    /// Applies the whole configuration to the connected device.
    pub fn apply_all(&mut self) -> Result<ApplyReport, String> {
        let profile = self
            .present
            .as_ref()
            .ok_or("the mouse is not connected; move it or turn it on")?
            .profile;
        let desired = self.desired(profile);
        Self::warn_conflicts();
        let report = self.with_reader(|r, p| apply::apply(r, p, &desired))?;
        self.record(&report);
        Ok(report)
    }

    /// `Reapply`: reloads the config file, then applies it.
    pub fn reapply(&mut self) -> Result<ReapplyReply, String> {
        self.reload_config();
        let report = self.apply_all()?;
        Ok(ReapplyReply {
            schema: REAPPLY_SCHEMA.into(),
            config_error: self.config_error.clone(),
            report,
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

    /// `Set`: applies one setting if the mouse is connected and stores it,
    /// unless the mouse rejected the value.
    pub fn set(&mut self, setting: &str, value: &str) -> Result<SetReply, String> {
        let kind = SettingKind::from_name(setting).ok_or_else(|| {
            let names: Vec<&str> = SettingKind::ALL.iter().map(|k| k.name()).collect();
            format!("unknown setting `{setting}`; known: {}", names.join(", "))
        })?;
        let target = kind.parse_value(value)?;
        let profile = self.target_profile()?;
        if !profile.supports(kind) {
            return Err(format!("{} does not support {setting}", profile.name));
        }
        if let Some(e) = &self.config_error {
            return Err(format!(
                "the config file has an error, so it was not changed:\n{e}\n\
                 Fix the file, then run `hid-lab reapply`."
            ));
        }

        let applied = if self.present.is_some() {
            Self::warn_conflicts();
            let report = self.with_reader(|r, p| apply::apply(r, p, &[target]))?;
            self.record(&report);
            Some(report)
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

    /// `GetStatus`. Reads the current values from the mouse (reads only).
    pub fn status(&self) -> Status {
        let (desired, current) = match &self.present {
            Some(p) => {
                let desired = self.desired(p.profile);
                let current = match self.with_reader(apply::read_current) {
                    Ok(values) => p
                        .profile
                        .settings
                        .iter()
                        .zip(values)
                        .map(|(&setting, v)| CurrentValue {
                            setting,
                            error: v.as_ref().err().cloned(),
                            value: v.ok(),
                        })
                        .collect(),
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
        Status {
            schema: STATUS_SCHEMA.into(),
            daemon_version: env!("CARGO_PKG_VERSION").into(),
            config_file: tilde(&self.config_path),
            config_error: self.config_error.clone(),
            state_error: self.state_error.clone(),
            device: self.present.as_ref().map(info_for),
            desired,
            current,
            last_apply: self.last_apply.as_ref().map(|(r, _)| r.clone()),
            last_apply_unix: self.last_apply.as_ref().map(|(_, t)| *t),
            conflicts: conflicts::scan(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hidlab_devices::MX_MASTER_3S;
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
