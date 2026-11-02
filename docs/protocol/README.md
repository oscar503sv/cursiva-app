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
| R-1 | Linux kernel | `drivers/hid/hid-logitech-hidpp.c` | GPL-2.0-only | Behavior described, no code copied |
| R-2 | Solaar | `lib/logitech_receiver/` (`hidpp20.py`, `settings_templates.py`, `special_keys.py`, `hidpp20_constants.py`, `common.py`) | GPL-2.0-or-later | Behavior described, no code copied |
| R-3 | GNOME Settings Daemon | `plugins/power/gsd-power-manager.c`, tag `50.1` (the version in Fedora 44) | GPL-2.0-or-later | Behavior described, no code copied |

Milestone 3 entries were checked against R-1 at commit `4403278e4ea2` and R-2
at commit `e7304c4c451c` (both `master`, read 2026-10-21). Function numbers
below are 0..15; the sources write them as `function << 4` (`0x10` = f1).

## Evidence files

| File | What | Captured |
|---|---|---|
| `mx-master-3s-features.json` | `hid-lab lab features --json`, MX Master 3S over Bluetooth, `HID_UNIQ` redacted, full TX/RX transcript | 2026-10-21, hid-lab 0.1.0, Fedora 44, kernel 7.2.11, BlueZ 5.87 |
| `mx-master-3s-state.json` | `hid-lab lab read --json` (observation O-4), `HID_UNIQ` and firmware unit ID redacted (also in the transcript) | 2026-10-21, same system |

Transcript entries are referenced below as `request N` (the `request` field).

### Observation O-1 (2026-10-21)

`hid-lab lab features` was run twice in a row in the user's session, as a
normal user, by Claude Code. Both runs returned the same protocol version and
the same 36 entries. The first run is the committed evidence file.

### Observation O-2 (2026-10-21)

The user ran `hid-lab lab features` on their own Fedora 44 machine, over
Bluetooth, as a normal user. The user reported that the result matches
`mx-master-3s-features.json`. Transcript not committed.

### Observation O-3 (2026-10-21)

The user ran `hid-lab lab features --json` again while moving the mouse and
turning the wheel. The user reported `ignored_reports = 48` and the same 36
entries (indices 0x00..0x23). Transcript not committed.

O-1, O-2 and O-3 together make four runs with identical results, two of them
by the user. Facts marked CONFIRMED below cite these observations.

## Transport facts

### T-1 — Report IDs declared by the MX Master 3S over Bluetooth

Status: **CONFIRMED** (read from sysfs, not a protocol interpretation)

The report descriptor declares report IDs `0x02` and `0x11`, and usage pages
`0x0001`, `0x0009`, `0x000C`, `0xFF43`. It does **not** declare `0x10`.
Evidence: `device.report_ids` and `device.usage_pages` in the evidence file.

Consequence: over Bluetooth this mouse is addressed with long reports only.

## Framing facts

### P-001 — Report sizes, and long reports only over Bluetooth

| Fact | Status | Evidence |
|---|---|---|
| Long report = ID `0x11`, 20 bytes | **CONFIRMED** | Every TX/RX in O-1 is 20 bytes with ID `0x11`; O-2 and O-3 succeed with the same session |
| The MX Master 3S over Bluetooth uses long reports only | **CONFIRMED** | T-1 (descriptor has no `0x10`); full enumeration with long reports only in O-1, O-2, O-3 |
| Short report = ID `0x10`, 7 bytes | LIKELY | R-1, R-2. Not testable on this device; relevant later for the Bolt receiver |

### P-002 — Report layout

Status: **CONFIRMED** (O-1, O-2, O-3)

`[report_id, device_index, feature_index, (function << 4) | sw_id, params...]`.
Source: R-1, R-2. Evidence: every response in O-1 echoes bytes 1–3 of its
request, e.g. request 1 TX `11 ff 00 1b ...`, RX `11 ff 00 1b 04 05 5a ...`.
O-2 and O-3 return the same data through the same matching rules.

### P-003 — Device index `0xFF` for a directly connected device

Status: **CONFIRMED** for Bluetooth (O-1, O-2, O-3)

Source: R-1, R-2. Evidence: all requests sent with `0xFF` were answered with
`0xFF` over Bluetooth. Receiver indices 1..6 remain LIKELY and untested.

### P-004 — Software ID is echoed; notifications use 0

| Fact | Status | Evidence |
|---|---|---|
| The device echoes the request's non-zero `sw_id` (`0x0B`) | **CONFIRMED** | Every response in O-1 carries `0x?B` in byte 3; O-2 and O-3 matched on it |
| Device notifications carry `sw_id = 0` | **CONFIRMED** | 7 notifications with `sw_id 0` in O-5 (`evidence/m3/04` and `05`); see P-029 |
| The daemon's `sw_id` `0x0C` is echoed too | **CONFIRMED** | O-7: every daemon apply in `evidence/m4b/journal.txt` verified by read-back, and every `status` read the four settings |

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

Status: **CONFIRMED** (O-1, O-2, O-3)

Evidence: request 4, getFeatureID(0) returns ID `0x0000`.

### P-009 — Root function 0, getFeature

Status: **CONFIRMED** for a supported feature (O-1, O-2, O-3). The
"index 0 means not supported" answer has not been observed and stays LIKELY.

Request params `[id_hi, id_lo]`. Response `[feature_index, feature_type, feature_version]`;
index 0 means not supported (for any ID other than root). Source: R-1, R-2.
Evidence: request 2, getFeature(`0x0001`) → `01 00 02`. This agrees with
request 5, getFeatureID(1) → `0x0001`, type `0x00`, version 2.

### P-010 — Root function 1, getProtocolVersion and ping echo

Status: **CONFIRMED** (O-1, O-2, O-3; hid-lab fails with a ping mismatch
error otherwise, and none occurred)

