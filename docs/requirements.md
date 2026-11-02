# hid-lab — Requirements (v2)

## Project name

**hid-lab** — A native Linux configuration app and background service for Logitech HID++ mice, starting with the **Logitech MX Master 3S**, built on a reusable HID inspection and protocol-research toolkit.

### Names and identifiers

| What | Name |
|---|---|
| Desktop app (user-facing name) | **Cursiva** |
| Application ID (GTK, `.desktop`, icons) | `dev.ozkkar.Cursiva` |
| Daemon D-Bus well-known name | `dev.ozkkar.Cursiva.Daemon1` |
| CLI binary | `hid-lab` |
| Internal crates | `hidlab-*` |

The D-Bus object paths and interface names derive from the daemon name
(`/dev/ozkkar/Cursiva/Daemon1`, interface `dev.ozkkar.Cursiva.Daemon1`).

---

## 1. Vision

Make the MX Master 3S as easy to use and configure on **Fedora + GNOME + Wayland** as it is on Windows with Logi Options+, without losing functionality.

The user must be able to:

1. open a native GNOME app;
2. see the mouse, its connection and battery;
3. change any supported setting with immediate effect;
4. assign actions to buttons and gestures;
5. define per-application behavior;
6. close the app and have everything keep working, including after reboot, sleep, reconnect, or switching Easy-Switch channels.

The reverse-engineering toolkit (inspection, capture, diff, evidence) remains part of the project, but as a **developer tool that serves the product**, not as the product itself.

### Why this project exists

Existing Linux tools (Solaar, logiops) were tested on the target system and did not work reliably enough. The project must start by documenting **exactly what failed and why** (see Milestone 0), so the design fixes real problems instead of assumed ones.

---

## 2. Target environment

Primary (must work perfectly):

- Fedora Linux (current release), x86_64
- GNOME Shell on Wayland
- systemd user session
- MX Master 3S connected via **Bluetooth LE** — this is the user's daily setup and the **primary** connection. Every milestone is developed and accepted over Bluetooth first.
- MX Master 3S via **Logi Bolt receiver** — supported, tested before each release, but secondary. Bolt work is scheduled in Milestone B (see §18), not in Milestone 4.

Secondary (do not break, do not prioritize):

- other Linux distributions with systemd
- X11 sessions
- other HID++ 2.0 Logitech mice

Out of scope: Windows and macOS builds of the app. The core library may remain portable, but no effort is spent on it.

---

## 3. Feature parity target

This table is the product scope. Each feature is tracked to completion.

| Options+ feature | Target | Notes |
|---|---|---|
| Battery level and charging state | MVP | Desktop notification at low battery |
| Pointer speed / DPI | MVP | Slider plus presets |
| Pointer acceleration | MVP | Via GNOME/libinput settings, not the device |
| Main wheel: SmartShift on/off | MVP | |
| Main wheel: SmartShift sensitivity | MVP | |
| Main wheel: ratchet vs. free-spin default | MVP | |
| Main wheel: scroll direction | MVP | Via GNOME's natural-scrolling setting, not the device. The 0x2121 wheel mode is owned by the kernel driver and is not written |
| Main wheel: smooth / high-resolution scrolling | MVP | Must not break libinput behavior |
| Thumb wheel: speed and direction | MVP | Direction is written to the device (0x2150 invert) |
| Thumb wheel: custom action (e.g. zoom, tab switch) | v1 | Requires diversion |
| Button remapping (middle, back, forward, top button) | MVP | |
| Gesture button with directional gestures | v1 | Requires diversion + raw movement |
| Per-application settings and actions | v1 | Requires GNOME Shell extension (see §8) |
| Easy-Switch channel display | v1 | Read-only |
| Settings survive reconnect, sleep, reboot, channel switch | MVP | Core requirement, see §7 |
| Firmware version display | v1 | Read-only |
| Logi Flow (cross-computer control) | Out of scope | Possible future project |
| Smart Actions / AI Prompt Builder | Out of scope | |
| Firmware updates | Out of scope | Never |

"MVP" = first release the user can rely on daily.
"v1" = full parity release.

---

## 4. Non-goals

Do NOT:

- modify, flash, or update firmware;
- bypass authentication, licensing, or access controls;
- require root for normal operation;
- reimplement Logi Flow in the first versions;
- support every Logitech device before the MX Master 3S is complete;
- send any write to a device that has not been verified against evidence.

