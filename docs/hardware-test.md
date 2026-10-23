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
| 3 | Pass | 4 notifications from 0x2121 function 1; the user pressed 4 times (the instruction said 3) |
| 4 | Pass | Charging status 1 with cable; 3 battery notifications from 0x1004 function 0; no conflicting code (2 or 4) seen |
| 5 | Pass | Channel 2 before and after; the mouse reconnected and was found without arguments |

The optional fwupd comparison in step 1 was not run.

Privacy issue found in step 4: `lab watch` kept other programs' HID++ answers
unredacted, including the unit ID returned to the concurrent `lab read`.
Fixed in `lab watch`; the committed copy of that capture was sanitized. The
user deleted the original file.

## M4a — Hotplug and write experiments (MX Master 3S over Bluetooth)

Run in this order. Steps H4-02 to H4-06 write to the mouse, but each ends
by restoring the original value. Do not run Solaar, logid or any other
mouse tool during these tests.

### Preparation

```bash
cargo build --release
mkdir -p ~/hidlab-m4a
./target/release/hid-lab lab devices
DEV=hidraw2   # use the node `lab devices` shows for the MX Master 3S
./target/release/hid-lab lab features --json > ~/hidlab-m4a/features.json
./target/release/hid-lab lab read --json > ~/hidlab-m4a/00-before.json
```

If the mouse reconnects during a test, run `lab devices` again. If the node
changed, set `DEV` to the new one.

### H4-01 — Hotplug

```bash
./target/release/hid-lab lab hotplug --duration 180 --json > ~/hidlab-m4a/01-hotplug.json
```

While it listens, wait about 15 seconds between actions:

1. Turn the mouse off with the switch underneath, wait 10 seconds, turn it on and move it.
2. Press Easy-Switch to move to another channel, wait 10 seconds, return to channel 2 and move the mouse.
3. Turn Bluetooth off in GNOME's quick settings, wait 10 seconds, turn it on and move the mouse.

Pass: for each action the file has a `remove` and then an `add` for
`046D:B034`, and every `add` has access `ok`.

Observation, not pass/fail: does the mouse disconnect by itself when idle?

```bash
./target/release/hid-lab lab hotplug --duration 900 --json > ~/hidlab-m4a/01b-idle.json
```

Do not touch the mouse for about 14 minutes, then move it during the last
minute. Report whether a `remove` appeared while it was idle.

### H4-02 — DPI

```bash
./target/release/hid-lab lab write dpi 1600 --dry-run
./target/release/hid-lab lab write dpi 1600 $DEV --write --json > ~/hidlab-m4a/02a-dpi-1600.json
```

Move the mouse slowly the same physical distance as before, for example
along the edge of a book. The pointer must travel clearly farther.

```bash
./target/release/hid-lab lab write dpi 400 $DEV --write --json > ~/hidlab-m4a/02b-dpi-400.json
```

The same movement must move the pointer clearly less.

```bash
./target/release/hid-lab lab write dpi --restore $DEV --write --json > ~/hidlab-m4a/02c-dpi-restore.json
```

The pointer speed must feel as it did at the start.

Pass: the three files have `"result": "pass"` (check with
`grep '"result"' ~/hidlab-m4a/02*.json`), and the pointer speed changed as
described. Promotes P-030.

### H4-03 — SmartShift mode

Terminal 1:

```bash
./target/release/hid-lab lab watch --duration 120 --json \
  --features-from ~/hidlab-m4a/features.json > ~/hidlab-m4a/03-watch-mode.json
```

Terminal 2, while terminal 1 listens:

```bash
./target/release/hid-lab lab write smartshift-mode free-spin $DEV --write --json > ~/hidlab-m4a/03a-free-spin.json
```

Turn the wheel slowly: it must spin freely, without clicks.

```bash
./target/release/hid-lab lab write smartshift-mode ratchet $DEV --write --json > ~/hidlab-m4a/03b-ratchet.json
```

Turn the wheel slowly: it must click again.

```bash
./target/release/hid-lab lab write smartshift-mode --restore $DEV --write --json > ~/hidlab-m4a/03c-restore.json
```

