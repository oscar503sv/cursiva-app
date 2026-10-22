mod common;

use common::{FakeDevice, map_for};
use hidlab_features::{Reader, reprog_controls};

#[test]
fn reads_every_control_and_its_reporting() {
    // SYNTHETIC: two controls, CIDs 0x0050 and 0x00C3.
    let dev = FakeDevice::new(
        &[0x1B04],
        Box::new(|_, func, p| match func {
            0 => Ok(vec![2]),
            1 => match p[0] {
                0 => Ok(vec![0x00, 0x50, 0x00, 0x38, 0x01, 0, 0, 0, 0]),
                _ => Ok(vec![0x00, 0xC3, 0x00, 0xA9, 0x31, 0, 0, 0, 0x01]),
            },
            2 => Ok(vec![p[0], p[1], 0x00, 0x00, 0x00, 0x00]),
            _ => Err(0x07),
        }),
    );
    let mut s = dev.session();
    let map = map_for(&mut s, &[0x1B04]);
    let rc = reprog_controls::read(&mut Reader::new(&mut s, map)).unwrap();
    assert_eq!(rc.count, 2);
    assert_eq!(rc.controls[1].cid, "0x00C3");
    assert!(rc.controls.iter().all(|c| c.reporting.is_some()));
    assert!(rc.notes.is_empty());
    let seen = s.into_parts().0.seen;
    assert!(seen.iter().all(|&(_, f)| f <= 2), "{seen:?}");
}