---

## 5. Architecture

### Components

```text
hid-lab/
├── Cargo.toml                (workspace)
├── crates/
│   ├── hidlab-transport/     hidraw access, device discovery, hotplug (udev)
│   ├── hidlab-hidpp/         generic HID++ 2.0 protocol: framing, feature
│   │                         enumeration, request/response, notifications
│   ├── hidlab-features/      typed APIs per HID++ feature (battery, DPI,
│   │                         SmartShift, wheel, thumb wheel, reprog controls)
│   ├── hidlab-devices/       per-device profiles (MX Master 3S first):
│   │                         which features, buttons, defaults
│   ├── hidlab-daemon/        user service: applies config, handles reconnect,
│   │                         diverted buttons, gestures, per-app rules,
│   │                         emits actions via uinput, exposes D-Bus API
│   ├── hidlab-cli/           CLI client of the daemon + lab commands
│   ├── hidlab-lab/           capture, import pcaps, diff, evidence records
│   └── hidlab-gtk/           GTK4 + Libadwaita GUI, D-Bus client only
├── gnome-extension/          GNOME Shell extension: publishes focused app
├── packaging/
│   ├── udev/                 udev rules (hidraw + uinput, uaccess)
│   ├── systemd/              user unit
│   └── fedora/               RPM spec (later: COPR)
└── docs/
```

Start with fewer crates and split when the boundary is proven. The **dependency direction** is mandatory from day one:

```text
gtk ─┐
cli ─┼─► D-Bus ─► daemon ─► devices ─► features ─► hidpp ─► transport
ext ─┘                                                ▲
lab ──────────────────────────────────────────────────┘
```

### Rules

1. **The daemon is the single owner of the device.** GUI and CLI never talk to hidraw directly for configuration; they call the daemon over D-Bus. This avoids two programs fighting over the mouse.
2. **The GUI contains no device logic.** If the GUI is closed, everything still works.
3. **`hidlab-hidpp` is generic.** It knows HID++ 2.0, not the MX Master 3S.
4. **Device-specific knowledge lives only in `hidlab-devices`.**
5. **The lab crate is read-only** and never required by the daemon at runtime.

---

## 6. Transport and connection

### Bolt receiver

Scheduled in Milestone B (§18). Milestone 4 is Bluetooth only.

- The receiver exposes a hidraw node; paired devices are addressed by **device index** through it.
- The daemon must enumerate paired devices on the receiver and track each one independently.
- Receiver-level notifications (device connected / disconnected) must be handled.

### Bluetooth LE

- The mouse exposes its own hidraw node; HID++ is addressed directly to it.
- Bluetooth disconnect/reconnect is frequent (sleep, power saving). It is a normal event.

### Both

- Hotplug via udev monitoring. No polling loops.
- The same mouse may appear over Bolt and Bluetooth on different Easy-Switch channels. Identify it by serial / unit ID where available and share its configuration.
- Coexistence with the kernel driver `hid-logitech-hidpp`, which also speaks HID++ on the same device. The daemon must tolerate unsolicited messages and must not break kernel-handled features (e.g. high-resolution scroll, battery reporting to UPower).
- Detect running Solaar or logid and warn clearly: two configurators on one device cause unpredictable results.

Exact receiver PIDs, device indexes and report IDs must be **read from the device and recorded as evidence**, not hard-coded from memory.

---

## 7. Persistence and re-application (core requirement)

Many settings are lost when the mouse powers off, sleeps, reconnects, or switches Easy-Switch channel. Diversion of buttons is always lost. This is the most likely reason existing tools "don't work well", and it is the most important behavior to get right.

The daemon must:

1. store the desired configuration on disk (`~/.config/hid-lab/`, human-readable TOML);
2. detect every (re)connection event, including:
   - receiver plug/unplug,
   - Bluetooth reconnect,
   - wake from sleep,
   - return from another Easy-Switch channel,
   - system resume (logind `PrepareForSleep`);
3. re-apply the full configuration after each event;
4. verify by reading back where the feature supports it;
5. log mismatches at WARN with an actionable message.

Acceptance test: configure the mouse, then perform each event in the list above. In Milestone 4 the test runs over Bluetooth only, so receiver plug/unplug is excluded; it is added in Milestone B. After each one, every setting and every button mapping must be active within 2 seconds, without user action.

---

## 8. Per-application behavior on Wayland

