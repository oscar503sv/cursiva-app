mod common;

use common::{FakeDevice, map_for};
use hidlab_features::{Reader, battery};

#[test]
fn reads_capabilities_and_status() {
    // SYNTHETIC
    let dev = FakeDevice::new(
        &[0x1004],
        Box::new(|_, func, _| match func {
            0 => Ok(vec![0x0F, 0x03]),
            1 => Ok(vec![80, 0x08, 0x01, 0x01]),
            _ => Err(0x07),
        }),
    );
    let mut s = dev.session();
    let map = map_for(&mut s, &[0x1004]);
    let b = battery::read(&mut Reader::new(&mut s, map)).unwrap();
    assert_eq!(b.status.state_of_charge, 80);
    assert_eq!(b.status.charging.kernel, Some("charging"));
    assert!(b.capabilities.is_some());
}