Request params `[0, 0, ping]`. Response `[major, minor, ping]`. Source: R-1.
Evidence: request 1 sent ping `0x5A` and received `04 05 5a`. Protocol 4.5
matches the kernel log `HID++ 4.5 device connected` (diagnosis.md).

## IFeatureSet (0x0001)

### P-011 — Function 0, getCount (root not counted)

Status: **CONFIRMED** (O-1, O-2, O-3)

Evidence: request 3 → `0x23` (35). getFeatureID answered for every index
0..=35 (36 entries, requests 4–39), so the count excludes root. Source: R-1, R-2.

### P-012 — Function 1, getFeatureID

Status: **CONFIRMED** (O-1, O-2, O-3)

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

### P-014 — Coexistence with the kernel driver and normal input

Status: **CONFIRMED** (O-3)

The hidraw node is shared with the bound `logitech-hidpp-device` kernel
driver and with normal mouse input. hid-lab reads until its deadline and
accepts only a report whose device index, feature index, function and
`sw_id` all match the request. Everything else is ignored and counted.

Evidence:

- O-3: the user moved the mouse and turned the wheel during enumeration.
  `ignored_reports` was 48 and all 36 entries came back, identical to the
  evidence file. No request failed.
- O-1 and O-2: idle mouse, 0 ignored, same result.
- Synthetic coverage of each noise type (mouse reports, `sw_id 0`
  notifications, foreign `sw_id`, foreign errors, truncated reports) in
  `crates/hidlab-hidpp/tests/fake_device.rs`.

Not covered: which report IDs the 48 ignored reports had. The O-3
transcript was not committed. Commit one with the next capture if that
breakdown is needed.

### P-015 — Latency over Bluetooth

Status: observation only

In O-1 the first answer arrived after about 137 ms. Each later answer took
about 15 ms. The whole enumeration took about 0.77 s.

## MX Master 3S feature list (Bluetooth, PID `0xB034`)

Status of the list (IDs, indices, type bytes, versions): **CONFIRMED**
(O-1, O-2, O-3: four identical runs, two by the user).
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

## Milestone 3 — feature state (read-only)

hid-lab only sends the functions listed in
`crates/hidlab-features/src/allowlist.rs`. A test proves that no function the
references use as a write is in that list, and another test checks every
request a full read sends against it. Functions that no reference documents
are never called, because they could be writes. Feature indices are resolved
with getFeature on every run.

### Observation O-4 (2026-10-21)

`hid-lab lab read --json` was run in the user's session by Claude Code, with
the mouse not charging (kernel status Discharging). The mouse was **moving**
during this run: the session ignored 36 input reports with report ID `0x02`
(pointer movement) and no unrelated HID++ report. All requests still
matched. A separate text-mode run a moment earlier ignored 0 reports.
Correction: an earlier version of this entry said the mouse was not touched;
the user pointed out the 36 ignored reports in the evidence file. The
evidence file is `mx-master-3s-state.json`.

### Observation O-5 (2026-10-21) — user's hardware tests H3

The user ran every step of `docs/hardware-test.md` section M3 on their own
machine, as a normal user, over Bluetooth. Captures are in `evidence/m3/`
(Bluetooth address and unit ID redacted; see the `sanitized` field of the two
watch files). Each claim below was checked against these files.

| File | Step | Conditions |
|---|---|---|
| `features.json` | prep | identical feature list to `mx-master-3s-features.json` |
| `01-baseline.json` | 1 | idle, no cable |
| `02-after-press-1.json` | 2 | after one press of the mode button |
| `03-after-press-2.json` | 2 | after a second press |
| `04-watch-mode-button.json` | 3 | `lab watch` 30 s while pressing the mode button |
| `05-watch-cable.json` | 4 | `lab watch` 120 s: cable plugged, `lab read`, cable unplugged |
| `06-read-charging.json` | 4 | cable plugged |
| `07-after-channel-switch.json` | 5 | after switching to another channel and back |

User's report: the 7 non-virtual controls correspond to the 7 physical
buttons; each mode-button press toggled the wheel between clicks and free
spin, matching the reads; notifications appeared on mode-button presses; the
cable test worked; channel LED 2 was lit before and after the switch.

All five reads: no error, every target feature read, the same feature indices,
0 ignored reports.

### P-020 — 0x0005 DeviceName

Sources: R-1 (GET_COUNT, GET_DEVICE_NAME, GET_TYPE), R-2 (`get_name`, `get_kind`, `DEVICE_KIND`).

```text
f0 getDeviceNameCount()       -> [length]
f1 getDeviceName(char_index)  -> up to 16 name bytes per long report; repeat until length
f2 getDeviceType()            -> [type]   R-2 table: 0 keyboard ... 3 mouse ... 7 receiver
```

| Fact | Status | Evidence |
|---|---|---|
| f0 length, f1 at char index 0, f2 type | **CONFIRMED** | O-4 and five O-5 reads: length 12, name `MX Master 3S` (the end of `HID_NAME`), type 3 = mouse, matching the device |
| f1 with char index > 0 (names longer than 16 bytes) | LIKELY | Not exercised: this name fits one chunk |

### P-021 — 0x0003 DeviceFwVersion

Sources: R-2 (`get_firmware`, `get_ids`, `FirmwareKind`), R-1 (`hidpp_get_serial`: bytes 1..5 of f0 are the serial).

```text
f0 getDeviceInfo()   -> [entity_count, unit_id[4], byte5?, transport_bits, model_ids[6], ...]
                        transport bits 0x01 bt, 0x02 btle, 0x04 wpid, 0x08 usb;
                        each set bit consumes 2 bytes of model_ids, in that order
f1 getFwInfo(entity) -> [type, ...]   type & 0x0F: 0 firmware, 1 bootloader, 2 hardware, 3 other
   types 0 and 1: [type, name[3], major, minor, build u16 BE, byte8?, extras...]
   type 2:        [type, hardware_version]
```

