//! A small, non-panicking walker over HID report descriptors.
//!
//! Item format (USB HID 1.11, section 6.2.2.2):
//! short item prefix byte = `tag(4) | type(2) | size(2)`, where size code 3
//! means 4 data bytes; a prefix of `0xFE` introduces a long item
//! `[0xFE, data_len, long_tag, data...]`.
//!
//! Only what hid-lab needs is collected: report IDs (global tag 8), usage
//! pages (global tag 0), and where Input reports carry buttons (usage page
//! 0x09), so the daemon can show button presses without the kernel's input
//! devices.

use std::collections::BTreeSet;

use serde::Serialize;

/// HID++ short report ID (LIKELY, see docs/protocol/README.md).
pub const HIDPP_SHORT_REPORT_ID: u8 = 0x10;
/// HID++ long report ID (LIKELY, see docs/protocol/README.md).
pub const HIDPP_LONG_REPORT_ID: u8 = 0x11;

const ITEM_TYPE_MAIN: u8 = 0;
const ITEM_TYPE_GLOBAL: u8 = 1;
const ITEM_TYPE_LOCAL: u8 = 2;
const MAIN_TAG_INPUT: u8 = 8;
const GLOBAL_TAG_USAGE_PAGE: u8 = 0;
const GLOBAL_TAG_REPORT_SIZE: u8 = 7;
const GLOBAL_TAG_REPORT_ID: u8 = 8;
const GLOBAL_TAG_REPORT_COUNT: u8 = 9;
const GLOBAL_TAG_PUSH: u8 = 10;
const GLOBAL_TAG_POP: u8 = 11;
const LOCAL_TAG_USAGE_MINIMUM: u8 = 1;
const LONG_ITEM_PREFIX: u8 = 0xFE;
/// HID usage page Button.
pub const USAGE_PAGE_BUTTON: u16 = 0x09;
/// Input item flag: variable (one bit per button), not array.
const INPUT_FLAG_VARIABLE: u32 = 0x02;

/// Where one Input item carries buttons, one bit each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ButtonField {
    /// The report ID, or `None` when the descriptor declares none.
    pub report_id: Option<u8>,
    /// Bit offset after the report ID byte.
    pub bit_offset: u32,
    pub count: u32,
    /// Button number of the first bit (usually 1).
    pub first_button: u16,
}