Wayland does not let ordinary programs know which window is focused. Options+-style per-app settings therefore require cooperation from GNOME Shell.

Design:

- A small **GNOME Shell extension** listens for focus changes and publishes the focused application's ID (desktop app ID / `WM_CLASS` equivalent) on D-Bus.
- The daemon subscribes and switches the active profile.
- Profiles may override: button actions, gesture actions, thumb wheel action, SmartShift, DPI.
- If the extension is not installed, the app works with a single global profile and the GUI explains how to enable per-app profiles.

The extension must be minimal, contain no device logic, and support the GNOME Shell versions shipped by current Fedora releases.

---

## 9. Buttons, gestures and actions

### Diversion

To customize buttons and the thumb wheel beyond basic remapping, the daemon asks the mouse (via the HID++ reprogrammable-controls feature) to **divert** controls to software. The mouse then sends HID++ notifications instead of normal mouse events.

For gestures, the daemon diverts the gesture button and requests raw movement while it is held, then classifies the direction.

### Action output

Actions are emitted through a **virtual input device (uinput)** owned by the daemon, so they work on Wayland in any app.

Supported action types:

| Action | Implementation |
|---|---|
| Key combination (e.g. `Ctrl+C`) | uinput |
| Mouse button | uinput |
| Scroll (horizontal/vertical, with amount) | uinput |
| GNOME actions (overview, show apps, switch workspace) | key combination or GNOME D-Bus |
| Media keys / volume | uinput |
| Launch application or command | daemon spawn, user-configured only |
| Do nothing / native behavior | undivert |

### Gesture classification

- Configurable threshold distance.
- Directions: up, down, left, right, and press-without-movement.
- Each direction maps to any action above.
- Optional: repeat while moving (e.g. continuous zoom).

---

## 10. HID++ feature support

The daemon must first **enumerate** the features the connected mouse reports (HID++ 2.0 root and feature-set features) and only use features the device actually reports.

Expected feature families, to be **confirmed by enumeration on the real device**:

- unified battery / battery status
- adjustable DPI
- SmartShift (standard and/or enhanced variant)
- high-resolution wheel
- thumb wheel
- reprogrammable controls (diversion, raw XY)
- device name, firmware info, change host

Feature IDs, function numbers, and payload layouts may be taken from open-source references (Solaar, logiops, the Linux kernel driver) as **starting hypotheses**. Each must then be verified on the MX Master 3S and recorded in `docs/protocol/` with evidence (see §13). Record the source and license of every reference used. Do not copy code without a compatible license.

---

## 11. GUI (GTK4 + Libadwaita)

The GUI is part of the MVP, because "easy like Windows" is the goal.

### Views

1. **Home** — mouse illustration (original artwork, not Logitech assets), connection type, battery, active profile.
2. **Point & Scroll** — DPI, scroll direction, smooth scrolling, SmartShift, thumb wheel.
3. **Buttons** — click a button on the illustration, choose an action from a list.
4. **Gestures** — four directions + press, each with an action.
5. **Applications** — list of per-app profiles; add app from installed applications.
6. **Settings** — start at login, notifications, troubleshooting, export diagnostics.

### Design requirements

The interface must be **elegant and intuitive**. Concretely:

**Visual**

- Native GNOME look: Libadwaita widgets, accent color, light and dark mode, rounded cards, generous spacing. It should look like it shipped with GNOME.
- The **mouse illustration is the centerpiece**: a clean, original vector drawing (SVG), not a photo and not Logitech artwork. Buttons on the illustration are hotspots that highlight on hover and when physically pressed.
- Subtle motion only: hotspot highlight, smooth transitions between views. No decorative animation.
- Typography and icons from the system (Cantarell/Adwaita Sans, Adwaita icons). No custom fonts.

**Interaction**

- **Click the part of the mouse you want to change.** Clicking the thumb wheel opens thumb-wheel settings; clicking a button opens its action picker. Navigation by the sidebar is the secondary path.
- **Live feedback**: pressing a physical button highlights it in the app; moving the DPI slider changes pointer speed immediately; gestures show a live direction indicator while the gesture button is held.
- **Plain language**: "Scroll mode: precise / free-spin / automatic" rather than "SmartShift threshold 30". Technical values appear only in an "Advanced" expander.
- **Action picker** with search and categories (Navigation, Editing, Media, GNOME, Custom shortcut). Recording a custom shortcut is done by pressing it.
- **Every change is reversible**: "Restore default" per section, and an undo toast after changes.
- Changes apply immediately (no "Save" button), like Options+.
- **Zero-config first run**: if the mouse is found, open straight into the Home view. If not, a friendly guide explains the likely cause (permissions, Bluetooth off, daemon not running) with a one-click fix where possible.