The unit ID is a serial number. It is redacted by default in decoded output,
in `lab read` transcripts and, since the fix after O-5, in `lab watch`.

| Fact | Status | Evidence |
|---|---|---|
| f0 entity count, transport bits and model ID position | **CONFIRMED** | Six reads (O-4 and O-5): `btle=B034`, equal to the Bluetooth product ID in `HID_ID`; 3 entities |
| Unit ID at f0 bytes 1..5 | LIKELY | R-1 and R-2 agree; no independent source on this system |
| f1 layout for types 0 and 1 | LIKELY | Readable names `BL1` and `RBM`, versions 69.01 and 22.01, build `0x0006`, stable in six reads (O-4 and O-5); no independent version check (the optional fwupd comparison was not run) |
| Entity 2 type byte `0x05`, raw `05 00 00 00 00 00 01 26` | UNKNOWN | Not in R-2's table |
| f0 byte 5, f1 byte 8, extras `b0 34 bb 51 90 e8` | UNKNOWN | Extras checked not to contain the unit ID or the Bluetooth address |

### P-022 — 0x1004 UnifiedBattery

Sources: R-1 (GET_CAPABILITIES, GET_STATUS, level and flag bits, status mapping, event 0), R-2 (`decipher_battery_unified`, `notifications.py`).

```text
f0 getCapabilities() -> [supported_levels, flags]   flags bit0 rechargeable, bit1 state_of_charge
f1 getStatus()       -> [soc_%, level_bits, charging_status, external_power]
                        level bits: 0x01 critical, 0x02 low, 0x04 good, 0x08 full
event f0 (sw_id 0)   -> same layout as getStatus
```

| Fact | Status | Evidence |
|---|---|---|
| Byte 0 = state of charge, % | **CONFIRMED** | Equal to the kernel's `capacity` in all six reads, at two values (35 and 40) |
| charging_status 0 = discharging | **CONFIRMED** | Reads without cable; kernel status Discharging |
| charging_status 1 = charging | **CONFIRMED** | `06-read-charging.json` with cable; kernel status Charging; notifications in `05-watch-cable.json` |
| charging_status 2 (kernel: charging slow; Solaar: almost full) | HYPOTHESIS | References conflict; not observed |
| charging_status 3 (complete / full) | LIKELY | References agree; not observed |
| charging_status 4 (kernel: error; Solaar: slow recharge) | HYPOTHESIS | References conflict; not observed |
| Byte 3 = 1 while a cable is connected, 0 otherwise | LIKELY | R-1 names it `external_power_status` without interpreting it; observed 0 → 1 → 0 across one plug/unplug |
| Level bit `0x04` good | LIKELY | Only this level observed |
| f0 capabilities `0f 03` | LIKELY | One value |
| Event f0 with the getStatus layout | **CONFIRMED** | `05-watch-cable.json`: 3 notifications whose bytes match the reads and the cable state |

Battery notifications in `05-watch-cable.json` (feature index `0x08`):

| Time | Bytes | Meaning | Matches |
|---|---|---|---|
| 25.0 s | `23 04 01 01` | 35 %, good, charging, byte 3 = 1 | cable plugged |
| 85.6 s | `28 04 01 01` | 40 %, good, charging, byte 3 = 1 | while charging |
| 107.7 s | `28 04 00 00` | 40 %, good, discharging, byte 3 = 0 | cable unplugged |

The charge rose from 35 % to 40 % in about one minute of charging. The
kernel reported the same values. Why the step is that large is UNKNOWN.

### P-023 — 0x1814 ChangeHost

Source: R-2 (`ChangeHost` setting, read f0). f1 (setCurrentHost) is a write that switches the mouse away; it is never called.

```text
f0 getHostInfo() -> [host_count, current_host (0-based), ...]
```

Status: **CONFIRMED** (O-5). All six reads (O-4 and O-5) return `03 01`: 3 hosts and
host 1, shown as channel 2. The user saw LED 2 lit before and after a real
channel switch. The switch did reconnect the mouse: the kernel registered
its battery again under a new name (`hidpp_battery_0` before,
`hidpp_battery_1` after). The hidraw node stayed `hidraw2` this time, and
`lab read` found the mouse without arguments. Only current-host value 1 can
be observed from this computer.

### P-024 — 0x1B04 ReprogControlsV4

Sources: R-1 (GET_COUNT, GET_CID_INFO, flag bit0 mouse, bit5 divertable), R-2 (`KeysArrayV4._query_key`, `_getCidReporting`, `KeyFlag`, `MappingFlag`, `special_keys.CONTROL`, `Task`). f3 (setCidReporting) is sent only by `lab write` to remap, never to divert (P-036).

```text
f0 getCount()             -> [count]
f1 getCidInfo(index)      -> [cid u16 BE, tid u16 BE, flags_lo, pos, group, gmask, flags_hi]
                             flags = flags_lo | flags_hi << 8 (R-2 KeyFlag names)
f2 getCidReporting(cid BE)-> [cid u16 BE, mflags_lo, mapped_to u16 BE, mflags_hi]
                             mflags = mflags_lo | mflags_hi << 8 (R-2 MappingFlag names);
                             mapped_to 0 means "itself"
```

Controls (identical in O-4 and all five O-5 reads; reference names from R-2):

| # | CID | Reference name | TID | Flags | Reporting |
|---|---|---|---|---|---|
| 0 | 0x0050 | Left_Button | 0x0038 | mouse, analytics_key_events | none |
| 1 | 0x0051 | Right_Button | 0x0039 | mouse, analytics_key_events | none |
| 2 | 0x0052 | Middle_Button | 0x003A | mouse, reprogrammable, divertable, raw_xy, analytics_key_events | none |
| 3 | 0x0053 | Back_Button | 0x003C | same as #2 | none |
| 4 | 0x0056 | Forward_Button | 0x003E | same as #2 | none |
| 5 | 0x00C3 | Mouse_Gesture_Button | 0x00A9 | same as #2 | none |
| 6 | 0x00C4 | Smart_Shift | 0x009D | same as #2 | none |
| 7 | 0x00D7 | Virtual_Gesture_Button | 0x00B4 | divertable, virtual, raw_xy, force_raw_xy | none |

