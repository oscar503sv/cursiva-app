//! Read every milestone-3 feature in one pass.

use hidlab_hidpp::{FeatureId, FeatureInfo, Session};
use hidlab_transport::RawChannel;
use serde::Serialize;

use crate::reader::{FeatureMap, ReadError, Reader};
use crate::{
    battery, change_host, device_name, dpi, firmware, hires_wheel, reprog_controls, smartshift,
    thumbwheel,
};

/// Features read by [`read_all`], in reading order.
pub const TARGET_FEATURES: [FeatureId; 9] = [
    device_name::FEATURE,
    firmware::FEATURE,
    battery::FEATURE,
    change_host::FEATURE,
    reprog_controls::FEATURE,
    dpi::FEATURE,
    smartshift::FEATURE,
    hires_wheel::FEATURE,
    thumbwheel::FEATURE,
];

/// Outcome of reading one feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Reading<T> {
    /// The device does not report this feature. Nothing was sent.
    NotSupported,
    Ok {
        value: T,
    },
    /// The device rejected a request or the answer could not be decoded.
    Failed {
        error: String,
    },
}

impl<T> Reading<T> {
    pub fn value(&self) -> Option<&T> {
        match self {
            Reading::Ok { value } => Some(value),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceState {
    /// Where each target feature lives on this device (resolved now).
    pub resolved: Vec<FeatureInfo>,
    pub device_name: Reading<device_name::DeviceName>,
    pub firmware: Reading<firmware::Firmware>,
    pub battery: Reading<battery::Battery>,
    pub change_host: Reading<change_host::ChangeHost>,
    pub reprog_controls: Reading<reprog_controls::ReprogControls>,
    pub dpi: Reading<dpi::Dpi>,
    pub smartshift: Reading<smartshift::SmartShift>,
    pub hires_wheel: Reading<hires_wheel::HiResWheel>,
    pub thumbwheel: Reading<thumbwheel::Thumbwheel>,
}

fn one<C, T>(
    r: &mut Reader<'_, C>,
    feature: FeatureId,
    f: impl FnOnce(&mut Reader<'_, C>) -> Result<T, ReadError>,
) -> Result<Reading<T>, ReadError>
where
    C: RawChannel,
{
    if r.map().get(feature).is_none() {
        return Ok(Reading::NotSupported);
    }
    match f(r) {
        Ok(value) => Ok(Reading::Ok { value }),
        Err(e) if e.is_fatal() => Err(e),
        Err(e) => Ok(Reading::Failed {
            error: e.to_string(),
        }),
    }
}

/// Resolves feature indices, then reads every target feature. Read-only.
///
/// Timeouts and disconnects abort; other errors are recorded per feature.
pub fn read_all<C: RawChannel>(session: &mut Session<C>) -> Result<DeviceState, ReadError> {
    let map = FeatureMap::resolve(session, &TARGET_FEATURES).map_err(ReadError::from)?;
    let resolved: Vec<FeatureInfo> = TARGET_FEATURES
        .iter()
        .filter_map(|&id| map.get(id).copied())
        .collect();
    let mut r = Reader::new(session, map);
    Ok(DeviceState {
        resolved,
        device_name: one(&mut r, device_name::FEATURE, device_name::read)?,
        firmware: one(&mut r, firmware::FEATURE, firmware::read)?,
        battery: one(&mut r, battery::FEATURE, battery::read)?,
        change_host: one(&mut r, change_host::FEATURE, change_host::read)?,
        reprog_controls: one(&mut r, reprog_controls::FEATURE, reprog_controls::read)?,
        dpi: one(&mut r, dpi::FEATURE, dpi::read)?,
        smartshift: one(&mut r, smartshift::FEATURE, smartshift::read)?,
        hires_wheel: one(&mut r, hires_wheel::FEATURE, hires_wheel::read)?,
        thumbwheel: one(&mut r, thumbwheel::FEATURE, thumbwheel::read)?,
    })
}
