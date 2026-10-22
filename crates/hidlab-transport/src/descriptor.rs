//! A small, non-panicking walker over HID report descriptors.
//!
//! Item format (USB HID 1.11, section 6.2.2.2):
//! short item prefix byte = `tag(4) | type(2) | size(2)`, where size code 3
//! means 4 data bytes; a prefix of `0xFE` introduces a long item
//! `[0xFE, data_len, long_tag, data...]`.
//!
//! Only what hid-lab needs is collected: report IDs (global tag 8) and usage
//! pages (global tag 0).

use std::collections::BTreeSet;

use serde::Serialize;

/// HID++ short report ID (LIKELY, see docs/protocol/README.md).
pub const HIDPP_SHORT_REPORT_ID: u8 = 0x10;
/// HID++ long report ID (LIKELY, see docs/protocol/README.md).
pub const HIDPP_LONG_REPORT_ID: u8 = 0x11;

const ITEM_TYPE_GLOBAL: u8 = 1;
const GLOBAL_TAG_USAGE_PAGE: u8 = 0;
const GLOBAL_TAG_REPORT_ID: u8 = 8;
const LONG_ITEM_PREFIX: u8 = 0xFE;

/// What the walker learned from a report descriptor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DescriptorSummary {
    /// Every report ID declared.
    pub report_ids: BTreeSet<u8>,
    /// Every usage page declared.
    pub usage_pages: BTreeSet<u16>,
    /// Total number of items walked.
    pub items: usize,
    /// True if the descriptor ended in the middle of an item.
    pub truncated: bool,
}

impl DescriptorSummary {
    /// True if the descriptor declares a HID++ short or long report ID.
    pub fn looks_like_hidpp(&self) -> bool {
        self.has_short_report() || self.has_long_report()
    }

    /// True if report ID `0x10` is declared.
    pub fn has_short_report(&self) -> bool {
        self.report_ids.contains(&HIDPP_SHORT_REPORT_ID)
    }

    /// True if report ID `0x11` is declared.
    pub fn has_long_report(&self) -> bool {
        self.report_ids.contains(&HIDPP_LONG_REPORT_ID)
    }
}

/// Walks a report descriptor. Never panics; malformed input sets `truncated`.
pub fn walk_descriptor(bytes: &[u8]) -> DescriptorSummary {
    let mut summary = DescriptorSummary::default();
    let mut pos = 0usize;

    while pos < bytes.len() {
        let prefix = bytes[pos];

        if prefix == LONG_ITEM_PREFIX {
            // [0xFE, bDataSize, bLongItemTag, data...]
            let Some(&data_len) = bytes.get(pos + 1) else {
                summary.truncated = true;
                break;
            };
            let end = pos + 3 + usize::from(data_len);
            if end > bytes.len() {
                summary.truncated = true;
                break;
            }
            summary.items += 1;
            pos = end;
            continue;
        }

        let size = match prefix & 0x03 {
            3 => 4,
            n => usize::from(n),
        };
        let item_type = (prefix >> 2) & 0x03;
        let tag = prefix >> 4;
        let data_start = pos + 1;
        let data_end = data_start + size;
        let Some(data) = bytes.get(data_start..data_end) else {
            summary.truncated = true;
            break;
        };
        summary.items += 1;

        if item_type == ITEM_TYPE_GLOBAL {
            let value = le_value(data);
            match tag {
                GLOBAL_TAG_USAGE_PAGE => {
                    // Usage pages are 16-bit; a 4-byte form carries it in the low half.
                    summary.usage_pages.insert((value & 0xFFFF) as u16);
                }
                GLOBAL_TAG_REPORT_ID => {
                    if let Ok(id) = u8::try_from(value) {
                        summary.report_ids.insert(id);
                    }
                }
                _ => {}
            }
        }

        pos = data_end;
    }

    summary
}