| Fact | Status | Evidence |
|---|---|---|
| f0 count = 8; f1 CID and TID positions | **CONFIRMED** | Six identical reads (O-4 and O-5); the 7 non-virtual controls match the 7 physical buttons (user, O-5) |
| f1 bit0 `mouse`, bit7 `virtual` | **CONFIRMED** | Set on exactly the 7 physical controls and the 1 virtual control |
| Which physical button each CID is | **CONFIRMED** for 0x0053 back, 0x0056 forward, 0x00C4 mode button; LIKELY for the rest | O-10: remapping each changed what that physical button did. 0x00C3 was remapped to forward, which it already does by itself (O-10), so the test could not tell; 0x0052 was not remapped |
| Other capability bits (divertable, raw_xy, ...) | LIKELY | Not testable without writes |
| f2 echoes the CID; mapped_to = itself | **CONFIRMED** | All 8 controls, six reads (O-4 and O-5) |
| f2 reporting flag bits | LIKELY | Only the value 0 (nothing diverted) observed |

### P-025 — 0x2201 AdjustableDpi

Status: **LIKELY** (unchanged). Source: R-2 only (`AdjustableDpi`: list f1, read f2; `produce_dpi_list`). f0 is not documented and not called. f3 (setSensorDpi) is never called.

```text
f1 getSensorDpiList(sensor 0) -> [sensor, u16 BE values..., 0x0000]
                                 a value with top 3 bits 111 is a step (value & 0x1FFF);
                                 the range runs from the previous value to the next one
f2 getSensorDpi()             -> [sensor, dpi u16 BE, default_dpi u16 BE]; dpi 0 = default
```

Only the first list page is read. List `200..8000 step 50`, current 1000,
default 1000, stable in six reads (O-4 and O-5). Only one current value was observed,
so this needs a DPI change (milestone 4) to confirm.

### P-026 — 0x2110 SmartShift

Source: R-2 only (`ScrollRatchet`, `SmartShift`: read f0). f1 is a write and is never called.

```text
f0 getRatchetControlMode() -> [mode, threshold, byte2]   mode 1 free-spin, 2 ratchet
```

| Fact | Status | Evidence |
|---|---|---|
| Mode 2 = ratchet, 1 = free spin | **CONFIRMED** | `01` mode 2, `02` mode 1, `03` mode 2, each matching what the user felt on the wheel |
| Byte 1 = auto-disengage threshold | **CONFIRMED** | Reads 10, 3 and 40 after writing 3 and 40, with the wheel's feel matching (O-6, H4-04, P-031) |
| Byte 2 | UNKNOWN | Always `0x0a`, also while byte 1 was 3 or 40 (O-6), so it is not a copy of the current threshold |

### P-027 — 0x2121 HiResWheel

Sources: R-1 (f0 multiplier; f2 setWheelMode bits, written by the kernel on connect with high resolution on), R-2 (`get_hires_wheel`: f0, f1, f3; `notifications.py`: event f1 byte 0 = ratchet).

```text
f0 getWheelCapability()    -> [multiplier, flags]   0x08 has invert, 0x04 has ratchet switch
f1 getWheelMode()          -> [mode]   bit0 diverted to HID++, bit1 high resolution, bit2 inverted
f3 getRatchetSwitchState() -> [state]  bit0 ratchet
event f1 (sw_id 0)         -> [ratchet]  1 ratchet, 0 free spin (R-2)
```

hid-lab never calls f2. The kernel owns this mode; later milestones must not
change it without coordinating with the kernel driver.

| Fact | Status | Evidence |
|---|---|---|
| f3 bit0 = ratchet | **CONFIRMED** | `01` 1, `02` 0, `03` 1, in step with SmartShift mode and the user's feel |
| f1 mode `0x02` = high resolution on | LIKELY | One value, consistent with the kernel enabling it on connect |
| f0 multiplier 15, has invert, has ratchet switch | LIKELY | One value |
| f0 flag bit `0x10`, f0 bytes 2..3 (`18 18`) | UNKNOWN | f0 answers `0f 1c 18 18` |
| Event f1 with byte 0 = ratchet state | **CONFIRMED** | 4 notifications alternating `00`/`01` for 4 presses (user, O-5), ending in ratchet as the reads before and after show |
| A 0x2110 mode write also sends event f1 | **CONFIRMED** | `03-watch-mode.json` (O-6): `00` about 6 ms after the free-spin write's answer and `01` after the ratchet write's answer; each of the 4 mode writes in `log.jsonl` ignored exactly this report (`11 ff 0f 10 ..`) |
| A threshold write sends event f1 | not observed | `04-watch-threshold.json` (O-6): no 0x2121 notification for the two threshold writes or the flicks after them |

### P-028 — 0x2150 Thumbwheel

Status: **LIKELY** (unchanged). Source: R-2 only (`ThumbMode`, `ThumbInvert`: read f1). f0 is not documented and not called. f2 is a write and is never called.

```text
f1 getThumbwheelStatus() -> [b0, b1, ...]   b0 bit0 diverted, b1 bit0 inverted
```

`00 00` in six reads (O-4 and O-5). Only the default can be observed without writes.

### P-029 — Notifications (software ID 0)

