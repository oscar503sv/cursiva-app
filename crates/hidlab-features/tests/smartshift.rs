mod common;

use common::{FakeDevice, map_for};
use hidlab_features::{Reader, smartshift};

#[test]
fn reads_mode() {
    // SYNTHETIC
    let dev = FakeDevice::new(&[0x2110], Box::new(|_, _, _| Ok(vec![1, 20, 0])));
    let mut s = dev.session();
    let map = map_for(&mut s, &[0x2110]);
    let ss = smartshift::read(&mut Reader::new(&mut s, map)).unwrap();
    assert_eq!(ss.mode, Some("free_spin"));
    assert_eq!(s.into_parts().0.seen, [(0x2110, 0)]);
}
