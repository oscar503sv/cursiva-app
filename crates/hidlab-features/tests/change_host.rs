mod common;

use common::{FakeDevice, map_for};
use hidlab_features::{Reader, change_host};

#[test]
fn reads_host_info_only() {
    // SYNTHETIC
    let dev = FakeDevice::new(&[0x1814], Box::new(|_, _, _| Ok(vec![3, 0])));
    let mut s = dev.session();
    let map = map_for(&mut s, &[0x1814]);
    let h = change_host::read(&mut Reader::new(&mut s, map)).unwrap();
    assert_eq!(h.current_channel, 1);
    assert_eq!(s.into_parts().0.seen, [(0x1814, 0)]);
}