| Fact | Status | Evidence |
|---|---|---|
| The device sends unsolicited HID++ reports with `sw_id 0` | **CONFIRMED** | 7 in O-5: 3 battery, 4 wheel |
| Battery: 0x1004 event f0 | **CONFIRMED** | See P-022 |
| Mode button: 0x2121 event f1, not 0x2110 | **CONFIRMED** | `04-watch-mode-button.json`: one 0x2121 f1 notification per press (4 presses, 4 notifications); none from 0x2110 |
| Software mode change: 0x2121 event f1, not 0x2110 | **CONFIRMED** | See P-027; none from 0x2110 in O-6 |
| `lab watch` sees other programs' traffic without interfering | **CONFIRMED** | `05-watch-cable.json`: the user's `lab read` (sw_id `0x0B`) appears as 43 other-HID++ reports, and that read succeeded |
| Kernel driver HID++ traffic during normal use | not observed | No reports with other software IDs besides hid-lab's in 150 s of watching |

#### Resolved question

The test asked for three presses, and the capture had four notifications.
The user confirmed four presses, so there is one notification per press.

## Milestone 4a — write functions under test

These are the only write functions hid-lab can send. They are listed in
`WRITE_EXPERIMENT_ALLOWLIST` (`crates/hidlab-features/src/allowlist.rs`)
and are reachable only through `hid-lab lab write --write` (requirements
§13 exception). 0x2121 setWheelMode and 0x1814 setCurrentHost are in
`NEVER_WRITE`. Each entry stays LIKELY until its hardware test in
`docs/hardware-test.md`, section M4a, passes.

### Observation O-6 (2026-10-21) — user's hardware tests H4

The user ran section M4a of `docs/hardware-test.md` on their own machine, as
a normal user, over Bluetooth. Captures are in `evidence/m4a/`, including the
user's `log.jsonl` (18 `--write` runs). The Bluetooth address and unit ID were
already redacted by hid-lab (every 0x0003 getDeviceInfo answer reads
`03 ** ** ** **`); the two watch files had the home directory replaced with
`~` (their `sanitized` field).

| File | Test | Result in file |
|---|---|---|
| `01-hotplug.json` | H4-01, three disconnect actions | 3 `remove`/`add` pairs, every `add` access `ok` |
| `01b-idle.json` | H4-01, 900 s idle | no events |
| `02a`, `02b`, `02c` | H4-02 DPI 1600, 400, restore | `pass` ×3 |
| `03a`, `03b`, `03c`, `03-watch-mode.json` | H4-03 mode free-spin, ratchet, restore | `pass`, `pass`, `no_change` |
| `04a`, `04b`, `04-watch-threshold.json` | H4-04 threshold 3, 40 | `pass` ×2; **the restore step was not run** (no `04c` file, no log line) |
| `05a`, `05b` | H4-05 thumb wheel invert on, restore | `pass` ×2 |
| `06a`..`06f` | H4-06 persistence | see P-033 |

User's report: H4-01 normal, no disconnection; H4-02 the pointer felt faster,
then slower, then normal; H4-03 the wheel spun freely for a moment and then
clicked again; H4-04 "all fine", and when asked afterwards: at 3 the wheel switched to free spin clearly more easily than at 40, where the same flick kept clicking; H4-05 the thumb wheel scrolled one way, then
the opposite way, then back to normal; H4-06 fast pointer and free-spinning
wheel, working throughout, then back to normal.

The threshold left at 40 by H4-04 was still 40 when H4-06 started, so H4-06's
threshold write was `no_change`, and the original (10) saved in H4-04 was
used by H4-06's restore. All four originals were removed at the end;
`06f-restored.json` matches `00-before.json` for every written setting.

### P-030 — 0x2201 setSensorDpi (f3)

Status: **CONFIRMED** (O-6). 1600, 400 and 1000 written and read back, the
default stayed 1000 each time, and the user felt the pointer get faster, then
slower, then normal. A fourth write (1600) in H4-06 also passed. The answer
echoes the parameters (`00 06 40`). Source: R-2 `AdjustableDpi` (`write_fnid` 0x30, `write_prefix_bytes` `00`, 2-byte value).

```text
f3 setSensorDpi(sensor, dpi u16 BE)   e.g. 1600 -> params 00 06 40
```

hid-lab only sends values in the list from getSensorDpiList. Verification:
getSensorDpi must return the new value, and the default must not change.
Test: H4-02.

### P-031 — 0x2110 setRatchetControlMode (f1)

Status:

| Fact | Status | Evidence |
|---|---|---|
| `[mode, 00]` sets the mode | **CONFIRMED** | 4 writes (O-6: `03a`, `03b`, H4-06 set and restore), each read back; the user felt free spin and then clicks |
| `00` in the threshold byte keeps the threshold | **CONFIRMED** | Threshold unchanged after all 4 mode writes (10, 10, 40, 10) |
| `[00, threshold]` stores the threshold | **CONFIRMED** | 3 and 40 read back (`04a`, `04b`) |
| `00` in the mode byte keeps the mode | **CONFIRMED** | Mode unchanged (ratchet) after both threshold writes |
| The threshold changes how easily the wheel switches to free spin | **CONFIRMED** | User, asked after O-6: at 3 the wheel switched to free spin clearly more easily than at 40; at 40 the same flick kept clicking. Values 3 and 40 read back |
| Automatic switching to free spin sends a 0x2121 notification | not observed | `04-watch-threshold.json` has none, although the wheel did switch at threshold 3 (user) |
| The answer echoes the parameters | **CONFIRMED** | `01`, `02`, `00 03`, `00 28` |

Source: R-2 `ScrollRatchet` (writes `[mode]`) and `SmartShift` (writes `[0, threshold]`), `write_fnid` 0x10.

```text
f1 setRatchetControlMode(mode, threshold)
   mode only:      [mode, 00]        1 free-spin, 2 ratchet
   threshold only: [00, threshold]   1..50 (R-2's range)
```

Verification reads back both fields: the written one must change and the
other must not.
Threshold 255 (R-2 maps 50 and above to it) is not used. Tests: H4-03, H4-04.

### P-032 — 0x2150 setThumbwheelReporting (f2)

