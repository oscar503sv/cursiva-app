# HID++ protocol evidence log

Every protocol fact hid-lab relies on is recorded here with a status, its
source and its evidence. Status levels (requirements §13):

```text
CONFIRMED   verified on the real device, multiple values, reproducible
LIKELY      consistent with reference implementations and one observation
HYPOTHESIS  inferred, not yet tested
UNKNOWN
```

## References

| ID | Project | File | License | Use |
|---|---|---|---|---|
| R-1 | Linux kernel | `drivers/hid/hid-logitech-hidpp.c` | GPL-2.0 | Behavior described, no code copied |
| R-2 | Solaar | `lib/logitech_receiver/` (hidpp20, base) | GPL-2.0 | Behavior described, no code copied |

## Evidence files

| File | What | Captured |
|---|---|---|
| `mx-master-3s-features.json` | `hid-lab lab features --json`, MX Master 3S over Bluetooth, `HID_UNIQ` redacted, full TX/RX transcript | 2026-10-21, hid-lab 0.1.0, Fedora 44, kernel 7.2.11, BlueZ 5.87 |

Transcript entries are referenced below as `request N` (the `request` field).

### Observation O-1 (2026-10-21)

`hid-lab lab features` was run twice in a row in the user's session, as a
normal user, by Claude Code. Both runs returned the same protocol version and
the same 36 entries. The first run is the committed evidence file.

**Promotion rule.** Entries below marked *observed in O-1* move from LIKELY to
CONFIRMED once the user runs the following command themselves and the feature
list matches the evidence file:

```bash
./target/release/hid-lab lab features --json > /tmp/features.json
diff <(jq .features /tmp/features.json) <(jq .features docs/protocol/mx-master-3s-features.json)
```

## Transport facts

### T-1 — Report IDs declared by the MX Master 3S over Bluetooth

Status: **CONFIRMED** (read from sysfs, not a protocol interpretation)

The report descriptor declares report IDs `0x02` and `0x11`, and usage pages
`0x0001`, `0x0009`, `0x000C`, `0xFF43`. It does **not** declare `0x10`.
Evidence: `device.report_ids` and `device.usage_pages` in the evidence file.

Consequence: over Bluetooth this mouse is addressed with long reports only.

## Framing facts

### P-001 — Short and long report sizes

Status: **LIKELY** (long observed in O-1; short not testable on this device)

Short = report `0x10`, 7 bytes. Long = report `0x11`, 20 bytes.
Source: R-1, R-2. Evidence: every TX/RX in O-1 is 20 bytes with ID `0x11`.

### P-002 — Report layout

Status: **LIKELY** (observed in O-1)

`[report_id, device_index, feature_index, (function << 4) | sw_id, params...]`.
Source: R-1, R-2. Evidence: every response in O-1 echoes bytes 1–3 of its
request, e.g. request 1 TX `11 ff 00 1b ...`, RX `11 ff 00 1b 04 05 5a ...`.

### P-003 — Device index `0xFF` for a directly connected device

Status: **LIKELY** (observed in O-1)

Source: R-1, R-2. Evidence: all requests sent with `0xFF` were answered with
`0xFF` over Bluetooth.

### P-004 — Software ID is echoed; notifications use 0

Status: **LIKELY** (echo observed in O-1; sw_id 0 notifications not observed)

hid-lab uses `sw_id = 0x0B`. Every response in O-1 carries `0x?B` in byte 3.
Source: R-1, R-2.

### P-005 — The device may answer in a different report size than the request

Status: **HYPOTHESIS**

A short request whose answer has more than 3 parameter bytes (e.g.
getFeatureID, 4 bytes) cannot be answered in a short report. hid-lab therefore
accepts both sizes for any request. Not observable on this device, which only
declares long reports.

### P-006 — HID++ 2.0 error response

Status: **LIKELY** (not observed)

`feature_index = 0xFF`, params `[orig_feature_index, orig_function_swid, error_code]`.
Codes: `0x00` no error, `0x01` unknown, `0x02` invalid argument, `0x03` out of
range, `0x04` hardware error, `0x05` internal, `0x06` invalid feature index,
`0x07` invalid function, `0x08` busy, `0x09` unsupported. Source: R-1.

### P-007 — HID++ 1.0 error response

Status: **LIKELY** (not observed)

Sub-ID `0x8F` in the feature-index position, params
`[orig_sub_id, orig_address, error_code]`. Codes `0x01`..`0x0C` as named in
`crates/hidlab-hidpp/src/error_code.rs`. Source: R-1.

## Root feature (0x0000)

### P-008 — Root is at feature index `0x00`

Status: **LIKELY** (observed in O-1)

Evidence: request 4, getFeatureID(0) returns ID `0x0000`.

### P-009 — Root function 0, getFeature

Status: **LIKELY** (observed in O-1)

Request params `[id_hi, id_lo]`. Response `[feature_index, feature_type, feature_version]`;
index 0 means not supported (for any ID other than root). Source: R-1, R-2.
Evidence: request 2, getFeature(`0x0001`) → `01 00 02`. This agrees with
request 5, getFeatureID(1) → `0x0001`, type `0x00`, version 2.

### P-010 — Root function 1, getProtocolVersion

Status: **LIKELY** (observed in O-1)

Request params `[0, 0, ping]`. Response `[major, minor, ping]`. Source: R-1.
Evidence: request 1 sent ping `0x5A` and received `04 05 5a`. Protocol 4.5
matches the kernel log `HID++ 4.5 device connected` (diagnosis.md).

