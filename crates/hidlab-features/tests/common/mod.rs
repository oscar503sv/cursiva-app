//! A fake HID++ 2.0 device for typed-read tests.
//!
//! Every answer is SYNTHETIC. Nothing here is MX Master 3S data.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::time::Duration;

use hidlab_hidpp::{FeatureId, ReportKind, Session, SessionConfig};
use hidlab_transport::{RawChannel, TransportError};

/// Answers `(feature_id, function, params)` with `Ok(params)` or `Err(hidpp20_error_code)`.
pub type Handler = Box<dyn FnMut(u16, u8, &[u8]) -> Result<Vec<u8>, u8>>;

pub struct FakeDevice {
    /// Position = feature index. Index 0 is root and is answered internally.
    pub features: Vec<u16>,
    pub handler: Handler,
    /// Every request seen, as (feature_id, function).
    pub seen: Vec<(u16, u8)>,
    outbox: VecDeque<Vec<u8>>,
}

impl FakeDevice {
    pub fn new(features: &[u16], handler: Handler) -> Self {
        let mut all = vec![0x0000];
        all.extend_from_slice(features);
        Self {
            features: all,
            handler,
            seen: vec![],
            outbox: VecDeque::new(),
        }
    }

    pub fn session(self) -> Session<FakeDevice> {
        Session::new(
            self,
            SessionConfig {
                timeout: Duration::from_millis(50),
                ..SessionConfig::default()
            },
        )
    }

    fn long(fi: u8, b3: u8, params: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; ReportKind::Long.report_len()];
        out[0] = 0x11;
        out[1] = 0xFF;
        out[2] = fi;
        out[3] = b3;
        let n = params.len().min(16);
        out[4..4 + n].copy_from_slice(&params[..n]);
        out
    }
}

impl RawChannel for FakeDevice {
    fn send(&mut self, report: &[u8]) -> Result<(), TransportError> {
        let fi = report[2];
        let func = report[3] >> 4;
        let sw = report[3] & 0x0F;
        let params = &report[4..];
        let feature = self.features.get(usize::from(fi)).copied();

        // Noise before every answer: a mouse report and a notification.
        self.outbox
            .push_back(vec![0x02, 0x00, 0x01, 0x00, 0xFF, 0xFF, 0x00, 0x00]);
        self.outbox.push_back(Self::long(fi, func << 4, &[0xAA]));

        let answer = match feature {
            None => Err(0x06),
            Some(0x0000) if func == 0 => {
                let id = u16::from_be_bytes([params[0], params[1]]);
                let idx = self.features.iter().position(|&f| f == id).unwrap_or(0) as u8;
                Ok(vec![idx, 0x00, 0x00])
            }
            Some(id) => {
                self.seen.push((id, func));
                (self.handler)(id, func, params)
            }
        };
        let reply = match answer {
            Ok(p) => Self::long(fi, (func << 4) | sw, &p),
            Err(code) => Self::long(0xFF, fi, &[(func << 4) | sw, code]),
        };
        self.outbox.push_back(reply);
        Ok(())
    }

    fn recv(&mut self, buf: &mut [u8], _: Duration) -> Result<Option<usize>, TransportError> {
        Ok(self.outbox.pop_front().map(|r| {
            buf[..r.len()].copy_from_slice(&r);
            r.len()
        }))
    }
}

/// Resolves `ids` and returns a reader-ready map.
pub fn map_for(session: &mut Session<FakeDevice>, ids: &[u16]) -> hidlab_features::FeatureMap {
    let ids: Vec<FeatureId> = ids.iter().map(|&i| FeatureId(i)).collect();
    hidlab_features::FeatureMap::resolve(session, &ids).unwrap()
}
