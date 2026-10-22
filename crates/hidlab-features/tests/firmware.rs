mod common;

use common::{FakeDevice, map_for};
use hidlab_features::{Reader, firmware};

#[test]
fn reads_info_and_every_entity() {
    // SYNTHETIC answers.
    let dev = FakeDevice::new(
        &[0x0003],
        Box::new(|_, func, p| match (func, p[0]) {
            (0, _) => Ok(vec![2, 9, 9, 9, 9, 0, 0x02, 0xB0, 0x99, 0, 0, 0, 0]),
            (1, 0) => Ok(vec![0x00, b'A', b'B', b'C', 0x01, 0x02, 0x00, 0x10, 0x00]),
            (1, 1) => Err(0x02),
            _ => Err(0x07),
        }),
    );
    let mut s = dev.session();
    let map = map_for(&mut s, &[0x0003]);
    let fw = firmware::read(&mut Reader::new(&mut s, map)).unwrap();
    assert_eq!(fw.info.model_ids[0].id, "B099");
    assert_eq!(fw.entities.len(), 1);
    assert_eq!(fw.entities[0].version.as_deref(), Some("01.02"));
    assert_eq!(fw.notes.len(), 1);
}
