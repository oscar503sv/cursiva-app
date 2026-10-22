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
| Device notifications carry `sw_id = 0` | LIKELY | R-1, R-2. The 48 ignored reports in O-3 were not decoded one by one |

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
(pointer movement), and no unrelated HID++ report. All requests still
matched. A separate text-mode run a moment earlier ignored 0 reports.
Correction: an earlier version of this entry said the mouse was not touched;
the user pointed out the 36 ignored reports in the evidence file. The evidence file is
`mx-master-3s-state.json`. Values below marked "O-4" come from it. No
physical action was performed, so nothing here is CONFIRMED yet. The
promotion tests are listed in `docs/hardware-test.md`, section M3.

### P-020 — 0x0005 DeviceName

Status: **LIKELY**. Sources: R-1 (GET_COUNT, GET_DEVICE_NAME, GET_TYPE), R-2 (`get_name`, `get_kind`, `DEVICE_KIND`).

```text
f0 getDeviceNameCount()       -> [length]
f1 getDeviceName(char_index)  -> up to 16 name bytes per long report; repeat until length
f2 getDeviceType()            -> [type]   R-2 table: 0 keyboard ... 3 mouse ... 7 receiver
```

O-4: length 12, name `MX Master 3S`, type 3. Test: H3-01.

### P-021 — 0x0003 DeviceFwVersion

Status: **LIKELY**. Sources: R-2 (`get_firmware`, `get_ids`, `FirmwareKind`), R-1 (`hidpp_get_serial`: bytes 1..5 of f0 are the serial).

```text
f0 getDeviceInfo()   -> [entity_count, unit_id[4], byte5?, transport_bits, model_ids[6], ...]
                        transport bits 0x01 bt, 0x02 btle, 0x04 wpid, 0x08 usb;
                        each set bit consumes 2 bytes of model_ids, in that order
f1 getFwInfo(entity) -> [type, ...]   type & 0x0F: 0 firmware, 1 bootloader, 2 hardware, 3 other
   types 0 and 1: [type, name[3], major, minor, build u16 BE, byte8?, extras...]
   type 2:        [type, hardware_version]
```

The unit ID is a serial number. It is redacted by default in decoded output
and in the transcript; `--show-unique` shows it.

O-4: 3 entities; transport bits `0x02` with model ID `btle=B034`, which equals
the Bluetooth product ID in `HID_ID`. Entity 0: bootloader `BL1` 69.01 build
`0x0006`. Entity 1: firmware `RBM` 22.01 build `0x0006`. Both have extras
`b0 34 bb 51 90 e8`; meaning UNKNOWN, checked not to contain the unit ID or
the Bluetooth address. Entity 2: type byte `0x05`, not in R-2's table
(UNKNOWN), raw `05 00 00 00 00 00 01 26`. Byte 5 and byte 8: UNKNOWN.
Test: H3-02.

### P-022 — 0x1004 UnifiedBattery

Status: **LIKELY**, with a conflict. Sources: R-1 (GET_CAPABILITIES, GET_STATUS, level and flag bits, status mapping), R-2 (`decipher_battery_unified`).

```text
f0 getCapabilities() -> [supported_levels, flags]   flags bit0 rechargeable, bit1 state_of_charge
f1 getStatus()       -> [soc_%, level_bits, charging_status, external_power]
                        level bits: 0x01 critical, 0x02 low, 0x04 good, 0x08 full
```

| charging_status | R-1 kernel | R-2 Solaar | Status |
|---|---|---|---|
| 0 | discharging | discharging | LIKELY |
| 1 | charging | recharging | LIKELY |
| 2 | charging slow | almost full | HYPOTHESIS (references conflict) |
| 3 | complete | full | LIKELY |
| 4 | error | slow recharge | HYPOTHESIS (references conflict) |

`external_power` (byte 3) is passed along by R-1 but not interpreted by either
reference: UNKNOWN.

The kernel reports the same feature through
`/sys/class/power_supply/hidpp_battery_N`, whose `device` link is the HID
device of the mouse. `lab read` compares both. This checks hid-lab's decoding
against the kernel's, not against an independent sensor.

O-4: capabilities all four levels, flags rechargeable and state_of_charge;
status 35 %, level good, charging 0, external power 0. Kernel
`hidpp_battery_0`: capacity 35, Discharging. Match. Tests: H3-03, H3-04.

### P-023 — 0x1814 ChangeHost

