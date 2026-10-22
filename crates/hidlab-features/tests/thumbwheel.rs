mod common;

use common::{FakeDevice, map_for};
use hidlab_features::{Reader, thumbwheel};

#[test]
fn reads_status_with_function_1_only() {
    // SYNTHETIC
    let dev = FakeDevice::new(&[0x2150], Box::new(|_, _, _| Ok(vec![0x00, 0x00])));
    let mut s = dev.session();
    let map = map_for(&mut s, &[0x2150]);
    let t = thumbwheel::read(&mut Reader::new(&mut s, map)).unwrap();
    assert!(!t.diverted && !t.inverted);
    assert_eq!(s.into_parts().0.seen, [(0x2150, 1)]);
}
