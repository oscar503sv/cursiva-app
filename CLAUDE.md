# CLAUDE.md — hid-lab

Instructions for Claude Code working in this repository. Read this file fully before every task.

## What this project is

**hid-lab** is a native Linux app + background service to configure a **Logitech MX Master 3S** on **Fedora 44 / GNOME / Wayland** as easily as Logi Options+ does on Windows, without losing functionality. Written in **Rust (stable)**.

- Full requirements: `docs/requirements.md` (source of truth for scope, architecture and milestones).
- Evidence so far: `docs/diagnosis.md`.
- udev rule: `packaging/udev/70-hid-lab-logitech.rules`.

The user communicates in **Spanish**. Reply in Spanish; code, comments, commit messages and docs in English.

## Known facts about the target (CONFIRMED on the user's machine)

- Fedora 44, kernel `7.2.11-200.fc44.x86_64`, BlueZ 5.87, GNOME on Wayland.
- The mouse is used over **Bluetooth** (primary). Bolt receiver is secondary.
- Bluetooth HID ID: `0005:046D:B034` (bus 0005 = Bluetooth, via `uhid`).
- Appears as `/dev/hidraw2` today, but the number is **not stable**. Always locate the device by bus/vendor/product from sysfs, never by `/dev/hidrawN`.
- Kernel driver `logitech-hidpp-device` is bound and reports `HID++ 4.5 device connected`. hid-lab must **coexist** with it.
- Without the udev rule, `/dev/hidraw*` is `root:root 0600` and the user cannot open it. This was the cause of "nothing can be configured".

## Current milestone: 1 + 2 (discovery and HID++ feature enumeration)

Goal: two commands that work as a normal user over Bluetooth.

```bash
hid-lab lab devices [--json] [--show-unique]
hid-lab lab features [<device>] [--json] [--index <n>] [--timeout-ms <ms>]
```

`lab features` is the first real evidence of the project: it must list every HID++ feature the MX Master 3S reports.

### Workspace (start with these three crates only)

```text
hid-lab/
├── Cargo.toml                 workspace, edition 2024, resolver 3
├── crates/
│   ├── hidlab-transport/      sysfs discovery, report-descriptor walker, hidraw I/O
│   ├── hidlab-hidpp/          generic HID++ 2.0: framing, request/response, enumeration
│   └── hidlab-cli/            binary `hid-lab`
├── docs/
│   ├── requirements.md
│   ├── diagnosis.md
│   └── protocol/README.md     evidence log for every protocol fact
└── packaging/udev/70-hid-lab-logitech.rules
```

Dependency direction is mandatory: `cli → hidpp → transport`. `hidpp` knows nothing about the MX Master 3S.

### Dependencies (keep it to these unless you explain why)

| Crate | Why |
|---|---|
| `libc` | `poll(2)` for read timeouts on `/dev/hidraw*` |
| `serde` (derive) + `serde_json` | stable `--json` output |
| `thiserror` | typed errors |

Verify current versions with `cargo add`. A hand-written argument parser is fine for now; if you prefer `clap`, explain why first.

### hidlab-transport

- `discover()` reads `/sys/class/hidraw/*`; `discover_in(root)` takes a root path so tests can use a fake sysfs tree in a temp dir.
- From `device/uevent`: parse `HID_ID=BBBB:VVVVVVVV:PPPPPPPP`, `HID_NAME`, `HID_UNIQ`. Bus `0003` = USB, `0005` = Bluetooth.
- Driver name from the `device/driver` symlink.
- Read `device/report_descriptor` and walk it with a small HID item parser (short items: `tag(4)|type(2)|size(2)`, size code 3 = 4 bytes; long item prefix `0xFE`). Collect report IDs (global tag 8) and usage pages (global tag 0). Never panic on malformed input; report `truncated`.
- A device "looks like HID++" if its descriptor declares report ID `0x10` and/or `0x11`.
- Access check: try to open read+write; map `PermissionDenied` to an actionable error that explains the udev rule and the exact commands to install it.
- `HID_UNIQ` (Bluetooth address / serial) is **redacted by default**; shown only with `--show-unique`.
- `trait RawChannel { send(&mut self, &[u8]); recv(&mut self, &mut [u8], Duration) -> Option<usize> }` and a `Hidraw` implementation using `poll`. Disconnects (`ENODEV`, missing node) are a distinct error, not a panic.

### hidlab-hidpp

The following come from public references (Linux kernel `drivers/hid/hid-logitech-hidpp.c`, Solaar — both GPL-2.0). Treat them as **LIKELY hypotheses**: implement them, but record them in `docs/protocol/README.md` with status LIKELY and upgrade to CONFIRMED only after the user runs the command on the real mouse. **Do not copy code** from those projects; implement from the described behavior and cite the source.

