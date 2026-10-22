//! Full read against a fake device that reports every target feature.
//! All answers are SYNTHETIC.

mod common;

use common::FakeDevice;
use hidlab_features::{Reading, allowed, read_all};
use hidlab_hidpp::{FeatureId, HidppError};

fn full_device() -> FakeDevice {
    FakeDevice::new(
        &[
            0x0005, 0x0003, 0x1004, 0x1814, 0x1B04, 0x2201, 0x2110, 0x2121, 0x2150,
        ],
        Box::new(|id, func, p| match (id, func) {
            (0x0005, 0) => Ok(vec![4]),
            (0x0005, 1) => Ok(b"Fake"[usize::from(p[0])..].to_vec()),
            (0x0005, 2) => Ok(vec![3]),
            (0x0003, 0) => Ok(vec![1, 1, 2, 3, 4, 0, 0x02, 0xB0, 0x99, 0, 0, 0, 0]),
            (0x0003, 1) => Ok(vec![0, b'F', b'A', b'K', 1, 0, 0, 1, 0]),
            (0x1004, 0) => Ok(vec![0x0F, 0x03]),
            (0x1004, 1) => Ok(vec![50, 0x04, 0, 0]),
            (0x1814, 0) => Ok(vec![3, 0]),
            (0x1B04, 0) => Ok(vec![1]),
            (0x1B04, 1) => Ok(vec![0x00, 0x50, 0x00, 0x38, 0x01, 0, 0, 0, 0]),
            (0x1B04, 2) => Ok(vec![0x00, 0x50, 0, 0, 0, 0]),
            (0x2201, 1) => Ok(vec![0, 0x00, 0xC8, 0xE0, 0x32, 0x0F, 0xA0, 0, 0]),
            (0x2201, 2) => Ok(vec![0, 0x03, 0xE8, 0x03, 0xE8]),
            (0x2110, 0) => Ok(vec![2, 10, 0]),
            (0x2121, 0) => Ok(vec![8, 0x0C]),
            (0x2121, 1) => Ok(vec![0x02]),
            (0x2121, 3) => Ok(vec![0x01]),
            (0x2150, 1) => Ok(vec![0, 0]),
            _ => Err(0x07),
        }),
    )
}

#[test]
fn every_request_sent_is_in_the_allowlist() {
    let mut s = full_device().session();
    let state = read_all(&mut s).unwrap();
    assert_eq!(state.resolved.len(), 9);
    assert!(matches!(state.device_name, Reading::Ok { .. }));
    assert!(matches!(state.thumbwheel, Reading::Ok { .. }));
    assert_eq!(state.dpi.value().unwrap().current.effective_dpi, 1000);

    let (dev, transcript) = s.into_parts();
    assert!(!dev.seen.is_empty());
    for &(id, func) in &dev.seen {
        assert!(
            allowed(FeatureId(id), func).is_some(),
            "sent non-allowlisted 0x{id:04X} f{func}"
        );
    }
    // Root getFeature calls are answered internally; they are allowlisted too.
    assert!(allowed(FeatureId(0), 0).is_some());
    assert!(!transcript.entries.is_empty());
}

#[test]
fn missing_features_are_not_supported_and_get_no_requests() {
    let dev = FakeDevice::new(
        &[0x1004],
        Box::new(|_, f, _| match f {
            0 => Ok(vec![0, 2]),
            _ => Ok(vec![10, 2, 0, 0]),
        }),
    );
    let mut s = dev.session();
    let state = read_all(&mut s).unwrap();
    assert!(matches!(state.battery, Reading::Ok { .. }));
    assert!(matches!(state.dpi, Reading::NotSupported));
    assert!(matches!(state.reprog_controls, Reading::NotSupported));
    assert!(s.into_parts().0.seen.iter().all(|&(id, _)| id == 0x1004));
}

#[test]
fn device_error_marks_one_feature_failed() {
    let dev = FakeDevice::new(
        &[0x2110, 0x2150],
        Box::new(|id, _, _| match id {
            0x2110 => Err(0x05),
            _ => Ok(vec![0, 1]),
        }),
    );
    let mut s = dev.session();
    let state = read_all(&mut s).unwrap();
    assert!(matches!(state.smartshift, Reading::Failed { .. }));
    assert!(state.thumbwheel.value().unwrap().inverted);
}

#[test]
fn silent_device_aborts_with_timeout() {
    let mut dev = full_device();
    dev.silent = vec![0x1004];
    let mut s = dev.session();
    let err = read_all(&mut s).unwrap_err();
    assert!(err.is_fatal());
    assert!(matches!(
        err,
        hidlab_features::ReadError::Hidpp(HidppError::Timeout { .. })
    ));
}