Status: **LIKELY**. Source: R-2 (`ChangeHost` setting, read f0). f1 (setCurrentHost) is a write that switches the mouse away; it is never called.

```text
f0 getHostInfo() -> [host_count, current_host (0-based), ...]
```

O-4: 3 hosts, current host 1, shown as channel 2. Bytes 2.. were zero.
Test: H3-05.

### P-024 — 0x1B04 ReprogControlsV4

Status: **LIKELY**. Sources: R-1 (GET_COUNT, GET_CID_INFO, flag bit0 mouse, bit5 divertable), R-2 (`KeysArrayV4._query_key`, `_getCidReporting`, `KeyFlag`, `MappingFlag`, `special_keys.CONTROL`, `Task`). f3 (setCidReporting) is never called.

```text
f0 getCount()             -> [count]
f1 getCidInfo(index)      -> [cid u16 BE, tid u16 BE, flags_lo, pos, group, gmask, flags_hi]
                             flags = flags_lo | flags_hi << 8 (R-2 KeyFlag names)
f2 getCidReporting(cid BE)-> [cid u16 BE, mflags_lo, mapped_to u16 BE, mflags_hi]
                             mflags = mflags_lo | mflags_hi << 8 (R-2 MappingFlag names);
                             mapped_to 0 means "itself"
```

O-4 (reference names from R-2, not verified):

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

Every control reported itself as its mapping target and no reporting flag was
set, which fits a mouse that no software has diverted. Test: H3-06.

### P-025 — 0x2201 AdjustableDpi

Status: **LIKELY**. Source: R-2 only (`AdjustableDpi`: list f1, read f2; `produce_dpi_list`). f0 is not documented and not called. f3 (setSensorDpi) is never called.

```text
f1 getSensorDpiList(sensor 0) -> [sensor, u16 BE values..., 0x0000]
                                 a value with top 3 bits 111 is a step (value & 0x1FFF);
                                 the range runs from the previous value to the next one
f2 getSensorDpi()             -> [sensor, dpi u16 BE, default_dpi u16 BE]; dpi 0 = default
```

Only the first list page is read. O-4: list `200..8000 step 50`, terminated;
current 1000, default 1000. Test: H3-07.

### P-026 — 0x2110 SmartShift

Status: **LIKELY**. Source: R-2 only (`ScrollRatchet`, `SmartShift`: read f0). f1 is a write and is never called.

```text
f0 getRatchetControlMode() -> [mode, threshold, byte2]   mode 1 free-spin, 2 ratchet
```

O-4: mode 2 (ratchet), threshold 10, byte 2 `0x0a`. Byte 2 is UNKNOWN; it
equalled the threshold in O-4, which is an observation, not a meaning.
Test: H3-08.

### P-027 — 0x2121 HiResWheel

Status: **LIKELY**. Sources: R-1 (f0 multiplier; f2 setWheelMode bits, written by the kernel on connect with high resolution on), R-2 (`get_hires_wheel`: f0, f1, f3).

```text
f0 getWheelCapability()    -> [multiplier, flags]   0x08 has invert, 0x04 has ratchet switch
f1 getWheelMode()          -> [mode]   bit0 diverted to HID++, bit1 high resolution, bit2 inverted
f3 getRatchetSwitchState() -> [state]  bit0 ratchet
```

hid-lab never calls f2. The kernel owns this mode; later milestones must not
change it without coordinating with the kernel driver.

O-4: multiplier 15, has invert, has ratchet switch; mode `0x02` (high
resolution, as the kernel sets it); ratchet switch `0x01`, consistent with
SmartShift mode 2 at the same moment. Test: H3-08.

### P-028 — 0x2150 Thumbwheel

Status: **LIKELY**. Source: R-2 only (`ThumbMode`, `ThumbInvert`: read f1). f0 is not documented and not called. f2 is a write and is never called.

```text
f1 getThumbwheelStatus() -> [b0, b1, ...]   b0 bit0 diverted, b1 bit0 inverted
```

O-4: `00 00`, not diverted, not inverted. With reads only, only the default
can be observed; inversion stays LIKELY until a later milestone writes it.
Test: H3-10.

### P-029 — Notifications (software ID 0)

Status: **LIKELY** that the device sends HID++ reports with `sw_id 0`
unprompted (R-1, R-2). R-1 handles 0x1004 event 0 as a battery status
update with the getStatus layout. `hid-lab lab watch` listens without sending
and shows each such report with its feature index and raw bytes. No payload
meaning is assumed. Tests: H3-11, H3-12.