fn le_value(data: &[u8]) -> u32 {
    data.iter()
        .rev()
        .fold(0u32, |acc, &b| (acc << 8) | u32::from(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    // SYNTHETIC descriptor: a mouse collection (report 0x02) plus vendor
    // collections with report IDs 0x10 and 0x11. Hand-written for tests;
    // not captured from real hardware.
    const SYNTHETIC_HIDPP: &[u8] = &[
        0x05, 0x01, // Usage Page (Generic Desktop)
        0x09, 0x02, // Usage (Mouse)
        0xA1, 0x01, // Collection (Application)
        0x85, 0x02, //   Report ID (2)
        0x09, 0x01, //   Usage (Pointer)
        0xC0, // End Collection
        0x06, 0x00, 0xFF, // Usage Page (Vendor 0xFF00)
        0x09, 0x01, // Usage (1)
        0xA1, 0x01, // Collection (Application)
        0x85, 0x10, //   Report ID (0x10)
        0x95, 0x06, //   Report Count (6)
        0x75, 0x08, //   Report Size (8)
        0xC0, // End Collection
        0x06, 0x00, 0xFF, // Usage Page (Vendor 0xFF00)
        0x09, 0x02, // Usage (2)
        0xA1, 0x01, // Collection (Application)
        0x85, 0x11, //   Report ID (0x11)
        0x95, 0x13, //   Report Count (19)
        0xC0, // End Collection
    ];

    #[test]
    fn finds_report_ids_and_usage_pages() {
        let s = walk_descriptor(SYNTHETIC_HIDPP);
        assert!(!s.truncated);
        assert_eq!(
            s.report_ids.iter().copied().collect::<Vec<_>>(),
            [0x02, 0x10, 0x11]
        );
        assert_eq!(
            s.usage_pages.iter().copied().collect::<Vec<_>>(),
            [0x0001, 0xFF00]
        );
        assert!(s.looks_like_hidpp());
        assert!(s.has_long_report());
        assert!(s.has_short_report());
    }

    #[test]
    fn plain_mouse_is_not_hidpp() {
        // SYNTHETIC
        let s = walk_descriptor(&[0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x85, 0x01, 0xC0]);
        assert!(!s.looks_like_hidpp());
    }

    #[test]
    fn four_byte_items_are_skipped_correctly() {
        // SYNTHETIC: Logical Maximum with 4 data bytes, then Report ID 0x11.
        let s = walk_descriptor(&[0x27, 0xFF, 0xFF, 0x00, 0x00, 0x85, 0x11]);
        assert!(!s.truncated);
        assert_eq!(s.items, 2);
        assert!(s.has_long_report());
    }

    #[test]
    fn long_items_are_skipped() {
        // SYNTHETIC: long item with 2 data bytes, then Report ID 0x10.
        let s = walk_descriptor(&[0xFE, 0x02, 0x42, 0xAA, 0xBB, 0x85, 0x10]);
        assert!(!s.truncated);
        assert_eq!(s.items, 2);
        assert!(s.has_short_report());
    }

    #[test]
    fn truncated_short_item_is_reported() {
        let s = walk_descriptor(&[0x85, 0x10, 0x06, 0x00]);
        assert!(s.truncated);
        assert!(s.has_short_report());
    }

    #[test]
    fn truncated_long_item_is_reported() {
        assert!(walk_descriptor(&[0xFE]).truncated);
        assert!(walk_descriptor(&[0xFE, 0x05, 0x01, 0x00]).truncated);
    }

    #[test]
    fn empty_descriptor_is_empty() {
        let s = walk_descriptor(&[]);
        assert_eq!(s, DescriptorSummary::default());
    }

    #[test]
    fn never_panics_on_arbitrary_bytes() {
        // Deterministic pseudo-random input (xorshift), many lengths.
        let mut state = 0x1234_5678u32;
        for len in 0..512 {
            let bytes: Vec<u8> = (0..len)
                .map(|_| {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    state as u8
                })
                .collect();
            let _ = walk_descriptor(&bytes);
        }
    }
}
