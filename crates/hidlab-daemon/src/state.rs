//! What the daemon remembers between runs, in
//! `$XDG_STATE_HOME/hid-lab/daemon/state.toml`.
//!
//! Today this is the last SmartShift mode chosen per device (requirements,
//! Milestone 4): by the app or by the mode button. When the file
//! has no mode, the config's initial mode is used.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use hidlab_devices::DeviceProfile;
use serde::{Deserialize, Serialize};

use crate::config::SmartShiftMode;

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub smartshift_mode: Option<SmartShiftMode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub format_version: u32,
    #[serde(rename = "device", default, skip_serializing_if = "BTreeMap::is_empty")]
    pub devices: BTreeMap<String, DeviceState>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            format_version: FORMAT_VERSION,
            devices: BTreeMap::new(),
        }
    }
}

impl State {
    /// Reads `path`. A missing file is an empty state.
    pub fn load(path: &Path) -> Result<Self, String> {
        let shown = crate::paths::tilde(path);
        let text = match fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(format!("cannot read {shown}: {e}")),
        };
        let state: State =
            toml::from_str(&text).map_err(|e| format!("{shown} is damaged:\n{e}"))?;
        if state.format_version != FORMAT_VERSION {
            return Err(format!(
                "{shown} has format_version {}; expected {FORMAT_VERSION}",
                state.format_version
            ));
        }
        Ok(state)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let body = toml::to_string(self).map_err(|e| e.to_string())?;
        crate::paths::write_atomic(
            path,
            &format!("# hid-lab daemon state. Managed by hid-labd; do not edit.\n\n{body}"),
        )
    }

    pub fn mode(&self, profile: &DeviceProfile) -> Option<SmartShiftMode> {
        self.devices
            .get(profile.key)
            .and_then(|d| d.smartshift_mode)
    }

    pub fn set_mode(&mut self, profile: &DeviceProfile, mode: SmartShiftMode) {
        self.devices
            .entry(profile.key.to_owned())
            .or_default()
            .smartshift_mode = Some(mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hidlab_devices::MX_MASTER_3S;

    #[test]
    fn roundtrips_and_rejects_damage() {
        let mut s = State::default();
        assert_eq!(s.mode(&MX_MASTER_3S), None);
        s.set_mode(&MX_MASTER_3S, SmartShiftMode::FreeSpin);
        let text = toml::to_string(&s).unwrap();
        assert!(text.contains("smartshift_mode = \"free-spin\""), "{text}");
        let back: State = toml::from_str(&text).unwrap();
        assert_eq!(back.mode(&MX_MASTER_3S), Some(SmartShiftMode::FreeSpin));

        let dir = std::env::temp_dir().join(format!("hidlab-state-{}", std::process::id()));
        let file = dir.join("state.toml");
        assert_eq!(State::load(&file).unwrap(), State::default());
        fs::create_dir_all(&dir).unwrap();
        fs::write(&file, "format_version = 1\nbogus = 3\n").unwrap();
        assert!(State::load(&file).unwrap_err().contains("damaged"));
        fs::remove_dir_all(&dir).unwrap();
    }
}