## IFeatureSet (0x0001)

### P-011 — Function 0, getCount (root not counted)

Status: **LIKELY** (observed in O-1)

Evidence: request 3 → `0x23` (35). getFeatureID answered for every index
0..=35 (36 entries, requests 4–39), so the count excludes root. Source: R-1, R-2.

### P-012 — Function 1, getFeatureID

Status: **LIKELY** (observed in O-1)

Request params `[index]`. Response `[id_hi, id_lo, feature_type, feature_version]`.
Evidence: requests 4–39. Source: R-1, R-2.

### P-013 — Feature-type flag bits

| Bit | Meaning | Status |
|---|---|---|
| `0x80` | obsolete | LIKELY (R-2; not seen in O-1) |
| `0x40` | hidden | LIKELY (R-2; seen in O-1) |
| `0x20` | engineering / internal (references name it differently) | LIKELY (R-2; seen in O-1) |
| `0x10` | not named by hid-lab | UNKNOWN (seen in O-1) |
| `0x08` | not named by hid-lab | UNKNOWN (seen in O-1, index `0x19`) |

## Coexistence

### P-014 — Unrelated traffic on the shared hidraw node

Status: **UNKNOWN** on the real device

During O-1 the mouse was idle and no unrelated report was received, so
`ignored_reports` was 0. The filtering of mouse reports, notifications and
foreign sw_ids is covered only by the synthetic fake device in
`crates/hidlab-hidpp/tests/fake_device.rs`. To test it, run
`lab features --json` while moving the mouse and check that `ignored_reports`
is greater than 0 and the feature list is unchanged.

### P-015 — Latency over Bluetooth

Status: observation only

In O-1 the first answer arrived after about 137 ms. Each later answer took
about 15 ms. The whole enumeration took about 0.77 s.

## MX Master 3S feature list (Bluetooth, PID `0xB034`)

Status of the list: **LIKELY** (observed in O-1, two identical runs).
Names are reference names from R-1/R-2 and are **not verified**. "unknown"
means the ID is not in hid-lab's reference table; no name is guessed.

| Index | Feature ID | Reference name | Type flags | Version |
|---|---|---|---|---|
| 0x00 | 0x0000 | Root | 0x00 | 2 |
| 0x01 | 0x0001 | FeatureSet | 0x00 | 2 |
| 0x02 | 0x0003 | DeviceFwVersion | 0x00 | 4 |
| 0x03 | 0x0005 | DeviceName | 0x00 | 0 |
| 0x04 | 0x1D4B | unknown | 0x00 | 0 |
| 0x05 | 0x0020 | unknown | 0x00 | 0 |
| 0x06 | 0x0021 | unknown | 0x00 | 1 |
| 0x07 | 0x0007 | unknown | 0x00 | 0 |
| 0x08 | 0x1004 | UnifiedBattery | 0x00 | 3 |
| 0x09 | 0x1B04 | ReprogControlsV4 | 0x00 | 5 |
| 0x0A | 0x1814 | ChangeHost | 0x00 | 1 |
| 0x0B | 0x1815 | unknown | 0x00 | 2 |
| 0x0C | 0x2250 | unknown | 0x00 | 1 |
| 0x0D | 0x2201 | AdjustableDpi | 0x00 | 2 |
| 0x0E | 0x2110 | SmartShift | 0x00 | 0 |
| 0x0F | 0x2121 | HiResWheel | 0x00 | 1 |
| 0x10 | 0x2150 | Thumbwheel | 0x00 | 0 |
| 0x11 | 0x2251 | unknown | 0x00 | 0 |
| 0x12 | 0x00C3 | unknown | 0x00 | 0 |
| 0x13 | 0x1802 | unknown | 0x70 | 0 |
| 0x14 | 0x1803 | unknown | 0x70 | 0 |
| 0x15 | 0x1806 | unknown | 0x70 | 8 |
| 0x16 | 0x1816 | unknown | 0x70 | 0 |
| 0x17 | 0x1805 | unknown | 0x60 | 0 |
| 0x18 | 0x1830 | unknown | 0x70 | 0 |
| 0x19 | 0x1891 | unknown | 0x68 | 7 |
| 0x1A | 0x18A1 | unknown | 0x70 | 0 |
| 0x1B | 0x1E00 | unknown | 0x40 | 0 |
| 0x1C | 0x1E02 | unknown | 0x60 | 0 |
| 0x1D | 0x1602 | unknown | 0x00 | 0 |
| 0x1E | 0x1EB0 | unknown | 0x70 | 0 |
| 0x1F | 0x1861 | unknown | 0x70 | 1 |
| 0x20 | 0x9300 | unknown | 0x70 | 0 |
| 0x21 | 0x9001 | unknown | 0x70 | 0 |
| 0x22 | 0x1E22 | unknown | 0x70 | 0 |
| 0x23 | 0x9205 | unknown | 0x70 | 0 |

Notes for milestone 3 (no conclusions drawn yet):

- The device reports `0x2110` (SmartShift), not `0x2111` (SmartShiftEnhanced).
- Feature indices are assigned by the device. Code must locate features with
  getFeature at runtime and never hard-code the indices above.
- `0x2250` and `0x2251` are not in the reference table. Their meaning is
  UNKNOWN until a public source is cited and the device behavior is checked.
