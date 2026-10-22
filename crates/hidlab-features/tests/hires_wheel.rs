mod common;

use common::{FakeDevice, map_for};
use hidlab_features::{Reader, hires_wheel};

#[test]
fn reads_capability_mode_and_ratchet() {
    // SYNTHETIC
    let dev = FakeDevice::new(
        &[0x2121],
        Box::new(|_, func, _| match func {
            0 => Ok(vec![8, 0x0C]),
            1 => Ok(vec![0x02]),
            3 => Ok(vec![0x01]),
            _ => Err(0x07),
        }),
    );
    let mut s = dev.session();
    let map = map_for(&mut s, &[0x2121]);
    let w = hires_wheel::read(&mut Reader::new(&mut s, map)).unwrap();
    assert!(w.mode.high_resolution);
    assert!(w.ratchet_switch.unwrap().ratchet);
    assert_eq!(
        s.into_parts().0.seen,
        [(0x2121, 0), (0x2121, 1), (0x2121, 3)]
    );
}
