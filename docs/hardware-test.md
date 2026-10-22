# Hardware test checklist

Manual tests on the real device. Each test names the protocol entry it can
promote in `docs/protocol/README.md`. Run as a normal user, never with sudo.

## M3 — Read-only features (MX Master 3S over Bluetooth)

Run the tests in this order. The channel switch (H3-05) goes last because
it disconnects the mouse.

### Preparation

```bash
cargo build --release
mkdir -p ~/hidlab-m3
./target/release/hid-lab lab features --json > ~/hidlab-m3/features.json
```

The features file lets `lab watch` print feature names without sending
anything to the mouse.

### Step 1 — Baseline read (H3-01, H3-02, H3-03, H3-06, H3-07, H3-10)

Do not touch the mouse and leave the cable unplugged.

```bash
./target/release/hid-lab lab read
./target/release/hid-lab lab read --json > ~/hidlab-m3/01-baseline.json
```

| Test | Check | Promotes |
|---|---|---|
| H3-01 | Name is `MX Master 3S` and type is 3 (mouse). | P-020 |
| H3-02 | Model ID `btle` is `B034`, the same product ID that `lab devices` shows. Optional: compare the firmware version with `fwupdmgr get-devices`, if fwupd lists the mouse. | P-021 |
| H3-03 | The battery line says `matches`: the HID++ percentage equals the kernel's `capacity`. Also run `cat /sys/class/power_supply/hidpp_battery_*/capacity`. | P-022 (charge, discharging) |
| H3-06 | Compare the controls table with the physical buttons: left, right, wheel click, back, forward, thumb (gesture) button, and the mode button behind the wheel. Seven physical buttons plus one row flagged `virtual` is the expected match. Report any button that has no row, or any extra non-virtual row. | P-024 |
| H3-07 | The current DPI is inside the listed range. | P-025 (list and layout only) |
| H3-10 | The thumb wheel shows not diverted and not inverted. | P-028 stays LIKELY: only the default is observable |

### Step 2 — Mode button with reads (H3-08)

1. Run `./target/release/hid-lab lab read` and note the SmartShift mode and the ratchet switch.
2. Press the mode button behind the wheel once. Spin the wheel and note whether it clicks (ratchet) or spins freely.
3. Run `./target/release/hid-lab lab read --json > ~/hidlab-m3/02-after-press-1.json`.
4. Press the mode button again and spin the wheel.
5. Run `./target/release/hid-lab lab read --json > ~/hidlab-m3/03-after-press-2.json`.

Pass: the SmartShift mode alternates between 2 (ratchet) and 1 (free-spin).
The ratchet switch bit alternates with it (1 with ratchet, 0 with
free-spin). Both agree with what the wheel feels like. Promotes P-026 mode
codes and P-027 ratchet bit.

### Step 3 — Mode button notifications (H3-11)

```bash
./target/release/hid-lab lab watch --duration 30 --features-from ~/hidlab-m3/features.json
```

While it listens, press the mode button three times, a few seconds apart.
Then do it again with `--json > ~/hidlab-m3/04-watch-mode-button.json`.

Pass: one or more notifications appear for each press. Note which feature
they come from. No meaning is assumed for their bytes. Promotes P-029 for the
features that show up.

### Step 4 — Charging cable (H3-04, H3-12)

Terminal 1:

```bash
./target/release/hid-lab lab watch --duration 120 --json \
  --features-from ~/hidlab-m3/features.json > ~/hidlab-m3/05-watch-cable.json
```

Within the 120 seconds:

1. Plug in the USB-C cable.
2. Wait 30 seconds.
3. In terminal 2, run `./target/release/hid-lab lab read --json > ~/hidlab-m3/06-read-charging.json`.
4. Unplug the cable.

Pass:

- In the charging read, charging status changes from 0 to 1. If it reads 2,
  that is the conflicting code. Note the battery percentage shown by GNOME at
  that moment.
- The kernel status shows Charging and the percentage still matches.
- The watch file contains notifications on the battery feature's index
  around the plug and unplug times.

Promotes P-022 code 1 and P-029 for 0x1004. Codes 2 and 4 stay HYPOTHESIS
unless they appear.

### Step 5 — Channel switch (H3-05), last

1. Run `./target/release/hid-lab lab read` and note the host channel. Check that the lit channel LED under the mouse shows the same number.
2. Press the Easy-Switch button to move to another channel. The mouse disconnects from this computer.
3. Press it again until you are back on the original channel. Move the mouse until the pointer responds.
4. Run `./target/release/hid-lab lab devices`. The hidraw number may have changed, which is normal.
5. Run `./target/release/hid-lab lab read --json > ~/hidlab-m3/07-after-channel-switch.json`.

Pass: the channel count and current channel are the same before and after,
and both match the LED. `lab read` finds the mouse again without
arguments. Promotes P-023 and confirms reconnects are handled.

### What to send back

The files in `~/hidlab-m3/`, plus a short note for each step: what you did,
what you saw or felt on the mouse, and anything unexpected. The JSON files
already redact the Bluetooth address and the unit ID.

### Results, run 1 (2026-10-21)

Run by the user on Fedora 44, Bluetooth, hid-lab 0.1.0. Captures are in
`docs/protocol/evidence/m3/`. Details and promotions are in
`docs/protocol/README.md`, observation O-5.

| Step | Result | Notes |
|---|---|---|
| 1 | Pass | All target features read; battery matches the kernel; 7 physical controls plus 1 virtual |
| 2 | Pass | SmartShift mode 2 → 1 → 2 and ratchet bit 1 → 0 → 1, matching the wheel's feel |
| 3 | Pass, one open question | 4 notifications from 0x2121 function 1 for an instruction of 3 presses |
| 4 | Pass | Charging status 1 with cable; 3 battery notifications from 0x1004 function 0; no conflicting code (2 or 4) seen |
| 5 | Pass | Channel 2 before and after; the mouse reconnected and was found without arguments |

The optional fwupd comparison in step 1 was not run.

Privacy issue found in step 4: `lab watch` kept other programs' HID++ answers
unredacted, including the unit ID returned to the concurrent `lab read`.
Fixed in `lab watch`; the committed copy of that capture was sanitized. The
original file in `~/hidlab-m3/` still contains the unit ID.
