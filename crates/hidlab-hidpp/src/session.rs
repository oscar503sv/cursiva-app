//! Request/response over a [`RawChannel`] that is shared with other traffic.
//!
//! The kernel driver, normal mouse input and device notifications all arrive
//! on the same hidraw node. After sending a request, the session reads until
//! its deadline and only accepts a report whose device index, feature index,
//! function and software ID all match. Everything else is counted and logged
//! as ignored.

use std::time::{Duration, Instant};

use hidlab_transport::RawChannel;

use crate::error::HidppError;
use crate::frame::{DEVICE_INDEX_DIRECT, Frame, ReportKind, Request, decode, hex};
use crate::transcript::{Direction, Disposition, Transcript, TranscriptEntry};

/// Default software ID for hid-lab requests. Non-zero; 0 marks notifications.
pub const DEFAULT_SW_ID: u8 = 0x0B;
/// Default per-request timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(2000);

/// Session parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionConfig {
    pub device_index: u8,
    pub report: ReportKind,
    pub sw_id: u8,
    pub timeout: Duration,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            device_index: DEVICE_INDEX_DIRECT,
            report: ReportKind::Long,
            sw_id: DEFAULT_SW_ID,
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

/// A HID++ conversation with one device.
pub struct Session<C: RawChannel> {
    channel: C,
    config: SessionConfig,
    transcript: Transcript,
    ignored: u64,
    requests: u32,
    started: Instant,
}

impl<C: RawChannel> Session<C> {
    pub fn new(channel: C, config: SessionConfig) -> Self {
        Self {
            channel,
            config,
            transcript: Transcript::default(),
            ignored: 0,
            requests: 0,
            started: Instant::now(),
        }
    }

    pub fn config(&self) -> &SessionConfig {
        &self.config
    }

    pub fn transcript(&self) -> &Transcript {
        &self.transcript
    }

    /// Total number of unrelated reports ignored so far.
    pub fn ignored(&self) -> u64 {
        self.ignored
    }

    pub fn into_parts(self) -> (C, Transcript) {
        (self.channel, self.transcript)
    }

    /// Sends one request and returns the parameters of the matching response.
    pub fn request(
        &mut self,
        feature_index: u8,
        function: u8,
        params: &[u8],
    ) -> Result<Vec<u8>, HidppError> {
        let req = Request {
            device_index: self.config.device_index,
            feature_index,
            function,
            sw_id: self.config.sw_id,
            params: params.to_vec(),
        };
        let bytes = req.encode(self.config.report)?;
        self.requests += 1;
        self.log(Direction::Tx, Disposition::Sent, &bytes);
        self.channel.send(&bytes)?;

        let start = Instant::now();
        let deadline = start + self.config.timeout;
        let ignored_before = self.ignored;
        let mut buf = [0u8; 64];

        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(self.timeout(feature_index, function, start, ignored_before));
            }
            let Some(n) = self.channel.recv(&mut buf, remaining)? else {
                return Err(self.timeout(feature_index, function, start, ignored_before));
            };
            let report = &buf[..n.min(buf.len())];

            let frame = match decode(report) {
                Ok(f) => f,
                Err(_) => {
                    self.ignored += 1;
                    self.log(Direction::Rx, Disposition::Malformed, report);
                    continue;
                }
            };

            match self.classify(&frame, feature_index, function) {
                Match::Response(params) => {
                    self.log(Direction::Rx, Disposition::Matched, report);
                    return Ok(params);
                }
                Match::Error(err) => {
                    self.log(Direction::Rx, Disposition::MatchedError, report);
                    return Err(err);
                }
                Match::Unrelated => {
                    self.ignored += 1;
                    self.log(Direction::Rx, Disposition::Ignored, report);
                }
            }
        }
    }

    fn classify(&self, frame: &Frame, feature_index: u8, function: u8) -> Match {
        let cfg = &self.config;
        match *frame {
            Frame::Message {
                device_index,
                feature_index: fi,
                function: func,
                sw_id,
                ref params,
                ..
            } if device_index == cfg.device_index
                && fi == feature_index
                && func == function
                && sw_id == cfg.sw_id =>
            {
                Match::Response(params.clone())
            }
            Frame::Error20 {
                device_index,
                orig_feature_index,
                orig_function,
                sw_id,
                code,
            } if device_index == cfg.device_index
                && orig_feature_index == feature_index
                && orig_function == function
                && sw_id == cfg.sw_id =>
            {
                Match::Error(HidppError::Device {
                    feature_index,
                    function,
                    code,
                })
            }
            Frame::Error10 {
                device_index,
                orig_sub_id,
                orig_address,
                code,
            } if device_index == cfg.device_index
                && orig_sub_id == feature_index
                && orig_address == ((function << 4) | cfg.sw_id) =>
            {
                Match::Error(HidppError::Device10 {
                    feature_index,
                    function,
                    code,
                })
            }
            _ => Match::Unrelated,
        }
    }

    fn timeout(&self, feature_index: u8, function: u8, start: Instant, before: u64) -> HidppError {
        HidppError::Timeout {
            feature_index,
            function,
            waited: start.elapsed(),
            ignored: self.ignored - before,
        }
    }

    fn log(&mut self, dir: Direction, disposition: Disposition, bytes: &[u8]) {
        let t_us = u64::try_from(self.started.elapsed().as_micros()).unwrap_or(u64::MAX);
        self.transcript.push(TranscriptEntry {
            request: self.requests,
            t_us,
            dir,
            disposition,
            hex: hex(bytes),
        });
    }
}

