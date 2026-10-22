//! Shared rendering helpers: device JSON, redaction, text tables.

use hidlab_transport::{Access, HidrawDevice};
use serde::Serialize;

pub const REDACTED: &str = "<redacted>";

/// Stable JSON shape of one device.
#[derive(Debug, Serialize)]
pub struct DeviceJson {
    pub number: usize,
    pub hidraw: String,
    pub node: String,
    pub hid_device: Option<String>,
    pub bus: String,
    pub bus_code: String,
    pub vendor_id: String,
    pub product_id: String,
    pub name: Option<String>,
    /// `"<redacted>"` unless `--show-unique`; `null` if the device has none.
    pub unique: Option<String>,
    pub driver: Option<String>,
    pub logitech: bool,
    /// `null` when the descriptor could not be read.
    pub hidpp: Option<bool>,
    pub hidpp_short_report: Option<bool>,
    pub hidpp_long_report: Option<bool>,
    pub report_ids: Option<Vec<String>>,
    pub usage_pages: Option<Vec<String>>,
    pub descriptor_truncated: Option<bool>,
    pub descriptor_error: Option<String>,
    pub access: Access,
    pub access_detail: Option<String>,
}

impl DeviceJson {
    pub fn new(number: usize, d: &HidrawDevice, show_unique: bool) -> Self {
        let desc = d.descriptor.as_ref();
        Self {
            number,
            hidraw: d.hidraw_name.clone(),
            node: d.node.display().to_string(),
            hid_device: d.hid_device.clone(),
            bus: d.uevent.bus.label(),
            bus_code: format!("0x{:04x}", d.uevent.bus.code()),
            vendor_id: format!("0x{:04x}", d.uevent.vendor_id),
            product_id: format!("0x{:04x}", d.uevent.product_id),
            name: d.uevent.name.clone(),
            unique: d.uevent.uniq.as_ref().map(|u| redact(u, show_unique)),
            driver: d.driver.clone(),
            logitech: d.is_logitech(),
            hidpp: desc.map(|s| s.looks_like_hidpp()),
            hidpp_short_report: desc.map(|s| s.has_short_report()),
            hidpp_long_report: desc.map(|s| s.has_long_report()),
            report_ids: desc.map(|s| s.report_ids.iter().map(|r| format!("0x{r:02x}")).collect()),
            usage_pages: desc.map(|s| s.usage_pages.iter().map(|u| format!("0x{u:04x}")).collect()),
            descriptor_truncated: desc.map(|s| s.truncated),
            descriptor_error: d.descriptor_error.clone(),
            access: d.access.status,
            access_detail: d.access.detail.clone(),
        }
    }
}

pub fn redact(value: &str, show: bool) -> String {
    if show {
        value.to_owned()
    } else {
        REDACTED.to_owned()
    }
}

pub fn access_label(a: Access) -> &'static str {
    match a {
        Access::Ok => "ok",
        Access::PermissionDenied => "denied",
        Access::Missing => "no node",
        Access::Disconnected => "gone",
        Access::Error => "error",
    }
}

pub fn hidpp_label(d: &HidrawDevice) -> &'static str {
    match &d.descriptor {
        None => "?",
        Some(s) if s.looks_like_hidpp() => "yes",
        Some(_) => "no",
    }
}

/// Renders rows as a `|`-separated table with padded columns.
pub fn table(header: &[&str], rows: &[Vec<String>]) -> String {
    let mut widths: Vec<usize> = header.iter().map(|h| h.chars().count()).collect();
    for row in rows {
        for (w, cell) in widths.iter_mut().zip(row) {
            *w = (*w).max(cell.chars().count());
        }
    }
    let line = |cells: Vec<&str>| -> String {
        let last = cells.len().saturating_sub(1);
        cells
            .iter()
            .enumerate()
            .map(|(i, c)| {
                if i == last {
                    (*c).to_owned()
                } else {
                    format!("{c:<w$}", w = widths[i])
                }
            })
            .collect::<Vec<_>>()
            .join(" | ")
    };
    let mut out = line(header.to_vec());
    out.push('\n');
    out.push_str(
        &widths
            .iter()
            .map(|w| "-".repeat(*w))
            .collect::<Vec<_>>()
            .join("-|-"),
    );
    out.push('\n');
    for row in rows {
        out.push_str(&line(row.iter().map(String::as_str).collect()));
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction() {
        assert_eq!(redact("aa:bb", false), REDACTED);
        assert_eq!(redact("aa:bb", true), "aa:bb");
    }

    #[test]
    fn table_pads_columns() {
        let t = table(&["a", "bb"], &[vec!["xyz".into(), "1".into()]]);
        assert_eq!(t, "a   | bb\n----|---\nxyz | 1\n");
    }
}