Status: **CONFIRMED** (O-6). `00 01` and `00 00` written and read back with
diverted and the other bits unchanged, and the user felt the thumb wheel
direction flip and come back. A third write (`00 01`) in H4-06 also passed.
Unlike the other two writes, the answer does not echo: its parameters are all
zero. Source: R-2 `ThumbInvert` (`write_fnid` 0x20, true value `00 01`, mask `00 01`, merged with the current value).

```text
f2 setThumbwheelReporting(b0, b1)   b1 bit0 = invert; all other bits copied from getThumbwheelStatus
```

Verification: inverted must change; diverted and the other bits must not.
Test: H4-05.

### P-033 — What the mouse keeps across events

Source: O-6, H4-06 (one run of each event), updated with O-9 (M4c, where
the daemon read every value before re-applying; see that table). DPI 1600, threshold 40, thumb
wheel inverted and free spin were set before the events. The kernel's HID
device name (`0005:046D:B034.NNNN`) shows whether the mouse reconnected.

| Event | File | Reconnected | DPI | Threshold | Thumb invert | Mode |
|---|---|---|---|---|---|---|
| set | `06a` | — | 1600 | 40 | on | free spin |
| Easy-Switch away and back | `06b` | **no** (`.0007` kept) | 1600 | 40 | on | free spin |
| switch off and on | `06c` | yes (`.0008`) | **1000** | **10** | **off** | free spin |
| 15 min idle | `06d` | no (`.0008` kept) | 1000 | 10 | off | free spin |
| `systemctl suspend` and wake | `06e` | yes (`.0009`) | 1000 | 10 | off | free spin |

| Fact | Status | Evidence |
|---|---|---|
| Switching the mouse off resets DPI and thumb inversion to the defaults | **CONFIRMED** | Two power cycles: `06c` (O-6), and H4B-05 (O-7), where the daemon's re-apply read DPI 1000 and inversion off before writing 1600 and on (`evidence/m4b/05-status.json`, `last_apply.items[].before`) |
| Switching the mouse off resets the threshold to 10 | **CONFIRMED** | Three power cycles, two values: `06c` (40 to 10), M4c A1 and M2 (30 to 10; `evidence/m4c/01-power.json`, `m2-after-power.json`, `items[].before`) |
| The SmartShift mode survives switching the mouse off | **CONFIRMED** | Both values: free spin in `06c` and M4c A1 (`01-power.json`), ratchet in M4c M2 (`m2-after-power.json`); `before` equals the wanted mode and nothing was written |
| A channel switch with a reconnect resets DPI, threshold and inversion; the mode stays | LIKELY | One run, M4c A3 (`03-channel.json`: before 1000, 10, off; mode free spin kept). `06b` had no reconnect and kept everything |
| A suspend that drops the link resets them; the mode stays | LIKELY | One run, M4c A4 (`04-suspend.json`, same values) |
| GNOME Bluetooth off and on resets them; the mode stays | LIKELY | One run, M4c A5 (`05-bluetooth.json`, same values) |
| A disconnect of about a second keeps them | HYPOTHESIS | After the reboot in M4c the mouse connected, disconnected and connected again with no second check between (under 1.1 s); the second apply read 1600, 30, on and wrote nothing (`06-reboot.json`). One observation; the gap is not timed |
| The mouse does not disconnect after 15 min idle | **CONFIRMED** | Two runs: `06d` (O-6) and M4c A2 (`02-before-idle.json` and `02-idle.json` show the same apply record; no disconnect in the journal) |
| The kernel's 0x2121 mode is `0x02` after every reconnect | LIKELY | `06c`, `06e`, consistent with R-1 |

### P-034 — Hotplug events over Bluetooth

Status: **CONFIRMED** (O-6, `01-hotplug.json`). Switching the mouse off,
Easy-Switch away and back, and turning GNOME's Bluetooth off and on each gave
one `remove` and then one `add` for `0005:046D:B034`, 13 to 20 s apart. Each
`add` already had access `ok` (the udev `uaccess` rule had run). The node
stayed `hidraw2`, while the HID device suffix grew by one on every reconnect
(`.0004` to `.0007`). The removed device's IDs were parsed from the event's
path.

### Observation O-7 (2026-10-22) — user's hardware tests H4B

The user ran section M4b of `docs/hardware-test.md` with `hid-labd` installed
as a systemd user unit. Captures are in `evidence/m4b/`: status JSON files,
the daemon journal, and the final `config.toml` and `state.toml`. They hold
no home path and no Bluetooth address; the two `lab read` files mask the
unit ID (`03 ** ** ** **`). The user ran H4B-05 twice: the first time
without switching the mouse off; the repeat set only DPI 1600 and inversion
on, with threshold and mode at their defaults. `05-status.json` is from the
repeat. The daemon was disabled and enabled again between runs, which the
journal shows as stop/start pairs.

| Check | Result |
|---|---|
| Runs as the user, no root (H4B-01) | user's report |
| `set` writes and verifies each of the four settings (H4B-02) | 4 × `applied 1 setting(s) ... verified` in the journal; `02-status.json` all equal to the wanted values |
| DPI 1625 rejected before sending, not saved (H4B-02) | journal `Rejected: 1625 DPI is not in the list` |
| Broken config: last valid kept, `set` refuses (H4B-03) | journal parse error; `03-status-broken.txt` shows DPI 800 in use |
| Applies at start (H4B-04) | journal `applied 4 setting(s) in 74 ms: verified` after the restart |
| Reconnect noticed (H4B-05) | journal `mouse disconnected`, then `connected` with `.0003` to `.0004` |
| Re-apply after the reconnect (H4B-05) | `05-status.json`: DPI and inversion written, mode and threshold unchanged, verified in 157 ms |
| `lab write` refuses, Solaar detected, second instance refused (H4B-06) | user's report; journal shows the `solaar` warning |
| Mouse back to the start | `06-after.json` equals `00-before.json`: DPI 1000, SmartShift `02 0a 0a`, thumb wheel `00 00` |

