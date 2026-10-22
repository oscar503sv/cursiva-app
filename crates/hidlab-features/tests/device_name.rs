mod common;

use common::{FakeDevice, map_for};
use hidlab_features::{Reader, device_name};

#[test]
fn reads_name_in_chunks_and_type() {
    // SYNTHETIC 20-character name, so two 16-byte chunks are needed.
    let name = b"Synthetic Test Mouse";
    let dev = FakeDevice::new(
        &[0x0005],
        Box::new(move |_, func, p| match func {
            0 => Ok(vec![name.len() as u8]),
            1 => Ok(name[usize::from(p[0])..].to_vec()),
            2 => Ok(vec![3]),
            _ => Err(0x07),
        }),
    );
    let mut s = dev.session();
    let map = map_for(&mut s, &[0x0005]);
    let mut r = Reader::new(&mut s, map);
    let n = device_name::read(&mut r).unwrap();
    assert_eq!(n.name, "Synthetic Test Mouse");
    assert_eq!(n.length, 20);
    assert_eq!(n.device_type.unwrap().reference_label, Some("mouse"));
    assert!(n.notes.is_empty());
    let (dev, _) = s.into_parts();
    assert_eq!(
        dev.seen,
        [(0x0005, 0), (0x0005, 1), (0x0005, 1), (0x0005, 2)]
    );
}

#[test]
fn type_error_is_a_note_not_a_failure() {
    let dev = FakeDevice::new(
        &[0x0005],
        Box::new(|_, func, _| match func {
            0 => Ok(vec![2]),
            1 => Ok(b"ab".to_vec()),
            _ => Err(0x02),
        }),
    );
    let mut s = dev.session();
    let map = map_for(&mut s, &[0x0005]);
    let n = device_name::read(&mut Reader::new(&mut s, map)).unwrap();
    assert_eq!(n.name, "ab");
    assert_eq!(n.device_type, None);
    assert_eq!(n.notes.len(), 1);
}
