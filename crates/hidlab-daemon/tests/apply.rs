//! Applying settings against a stateful fake device. All data SYNTHETIC.

#[path = "../../hidlab-features/tests/common/mod.rs"]
mod common;

use common::{FakeDevice, map_for};
use hidlab_daemon::api::DpiRange;
use hidlab_daemon::apply::{ItemResult, apply, capabilities, read_snapshots, values_of};
use hidlab_devices::MX_MASTER_3S;
use hidlab_features::Reader;
use hidlab_features::reprog_controls::RemapControl;
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
    assert_eq!(report.items[0].result, ItemResult::Rejected);
    assert!(report.items[0].error.as_deref().unwrap().contains("1625"));
    assert!(!s.into_parts().0.seen.contains(&(0x2201, 3)));
}

#[test]
fn threshold_off_is_written() {
    // 255 keeps the wheel ratcheted (P-035, CONFIRMED by H5).
    let mut s = stateful(true).session();
    let map = map_for(&mut s, &FEATURES);
    let mut r = Reader::new(&mut s, map);
    let report = apply(
        &mut r,
        &MX_MASTER_3S,
        &[SettingValue::SmartshiftThreshold(255)],
    );
    assert!(report.ok, "{report:?}");
    assert_eq!(report.items[0].result, ItemResult::Written);
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

/// 0x1B04 with the group layout of P-024: back (0x53), forward (0x56) and
/// middle (0x52) in group 2 with mask 3, left (0x50) in group 1.
fn buttons() -> FakeDevice {
    let cids = [0x50u8, 0x52, 0x53, 0x56];
    let mut mapped = [0u16; 4];
    FakeDevice::new(
        &[0x1B04],
        Box::new(move |_, func, p| match func {
            0 => Ok(vec![cids.len() as u8]),
            1 => {
                let cid = cids[usize::from(p[0])];
                let (group, mask) = if cid == 0x50 { (1, 1) } else { (2, 3) };
                Ok(vec![0x00, cid, 0x00, 0x00, 0x31, 0, group, mask, 0])
            }
            2 => {
                let i = cids.iter().position(|&c| c == p[1]).ok_or(0x02u8)?;
                let [h, l] = mapped[i].to_be_bytes();
                Ok(vec![p[0], p[1], 0x00, h, l, 0x00])
            }
            3 => {
                let i = cids.iter().position(|&c| c == p[1]).ok_or(0x02u8)?;
                mapped[i] = u16::from_be_bytes([p[3], p[4]]);
                Ok(p[..5].to_vec())
            }
            _ => Err(0x07),
        }),
    )
}

#[test]
fn remaps_are_written_with_one_control_list_read() {
    let mut s = buttons().session();
    let map = map_for(&mut s, &[0x1B04]);
    let mut r = Reader::new(&mut s, map);
    let report = apply(
        &mut r,
        &MX_MASTER_3S,
        &[
            SettingValue::ButtonRemap(RemapControl::Back, 0x52),
            SettingValue::ButtonRemap(RemapControl::Forward, 0x56),
            SettingValue::ButtonRemap(RemapControl::Middle, 0x50),
        ],
    );
    let results: Vec<ItemResult> = report.items.iter().map(|i| i.result).collect();
    // Forward already does its own task; mask 3 lets middle become left.
    assert_eq!(
        results,
        [
            ItemResult::Written,
            ItemResult::Unchanged,
            ItemResult::Written
        ]
    );
    let seen = s.into_parts().0.seen;
    let n = |f: u8| seen.iter().filter(|&&x| x == (0x1B04, f)).count();
    // The control list is read once for the three buttons.
    assert_eq!((n(0), n(1)), (1, 4));
    assert!(seen.contains(&(0x1B04, 3)));
}

#[test]
fn capabilities_come_from_the_same_reads() {
    let (mut settings, mut keys) = (stateful(true).handler, buttons().handler);
    let dev = FakeDevice::new(
        &[0x2201, 0x2110, 0x2150, 0x1B04],
        Box::new(move |id, f, p| {
            if id == 0x1B04 {
                keys(id, f, p)
            } else {
                settings(id, f, p)
            }
        }),
    );
    let mut s = dev.session();
    let map = map_for(&mut s, &[0x2201, 0x2110, 0x2150, 0x1B04]);
    let mut r = Reader::new(&mut s, map);
    let snaps = read_snapshots(&mut r, &MX_MASTER_3S);
    let caps = capabilities(&MX_MASTER_3S, &snaps).unwrap();
    // SYNTHETIC list: 200 to 8000 in steps of 50.
    assert_eq!(
        caps.dpi,
        [DpiRange {
            from: 200,
            to: 8000,
            step: 50
        }]
    );
    let back = caps
        .buttons
        .iter()
        .find(|b| b.button == RemapControl::Back)
        .unwrap();
    assert_eq!(back.targets, ["back", "left", "middle", "forward"]);
    // The fake has no gesture or mode control.
    let mode = caps
        .buttons
        .iter()
        .find(|b| b.button == RemapControl::ModeButton)
        .unwrap();
    assert!(mode.targets.is_empty());
    assert_eq!(caps.defaults, MX_MASTER_3S.defaults);
    let values = values_of(&MX_MASTER_3S, &snaps);
    assert_eq!(values[0], Ok(SettingValue::Dpi(1000)));
}

#[test]
fn mode_and_threshold_share_one_read() {
    let mut s = stateful(true).session();
    let map = map_for(&mut s, &FEATURES);
    let mut r = Reader::new(&mut s, map);
    let report = apply(
        &mut r,
        &MX_MASTER_3S,
        &[
            SettingValue::SmartshiftMode(2),
            SettingValue::SmartshiftThreshold(10),
        ],
    );
    assert!(report.ok, "{report:?}");
    let seen = s.into_parts().0.seen;
    // Both already match: one getRatchetControlMode, no write.
    assert_eq!(seen.iter().filter(|&&x| x == (0x2110, 0)).count(), 1);
    assert!(!seen.contains(&(0x2110, 1)));
}
