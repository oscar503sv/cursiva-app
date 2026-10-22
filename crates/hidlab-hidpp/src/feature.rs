//! Root (0x0000) and IFeatureSet (0x0001) functions, and full enumeration.
//!
//! Status: LIKELY (docs/protocol/README.md, P-008..P-012). Sources: Linux
//! `hid-logitech-hidpp.c` and Solaar (GPL-2.0), behavior described only.
//! All requests here are read-only queries.

use std::fmt;

use hidlab_transport::RawChannel;
use serde::Serialize;

use crate::error::HidppError;
use crate::session::Session;

/// Root feature is always at feature index 0.
pub const ROOT_INDEX: u8 = 0x00;
const ROOT_FN_GET_FEATURE: u8 = 0;
const ROOT_FN_GET_PROTOCOL_VERSION: u8 = 1;
const FEATURE_SET_FN_GET_COUNT: u8 = 0;
const FEATURE_SET_FN_GET_FEATURE_ID: u8 = 1;

/// A 16-bit HID++ 2.0 feature ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct FeatureId(pub u16);

impl FeatureId {
    pub const ROOT: FeatureId = FeatureId(0x0000);
    pub const FEATURE_SET: FeatureId = FeatureId(0x0001);
}

impl fmt::Display for FeatureId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:04X}", self.0)
    }
}

/// HID++ protocol version as reported by root getProtocolVersion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ProtocolVersion {
    pub major: u8,
    pub minor: u8,
}

/// A feature and where the device placed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FeatureInfo {
    pub index: u8,
    pub id: FeatureId,
    /// Raw feature type byte.
    pub flags: u8,
    pub version: u8,
}

/// Root getFeature. Returns `None` if the device does not support `id`.
pub fn get_feature<C: RawChannel>(
    s: &mut Session<C>,
    id: FeatureId,
) -> Result<Option<FeatureInfo>, HidppError> {
    let p = s.request(ROOT_INDEX, ROOT_FN_GET_FEATURE, &id.0.to_be_bytes())?;
    let [index, flags, version, ..] = p[..] else {
        return Err(HidppError::Malformed(format!(
            "getFeature response has {} bytes",
            p.len()
        )));
    };
    if index == 0 && id != FeatureId::ROOT {
        return Ok(None);
    }
    Ok(Some(FeatureInfo {
        index,
        id,
        flags,
        version,
    }))
}

/// Root getProtocolVersion with ping echo check.
pub fn get_protocol_version<C: RawChannel>(
    s: &mut Session<C>,
    ping: u8,
) -> Result<ProtocolVersion, HidppError> {
    let p = s.request(ROOT_INDEX, ROOT_FN_GET_PROTOCOL_VERSION, &[0, 0, ping])?;
    let [major, minor, echo, ..] = p[..] else {
        return Err(HidppError::Malformed(format!(
            "getProtocolVersion response has {} bytes",
            p.len()
        )));
    };
    if echo != ping {
        return Err(HidppError::PingMismatch {
            sent: ping,
            got: echo,
        });
    }
    Ok(ProtocolVersion { major, minor })
}

/// IFeatureSet getCount: number of features, not counting root.
pub fn feature_count<C: RawChannel>(
    s: &mut Session<C>,
    feature_set_index: u8,
) -> Result<u8, HidppError> {
    let p = s.request(feature_set_index, FEATURE_SET_FN_GET_COUNT, &[])?;
    p.first()
        .copied()
        .ok_or_else(|| HidppError::Malformed("getCount response is empty".into()))
}

/// IFeatureSet getFeatureID for one feature index.
pub fn feature_at<C: RawChannel>(
    s: &mut Session<C>,
    feature_set_index: u8,
    index: u8,
) -> Result<FeatureInfo, HidppError> {
    let p = s.request(feature_set_index, FEATURE_SET_FN_GET_FEATURE_ID, &[index])?;
    let [hi, lo, flags, version, ..] = p[..] else {
        return Err(HidppError::Malformed(format!(
            "getFeatureID response has {} bytes",
            p.len()
        )));
    };
    Ok(FeatureInfo {
        index,
        id: FeatureId(u16::from_be_bytes([hi, lo])),
        flags,
        version,
    })
}

/// A feature index whose query was rejected by the device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FeatureFailure {
    pub index: u8,
    pub error: String,
}

/// Result of a full feature enumeration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Enumeration {
    pub protocol: ProtocolVersion,
    /// Where getFeature(0x0001) says IFeatureSet lives.
    pub feature_set: FeatureInfo,
    /// getCount result (excludes root).
    pub count: u8,
    /// Indices 0..=count, in order.
    pub features: Vec<FeatureInfo>,
    /// Indices the device rejected individually.
    pub failures: Vec<FeatureFailure>,
}

/// Ping value used for getProtocolVersion.
pub const ENUMERATION_PING: u8 = 0x5A;

/// Reads protocol version and every feature. Read-only.
///
/// Timeouts and transport errors abort; a device error on a single index is
/// recorded in `failures` and enumeration continues.
pub fn enumerate<C: RawChannel>(s: &mut Session<C>) -> Result<Enumeration, HidppError> {
    let protocol = get_protocol_version(s, ENUMERATION_PING)?;
    let feature_set =
        get_feature(s, FeatureId::FEATURE_SET)?.ok_or(HidppError::FeatureSetMissing)?;
    let count = feature_count(s, feature_set.index)?;

    let mut features = Vec::with_capacity(usize::from(count) + 1);
    let mut failures = Vec::new();
    for index in 0..=count {
        match feature_at(s, feature_set.index, index) {
            Ok(info) => features.push(info),
            Err(e @ (HidppError::Device { .. } | HidppError::Device10 { .. })) => {
                failures.push(FeatureFailure {
                    index,
                    error: e.to_string(),
                })
            }
            Err(e) => return Err(e),
        }
    }

    Ok(Enumeration {
        protocol,
        feature_set,
        count,
        features,
        failures,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_id_display() {
        assert_eq!(FeatureId(0x1B04).to_string(), "0x1B04");
        assert_eq!(FeatureId::ROOT.to_string(), "0x0000");
    }
}
