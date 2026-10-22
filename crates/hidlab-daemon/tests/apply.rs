//! Applying settings against a stateful fake device. All data SYNTHETIC.

#[path = "../../hidlab-features/tests/common/mod.rs"]
mod common;

use common::{FakeDevice, map_for};
use hidlab_daemon::apply::{ItemResult, apply};
use hidlab_devices::MX_MASTER_3S;
use hidlab_features::Reader;
use hidlab_features::settings::SettingValue;

/// Stores DPI, SmartShift and thumb wheel state; `obey` false ignores writes.
fn stateful(obey: bool) -> FakeDevice {
    let mut dpi: u16 = 1000;
    let (mut mode, mut thr) = (2u8, 10u8);
    let mut thumb = [0x00u8, 0x00];
    FakeDevice::new(
        &[0x2201, 0x2110, 0x2150],
        Box::new(move |id, func, p| match (id, func) {
            (0x2201, 1) => Ok(vec![0, 0x00, 0xC8, 0xE0, 0x32, 0x1F, 0x40, 0, 0]),
            (0x2201, 2) => {
                let [h, l] = dpi.to_be_bytes();
                Ok(vec![0, h, l, 0x03, 0xE8])
            }
            (0x2201, 3) => {
                if obey {
                    dpi = u16::from_be_bytes([p[1], p[2]]);
                }
                Ok(p[..3].to_vec())
            }
            (0x2110, 0) => Ok(vec![mode, thr, 10]),
            (0x2110, 1) => {
                if obey && p[0] != 0 {
                    mode = p[0];
                }
                if obey && p[1] != 0 {
                    thr = p[1];
                }
                Ok(p[..2].to_vec())
            }
            (0x2150, 1) => Ok(thumb.to_vec()),
            (0x2150, 2) => {
                if obey {
                    thumb = [p[0], p[1]];
                }
                Ok(vec![])
            }
            _ => Err(0x07),
        }),
    )
}

const FEATURES: [u16; 3] = [0x2201, 0x2110, 0x2150];

#[test]
fn writes_only_what_differs_and_verifies() {
    let mut s = stateful(true).session();
    let map = map_for(&mut s, &FEATURES);
    let mut r = Reader::new(&mut s, map);
    let desired = [
        SettingValue::Dpi(1600),
        SettingValue::SmartshiftThreshold(10),
        SettingValue::ThumbwheelInvert(true),
        SettingValue::SmartshiftMode(1),
    ];
    let report = apply(&mut r, &MX_MASTER_3S, &desired);
    assert!(report.ok, "{report:?}");
    let results: Vec<ItemResult> = report.items.iter().map(|i| i.result).collect();
    assert_eq!(
        results,
        [
            ItemResult::Written,
            ItemResult::Unchanged,
            ItemResult::Written,
            ItemResult::Written
        ]
    );
    assert_eq!(report.items[0].before, Some(SettingValue::Dpi(1000)));
    let writes: Vec<(u16, u8)> = s
        .into_parts()
        .0
        .seen
        .into_iter()
        .filter(|&(id, f)| matches!((id, f), (0x2201, 3) | (0x2110, 1) | (0x2150, 2)))
        .collect();
    // The unchanged threshold was not written.
    assert_eq!(writes, [(0x2201, 3), (0x2150, 2), (0x2110, 1)]);
}

#[test]
fn a_device_that_ignores_writes_is_a_mismatch() {
    let mut s = stateful(false).session();
    let map = map_for(&mut s, &FEATURES);
    let mut r = Reader::new(&mut s, map);
    let report = apply(&mut r, &MX_MASTER_3S, &[SettingValue::Dpi(1600)]);
    assert!(!report.ok);
    assert_eq!(report.items[0].result, ItemResult::Mismatch);
    assert!(report.items[0].checks.iter().any(|c| !c.ok));
}

#[test]
fn invalid_dpi_is_not_sent() {
    let mut s = stateful(true).session();
    let map = map_for(&mut s, &FEATURES);
    let mut r = Reader::new(&mut s, map);
    let report = apply(&mut r, &MX_MASTER_3S, &[SettingValue::Dpi(1625)]);
    assert_eq!(report.items[0].result, ItemResult::Failed);
    assert!(report.items[0].error.as_deref().unwrap().contains("1625"));
    assert!(!s.into_parts().0.seen.contains(&(0x2201, 3)));
}

#[test]
fn a_silent_device_stops_the_run() {
    let mut dev = stateful(true);
    dev.silent = vec![0x2201];
    let mut s = dev.session();
    let map = map_for(&mut s, &FEATURES);
    let mut r = Reader::new(&mut s, map);
    let report = apply(
        &mut r,
        &MX_MASTER_3S,
        &[
            SettingValue::Dpi(1600),
            SettingValue::ThumbwheelInvert(true),
        ],
    );
    assert_eq!(report.items[0].result, ItemResult::Failed);
    assert!(
        report.items[1]
            .error
            .as_deref()
            .unwrap()
            .starts_with("not sent")
    );
    assert!(!s.into_parts().0.seen.iter().any(|&(id, _)| id == 0x2150));
}
