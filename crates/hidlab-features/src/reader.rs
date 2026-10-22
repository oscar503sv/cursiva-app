//! Gatekeeper between typed feature code and the HID++ session.

use std::collections::BTreeMap;

use hidlab_hidpp::{FeatureId, FeatureInfo, HidppError, Session, get_feature};
use hidlab_transport::RawChannel;
use thiserror::Error;

use crate::allowlist::{allowed, confirmed_write, write_allowed};

/// Errors from a typed read.
#[derive(Debug, Error)]
pub enum ReadError {
    #[error(
        "refused to call feature {feature} function {function}: not in the read-only allowlist"
    )]
    NotAllowed { feature: FeatureId, function: u8 },
    #[error("refused to write feature {feature} function {function}: not in the {list}")]
    WriteNotAllowed {
        feature: FeatureId,
        function: u8,
        list: &'static str,
    },
    #[error("the device does not report feature {0}")]
    NotSupported(FeatureId),
    #[error("cannot decode {what}: {reason}")]
    Decode { what: &'static str, reason: String },
    #[error(transparent)]
    Hidpp(#[from] HidppError),
}

impl ReadError {
    /// True if the session cannot continue (timeout, disconnect, I/O).
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            ReadError::Hidpp(HidppError::Timeout { .. } | HidppError::Transport(_))
        )
    }
}

/// Where the device placed each feature, resolved at runtime with getFeature.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeatureMap {
    found: BTreeMap<FeatureId, FeatureInfo>,
}

impl FeatureMap {
    /// Asks the device (root getFeature) where each of `ids` lives.
    pub fn resolve<C: RawChannel>(
        session: &mut Session<C>,
        ids: &[FeatureId],
    ) -> Result<Self, HidppError> {
        let mut found = BTreeMap::new();
        for &id in ids {
            if let Some(info) = get_feature(session, id)? {
                found.insert(id, info);
            }
        }
        Ok(Self { found })
    }

    /// Builds a map from known entries (tests, or a prior enumeration).
    pub fn from_infos(infos: impl IntoIterator<Item = FeatureInfo>) -> Self {
        Self {
            found: infos.into_iter().map(|i| (i.id, i)).collect(),
        }
    }

    pub fn get(&self, id: FeatureId) -> Option<&FeatureInfo> {
        self.found.get(&id)
    }
}

/// Proof that the user explicitly asked for a write (`--write` on the CLI).
/// Only `hid-lab lab write` creates one.
#[derive(Debug)]
pub struct WriteIntent {
    _private: (),
}

impl WriteIntent {
    /// Call only after the user passed an explicit write flag.
    pub fn explicit_user_request() -> Self {
        Self { _private: () }
    }
}

/// Which write list a caller may use.
#[derive(Debug, Clone, Copy)]
pub enum WriteAuth<'a> {
    /// `hid-lab lab write --write`: the write experiment allowlist.
    Experiment(&'a WriteIntent),
    /// The daemon: confirmed writes only.
    Confirmed,
}

/// Sends only allowlisted reads, addressed through a [`FeatureMap`].
pub struct Reader<'s, C: RawChannel> {
    session: &'s mut Session<C>,
    map: FeatureMap,
}

impl<'s, C: RawChannel> Reader<'s, C> {
    pub fn new(session: &'s mut Session<C>, map: FeatureMap) -> Self {
        Self { session, map }
    }

    pub fn map(&self) -> &FeatureMap {
        &self.map
    }

    pub fn session(&self) -> &Session<C> {
        self.session
    }

    /// Calls `function` of `feature`. Refuses anything not in the allowlist
    /// before a single byte is sent.
    pub fn call(
        &mut self,
        feature: FeatureId,
        function: u8,
        params: &[u8],
    ) -> Result<Vec<u8>, ReadError> {
        if allowed(feature, function).is_none() {
            return Err(ReadError::NotAllowed { feature, function });
        }
        let index = self
            .map
            .get(feature)
            .ok_or(ReadError::NotSupported(feature))?
            .index;
        Ok(self.session.request(index, function, params)?)
    }