enum Match {
    Response(Vec<u8>),
    Error(HidppError),
    Unrelated,
}

#[cfg(test)]
mod tests {
    use super::*;
    use hidlab_transport::TransportError;
    use std::collections::VecDeque;

    /// Replays a fixed list of incoming reports regardless of what is sent.
    struct Scripted {
        incoming: VecDeque<Vec<u8>>,
        sent: Vec<Vec<u8>>,
    }

    impl RawChannel for Scripted {
        fn send(&mut self, report: &[u8]) -> Result<(), TransportError> {
            self.sent.push(report.to_vec());
            Ok(())
        }
        fn recv(&mut self, buf: &mut [u8], _: Duration) -> Result<Option<usize>, TransportError> {
            Ok(self.incoming.pop_front().map(|r| {
                buf[..r.len()].copy_from_slice(&r);
                r.len()
            }))
        }
    }

    fn session(incoming: Vec<Vec<u8>>) -> Session<Scripted> {
        Session::new(
            Scripted {
                incoming: incoming.into(),
                sent: vec![],
            },
            SessionConfig {
                report: ReportKind::Short,
                timeout: Duration::from_millis(50),
                ..SessionConfig::default()
            },
        )
    }

    #[test]
    fn skips_noise_and_returns_match() {
        let mut s = session(vec![
            vec![0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00], // mouse report
            vec![0x10, 0xFF, 0x00, 0x10, 0x00, 0x00, 0x00], // notification, sw_id 0
            vec![0x10, 0xFF, 0x00, 0x11, 0x00, 0x00, 0x5A], // other sw_id
            vec![0x10, 0xFF, 0x01, 0x1B, 0x00, 0x00, 0x5A], // other feature index
            vec![0x10, 0x01, 0x00, 0x1B, 0x00, 0x00, 0x5A], // other device index
            vec![0x11, 0xFF],                               // malformed
            vec![0x10, 0xFF, 0x00, 0x1B, 0x04, 0x05, 0x5A], // match
        ]);
        let params = s.request(0x00, 1, &[0, 0, 0x5A]).unwrap();
        assert_eq!(params, [0x04, 0x05, 0x5A]);
        assert_eq!(s.ignored(), 6);
        let t = s.transcript();
        assert_eq!(t.entries.len(), 8);
        assert_eq!(t.entries[0].dir, Direction::Tx);
        assert_eq!(t.entries[0].hex, "10 ff 00 1b 00 00 5a");
        assert_eq!(t.count(Disposition::Matched), 1);
        assert_eq!(t.count(Disposition::Malformed), 1);
    }

    #[test]
    fn matching_error_is_returned_and_others_ignored() {
        let mut s = session(vec![
            vec![0x10, 0xFF, 0xFF, 0x00, 0x11, 0x02, 0x00], // error for other sw_id
            vec![0x10, 0xFF, 0xFF, 0x00, 0x1B, 0x02, 0x00], // our error
        ]);
        let err = s.request(0x00, 1, &[]).unwrap_err();
        assert!(matches!(
            err,
            HidppError::Device {
                code: crate::Hidpp20Error::InvalidArgument,
                ..
            }
        ));
        assert_eq!(s.ignored(), 1);
    }

    #[test]
    fn hidpp10_error_is_matched() {
        let mut s = session(vec![vec![0x10, 0xFF, 0x8F, 0x00, 0x1B, 0x01, 0x00]]);
        assert!(matches!(
            s.request(0x00, 1, &[]),
            Err(HidppError::Device10 { .. })
        ));
    }

    #[test]
    fn timeout_counts_ignored() {
        let mut s = session(vec![vec![0x02, 0, 0, 0]]);
        match s.request(0x00, 0, &[0x00, 0x01]) {
            Err(HidppError::Timeout { ignored, .. }) => assert_eq!(ignored, 1),
            other => panic!("expected timeout, got {other:?}"),
        }
    }
}