### Observation O-8 (2026-10-22) — first request after idle

Claude Code ran `hid-labd` from the 4c branch on a private session bus with
an empty config (reads only; the user's own daemon kept running), with
`HIDLAB_LOG=debug`. The mouse had not been touched for a while. The first
readiness ping (getProtocolVersion, ping `0xC5`, 250 ms timeout) got no
answer, and nothing else arrived. The retry 200 ms later was matched 17 ms
after sending. Both pings used the same ping byte, so the matched answer may
be a late answer to the first one. No file was kept. Status: UNKNOWN whether
the mouse is slow to answer after idle or the first request was lost; the
4c acceptance tests time every re-apply and will show whether this matters.

Update after O-9: in every M4c `connect` the first verification ended 211 to
258 ms later than the apply itself took (journal: `applied ... in N ms, M ms
after the event`), close to the 250 ms ping timeout; after the reboot the gap
was 119 and 121 ms. Without debug logs this does not tell whether the first
ping timed out. It stays UNKNOWN; every event still met the 2 s target.

### Observation O-9 (2026-11-02) — user's §7 acceptance tests M4c

The user ran section M4c of `docs/hardware-test.md` with the 4c `hid-labd`
installed as a systemd user unit. Captures are in `evidence/m4c/`: status
JSON files, the daemon journal (`--since today`, so it begins with stop/start
pairs and `run hid-lab reapply` lines from the 4b build that ran earlier that
day), and the final `config.toml` and `state.toml`. They hold no home path,
no Bluetooth address and no unit ID; nothing was sanitized. The user reported
that every setting came back by itself, felt correct, and `status` was all
`ok`. Kernel: `uname -r` was 7.2.13 when the session started and 7.2.14 after
the reboot in A6.

Wanted values during the events: DPI 1600, threshold 30, thumb wheel
inverted, free spin (ratchet from M1 on). "Read before" is DPI, mode,
threshold, inversion from `last_apply.report.items[].before`.

| Event | File | Trigger | Read before | First verification | Second check |
|---|---|---|---|---|---|
| Preparation (mouse asleep during the `set`s) | `00-configured.json` | `connect` | 1000, ratchet, 10, off | 631 ms | 1787 ms, nothing overwritten |
| A1 switch off and on | `01-power.json` | `connect` | 1000, **free spin**, 10, off | 580 ms | 1719 ms, nothing overwritten |
| A2 15 min idle | `02-idle.json` | none (no disconnect) | — | — | — |
| A3 Easy-Switch away, pointer stopped, back | `03-channel.json` | `connect` | 1000, free spin, 10, off | 602 ms | 1736 ms, nothing overwritten |
| A4 `systemctl suspend` | `04-suspend.json` | `connect` | 1000, free spin, 10, off | 586 ms | 1704 ms, nothing overwritten |
| A5 GNOME Bluetooth off and on | `05-bluetooth.json` | `connect` | 1000, free spin, 10, off | 550 ms | 1722 ms, nothing overwritten |
| A6 reboot | `06-reboot.json` | `connect` | 1600, free spin, 30, on | 239 ms | 1372 ms, nothing overwritten |
| M2 switch off and on | `m2-after-power.json` | `connect` | 1000, **ratchet**, 10, off | 573 ms | 1706 ms, nothing overwritten |
| M3 daemon stopped, button pressed, daemon started | `m3-restored.json` | `start` | 1600, free spin, 30, on | 663 ms | 1788 ms, nothing overwritten |

Other findings:

- A1: the journal shows one extra connect (`.0005`) applied and verified in
  527 ms, then a disconnect before its second check. The second check was
  dropped as designed, and the next connect (`.0006`) is the one in
  `01-power.json`. The cause of the short connection is not known.
- A4: the journal shows `system going to sleep`, `mouse disconnected`,
  `system resumed`, then the reconnect. The mouse was gone at wake-up, so the
  `resume` trigger itself was not exercised on hardware.
- A6: the daemon started before the mouse connected, so the trigger was
  `connect`. The first connect (383 ms, no second check) was followed by a
  quick disconnect and reconnect; `06-reboot.json` holds the second one,
  which found everything already set. The HID device suffix was `.0003` both
  times.
- M1: journal `SmartShift mode changed on the mouse to ratchet; remembered`;
  `state.toml` changed to `ratchet` (mode 2 in M2's `desired`).
- M3: only the mode was written; the other three were `unchanged`.
- The slowest event took 1788 ms from the event to the end of the second
  check, 212 ms under the 2 s target. Most of it is the fixed 1 s wait.
- Back to normal: each `set` verified 70 to 247 ms after the call.

## Milestone 5b — SmartShift off, button remapping, battery warnings

Sources read 2026-11-02: R-1 and R-2 at the pinned commits above, R-3 at
tag `50.1`.

### P-035 — 0x2110 threshold 255 keeps the wheel ratcheted

Source: R-2 `SmartShift` (setting "Scroll Wheel Ratchet Speed": "The mouse
wheel is always ratcheted at 50"; its write maps 50 and above to 255 and
sends `[00, 255]` through f1). R-1 does not use 0x2110.

```text
f1 setRatchetControlMode(00, ff)   mode unchanged, threshold 255 = no automatic switch
```

| Fact | Status | Evidence |
|---|---|---|
| `[00, ff]` stores threshold 255 and keeps the mode | **CONFIRMED** | O-10: `01-off.json` read back 255 with mode 2 unchanged; `01-restore.json` wrote 10 back the same way |
| At 255 a hard push never switches the ratcheted wheel to free spin | **CONFIRMED** | O-10, user: the wheel kept clicking at 255 and freed again at 10 |
| The mode button still switches modes at 255 | **CONFIRMED** | O-10, user (H5-01 step 4) |

The daemon starts sending 255 in phase 5c, when the config can hold it
(`SettingValue::confirmed` gates it until then).

### P-036 — 0x1B04 setCidReporting (f3), remap only

Sources: R-2 `ReprogrammableKeyV4._setCidReporting`, `remap`,
`remappable_to`; R-1 `hidpp20_reprog_controls_set_control_reporting` (same
function and CID/flags layout; R-1 sends it only to divert buttons of the
Signature M650, not to this mouse).

```text
f3 setCidReporting(cid u16 BE, flags, remap u16 BE)
   flags 0      = change no reporting flag (nothing diverted)
   remap 0      = keep the current mapping
   remap = cid  = back to the control's own task
   remap = X    = the control performs X's native task (not recursive)
```

R-2 sends these 5 bytes; the rest of the long report is zero (R-2 notes
that a sixth byte exists in version 4 and leaves it out). R-2 calls the
remap temporary. A control may be remapped only when its group mask is not
0, to itself or to a control whose group is in its mask (bit `n` = group
`n + 1`).

Groups and masks read from this mouse (`mx-master-3s-state.json`, the same
in every 0x1B04 read since O-4, and in the M5b dry runs):

| CID | Reference name | Group | Mask | Allowed targets |
|---|---|---|---|---|
| 0x0050 | Left_Button | 1 | 0x01 | left, right |
| 0x0051 | Right_Button | 1 | 0x01 | left, right |
| 0x0052, 0x0053, 0x0056, 0x00C3, 0x00C4 | Middle, Back, Forward, Mouse_Gesture, Smart_Shift | 2 | 0x03 | the 7 physical controls |
| 0x00D7 | Virtual_Gesture_Button | 3 | 0x00 | none |

| Fact | Status | Evidence |
|---|---|---|
| Group and mask values above | **CONFIRMED** | Seven identical reads (O-4, O-5, M5b dry runs) |
| The f3 layout remaps a control; flags 0 changes no reporting flag | **CONFIRMED** | O-10: four controls (back, forward, mode, gesture) and three targets (middle, back, forward), each read back with reporting flags `0x0000`; the user felt back, forward and mode do the target's task |
| The answer echoes the 5 parameters | **CONFIRMED** | All 11 writes in O-10 |
| A remap is read back by f2 as `mapped_to`; `remap = cid` restores it | **CONFIRMED** | All 11 writes and restores in O-10 |
| A remap is lost when the mouse is switched off | LIKELY | One power cycle (O-10, `06b-after-power.json`: back mapped to itself after `.0004` to `.0005`); the user thinks it was lost |

`lab write` refuses targets outside the mask and always sends flags `00`.
The tests also confirm which physical button each CID is (P-024, LIKELY).

### S-1 — GNOME already warns when the mouse battery is low

Source: R-3. `engine_device_warning_changed_cb` reacts to UPower's
`warning-level` for every device and, at `low` and `critical`, shows a
notification; the mouse kind has its own text ("Mouse battery low"). On the
user's machine UPower lists the MX Master 3S (`upower -i`, 2026-11-02: kind
mouse, 40 %, warning-level none; serial and path not recorded), fed by the
kernel's `hidpp_battery`.

| Fact | Status | Evidence |
|---|---|---|
| UPower reports this mouse's battery | **CONFIRMED** | `upower -e` and `upower -i`, user's machine |
| GNOME shows a low-battery notification for it | LIKELY | R-3 and the UPower entry; not seen, the battery was at 40 % |

Decision: Cursiva does not send its own low-battery notification
(requirements decision log, 2026-11-02).

### Observation O-10 (2026-11-02) — user's hardware tests H5

The user ran section M5b of `docs/hardware-test.md` with the daemon
stopped. Captures are in `evidence/m5b/`: the `lab write` JSON of every
write and restore, three `lab read` files and `log.jsonl`. The Bluetooth
address and the unit ID are redacted by the tool (`<redacted>`, `** ** ** **`
in the transcripts); no home path; nothing else was sanitized. The user
reported that everything behaved as the tests describe.

| Test | Bytes sent (f, params) | Answer | Read back | What the user felt |
|---|---|---|---|---|
| H5-01 off | 0x2110 f1 `00 ff` | `00 ff` | threshold 255, mode 2 | Wheel kept clicking on hard pushes; mode button still switched |
| H5-01 restore | 0x2110 f1 `00 0a` | `00 0a` | threshold 10, mode 2 | Hard push freed the wheel again |
| H5-02 back to middle | 0x1B04 f3 `00 53 00 00 52` | same | mapped_to 0x0052, flags 0 | Back opened links in a new tab |
| H5-02 restore | `00 53 00 00 53` | same | 0x0053 | Back went back |
| H5-03 forward to back | `00 56 00 00 53` | same | 0x0053 | Forward went back |
| H5-03 restore | `00 56 00 00 56` | same | 0x0056 | Forward went forward |
| H5-04 mode to middle | `00 c4 00 00 52` | same | 0x0052 | Mode button opened links in a new tab; the wheel did not change mode |
| H5-04 restore | `00 c4 00 00 c4` | same | 0x00C4 | Mode button switched modes |
| H5-05 gesture to forward | `00 c3 00 00 56` | same | 0x0056 | Went forward, as it already did before the remap |
| H5-05 restore | `00 c3 00 00 c3` | same | 0x00C3 | — |
| H5-06 back to middle | `00 53 00 00 52` | same | 0x0052 | — |
| H5-06 after power cycle | (read) | — | back mapped to 0x0053 (itself), HID device `.0004` to `.0005` | User thinks the remap was lost |
| H5-06 restore | none, `no_change` | — | — | — |

Other findings:

- Unremapped, the gesture button made the browser go forward (H5-05 step
  1). Which input event it sends is UNKNOWN; one observation.
- At the end `lab-write/` held only `log.jsonl` (every original restored),
  the daemon started and `hid-lab status` showed the four settings `ok`.

