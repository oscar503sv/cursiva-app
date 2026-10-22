mod common;

use common::{FakeDevice, map_for};
use hidlab_features::{Reader, dpi};

#[test]
fn reads_list_and_current() {
    // SYNTHETIC
    let dev = FakeDevice::new(
        &[0x2201],
        Box::new(|_, func, _| match func {
            1 => Ok(vec![0x00, 0x00, 0xC8, 0xE0, 0x32, 0x0F, 0xA0, 0x00, 0x00]),
            2 => Ok(vec![0x00, 0x06, 0x40, 0x03, 0xE8]),
            _ => Err(0x07),
        }),
    );
    let mut s = dev.session();
    let map = map_for(&mut s, &[0x2201]);
    let d = dpi::read(&mut Reader::new(&mut s, map)).unwrap();
    assert_eq!(d.current.effective_dpi, 1600);
    assert!(d.current_in_list);
    let (dev, _) = s.into_parts();
    assert_eq!(dev.seen, [(0x2201, 1), (0x2201, 2)]);
}
