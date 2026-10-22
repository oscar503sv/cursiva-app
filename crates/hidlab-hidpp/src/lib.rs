//! Generic HID++ 2.0 protocol support. Knows nothing about specific devices.

pub mod error;
pub mod error_code;
pub mod feature;
pub mod frame;
pub mod session;
pub mod transcript;

pub use error::HidppError;
pub use error_code::{Hidpp10Error, Hidpp20Error};
pub use feature::{
    ENUMERATION_PING, Enumeration, FeatureFailure, FeatureId, FeatureInfo, ProtocolVersion,
    ROOT_INDEX, enumerate, feature_at, feature_count, get_feature, get_protocol_version,
};
pub use frame::{
    DEVICE_INDEX_DIRECT, Frame, FrameError, HIDPP10_ERROR_SUB_ID, HIDPP20_ERROR_INDEX, ReportKind,
    Request, decode, hex,
};
pub use session::{DEFAULT_SW_ID, DEFAULT_TIMEOUT, Session, SessionConfig};
pub use transcript::{Direction, Disposition, Transcript, TranscriptEntry};