Pass: `03a` and `03b` have `"result": "pass"`, including the check that the
threshold did not change, and the wheel felt as described. `03c` normally
says `no_change`, because the mouse is already back in ratchet. Promotes
P-031 for the mode and the "0 = no change" hypothesis for the threshold.

Observation, not pass/fail: does `03-watch-mode.json` contain 0x2121
notifications at the moments of the writes? The answer decides whether the
daemon can learn about its own mode changes from notifications.

### H4-04 — SmartShift threshold

Make sure the wheel is in ratchet mode (clicking). Terminal 1:

```bash
./target/release/hid-lab lab watch --duration 180 --json \
  --features-from ~/hidlab-m4a/features.json > ~/hidlab-m4a/04-watch-threshold.json
```

Terminal 2:

```bash
./target/release/hid-lab lab write smartshift-threshold 3 $DEV --write --json > ~/hidlab-m4a/04a-threshold-3.json
```

Give the wheel the same moderate flick three times. With threshold 3 it
should switch to free spin easily.

```bash
./target/release/hid-lab lab write smartshift-threshold 40 $DEV --write --json > ~/hidlab-m4a/04b-threshold-40.json
```

Give the same moderate flick three times. With threshold 40 it should keep
clicking, or need a much harder flick to spin freely.

```bash
./target/release/hid-lab lab write smartshift-threshold --restore $DEV --write --json > ~/hidlab-m4a/04c-restore.json
```

Pass: the three files have `"result": "pass"`, including the check that the
mode did not change. The flicks behaved differently at 3 and at 40. The
watch file shows 0x2121 notifications (`00`, then `01` when the wheel slows)
for the flicks at 3, and fewer or none at 40. Promotes P-031 for the
threshold.

### H4-05 — Thumb wheel inversion

Open a window that scrolls sideways, for example a wide web page or GNOME
Text Editor with line wrapping off and a long line. Roll the thumb wheel
forward and note the direction.

```bash
./target/release/hid-lab lab write thumbwheel-invert on $DEV --write --json > ~/hidlab-m4a/05a-invert-on.json
```

Roll the thumb wheel forward: it must scroll the opposite way.

```bash
./target/release/hid-lab lab write thumbwheel-invert --restore $DEV --write --json > ~/hidlab-m4a/05b-restore.json
```

Rolling forward must scroll as at the start.

Pass: both files have `"result": "pass"`, including the checks that
`diverted` and the other bits did not change, and the direction flipped and
came back. Promotes P-032.

### H4-06 — Persistence, observation only

This step has no pass or fail. It records which settings survive which
events, to design the re-application in 4c.

Set four non-default values:

```bash
./target/release/hid-lab lab write dpi 1600 $DEV --write
./target/release/hid-lab lab write smartshift-threshold 40 $DEV --write
./target/release/hid-lab lab write thumbwheel-invert on $DEV --write
./target/release/hid-lab lab write smartshift-mode free-spin $DEV --write
./target/release/hid-lab lab read --json > ~/hidlab-m4a/06a-set.json
```

After each event, wait until the pointer moves again, run `lab devices`,
update `DEV` if needed, then read:

1. Easy-Switch to another channel and back:
   `./target/release/hid-lab lab read --json > ~/hidlab-m4a/06b-after-channel.json`
2. Mouse switch off and on:
   `./target/release/hid-lab lab read --json > ~/hidlab-m4a/06c-after-power.json`
3. Leave the mouse untouched for 15 minutes, then move it:
   `./target/release/hid-lab lab read --json > ~/hidlab-m4a/06d-after-idle.json`
4. Suspend the computer with `systemctl suspend`, wake it, move the mouse:
   `./target/release/hid-lab lab read --json > ~/hidlab-m4a/06e-after-suspend.json`

Restore everything and check that nothing is left over:

```bash
./target/release/hid-lab lab write dpi --restore $DEV --write
./target/release/hid-lab lab write smartshift-threshold --restore $DEV --write
./target/release/hid-lab lab write thumbwheel-invert --restore $DEV --write
./target/release/hid-lab lab write smartshift-mode --restore $DEV --write
./target/release/hid-lab lab read --json > ~/hidlab-m4a/06f-restored.json
ls ~/.local/state/hid-lab/lab-write/
```

The last command should list only `log.jsonl`. If a setting was lost
during an event, its restore may print `NO CHANGE`; that is expected.

