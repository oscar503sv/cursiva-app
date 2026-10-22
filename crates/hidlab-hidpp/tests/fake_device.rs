//! Enumeration against a fake HID++ 2.0 device that shares its channel with
//! noise, the way a real hidraw node is shared with mouse input and the
//! kernel driver.
//!
//! All feature tables here are SYNTHETIC. They are not MX Master 3S data.

use std::collections::VecDeque;
use std::time::Duration;

use hidlab_hidpp::{
    Disposition, Enumeration, FeatureId, HidppError, ReportKind, Session, SessionConfig, enumerate,
};
use hidlab_transport::{RawChannel, TransportError};

const DEV: u8 = 0xFF;

struct FakeDevice {
    /// Position = feature index. Index 0 must be root.
    features: Vec<(u16, u8, u8)>,
    protocol: (u8, u8),
    /// Feature indices whose getFeatureID answers with an error.
    reject_indices: Vec<u8>,
    /// Answer every request with a long report.
    respond_long: bool,
    /// Never answer (but still emit noise).
    silent: bool,
    /// Disconnect after this many requests.
    disconnect_after: Option<usize>,
    outbox: VecDeque<Vec<u8>>,
    requests_seen: usize,
}

impl FakeDevice {
    fn new(features: Vec<(u16, u8, u8)>) -> Self {
        Self {
            features,
            protocol: (4, 2),
            reject_indices: vec![],
            respond_long: false,
            silent: false,
            disconnect_after: None,
            outbox: VecDeque::new(),
            requests_seen: 0,
        }
    }

    fn frame(&self, kind: ReportKind, fi: u8, b3: u8, params: &[u8]) -> Vec<u8> {
        // A short report carries only 3 parameter bytes; larger answers
        // (e.g. getFeatureID, 4 bytes) must go out as long reports.
        let kind = if params.len() > kind.param_capacity() {
            ReportKind::Long
        } else {
            kind
        };
        let mut out = vec![0u8; kind.report_len()];
        out[0] = kind.report_id();
        out[1] = DEV;
        out[2] = fi;
        out[3] = b3;
        out[4..4 + params.len()].copy_from_slice(params);
        out
    }

    fn index_of(&self, id: u16) -> u8 {
        self.features
            .iter()
            .position(|f| f.0 == id)
            .map_or(0, |i| i as u8)
    }

    fn answer(&self, fi: u8, func: u8, params: &[u8]) -> Result<Vec<u8>, u8> {
        let feature = self.features.get(usize::from(fi)).ok_or(0x06u8)?.0;
        match (feature, func) {
            (0x0000, 0) => {
                let id = u16::from_be_bytes([params[0], params[1]]);
                let idx = self.index_of(id);
                let (_, flags, ver) = self.features[usize::from(idx)];
                if idx == 0 && id != 0 {
                    Ok(vec![0, 0, 0])
                } else {
                    Ok(vec![idx, flags, ver])
                }
            }
            (0x0000, 1) => Ok(vec![self.protocol.0, self.protocol.1, params[2]]),
            (0x0001, 0) => Ok(vec![(self.features.len() - 1) as u8]),
            (0x0001, 1) => {
                let i = params[0];
                if self.reject_indices.contains(&i) {
                    return Err(0x02);
                }
                let (id, flags, ver) = *self.features.get(usize::from(i)).ok_or(0x02u8)?;
                let [hi, lo] = id.to_be_bytes();
                Ok(vec![hi, lo, flags, ver])
            }
            _ => Err(0x07),
        }
    }
}

