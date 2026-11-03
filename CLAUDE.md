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

Repository: `git@github.com:oscar503sv/cursiva-app.git` (https://github.com/oscar503sv/cursiva-app). The project is meant to be public and open to contributors; packaging targets are RPM and, for the GUI, possibly Flathub. Project contact email (metadata, AppStream, User-Agent): `oscar@ozkkar.dev`. Git commit author email: `ozkar@outlook.com` (the address GitHub links to the user's account). Never put any other email in the repository or in requests.

The user communicates in **Spanish**. Reply in Spanish; code, comments, commit messages and docs in English.

## Project state

| Milestone | Status |
|---|---|
| 0 Diagnosis | Done. Cause: missing `uaccess` udev rule |
| 1 + 2 Discovery, HID++ core, feature enumeration | Done, framing CONFIRMED |
| 3 Read-only features | Done, hardware tests H3 passed (observation O-5) |
| 4 Hotplug, `lab write`, daemon, re-application (Bluetooth only) | Done. 4a (H4, O-6), 4b (H4B, O-7), 4c (§7 acceptance M4c, O-9) |
| B Logi Bolt receiver | Scheduled after 4; not part of the MVP (Bluetooth only) |
| 5 GUI MVP | Approved 2026-11-02 (plan: phases 5a–5h, see requirements Milestone 5). 5a mockups approved (`docs/mockups/`). 5b done (H5, O-10: threshold `off` and 0x1B04 remap CONFIRMED). 5c daemon API v2 done (H5C, O-12). 5d GTK app skeleton done (`cursiva`, H5D passed). 5e Pointer & scrolling page done (H5E passed after two slider fixes). Next: 5f Buttons page and action picker |
| 6–8 | Not started; do not start without approval |

## Known facts about the target (CONFIRMED on the user's machine)

- Fedora 44, kernel `7.2.11-200.fc44.x86_64` at Milestone 0 and `7.2.14-200.fc44.x86_64` at M4c, BlueZ 5.87, GNOME on Wayland.
- The mouse is used over **Bluetooth**. Bluetooth HID ID `0005:046D:B034`, via `uhid`.
- The hidraw number is **not stable**. Locate the device by bus/vendor/product from sysfs, never by `/dev/hidrawN`.
- Kernel driver `logitech-hidpp-device` stays bound; hid-lab coexists with it (CONFIRMED, P-014). The kernel **writes** the 0x2121 wheel mode (high resolution on) when the mouse connects; hid-lab must not write 0x2121.
- HID++ protocol 4.5. Over Bluetooth the mouse declares only the **long** report `0x11`; device index `0xFF`.
- 35 features plus root; indices are assigned by the device and must be resolved with getFeature at runtime.
- The kernel exposes the battery as `/sys/class/power_supply/hidpp_battery_N`; the name changes on reconnect.
- Main-wheel scroll direction is GNOME's natural-scrolling setting, not a device write.
- Write functions CONFIRMED by H4: 0x2201 setSensorDpi, 0x2110 setRatchetControlMode (mode and threshold; `00` in the other byte means no change), 0x2150 setThumbwheelReporting.
- Input report `0x02` carries 16 button bits, then X/Y (12 bits each), wheel and AC Pan (T-2, from the descriptor). Buttons 1–5 are left, right, middle, back, forward and 6 is gesture; the mode button sets no bit (LIKELY, H5C-05).
- CONFIRMED by H5: threshold 255 keeps the wheel ratcheted (SmartShift off); 0x1B04 setCidReporting `[cid, 00, target]` remaps a button to another control's task, read back by f2 `mapped_to`. A remap is lost on power-off (LIKELY). Unremapped, the gesture button acts as forward (one observation).
- A 0x2110 mode write, like the mode button, produces a 0x2121 f1 notification. Automatic switching by the threshold does not.
- Switching the mouse off resets DPI, threshold and thumb-wheel inversion to 1000, 10 and off, and keeps the SmartShift mode (CONFIRMED, P-033). A channel switch, a suspend or GNOME Bluetooth off/on that reconnects does the same (LIKELY, one run each).

## Workspace

```text
crates/
├── hidlab-api/         D-Bus names, JSON types, settings vocabulary (serde only)
├── hidlab-transport/   sysfs discovery, report-descriptor walker, hidraw I/O (poll), power_supply lookup
├── hidlab-hidpp/       generic HID++ 2.0: framing, request/response matching, transcript, enumeration
├── hidlab-features/    typed feature access behind allowlists (crates/hidlab-features/src/allowlist.rs); `settings` plans, sends and verifies writes
├── hidlab-devices/     device profiles: MX Master 3S over Bluetooth and the settings it accepts
├── hidlab-daemon/      binary `hid-labd` + lib: config, state, apply engine, D-Bus service
├── hidlab-cli/         binary `hid-lab` (user commands via D-Bus; lab commands direct)
└── hidlab-gtk/         binary `cursiva`: GTK4 + Libadwaita app, D-Bus client through gio, mouse illustration (GskPath)
data/                   .desktop, AppStream metainfo, icons (app ID dev.ozkkar.Cursiva)
po/                     gettext domain `cursiva`: POTFILES, cursiva.pot, es.po (procedure in CONTRIBUTING.md)
```

Dependency direction is mandatory: `cli / gtk → (D-Bus) → daemon → devices → features → hidpp → transport`. `hidlab-api` (serde only) holds the D-Bus names, the JSON types and the settings vocabulary; every crate may use it, and the app uses nothing else of ours. The CLI does not link `hidlab-daemon`. `hidpp` and `features` are generic HID++; device-specific knowledge goes in `hidlab-devices` (requirements §5).

Commands today:

```bash
hid-lab status   [--json]
hid-lab set      <setting> <value> [--json]     # also remap-<button>, natural-scroll, acceleration
hid-lab reset    (<setting> | all) [--json]
hid-lab reapply  [--json]
hid-lab buttons  [--duration <s>] [--json]
hid-lab lab devices  [--json] [--show-unique]
hid-lab lab features [<device>] [--json] [--index <n>] [--timeout-ms <ms>] [--show-unique]
hid-lab lab read     [<device>] [--json] [--index <n>] [--timeout-ms <ms>] [--show-unique]
hid-lab lab watch    [<device>] [--duration <s>] [--json] [--features-from <file>]
hid-lab lab hotplug  [--duration <s>] [--json]
hid-lab lab write    <setting> (<value> | --restore) [<device>] (--dry-run | --write) [--json]
```

`lab write` state (saved originals, `log.jsonl`) lives in `$XDG_STATE_HOME/hid-lab/lab-write/`. `lab write --write` refuses while `hid-labd` owns its D-Bus name.

Daemon (`docs/daemon.md`): systemd user unit `packaging/systemd/hid-labd.service`, D-Bus activation file `packaging/dbus/dev.ozkkar.Cursiva.Daemon1.service`, config `~/.config/hid-lab/config.toml` (`docs/config-format.md`), state `$XDG_STATE_HOME/hid-lab/daemon/state.toml`, requests with sw_id `0x0C`. It writes only `CONFIRMED_WRITES` through `WriteAuth::Confirmed`, plus the GNOME settings `natural-scroll` and `accel-profile` (via `gsettings`) when asked with `SetDesktop`. Never call `SetDesktop`, `Set` or `Reset` on the user's real session from Claude Code. Never run a daemon test against the user's real config from Claude Code: use `XDG_CONFIG_HOME`/`XDG_STATE_HOME` in the scratchpad and an empty config, so nothing is written. The user's own `hid-labd.service` normally runs: never stop or restart it; run a test instance on a private bus with `dbus-run-session`.

Dependencies: `libc`, `serde`, `serde_json`, `thiserror`, `udev` (needs `systemd-devel` to build), `zbus` 5.13.2 (blocking API; 6.0.0 dropped it), `toml`. The app only: `gtk4` 0.11.5 (features `v4_18`), `libadwaita` 0.9.2 (`v1_7`), `gettext-rs` 0.8.0 (`gettext-system`); it needs Rust 1.92, `gtk4-devel`, `libadwaita-devel` and `gettext` (msgfmt). Explain why before adding any other crate, and verify its current version with `cargo info`.

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
- 0x1B04 setCidReporting is sent only to remap (flags byte `00`), never to divert, until Milestone 6. The daemon refuses values that `SettingValue::confirmed` rejects.

## Privacy

- `HID_UNIQ` (Bluetooth address) and the firmware unit ID (0x0003 getDeviceInfo bytes 1..5) are redacted by default everywhere: decoded output, transcripts, and other programs' traffic captured by `lab watch`.
- Before committing user captures: check for the unit ID in every hex form, replace home paths with `~`, and record any sanitizing in the file.

## Non-negotiable rules

1. Read `docs/requirements.md` before designing anything new.
2. Small, focused commits in English using [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`, optional scope such as `feat(daemon):`). One concern per commit. **No `Co-Authored-By` trailer and no Claude attribution** in commits or pull requests.
3. After every change: `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`.
4. No root at runtime. Never suggest running hid-lab with sudo as a workaround.
5. Disconnects and reconnects are normal events. No panics on expected failures, including a closed stdout pipe.
6. Errors are actionable: what happened, likely cause, exact command to try.
7. Keep `docs/` and this file in sync with behavior.
8. Propose a plan and wait for approval before each milestone or phase.
9. Ask the user only when a decision cannot be inferred from this file or `docs/requirements.md`.