    /// Sends one write from the list `auth` grants; refuses anything else
    /// before a byte is sent.
    pub fn write(
        &mut self,
        auth: WriteAuth<'_>,
        feature: FeatureId,
        function: u8,
        params: &[u8],
    ) -> Result<Vec<u8>, ReadError> {
        let (ok, list) = match auth {
            WriteAuth::Experiment(_) => (
                write_allowed(feature, function).is_some(),
                "write experiment allowlist",
            ),
            WriteAuth::Confirmed => (
                confirmed_write(feature, function).is_some(),
                "confirmed write list",
            ),
        };
        if !ok {
            return Err(ReadError::WriteNotAllowed {
                feature,
                function,
                list,
            });
        }
        let index = self
            .map
            .get(feature)
            .ok_or(ReadError::NotSupported(feature))?
            .index;
        Ok(self.session.request(index, function, params)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hidlab_hidpp::SessionConfig;
    use hidlab_transport::TransportError;
    use std::time::Duration;

    #[derive(Default)]
    struct Recorder {
        sent: Vec<Vec<u8>>,
    }

    impl RawChannel for Recorder {
        fn send(&mut self, report: &[u8]) -> Result<(), TransportError> {
            self.sent.push(report.to_vec());
            Ok(())
        }
        fn recv(&mut self, _: &mut [u8], _: Duration) -> Result<Option<usize>, TransportError> {
            Ok(None)
        }
    }

    fn info(id: u16, index: u8) -> FeatureInfo {
        FeatureInfo {
            index,
            id: FeatureId(id),
            flags: 0,
            version: 0,
        }
    }

    #[test]
    fn refuses_known_writes_without_sending() {
        let mut session = Session::new(Recorder::default(), SessionConfig::default());
        let map = FeatureMap::from_infos(
            crate::KNOWN_WRITES
                .iter()
                .enumerate()
                .map(|(i, &(id, _))| info(id, i as u8 + 2)),
        );
        let mut reader = Reader::new(&mut session, map);
        for &(id, function) in crate::KNOWN_WRITES {
            let err = reader.call(FeatureId(id), function, &[]).unwrap_err();
            assert!(matches!(err, ReadError::NotAllowed { .. }), "{err}");
        }
        let (channel, transcript) = session.into_parts();
        assert!(channel.sent.is_empty());
        assert!(transcript.entries.is_empty());
    }

    #[test]
    fn write_refuses_reads_and_forbidden_writes_without_sending() {
        let mut session = Session::new(Recorder::default(), SessionConfig::default());
        let map = FeatureMap::from_infos([info(0x2121, 15), info(0x1814, 10), info(0x1004, 8)]);
        let mut reader = Reader::new(&mut session, map);
        let intent = WriteIntent::explicit_user_request();
        for auth in [WriteAuth::Experiment(&intent), WriteAuth::Confirmed] {
            for (id, f) in [(0x2121, 2), (0x1814, 1), (0x1004, 1)] {
                let err = reader.write(auth, FeatureId(id), f, &[]).unwrap_err();
                assert!(matches!(err, ReadError::WriteNotAllowed { .. }), "{err}");
            }
        }
        assert!(session.into_parts().0.sent.is_empty());
    }

    #[test]
    fn unsupported_feature_sends_nothing() {
        let mut session = Session::new(Recorder::default(), SessionConfig::default());
        let mut reader = Reader::new(&mut session, FeatureMap::default());
        let err = reader.call(FeatureId(0x1004), 1, &[]).unwrap_err();
        assert!(matches!(err, ReadError::NotSupported(_)));
        assert!(session.into_parts().0.sent.is_empty());
    }

    #[test]
    fn allowed_read_uses_resolved_index() {
        let mut session = Session::new(
            Recorder::default(),
            SessionConfig {
                timeout: Duration::from_millis(1),
                ..SessionConfig::default()
            },
        );
        let mut reader = Reader::new(&mut session, FeatureMap::from_infos([info(0x1004, 0x08)]));
        // No answer: the request must still have gone to index 0x08, function 1.
        let _ = reader.call(FeatureId(0x1004), 1, &[]);
        let sent = &session.into_parts().0.sent;
        assert_eq!(sent.len(), 1);
        assert_eq!(&sent[0][..4], &[0x11, 0xFF, 0x08, 0x1B]);
    }
}