- Short report: ID `0x10`, 7 bytes. Long report: ID `0x11`, 20 bytes. Prefer **long** reports if the descriptor declares `0x11`.
- Layout: `[report_id, device_index, feature_index, (function << 4) | sw_id, params...]`.
- Device index `0xFF` for a directly connected device (Bluetooth). For a receiver, the paired device index (1..6), selected with `--index`.
- Root feature is always at feature index `0x00`:
  - function 0 `getFeature(feature_id: u16 BE)` → `[feature_index, feature_type, feature_version]` (index 0 = not supported);
  - function 1 `getProtocolVersion(0, 0, ping)` → `[major, minor, ping]`; verify the ping echo.
- Feature `0x0001` (IFeatureSet), located with `getFeature`:
  - function 0 `getCount` → number of features (excluding root);
  - function 1 `getFeatureID(index)` → `[id_hi, id_lo, type, version]`.
- HID++ 2.0 error: `feature_index == 0xFF`, params `[orig_feature_index, orig_function_swid, error_code]`. Also handle the HID++ 1.0 error form (sub-ID `0x8F`).
- Use a non-zero `sw_id` (e.g. `0x0B`). Notifications use `sw_id 0`.

**Coexistence rules** (the kernel driver and normal mouse input share the node):

- After sending, read until deadline and **ignore** anything that does not match `device_index`, `feature_index`, function and `sw_id`: mouse movement reports, notifications, and the kernel's own HID++ traffic. Count what was ignored.
- Timeout → typed error with a plain-language message.
- Record a **transcript** of every TX/RX (hex) for each request, included in `--json` output. This is the raw evidence.

Feature names may be shown from a small table as "reference name", clearly labeled as not verified: 0x0000 Root, 0x0001 FeatureSet, 0x0003 DeviceFwVersion, 0x0005 DeviceName, 0x1004 UnifiedBattery, 0x1814 ChangeHost, 0x1B04 ReprogControlsV4, 0x2110 SmartShift, 0x2111 SmartShiftEnhanced, 0x2121 HiResWheel, 0x2150 Thumbwheel, 0x2201 AdjustableDpi. Unknown IDs print as unknown, never guessed.

### hidlab-cli

- `lab devices`: table `# | node | bus | VID:PID | driver | HID++ | access | name`, Logitech first. `--json` with stable field names.
- `lab features [<device>]`: device selector accepts `hidrawN`, `/dev/hidrawN`, or the `#` from `lab devices`; if omitted, pick the only Logitech HID++ device, or ask the user to choose if there are several. Prints protocol version, then `index | feature ID | reference name | type flags | version`. `--json` includes the transcript.
- These commands send **read-only queries** (getFeature, getProtocolVersion, getCount, getFeatureID). No setting is changed. State this in `--help`.
- Errors are actionable: what happened, likely cause, exact command to try. No panics on expected failures.

### Tests (no hardware needed)

- Unit tests: uevent parsing, descriptor walker (synthetic descriptors, labeled as synthetic), frame encode/decode, error decoding.
- A **fake HID++ device** implementing `RawChannel` that answers root/IFeatureSet from a table and injects noise: mouse reports (ID 0x02), notifications (`sw_id 0`), responses with a different `sw_id`, and error responses. Test that enumeration still returns the right list.
- Fake sysfs tree for `discover_in`.
- Never fabricate "real" MX Master 3S data. Fixtures are synthetic until the user provides real output.

## Non-negotiable rules

1. Read `docs/requirements.md` before designing anything new.
2. Small, focused commits (`chore:`, `feat:`, `docs:`, `test:`). One concern per commit.
3. After every change: `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`.
4. **Never invent protocol details.** Every protocol fact goes into `docs/protocol/README.md` with status CONFIRMED / LIKELY / HYPOTHESIS / UNKNOWN, its source, and its evidence.
5. **No writes that change device settings** in this milestone. Later, writes may only use CONFIRMED findings, require explicit intent, and log every byte.
6. No root at runtime. Never suggest running hid-lab with sudo as a workaround.
7. Disconnects and reconnects are normal events.
8. Keep `docs/` in sync with behavior.
9. Ask the user only when a decision cannot be inferred from this file or `docs/requirements.md`.

## Definition of done for this milestone

On the user's Fedora, as a normal user, with the mouse on Bluetooth:

1. `cargo build --release` succeeds.
2. `./target/release/hid-lab lab devices` lists the MX Master 3S as Bluetooth, HID++ yes, access ok.
3. `./target/release/hid-lab lab features` prints the protocol version and the full feature list.
4. `--json` output of step 3 is saved to `docs/protocol/mx-master-3s-features.json` (with `HID_UNIQ` redacted) and summarized in `docs/protocol/README.md`, upgrading the framing facts above from LIKELY to CONFIRMED.
5. `docs/diagnosis.md` updated: T-001 result, and T-002 replaced by the output of `hid-lab lab features`.
6. All tests pass without hardware.

## Next milestones (do not start without the user's approval)

3. Read-only features: battery, DPI, SmartShift, wheel, thumb wheel, firmware.
4. systemd user daemon + D-Bus + config + first CONFIRMED writes + re-application on reconnect/sleep.
5. GTK4 + Libadwaita GUI (mockups approved first).