**Quality bar**

- Follow the GNOME Human Interface Guidelines.
- Adaptive layout that works at small window sizes.
- Fully keyboard-navigable; accessible labels on every control and hotspot.
- Spanish and English translations from the start (gettext).
- Before implementing the GUI, produce mockups of every view for approval.

### Rules

- Every error shown in the GUI is actionable (what happened, what to try).
- A tray/status icon is not required; GNOME discourages it. Battery appears via a notification at low level and via UPower where the kernel already reports it.

---

## 12. CLI

Two groups of commands sharing one binary.

### User commands (talk to the daemon)

```bash
hid-lab status
hid-lab set dpi 1600
hid-lab set smartshift on --sensitivity 30
hid-lab map button back --action "key:Alt+Left"
hid-lab profile list
hid-lab profile apply <name>
```

### Lab commands (developer tools, read-only by default)

```bash
hid-lab lab devices [--json]
hid-lab lab features <device>            # HID++ feature enumeration
hid-lab lab read <device>                # read-only state of known features
hid-lab lab watch <device>               # passive: HID++ notifications
hid-lab lab write <setting> <value>      # controlled write experiment (§13)
hid-lab lab descriptor <device>
hid-lab lab capture <device> [--duration 10s] [-o file]
hid-lab lab import capture.pcapng        # usbmon / USBPcap / btmon captures
hid-lab lab diff a.json b.json
hid-lab lab sanitize capture.json
```

`lab import` matters because the most informative captures come from **Windows with Options+** (USBPcap + Wireshark) or from a Windows VM with USB passthrough captured by `usbmon` on the Fedora host. On native Linux without Options+, nobody sends configuration commands, so there is little to observe.

Note: `usbmon` sees Bolt (USB) traffic only. Bluetooth traffic requires `btmon` or a Bluetooth capture on Windows.

Any lab command that writes to a device must require `--write`, name the target device, validate length and report type, support `--dry-run`, and log every byte sent.

---

## 13. Evidence system

Unchanged in spirit; mandatory for every protocol fact the daemon relies on.

Status levels:

```text
CONFIRMED   verified on the real device, multiple values, reproducible
LIKELY      consistent with reference implementations and one observation
HYPOTHESIS  inferred, not yet tested
UNKNOWN
```

Each finding in `docs/protocol/*.md` records:

- the feature and function;
- the payload layout;
- status;
- evidence: captures (file + report index + byte offsets), reference source (project, file, license), and test performed.

**Rule:** the daemon may only *write* using CONFIRMED findings. LIKELY findings may be used for *reads*.

**Exception for confirming writes:** a write function can only become CONFIRMED by being tried. `hid-lab lab write` is the only place where a LIKELY write function may be sent, and only when the user runs it explicitly with `--write`, for a function on its write allowlist, after reading and saving the original value, with every byte logged, a read-back comparison, and `--restore` to put the original value back. The daemon never uses this exception.

---

## 14. Permissions and installation

No root at runtime.

- udev rule granting the logged-in user access to the relevant hidraw nodes (`TAG+="uaccess"`), scoped to Logitech vendor ID.
- udev rule granting access to `/dev/uinput` for the daemon (also `uaccess`), documented clearly because it lets the user session emit input events.
- systemd **user** unit for the daemon, enabled at login.
- Packaging target: RPM spec, later a COPR repository. Flatpak is not suitable for the daemon (device and uinput access); the GUI may be packaged separately later.

Installation must be one command and must explain every permission it adds.

---

## 15. Configuration format

`~/.config/hid-lab/config.toml`, versioned:

```toml
format_version = 1

[device."mx-master-3s"]
dpi = 1600
smartshift = { enabled = true, sensitivity = 30 }
scroll = { natural = false, hires = true }
thumbwheel = { invert = false, action = "native" }

[device."mx-master-3s".buttons]
back = "native"
forward = "native"
middle = "native"
top = "smartshift-toggle"
gesture = "gestures"

[device."mx-master-3s".gestures]
up = "gnome:overview"
down = "gnome:show-desktop"
left = "key:Super+Page_Up"
right = "key:Super+Page_Down"
press = "gnome:overview"

[profile."org.mozilla.firefox"]
thumbwheel = { action = "tabs" }
```

