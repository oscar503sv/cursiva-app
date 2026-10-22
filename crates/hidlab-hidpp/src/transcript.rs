//! Raw TX/RX evidence log.

use serde::Serialize;

/// Direction of a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Tx,
    Rx,
}

/// How an RX report was treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    /// A request sent by hid-lab.
    Sent,
    /// The response to the pending request.
    Matched,
    /// An error response to the pending request.
    MatchedError,
    /// Unrelated traffic (mouse input, notifications, other software).
    Ignored,
    /// Bytes that could not be decoded as HID++.
    Malformed,
}

/// One report, with timing relative to the session start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TranscriptEntry {
    /// Sequence number of the request this entry belongs to (1-based).
    pub request: u32,
    pub t_us: u64,
    pub dir: Direction,
    pub disposition: Disposition,
    pub hex: String,
}

/// Every report sent or received during a session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Transcript {
    pub entries: Vec<TranscriptEntry>,
}

impl Transcript {
    pub fn push(&mut self, entry: TranscriptEntry) {
        self.entries.push(entry);
    }

    pub fn count(&self, disposition: Disposition) -> usize {
        self.entries
            .iter()
            .filter(|e| e.disposition == disposition)
            .count()
    }
}