impl RawChannel for FakeDevice {
    fn send(&mut self, report: &[u8]) -> Result<(), TransportError> {
        self.requests_seen += 1;
        if self
            .disconnect_after
            .is_some_and(|n| self.requests_seen > n)
        {
            return Err(TransportError::Disconnected {
                node: "/dev/hidraw-fake".into(),
            });
        }
        let kind = ReportKind::from_report_id(report[0]).expect("fake got non-HID++ request");
        assert_eq!(report.len(), kind.report_len(), "wrong request length");
        assert_eq!(report[1], DEV);
        let fi = report[2];
        let func = report[3] >> 4;
        let sw = report[3] & 0x0F;
        assert_ne!(sw, 0, "requests must use a non-zero sw_id");
        let params = report[4..].to_vec();
        let out_kind = if self.respond_long {
            ReportKind::Long
        } else {
            kind
        };
        let other_sw = if sw == 0x0F { 0x0E } else { sw + 1 };

        // Noise before every answer.
        self.outbox
            .push_back(vec![0x02, 0x00, 0x05, 0x00, 0xFD, 0xFF, 0x00, 0x00]); // mouse movement
        let notif = self.frame(ReportKind::Long, fi, func << 4, &[0xAA, 0xBB]); // sw_id 0
        self.outbox.push_back(notif);
        let wrong_sw = self.frame(out_kind, fi, (func << 4) | other_sw, &[0xEE, 0xEE, 0xEE]);
        self.outbox.push_back(wrong_sw);
        let foreign_err = self.frame(out_kind, 0xFF, fi, &[(func << 4) | other_sw, 0x05]);
        self.outbox.push_back(foreign_err);
        self.outbox.push_back(vec![0x11, 0xFF]); // truncated junk

        if self.silent {
            return Ok(());
        }
        let reply = match self.answer(fi, func, &params) {
            Ok(p) => self.frame(out_kind, fi, (func << 4) | sw, &p),
            Err(code) => self.frame(out_kind, 0xFF, fi, &[(func << 4) | sw, code]),
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

/// SYNTHETIC table: root, IFeatureSet, and a few arbitrary IDs, one unknown.
fn synthetic_table() -> Vec<(u16, u8, u8)> {
    vec![
        (0x0000, 0x00, 0x02),
        (0x0001, 0x00, 0x02),
        (0x0003, 0x00, 0x04),
        (0x1004, 0x00, 0x05),
        (0x00C2, 0x40, 0x00), // hidden flag
        (0xABCD, 0x20, 0x01), // unknown, engineering flag
        (0x2201, 0x00, 0x02),
    ]
}

fn run(
    dev: FakeDevice,
    report: ReportKind,
) -> (Result<Enumeration, HidppError>, Session<FakeDevice>) {
    let mut s = Session::new(
        dev,
        SessionConfig {
            report,
            timeout: Duration::from_millis(100),
            ..SessionConfig::default()
        },
    );
    (enumerate(&mut s), s)
}

#[test]
fn enumerates_through_noise_with_short_reports() {
    let table = synthetic_table();
    let (r, s) = run(FakeDevice::new(table.clone()), ReportKind::Short);
    let e = r.unwrap();
    assert_eq!((e.protocol.major, e.protocol.minor), (4, 2));
    assert_eq!(e.feature_set.index, 1);
    assert_eq!(usize::from(e.count), table.len() - 1);
    assert!(e.failures.is_empty());
    let got: Vec<_> = e
        .features
        .iter()
        .map(|f| (f.id.0, f.flags, f.version))
        .collect();
    assert_eq!(got, table);
    for (i, f) in e.features.iter().enumerate() {
        assert_eq!(usize::from(f.index), i);
    }

    // 3 + (count + 1) requests, 5 noise reports each, all ignored.
    let requests = 3 + table.len();
    assert_eq!(s.ignored(), (requests * 5) as u64);
    let t = s.transcript();
    assert_eq!(t.count(Disposition::Sent), requests);
    assert_eq!(t.count(Disposition::Matched), requests);
    assert_eq!(t.count(Disposition::Malformed), requests);
    assert!(t.entries.iter().all(|e| !e.hex.is_empty()));
}

#[test]
fn long_requests_and_long_answers() {
    let mut dev = FakeDevice::new(synthetic_table());
    dev.respond_long = true;
    let (r, _) = run(dev, ReportKind::Long);
    assert_eq!(r.unwrap().features.len(), synthetic_table().len());
}

#[test]
fn long_answers_to_short_requests_are_accepted() {
    let mut dev = FakeDevice::new(synthetic_table());
    dev.respond_long = true;
    let (r, _) = run(dev, ReportKind::Short);
    assert_eq!(r.unwrap().features.len(), synthetic_table().len());
}

#[test]
fn rejected_index_is_reported_and_enumeration_continues() {
    let mut dev = FakeDevice::new(synthetic_table());
    dev.reject_indices = vec![4];
    let (r, _) = run(dev, ReportKind::Short);
    let e = r.unwrap();
    assert_eq!(e.features.len(), synthetic_table().len() - 1);
    assert_eq!(e.failures.len(), 1);
    assert_eq!(e.failures[0].index, 4);
    assert!(e.failures[0].error.contains("invalid argument"));
    assert!(e.features.iter().all(|f| f.index != 4));
}

#[test]
fn missing_feature_set_is_typed() {
    let (r, _) = run(FakeDevice::new(vec![(0x0000, 0, 2)]), ReportKind::Short);
    assert!(matches!(r, Err(HidppError::FeatureSetMissing)));
}

#[test]
fn silent_device_times_out_with_ignored_count() {
    let mut dev = FakeDevice::new(synthetic_table());
    dev.silent = true;
    let (r, _) = run(dev, ReportKind::Short);
    match r {
        Err(HidppError::Timeout {
            feature_index: 0,
            function: 1,
            ignored,
            ..
        }) => assert_eq!(ignored, 5),
        other => panic!("expected timeout, got {other:?}"),
    }
}

#[test]
fn disconnect_mid_enumeration_is_typed() {
    let mut dev = FakeDevice::new(synthetic_table());
    dev.disconnect_after = Some(4);
    let (r, _) = run(dev, ReportKind::Short);
    assert!(matches!(
        r,
        Err(HidppError::Transport(TransportError::Disconnected { .. }))
    ));
}

#[test]
fn unknown_feature_id_is_kept_verbatim() {
    let (r, _) = run(FakeDevice::new(synthetic_table()), ReportKind::Short);
    assert!(
        r.unwrap()
            .features
            .iter()
            .any(|f| f.id == FeatureId(0xABCD))
    );
}