Illustrative. Final schema documented in `docs/config-format.md`. Invalid config must never crash the daemon: it logs the error, keeps the last valid config, and the GUI shows the problem.

---

## 16. Testing

- Unit tests: HID++ framing, feature parsers, gesture classifier, config parsing, diff.
- Fake device: a simulated HID++ device in tests that answers feature enumeration and records writes. The normal test suite must not need hardware.
- Golden files from real captures in `tests/data/` (sanitized).
- Manual hardware checklist in `docs/hardware-test.md` covering every row of §3 and every event in §7, over Bolt and over Bluetooth. Run before each release.

---

## 17. Error handling, logging, privacy

- Typed errors; no panics on expected device failures (disconnects are normal).
- Actionable messages, e.g.:

```text
Cannot open /dev/hidraw4 (permission denied).
The udev rules are probably not installed or not yet applied.
Try: reinstall hid-lab, then unplug and replug the receiver.
```

- Structured logging via journald. Raw HID bytes only at DEBUG/TRACE or in lab capture mode.
- `hid-lab lab sanitize` removes serial numbers, hostnames, usernames, paths before sharing captures.
- "Export diagnostics" in the GUI produces a sanitized bundle for bug reports.

---

## 18. Milestones

### Milestone 0 — Diagnose existing tools (no product code)

Document in `docs/diagnosis.md`, for Solaar and logiops on the target Fedora system, over Bolt and Bluetooth:

- what works;
- what fails, with exact steps and logs;
- root cause where identifiable (e.g. settings lost after sleep, no Wayland per-app support, Bolt support, gesture detection, permissions).

Each failure becomes an acceptance test for this project. Uninstall/disable both tools afterward to avoid conflicts.

**Initial report from the user (to be expanded with evidence):**

- Connection: Bluetooth.
- Symptom: existing tools detect nothing configurable; no setting can be changed.
- History: on an earlier Fedora release it worked partially.
- Status: UNKNOWN cause. Candidate causes to test, in this order:
  1. permissions on the Bluetooth hidraw node (udev rules not matching BLE devices);
  2. the tool cannot reach the HID++ interface over BLE (wrong hidraw node, or HID++ reports not exposed);
  3. conflict with the kernel `hid-logitech-hidpp` driver or BlueZ behavior after an update;
  4. outdated tool version in the Fedora repositories relative to the kernel/BlueZ;
  5. mouse firmware (lowest probability; firmware updates are out of scope).

Whatever the cause turns out to be, hid-lab must detect it at runtime and tell the user in plain language (see §17).

### Milestone 1 — Discovery and transport

- Workspace, `hidlab-transport`, `hid-lab lab devices [--json]`.
- Bluetooth device first (primary); Bolt receiver + paired devices second.
- Hotplug events (moved to Milestone 4).
- udev rules.

### Milestone 2 — HID++ core

- Framing, request/response, notifications.
- Feature enumeration: `hid-lab lab features`.
- Record the MX Master 3S feature list as the first evidence file.

### Milestone 3 — Read-only features

- Battery, DPI, SmartShift state, wheel and thumb-wheel state, firmware.
- All findings documented with status.

### Milestone 4 — Daemon + first writes (Bluetooth)

- Hotplug via udev monitoring.
- `hid-lab lab write` to confirm each write function on the real device, one at a time (§13 exception). First set: DPI (0x2201), SmartShift mode and threshold (0x2110), thumb-wheel inversion (0x2150).
- systemd user daemon, D-Bus API, config file.
- Writes from the daemon: only the functions confirmed above.
- Main-wheel scroll direction is not written to the device; GNOME's natural-scrolling setting controls it. The 0x2121 wheel mode is not written in this milestone.
- **Re-application on every event in §7.** This milestone is not done until the §7 acceptance test passes over Bluetooth.
- After re-applying and verifying, the daemon verifies again 1–2 s later to detect anything (for example the kernel driver) overwriting what was applied. Both checks are part of the measured time.
- SmartShift mode follows the user: the daemon listens to the 0x2121 ratchet notifications (one per mode-button press, CONFIRMED) and keeps the last mode chosen, by button or by the app. On reconnect, if the mouse lost that mode, the daemon restores it. The config file stores the threshold and the initial mode. Details, approved from the H4-06 data (P-033):
  - On each 0x2121 ratchet notification the daemon reads the 0x2110 mode instead of deriving it from the notification byte. A mode set through the app is stored directly.
  - The last mode is kept in `$XDG_STATE_HOME/hid-lab/daemon/state.toml` and survives daemon restarts. The config's mode is used only when that file does not exist.
  - On reconnect the daemon reads the mode and writes it only if it differs, then verifies twice. DPI, threshold and thumb-wheel inversion are re-applied on every reconnect, because switching the mouse off resets them.
  - Each computer keeps its own mode: a mode changed while the mouse was on another channel is reverted when it returns.

