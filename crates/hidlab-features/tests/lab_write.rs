//! Write experiments against a stateful fake device. All data SYNTHETIC.

mod common;

use common::{FakeDevice, map_for};
use hidlab_features::lab_write::{self, SettingKind, SettingValue};
use hidlab_features::{Reader, WriteIntent};

/// A fake that stores DPI, SmartShift and thumb wheel state. If
/// `zero_means_keep` is false, a 0 field overwrites the stored value.
fn stateful(zero_means_keep: bool) -> FakeDevice {
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
                dpi = u16::from_be_bytes([p[1], p[2]]);
                Ok(p[..3].to_vec())
            }
            (0x2110, 0) => Ok(vec![mode, thr, 10]),
            (0x2110, 1) => {
                if p[0] != 0 || !zero_means_keep {
                    mode = p[0];
                }
                if p[1] != 0 || !zero_means_keep {
                    thr = p[1];
                }
                Ok(vec![mode, thr])
            }
            (0x2150, 1) => Ok(thumb.to_vec()),
            (0x2150, 2) => {
                thumb = [p[0], p[1]];
                Ok(vec![])
            }
            _ => Err(0x07),
        }),
    )
}

fn run(dev: FakeDevice, target: SettingValue) -> lab_write::WriteOutcome {
    let mut s = dev.session();
    let map = map_for(&mut s, &[0x2201, 0x2110, 0x2150]);
    let mut r = Reader::new(&mut s, map);
    let before = lab_write::snapshot(&mut r, target.kind()).unwrap();
    let plan = lab_write::plan(&before, target).unwrap();
    let intent = WriteIntent::explicit_user_request();
    lab_write::execute(&mut r, &intent, &before, plan).unwrap()
}

#[test]
fn each_setting_writes_and_verifies() {
    for target in [
        SettingValue::Dpi(1600),
        SettingValue::SmartshiftMode(1),
        SettingValue::SmartshiftThreshold(40),
        SettingValue::ThumbwheelInvert(true),
    ] {
        let out = run(stateful(true), target);
        assert!(out.pass, "{target:?}: {:?}", out.checks);
        assert_eq!(out.after.value(target.kind()), Some(target));
    }
}

#[test]
fn side_effect_on_other_field_fails_verification() {
    let out = run(stateful(false), SettingValue::SmartshiftMode(1));
    assert!(!out.pass);
    let bad: Vec<_> = out
        .checks
        .iter()
        .filter(|c| !c.ok)
        .map(|c| c.field)
        .collect();
    assert_eq!(bad, ["threshold (unchanged)"]);
}

#[test]
fn only_the_planned_write_function_is_sent() {
    let mut s = stateful(true).session();
    let map = map_for(&mut s, &[0x2201, 0x2110, 0x2150]);
    let mut r = Reader::new(&mut s, map);
    let before = lab_write::snapshot(&mut r, SettingKind::ThumbwheelInvert).unwrap();
    let plan = lab_write::plan(&before, SettingValue::ThumbwheelInvert(true)).unwrap();
    lab_write::execute(&mut r, &WriteIntent::explicit_user_request(), &before, plan).unwrap();
    let seen = s.into_parts().0.seen;
    assert_eq!(seen, [(0x2150, 1), (0x2150, 2), (0x2150, 1)]);
}