impl ButtonField {
    /// Buttons held down in `report` as a bit set (bit `n` = button
    /// `first_button + n`), or `None` if the report is another one.
    pub fn pressed(&self, report: &[u8]) -> Option<u32> {
        let data = match self.report_id {
            Some(id) => report.strip_prefix(&[id])?,
            None => report,
        };
        let mut set = 0u32;
        for i in 0..self.count.min(32) {
            let bit = self.bit_offset + i;
            let byte = *data.get(usize::try_from(bit / 8).ok()?)?;
            if byte & (1 << (bit % 8)) != 0 {
                set |= 1 << i;
            }
        }
        Some(set)
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Globals {
    usage_page: u16,
    report_size: u32,
    report_count: u32,
    report_id: Option<u8>,
}

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
    /// Input items with buttons.
    pub button_fields: Vec<ButtonField>,
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
    let mut globals = Globals::default();
    let mut stack: Vec<Globals> = Vec::new();
    let mut usage_min: Option<u32> = None;
    // Input bits seen so far per report ID.
    let mut offsets: std::collections::BTreeMap<Option<u8>, u32> = Default::default();

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

        let value = le_value(data);
        match (item_type, tag) {
            (ITEM_TYPE_GLOBAL, GLOBAL_TAG_USAGE_PAGE) => {
                // Usage pages are 16-bit; a 4-byte form carries it in the low half.
                globals.usage_page = (value & 0xFFFF) as u16;
                summary.usage_pages.insert(globals.usage_page);
            }
            (ITEM_TYPE_GLOBAL, GLOBAL_TAG_REPORT_ID) => {
                if let Ok(id) = u8::try_from(value) {
                    summary.report_ids.insert(id);
                    globals.report_id = Some(id);
                }
            }
            (ITEM_TYPE_GLOBAL, GLOBAL_TAG_REPORT_SIZE) => globals.report_size = value,
            (ITEM_TYPE_GLOBAL, GLOBAL_TAG_REPORT_COUNT) => globals.report_count = value,
            (ITEM_TYPE_GLOBAL, GLOBAL_TAG_PUSH) => stack.push(globals),
            (ITEM_TYPE_GLOBAL, GLOBAL_TAG_POP) => {
                if let Some(g) = stack.pop() {
                    globals = g;
                }
            }
            (ITEM_TYPE_LOCAL, LOCAL_TAG_USAGE_MINIMUM) => usage_min = Some(value),
            (ITEM_TYPE_MAIN, MAIN_TAG_INPUT) => {
                let bits = globals.report_size.saturating_mul(globals.report_count);
                let offset = offsets.entry(globals.report_id).or_default();
                if globals.usage_page == USAGE_PAGE_BUTTON
                    && globals.report_size == 1
                    && value & INPUT_FLAG_VARIABLE != 0
                {
                    summary.button_fields.push(ButtonField {
                        report_id: globals.report_id,
                        bit_offset: *offset,
                        count: globals.report_count,
                        first_button: usage_min.and_then(|u| u16::try_from(u).ok()).unwrap_or(1),
                    });
                }
                *offset = offset.saturating_add(bits);
            }
            _ => {}
        }
        // Local items apply to the next main item only.
        if item_type == ITEM_TYPE_MAIN {
            usage_min = None;
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

    /// The MX Master 3S report descriptor over Bluetooth, read from sysfs on
    /// the user's machine in phase 5c (docs/protocol/evidence/m5c).
    const MX_MASTER_3S_BT: &[u8] = &[
        0x05, 0x01, 0x09, 0x02, 0xa1, 0x01, 0x85, 0x02, 0x09, 0x01, 0xa1, 0x00, 0x95, 0x10, 0x75,
        0x01, 0x15, 0x00, 0x25, 0x01, 0x05, 0x09, 0x19, 0x01, 0x29, 0x10, 0x81, 0x02, 0x05, 0x01,
        0x16, 0x01, 0xf8, 0x26, 0xff, 0x07, 0x75, 0x0c, 0x95, 0x02, 0x09, 0x30, 0x09, 0x31, 0x81,
        0x06, 0x15, 0x81, 0x25, 0x7f, 0x75, 0x08, 0x95, 0x01, 0x09, 0x38, 0x81, 0x06, 0x95, 0x01,
        0x05, 0x0c, 0x0a, 0x38, 0x02, 0x81, 0x06, 0xc0, 0xc0, 0x06, 0x43, 0xff, 0x0a, 0x02, 0x02,
        0xa1, 0x01, 0x85, 0x11, 0x75, 0x08, 0x95, 0x13, 0x15, 0x00, 0x26, 0xff, 0x00, 0x09, 0x02,
        0x81, 0x00, 0x09, 0x02, 0x91, 0x00, 0xc0,
    ];

    #[test]
    fn finds_the_buttons_of_the_mx_master_3s() {
        let s = walk_descriptor(MX_MASTER_3S_BT);
        assert!(!s.truncated);
        assert_eq!(
            s.button_fields,
            [ButtonField {
                report_id: Some(0x02),
                bit_offset: 0,
                count: 16,
                first_button: 1,
            }]
        );
        let f = s.button_fields[0];
        // SYNTHETIC reports: buttons 1 and 4 held, then none; a HID++ report.
        assert_eq!(f.pressed(&[0x02, 0x09, 0x00, 0, 0, 0, 0, 0]), Some(0b1001));
        assert_eq!(f.pressed(&[0x02, 0x00, 0x00, 0, 0]), Some(0));
        assert_eq!(f.pressed(&[0x11, 0xFF, 0x00]), None);
        assert_eq!(f.pressed(&[0x02]), None);
    }

    #[test]
    fn button_offsets_count_earlier_inputs_and_push_pop() {
        // SYNTHETIC: no report ID; an 8-bit input, then push, 3 buttons
        // from usage 2, pop.
        let s = walk_descriptor(&[
            0x05, 0x01, 0x75, 0x08, 0x95, 0x01, 0x81, 0x02, // 8 bits of generic desktop
            0xA4, // Push
            0x05, 0x09, 0x19, 0x02, 0x75, 0x01, 0x95, 0x03, 0x81, 0x02, // buttons
            0xB4, // Pop
            0x81, 0x02, // 8 more bits, generic desktop again
        ]);
        assert_eq!(
            s.button_fields,
            [ButtonField {
                report_id: None,
                bit_offset: 8,
                count: 3,
                first_button: 2,
            }]
        );
        assert_eq!(s.button_fields[0].pressed(&[0x00, 0b100]), Some(0b100));
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
