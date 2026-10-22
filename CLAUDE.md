# CLAUDE.md — hid-lab

Instructions for Claude Code working in this repository. Read this file fully before every task.

## What this project is

**hid-lab** is a native Linux app + background service to configure a **Logitech MX Master 3S** on **Fedora 44 / GNOME / Wayland** as easily as Logi Options+ does on Windows, without losing functionality. Written in **Rust (stable)**, licensed **GPL-3.0-or-later**.

- Requirements, scope and milestones: `docs/requirements.md` (source of truth; see its §22 decision log).
- Protocol evidence log: `docs/protocol/README.md`, with captures in `docs/protocol/` and `docs/protocol/evidence/`.
- Hardware test checklists and results: `docs/hardware-test.md`.
- Milestone 0 diagnosis: `docs/diagnosis.md`.
- udev rule: `packaging/udev/70-hid-lab-logitech.rules`.

Names: the desktop app is **Cursiva** (app ID `dev.ozkkar.Cursiva`); the daemon's D-Bus name is `dev.ozkkar.Cursiva.Daemon1`. The CLI (`hid-lab`) and the crates (`hidlab-*`) keep the hid-lab names.

The user communicates in **Spanish**. Reply in Spanish; code, comments, commit messages and docs in English.

## Project state

| Milestone | Status |
|---|---|
| 0 Diagnosis | Done. Cause: missing `uaccess` udev rule |
| 1 + 2 Discovery, HID++ core, feature enumeration | Done, framing CONFIRMED |
| 3 Read-only features | Done, hardware tests H3 passed (observation O-5) |
| 4 Hotplug, `lab write`, daemon, re-application (Bluetooth only) | Approved. 4a in progress; wait for H4 results before 4b; propose SmartShift-mode details before 4c |
| B Logi Bolt receiver | Scheduled after 4; not part of the MVP (Bluetooth only) |
| 5–8 | Not started; do not start without approval |

## Known facts about the target (CONFIRMED on the user's machine)

- Fedora 44, kernel `7.2.11-200.fc44.x86_64`, BlueZ 5.87, GNOME on Wayland.
- The mouse is used over **Bluetooth**. Bluetooth HID ID `0005:046D:B034`, via `uhid`.
- The hidraw number is **not stable**. Locate the device by bus/vendor/product from sysfs, never by `/dev/hidrawN`.
- Kernel driver `logitech-hidpp-device` stays bound; hid-lab coexists with it (CONFIRMED, P-014). The kernel **writes** the 0x2121 wheel mode (high resolution on) when the mouse connects; hid-lab must not write 0x2121.
- HID++ protocol 4.5. Over Bluetooth the mouse declares only the **long** report `0x11`; device index `0xFF`.
- 35 features plus root; indices are assigned by the device and must be resolved with getFeature at runtime.
- The kernel exposes the battery as `/sys/class/power_supply/hidpp_battery_N`; the name changes on reconnect.
- Main-wheel scroll direction is GNOME's natural-scrolling setting, not a device write.

## Workspace

```text
crates/
├── hidlab-transport/   sysfs discovery, report-descriptor walker, hidraw I/O (poll), power_supply lookup
├── hidlab-hidpp/       generic HID++ 2.0: framing, request/response matching, transcript, enumeration
├── hidlab-features/    typed read-only feature access behind an allowlist (crates/hidlab-features/src/allowlist.rs)
└── hidlab-cli/         binary `hid-lab`
```

Dependency direction is mandatory: `cli → features → hidpp → transport`. `hidpp` and `features` are generic HID++; device-specific knowledge goes in a future `hidlab-devices` crate (requirements §5).

Commands today:

```bash
hid-lab lab devices  [--json] [--show-unique]
hid-lab lab features [<device>] [--json] [--index <n>] [--timeout-ms <ms>] [--show-unique]
hid-lab lab read     [<device>] [--json] [--index <n>] [--timeout-ms <ms>] [--show-unique]
hid-lab lab watch    [<device>] [--duration <s>] [--json] [--features-from <file>]
```

Dependencies: `libc`, `serde`, `serde_json`, `thiserror`; approved for Milestone 4: `udev` (needs `systemd-devel` to build), `zbus`, `toml`. Explain why before adding any other crate, and verify its current version with `cargo info`.

## Protocol work

- Every protocol fact goes in `docs/protocol/README.md` with status CONFIRMED / LIKELY / HYPOTHESIS / UNKNOWN, its source and its evidence. CONFIRMED needs the real device, more than one value where possible, and reproducibility.
- References: Linux `drivers/hid/hid-logitech-hidpp.c` (GPL-2.0-only) and Solaar (GPL-2.0-or-later), pinned to the commits listed in the README. Describe behavior; never copy code. Download sources and read them; do not rely on memory or on summaries.
- A function that no source documents is never called: it could be a write.
- Feature and control names from references are shown as "reference name", marked as not verified. Unknown IDs print as unknown.
- Never fabricate device data. Test fixtures are labeled SYNTHETIC.
- When recording an observation, state only what the data shows. Check counts (for example ignored reports) in the files before describing conditions.

## Writes

- Milestone 3 and earlier send reads only, gated by the read allowlist.
- The daemon may only write with CONFIRMED write functions.
- The only exception is `hid-lab lab write` (requirements §13): explicit `--write`, `--dry-run`, target device shown, original value read and saved, every byte logged, read-back comparison, `--restore`. One write function at a time, each with a physical test the user runs.
- Never write the 0x2121 wheel mode. Never call 0x1814 setCurrentHost.

## Privacy

- `HID_UNIQ` (Bluetooth address) and the firmware unit ID (0x0003 getDeviceInfo bytes 1..5) are redacted by default everywhere: decoded output, transcripts, and other programs' traffic captured by `lab watch`.
- Before committing user captures: check for the unit ID in every hex form, replace home paths with `~`, and record any sanitizing in the file.

## Non-negotiable rules

1. Read `docs/requirements.md` before designing anything new.
2. Small, focused commits (`chore:`, `feat:`, `fix:`, `refactor:`, `docs:`, `test:`). One concern per commit.
3. After every change: `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`.
4. No root at runtime. Never suggest running hid-lab with sudo as a workaround.
5. Disconnects and reconnects are normal events. No panics on expected failures, including a closed stdout pipe.
6. Errors are actionable: what happened, likely cause, exact command to try.
7. Keep `docs/` and this file in sync with behavior.
8. Propose a plan and wait for approval before each milestone or phase.
9. Ask the user only when a decision cannot be inferred from this file or `docs/requirements.md`.
