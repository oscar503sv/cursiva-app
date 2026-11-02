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

#[test]
fn remap_reads_the_control_list_once() {
    // SYNTHETIC: back (0x53) and forward (0x56), both group 2, mask 3.
    let dev = FakeDevice::new(
        &[0x1B04],
        Box::new(|_, func, p| match func {
            0 => Ok(vec![2]),
            1 => {
                let cid = if p[0] == 0 { 0x53 } else { 0x56 };
                Ok(vec![0x00, cid, 0x00, 0x00, 0x31, 0, 2, 3, 0])
            }
            2 => Ok(vec![p[0], p[1], 0x00, 0x00, 0x00, 0x00]),
            _ => Err(0x07),
        }),
    );
    let mut s = dev.session();
    let map = map_for(&mut s, &[0x1B04]);
    let mut r = Reader::new(&mut s, map.clone());
    let back = reprog_controls::read_remap(&mut r, 0x53).unwrap();
    assert_eq!(back.targets, [0x53, 0x56]);
    let memo = r.into_memo();
    assert_eq!(memo.len(), 3);
    // A later reader on the same connection sends only getCidReporting.
    let mut r = Reader::with_memo(&mut s, map, memo);
    let fwd = reprog_controls::read_remap(&mut r, 0x56).unwrap();
    assert_eq!(fwd.targets, [0x56, 0x53]);
    let seen = s.into_parts().0.seen;
    let reads = |f: u8| seen.iter().filter(|&&s| s == (0x1B04, f)).count();
    assert_eq!((reads(0), reads(1), reads(2)), (1, 2, 2));
}