### Milestone B — Logi Bolt receiver

- Receiver discovery, paired-device enumeration by device index, receiver connection notifications, short reports.
- Hardware tests over Bolt for everything Milestones 1–4 cover over Bluetooth, and the §7 receiver plug/unplug event.
- Scheduled after Milestone 4. Not part of the MVP: the MVP is Bluetooth only.

### Milestone 5 — GUI MVP

- Home, Point & Scroll, basic Buttons.
- At this point the user can stop using any other tool. **This is the MVP release.**

Approved plan (2026-11-02):

- **5a** Mockups of every MVP view (Home, Point & Scroll, Buttons, action picker, first-run guide, Settings), light and dark, wide and narrow, and the original mouse illustration (`docs/mockups/`). No GUI code before approval.
- **5b** Protocol work: read the pinned references for 0x1B04 remapping (`setCidReporting`) and for a documented "SmartShift off" value; confirm each new write with `hid-lab lab write` and hardware tests H5. Check whether GNOME already notifies low mouse battery.
- **5c** Daemon API v2: battery and charging with change signals, DPI list and threshold limits, restore-default per setting, low-battery notification from the daemon, button remapping re-applied like the other settings, physical button presses for the GUI highlight (only while a client asks, only buttons the mouse reports normally), GNOME pointer settings written by the daemon.
- **5d** `hidlab-gtk` crate, binary `cursiva`: GTK4 + Libadwaita, gettext (English and Spanish), D-Bus through `gio`, illustration drawn with `GskPath`; `.desktop`, icon, AppStream metainfo.
- **5e** Home and Point & Scroll. **5f** Buttons. **5g** First-run guide, Settings, export diagnostics. **5h** One-command install script.
- Acceptance: §19 on the real mouse from a clean install.

Not in the MVP: smooth-scrolling switch (the kernel owns 0x2121; shown as "on, managed by the system"), thumb-wheel speed (needs diversion, Milestone 6), Gestures and Applications views (Milestones 6 and 7).

### Milestone 6 — Diversion, actions, gestures

- uinput action device, button diversion, gestures, thumb-wheel actions.

### Milestone 7 — Per-app profiles

- GNOME Shell extension, profile switching, Applications view.

### Milestone 8 — Lab tooling and packaging

- Capture, pcap import, diff, sanitize.
- RPM / COPR.

Lab tooling can be pulled earlier when a feature needs investigation, but the product milestones drive priority.

---

## 19. Definition of Done — MVP (end of Milestone 5)

On a fresh Fedora GNOME Wayland install, the user can:

1. install hid-lab with one command;
2. open the app and see the MX Master 3S over Bluetooth with battery level;
3. change DPI, SmartShift, scroll direction, smooth scroll, thumb-wheel direction, and basic button mappings, with immediate effect;
4. close the app;
5. reboot, sleep, switch Easy-Switch channel and back, turn the mouse off and on;
6. find every setting still active, without opening the app;
7. never need root, a terminal, or a config file for any of the above.

---

## 20. Rules for the AI coding agent

The agent MUST:

1. Inspect the repository before changing anything.
2. Respect the dependency direction in §5.
3. Prefer small, incremental changes and small commits.
4. Run `cargo fmt`, `cargo clippy --all-targets --all-features`, `cargo test` after changes.
5. Never invent protocol details. Reference implementations are hypotheses until verified (§13).
6. Never write to a device using a finding that is not CONFIRMED.
7. Treat disconnects and reconnects as normal events.
8. Keep device-specific code in `hidlab-devices`.
9. Explain why before adding a dependency; prefer well-maintained crates.
10. Verify current crate APIs from their documentation instead of relying on memory.
11. Keep documentation in sync with behavior.
12. Ask only when a decision cannot be safely inferred from this document or the repository.

### First task

