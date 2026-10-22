//! 0x2201 AdjustableDpi (read-only).
//!
//! Source: Solaar `settings_templates.py` `AdjustableDpi` (read 0x20) and
//! `produce_dpi_list` (list 0x10). The kernel does not use this feature.
//! Status: LIKELY. Function 0 is not called: no source documents it.
//!
//! Only the first list page is read. Solaar requests further pages with an
//! extra parameter whose meaning for 0x2201 is not documented; if the list
//! does not end inside the first answer, `terminated` is false.

use hidlab_hidpp::FeatureId;
use hidlab_transport::RawChannel;
use serde::Serialize;

use crate::reader::{ReadError, Reader};
use crate::util::{need, tail_hex};

pub const FEATURE: FeatureId = FeatureId(0x2201);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DpiEntry {
    Value { dpi: u16 },
    Range { from: u16, to: u16, step: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DpiList {
    pub sensor: u8,
    pub entries: Vec<DpiEntry>,
    /// A `0x0000` terminator was found.
    pub terminated: bool,
    pub raw_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CurrentDpi {
    pub sensor: u8,
    /// Raw current value; 0 means "use default" per Solaar.
    pub dpi: u16,
    pub default_dpi: u16,
    /// `dpi`, or `default_dpi` when `dpi` is 0.
    pub effective_dpi: u16,
    pub raw_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Dpi {
    pub list: DpiList,
    pub current: CurrentDpi,
    /// `effective_dpi` is one of the listed values.
    pub current_in_list: bool,
}

impl DpiList {
    pub fn contains(&self, dpi: u16) -> bool {
        self.entries.iter().any(|e| match *e {
            DpiEntry::Value { dpi: v } => v == dpi,
            DpiEntry::Range { from, to, step } => {
                dpi >= from && dpi <= to && step != 0 && (dpi - from) % step == 0
            }
        })
    }
}

pub fn decode_list(p: &[u8]) -> Result<DpiList, ReadError> {
    need(p, 1, "getSensorDpiList")?;
    let mut entries: Vec<DpiEntry> = Vec::new();
    let mut terminated = false;
    let words: Vec<u16> = p[1..]
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    let mut i = 0;
    while i < words.len() {
        let v = words[i];
        if v == 0 {
            terminated = true;
            break;
        }
        if v >> 13 == 0b111 {
            let step = v & 0x1FFF;
            let Some(&to) = words.get(i + 1) else {
                return Err(ReadError::Decode {
                    what: "getSensorDpiList",
                    reason: "step marker without range end".into(),
                });
            };
            let Some(DpiEntry::Value { dpi: from }) = entries.pop() else {
                return Err(ReadError::Decode {
                    what: "getSensorDpiList",
                    reason: "step marker without a start value".into(),
                });
            };
            if to <= from || step == 0 {
                return Err(ReadError::Decode {
                    what: "getSensorDpiList",
                    reason: format!("invalid range {from}..{to} step {step}"),
                });
            }
            entries.push(DpiEntry::Range { from, to, step });
            i += 2;
        } else {
            entries.push(DpiEntry::Value { dpi: v });
            i += 1;
        }
    }
    Ok(DpiList {
        sensor: p[0],
        entries,
        terminated,
        raw_hex: tail_hex(p, 0),
    })
}

pub fn decode_current(p: &[u8]) -> Result<CurrentDpi, ReadError> {
    need(p, 5, "getSensorDpi")?;
    let dpi = u16::from_be_bytes([p[1], p[2]]);
    let default_dpi = u16::from_be_bytes([p[3], p[4]]);
    Ok(CurrentDpi {
        sensor: p[0],
        dpi,
        default_dpi,
        effective_dpi: if dpi == 0 { default_dpi } else { dpi },
        raw_hex: tail_hex(p, 0),
    })
}

pub fn read<C: RawChannel>(r: &mut Reader<'_, C>) -> Result<Dpi, ReadError> {
    // Sensor 0, as Solaar does (all parameter bytes zero).
    let list = decode_list(&r.call(FEATURE, 1, &[0x00])?)?;
    let current = decode_current(&r.call(FEATURE, 2, &[0x00])?)?;
    Ok(Dpi {
        current_in_list: list.contains(current.effective_dpi),
        list,
        current,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_range_list() {
        // SYNTHETIC: 200..4000 step 50, then 0 terminator.
        let p = [0x00, 0x00, 0xC8, 0xE0, 0x32, 0x0F, 0xA0, 0x00, 0x00, 0, 0];
        let l = decode_list(&p).unwrap();
        assert!(l.terminated);
        assert_eq!(
            l.entries,
            [DpiEntry::Range {
                from: 200,
                to: 4000,
                step: 50
            }]
        );
        assert!(l.contains(1000));
        assert!(!l.contains(1025));
        assert!(!l.contains(4050));
    }

    #[test]
    fn decodes_discrete_list() {
        let p = [0x00, 0x01, 0x90, 0x03, 0x20, 0x00, 0x00];
        let l = decode_list(&p).unwrap();
        assert_eq!(
            l.entries,
            [DpiEntry::Value { dpi: 400 }, DpiEntry::Value { dpi: 800 }]
        );
    }

    #[test]
    fn unterminated_and_invalid_lists() {
        assert!(!decode_list(&[0x00, 0x01, 0x90]).unwrap().terminated);
        assert!(decode_list(&[0x00, 0xE0, 0x32, 0x0F, 0xA0]).is_err());
        assert!(decode_list(&[0x00, 0x00, 0xC8, 0xE0, 0x32]).is_err());
    }

    #[test]
    fn current_uses_default_when_zero() {
        let c = decode_current(&[0, 0x00, 0x00, 0x03, 0xE8]).unwrap();
        assert_eq!(c.effective_dpi, 1000);
        let c = decode_current(&[0, 0x06, 0x40, 0x03, 0xE8]).unwrap();
        assert_eq!(c.effective_dpi, 1600);
    }
}