### What to send back

The files in `~/hidlab-m4a/`, a copy of
`~/.local/state/hid-lab/lab-write/log.jsonl`, and a short note per test
about what you felt and saw. The files redact the Bluetooth address and do
not contain the unit ID.

### Results, run 1 (2026-10-21)

Run by the user on Fedora 44, Bluetooth, hid-lab 0.1.0. Captures are in
`docs/protocol/evidence/m4a/`. Details and promotions are in
`docs/protocol/README.md`, observation O-6 and P-030 to P-034.

| Test | Result | Notes |
|---|---|---|
| H4-01 | Pass | 3 `remove`/`add` pairs for 3 actions, every `add` access `ok`; no disconnect in 900 s idle |
| H4-02 | Pass | 1600, 400, 1000 read back; the user felt faster, slower, normal |
| H4-03 | Pass | Free spin, then ratchet, threshold unchanged; restore `no_change` as expected; each write produced one 0x2121 notification |
| H4-04 | Pass | 3 and 40 read back with the mode unchanged; the user confirmed afterwards that the wheel released clearly more easily at 3 than at 40. The restore step was skipped (the threshold was reset by H4-06's power cycle). The watch file has no 0x2121 notification: automatic switching does not notify, so the checklist's expectation was wrong |
| H4-05 | Pass | Inverted and back, other bits unchanged; the user felt the direction flip and return |
| H4-06 | Observed | Power off reset DPI, threshold and inversion; the mode survived. The channel switch in `06b` did not reconnect, so channel-switch persistence is unknown |

Open items for 4c: a channel switch that does reconnect, and a suspend with
non-default values still applied.

## M4b — Daemon, config file and D-Bus (MX Master 3S over Bluetooth)

Run in this order, as your normal user. Do not run Solaar, logid or
`hid-lab lab write` during these tests. At the end the mouse goes back to
the values it had before.

### Preparation

```bash
mkdir -p ~/hidlab-m4b
cargo build --release
./target/release/hid-lab lab read --json > ~/hidlab-m4b/00-before.json
cargo install --path crates/hidlab-daemon --root ~/.local
cargo install --path crates/hidlab-cli --root ~/.local
install -Dm644 packaging/systemd/hid-labd.service ~/.config/systemd/user/hid-labd.service
systemctl --user daemon-reload
ls ~/.config/hid-lab/ ~/.local/state/hid-lab/daemon/
```

The last command should say that both directories do not exist. If they
exist, tell me what is in them before going on. If `hid-lab` is not found
in the next steps, add `~/.local/bin` to `PATH` or call
`~/.local/bin/hid-lab`.

### H4B-01 — Start, status and no root

```bash
systemctl --user enable --now hid-labd.service
systemctl --user status hid-labd.service --no-pager
ps -o user=,comm= -C hid-labd
hid-lab status
hid-lab status --json > ~/hidlab-m4b/01-status.json
```

Pass: the unit is `active (running)`; `ps` shows your user, not root;
`status` shows the MX Master 3S with access `ok`, nothing wanted, and
current values equal to `00-before.json` (DPI, mode, threshold, thumb
wheel).

### H4B-02 — `set`, one setting at a time

```bash
hid-lab set dpi 1600
hid-lab set smartshift-threshold 3
hid-lab set thumbwheel-invert on
hid-lab set smartshift-mode free-spin
hid-lab status --json > ~/hidlab-m4b/02-status.json
cat ~/.config/hid-lab/config.toml ~/.local/state/hid-lab/daemon/state.toml
```

After each `set`, check the effect as in H4: faster pointer; the wheel
switches to free spin easily (before the mode change); the thumb wheel
scrolls the other way; the wheel spins freely.

Pass: each `set` says `written and verified` and `Saved to ...`; `status`
marks the four settings `ok`; `config.toml` holds dpi, threshold and invert;
`state.toml` holds `smartshift_mode = "free-spin"`.

Then try a value the mouse cannot take:

```bash
hid-lab set dpi 1625
```

Pass: it says `REJECTED, not sent` and `nothing was saved`, and
`config.toml` still says `dpi = 1600`.

### H4B-03 — Edit the file and reapply; a broken file

```bash
sed -i 's/^dpi = 1600/dpi = 800/' ~/.config/hid-lab/config.toml
hid-lab reapply
```

Pass: `dpi 800 written and verified`, the others `already set`, and the
pointer is slower.

Now break the file and try again:

```bash
echo 'bogus = 1' >> ~/.config/hid-lab/config.toml
hid-lab reapply
hid-lab set dpi 1000
hid-lab status > ~/hidlab-m4b/03-status-broken.txt
sed -i '/^bogus = 1$/d' ~/.config/hid-lab/config.toml
hid-lab reapply
```

Pass: the first `reapply` warns about `bogus` and applies the last valid
configuration (DPI 800 stays); `set` refuses with `was not changed`;
`status` shows the warning; after removing the line, `reapply` has no
warning.

### H4B-04 — Restart: the daemon applies at start

```bash
systemctl --user restart hid-labd.service
journalctl --user -u hid-labd.service -b --no-pager -o cat | tail -n 20
```

Pass: the log shows `serving dev.ozkkar.Cursiva.Daemon1`, the mouse
`connected`, and `applied 4 setting(s) ... verified`.

### H4B-05 — Reconnect is noticed, not yet re-applied

Turn the mouse off with the switch, wait 10 s, turn it on and move it.

```bash
hid-lab status
journalctl --user -u hid-labd.service -b --no-pager -o cat | tail -n 10
hid-lab reapply
hid-lab status --json > ~/hidlab-m4b/05-status.json
```

Pass: the log shows `mouse disconnected`, then `connected` with a new
`0005:046D:B034.NNNN`; the first `status` marks DPI, threshold and invert
`DIFFERS` (the mouse reset them, P-033) and the mode `ok`; after
`reapply`, all four are `ok`. Automatic re-application is phase 4c.

### H4B-06 — Owners and conflicts

```bash
hid-lab lab write dpi 1600 hidraw2 --write
cp /usr/bin/sleep ~/hidlab-m4b/solaar && ~/hidlab-m4b/solaar 60 &
hid-lab status | tail -n 3
~/.local/bin/hid-labd
```

Pass: `lab write` refuses with `hid-labd is running` and writes nothing;
`status` warns that `solaar` is running (this is a copy of `sleep` with
that name, to test the detection; it ends by itself after 60 s); the
second `hid-labd` exits with `another hid-labd already owns`.

### Back to the start

Use the values from `00-before.json` (on this mouse until now: DPI 1000,
ratchet, threshold 10, thumb wheel not inverted):

```bash
hid-lab set dpi 1000
hid-lab set smartshift-threshold 10
hid-lab set thumbwheel-invert off
hid-lab set smartshift-mode ratchet
hid-lab lab read --json > ~/hidlab-m4b/06-after.json
journalctl --user -u hid-labd.service -b --no-pager -o cat > ~/hidlab-m4b/journal.txt
cp ~/.config/hid-lab/config.toml ~/.local/state/hid-lab/daemon/state.toml ~/hidlab-m4b/
rm ~/hidlab-m4b/solaar
```

The daemon stays installed and enabled with these default values. To
disable it: `systemctl --user disable --now hid-labd.service`.

### What to send back

The files in `~/hidlab-m4b/` and a short note per test about what you saw
and felt.

### Results, run 1 (2026-10-22)

Run by the user. Captures are in `docs/protocol/evidence/m4b/`; details in
`docs/protocol/README.md`, observation O-7.

| Test | Result | Notes |
|---|---|---|
| H4B-01 | Pass | active (running), user's own user, status equal to `00-before.json` |
| H4B-02 | Pass | Each setting felt as expected; DPI 1625 rejected and not saved |
| H4B-03 | Pass | DPI 800 felt slower; the broken file kept the last valid config and `set` refused |
| H4B-04 | Pass | `applied 4 setting(s) ... verified` at restart |
| H4B-05 | Pass (second run) | First run without switching the mouse off; repeat: DPI and inversion `DIFFERS`, then all `ok` after `reapply` in 157 ms, felt again |
| H4B-06 | Pass | `lab write` refused, `solaar` detected, second instance exited |

## M4c — §7 acceptance test over Bluetooth

This closes Milestone 4. The daemon must bring every setting back, without
you doing anything, within 2 s of each event. Do not run `hid-lab
reapply` or `hid-lab set` during the events: the point is that nothing is
needed.

### Preparation

Install the new build and restart the daemon:

```bash
mkdir -p ~/hidlab-m4c
cargo install --path crates/hidlab-daemon --root ~/.local
cargo install --path crates/hidlab-cli --root ~/.local
systemctl --user restart hid-labd.service
hid-lab set dpi 1600
hid-lab set smartshift-threshold 30
hid-lab set thumbwheel-invert on
hid-lab set smartshift-mode free-spin
hid-lab status
hid-lab status --json > ~/hidlab-m4c/00-configured.json
```

All four settings must show `ok`. Free spin is set so you can tell whether
the mode is kept.

### How to check each event

After each event, as soon as the pointer moves:

1. Feel it: the pointer is fast, the thumb wheel scrolls the other way, and
   the main wheel spins freely.
2. Run `hid-lab status` and save `hid-lab status --json` to the file named
   in the event.

Pass for each event: in `status`, all four settings `ok`; under `Last
apply`, the trigger named in the table (or `Connect` where noted), `4
setting(s) verified`, `second check ... nothing overwritten` (or
`re-applied: verified`), and a total of **2000 ms or less**. If the trigger
is from an earlier event, the event did not reach the daemon: note it.

### The events

| # | Event | What to do | File | Expected trigger |
|---|---|---|---|---|
| A1 | Mouse off and on | Switch it off, wait 10 s, switch it on, move it | `01-power.json` | `Connect` |
| A2 | Mouse sleep | Leave it untouched for 15 minutes, then move it | `02-idle.json` | none new if it did not disconnect (as in H4-01); `Connect` if it did |
| A3 | Easy-Switch channel | Press Easy-Switch to another channel. **Wait until the pointer stops responding and the other channel's LED is lit**, then wait 10 s more. Press it again to return to channel 2 and move the mouse | `03-channel.json` | `Connect` |
| A4 | System suspend | `systemctl suspend`, wake the computer, move the mouse | `04-suspend.json` | `Connect` or `Resume` |
| A5 | Bluetooth off and on | Turn Bluetooth off in GNOME's quick settings, wait 10 s, turn it on, move the mouse | `05-bluetooth.json` | `Connect` |
| A6 | Reboot | Reboot, log in, move the mouse | `06-reboot.json` | `Start` or `Connect` |

For A2, first save `hid-lab status --json > ~/hidlab-m4c/02-before-idle.json`,
then start the 15 minutes.

### SmartShift mode follows the button

| # | What to do | Pass |
|---|---|---|
| M1 | Press the mode button once (the wheel clicks), then run `hid-lab status` and `cat ~/.local/state/hid-lab/daemon/state.toml` | `smartshift-mode` wanted and current are both `ratchet`; the state file says `ratchet`; the journal says `SmartShift mode changed on the mouse to ratchet; remembered` |
| M2 | Switch the mouse off and on, move it, run `hid-lab status --json > ~/hidlab-m4c/m2-after-power.json` | The wheel still clicks; `smartshift-mode` `ok` |
| M3 | Run `systemctl --user stop hid-labd.service`, press the mode button (free spin; the daemon does not see it, as if you changed it on another computer), run `systemctl --user start hid-labd.service`, then `hid-lab status --json > ~/hidlab-m4c/m3-restored.json` | Within 2 s the wheel clicks again by itself: the daemon restored the remembered `ratchet`; trigger `Start` |

### Back to normal

Your usual values (DPI 1000, threshold 10, not inverted, ratchet):

```bash
hid-lab set dpi 1000
hid-lab set smartshift-threshold 10
hid-lab set thumbwheel-invert off
hid-lab set smartshift-mode ratchet
journalctl --user -u hid-labd.service --since today --no-pager -o cat > ~/hidlab-m4c/journal.txt
cp ~/.config/hid-lab/config.toml ~/.local/state/hid-lab/daemon/state.toml ~/hidlab-m4c/
```

### What to send back

The files in `~/hidlab-m4c/` and, for each event, a short note: whether you
felt the settings come back without doing anything, and anything odd. If an
event fails, write down what `hid-lab status` showed and send the files
anyway.