1. Inspect the repository.
2. Write `docs/diagnosis.md` template for Milestone 0 (the user fills in the results).
3. Propose the minimal workspace for Milestone 1.
4. Identify the crates needed for hidraw access and udev hotplug, verify their current APIs, and explain how they interact with `/dev/hidraw*` for both Bolt and Bluetooth.
5. Propose the udev rules and explain each line.
6. Wait for approval before implementing.

---

## 21. Guiding principle

**Evidence first. Reliability second. Features third.**

The question this project answers is not "can Linux talk to this mouse?" (it can), but:

> "Can I configure my mouse once, and forget about it, like on Windows?"

---

## 22. Decision log

| Date | Decision |
|---|---|
| 2026-10-21 | Bolt receiver support moves out of Milestone 4 into Milestone B. The §7 acceptance test for Milestone 4 runs over Bluetooth only. |
| 2026-10-21 | Main-wheel scroll direction is controlled by GNOME's natural-scrolling setting, not written to the device. The 0x2121 mode is not written in Milestone 4. Thumb-wheel inversion (0x2150) may be written to the device. |
| 2026-10-21 | Write functions are confirmed one at a time with `hid-lab lab write` (§13 exception), starting with DPI, SmartShift mode and threshold, and thumb-wheel inversion. |
| 2026-10-21 | udev hotplug, deferred from Milestone 1, is part of Milestone 4. |
| 2026-10-21 | The desktop app is named **Cursiva**: app ID `dev.ozkkar.Cursiva`, daemon D-Bus name `dev.ozkkar.Cursiva.Daemon1`. The CLI and crates keep the hid-lab names. |
| 2026-10-21 | The MVP (Milestone 5) is Bluetooth only. Bolt stays in Milestone B. |
| 2026-10-21 | SmartShift mode: config stores threshold and initial mode; the daemon tracks the last chosen mode from 0x2121 notifications and restores it after reconnect if lost. |
| 2026-10-21 | Re-application is verified twice: right after applying, and again 1–2 s later. |
| 2026-10-21 | `hid-lab lab write --write` requires an explicit device; `--dry-run` may auto-select. |
| 2026-10-21 | Hotplug uses the `udev` crate (libudev; `systemd-devel` at build time). D-Bus uses `zbus`, config uses `toml`. |
| 2026-10-22 | SmartShift mode tracking for 4c approved as proposed (see Milestone 4), including: each computer keeps its own mode. |
| 2026-10-22 | D-Bus uses zbus 5.13.2, not 6.0.0: 6.0.0 (released 2026-10-16) removed the blocking API the daemon uses. |
| 2026-10-22 | Phase 4b daemon applies the configuration at start, on `hid-lab set` and on `hid-lab reapply`, and tracks connect/disconnect; automatic re-application on reconnect is phase 4c. Config schema in `docs/config-format.md` (`smartshift = { mode, threshold }`). |
| 2026-10-22 | Phase 4c: after a connect, start or resume the daemon pings every 200 ms (5 s budget), applies and verifies, and verifies again after 1 s (the low end of the 1–2 s range, to fit the 2 s target), re-applying once if something was overwritten. |
| 2026-11-02 | Milestone 4 closed: the user's §7 acceptance test over Bluetooth (M4c, `docs/hardware-test.md`) passed for every event, with at most 1788 ms from event to second check (observation O-9). |
| 2026-11-02 | Milestone 5 plan approved. "Basic button mappings" (§3, §19) means native remapping with 0x1B04: a button can act as another of the mouse's own controls; key combinations wait for Milestone 6 (uinput). |
| 2026-11-02 | The GUI highlights physical button presses (§11 live feedback) for the buttons the mouse reports in its normal input reports; the gesture and thumb controls need diversion (Milestone 6). |
| 2026-11-02 | The project is public (https://github.com/oscar503sv/cursiva-app) and open to contributors. Commits use Conventional Commits in English, with no AI co-author trailer. English and Spanish translations from the start. |
| 2026-11-02 | The GUI stays Flatpak-friendly: it talks only to the daemon (D-Bus) and to portals. GNOME pointer settings are written by the daemon, and the daemon is started by D-Bus activation (a session service file pointing at `hid-labd.service`), not by the GUI calling systemd. |
| 2026-11-02 | Cursiva sends no low-battery notification of its own: GNOME already shows one for the mouse through UPower (docs/protocol S-1). The Preferences switch is dropped. |
