# hid-labd, the user daemon

`hid-labd` applies the configuration to the mouse and serves a D-Bus API on
the session bus. It runs as your user, never as root. While it runs it is
the only program that changes the mouse's settings (requirements §5);
`hid-lab lab write --write` refuses to run alongside it.

## When it applies

| Trigger | Event | Second check |
|---|---|---|
| `start` | the daemon starts with the mouse connected | yes |
| `connect` | udev `add` for the mouse: switched on, back from another Easy-Switch channel, Bluetooth on again, woke and reconnected | yes |
| `resume` | logind `PrepareForSleep(false)` with the mouse still connected | yes |
| `reapply` | `hid-lab reapply` | no |
| `set` | `hid-lab set` | no |
| `reset` | `hid-lab reset` | no |

For `start`, `connect` and `resume` the daemon pings the mouse every
200 ms until it answers (at most 5 s), applies and verifies by reading
back, and **reads every configured setting again 1 s later**. If something
overwrote a setting in between (for example another configurator), it logs
a warning and re-applies once. All of it is measured from the event;
`hid-lab status` shows the times, and the target is 2 s (requirements §7).
Only settings that differ from the wanted value are written.

Each HID++ request takes about 15 ms over Bluetooth (observation O-11), so
the daemon avoids repeating reads:

- Feature indices are resolved once per connection.
- Device capabilities (the DPI list and the 0x1B04 control list) are read
  once and kept while the daemon runs, for the same model. They only
  decide which values are accepted; every write goes to the index
  resolved on the current connection.
- SmartShift mode and threshold share one read.

Button remaps are lost when the mouse is switched off (O-10), so the daemon
writes them again after every reconnect, like the other settings.

## SmartShift mode follows you

The daemon keeps the mouse open read-only and watches for the 0x2121
ratchet event, which the mouse sends when you press the mode button (and
when the mode is written). On each event it reads the 0x2110 mode and, if
it changed, stores it in `state.toml` and emits `ModeChanged`. After a
reconnect it restores that mode if the mouse has another one, for example
because you changed it while the mouse was on another computer: each
computer keeps its own mode.

Because the mouse is open, the daemon also wakes for every movement report
and skips it; that is the cost of seeing the mode button.

## Battery

`status` includes the battery level and charging state that the kernel
driver publishes in `/sys/class/power_supply` (the daemon sends no HID++
request for it). The daemon listens to udev `power_supply` events, which
the kernel sends when the battery reports a change, and emits
`BatteryChanged` when the level or state differs from the last one; it
never polls. GNOME already warns about a low mouse battery (S-1), so the
daemon sends no notification of its own.

## Button presses

While a client holds a `WatchButtons` lease (at most 60 s, renewed by the
client), the daemon reports buttons going down and up with `ButtonPressed`.
It reads them from the mouse's standard input report, which it already
receives (report 0x02 with 16 button bits on the MX Master 3S over
Bluetooth, located from the report descriptor). Nothing is written and the
buttons keep working as usual. A remapped button reports the control whose
task it performs. The mode button is seen through the ratchet event, press
only. `hid-lab buttons` shows them in a terminal; the app uses the same
signal to highlight the button on the drawing.

## Install (no root)

From the source directory:

```bash
cargo install --path crates/hidlab-daemon --root ~/.local
cargo install --path crates/hidlab-cli --root ~/.local
install -Dm644 packaging/systemd/hid-labd.service ~/.config/systemd/user/hid-labd.service
install -Dm644 packaging/dbus/dev.ozkkar.Cursiva.Daemon1.service \
  ~/.local/share/dbus-1/services/dev.ozkkar.Cursiva.Daemon1.service
busctl --user call org.freedesktop.DBus /org/freedesktop/DBus org.freedesktop.DBus ReloadConfig
systemctl --user daemon-reload
systemctl --user enable --now hid-labd.service
```

This puts `hid-labd` and `hid-lab` in `~/.local/bin` and starts the daemon
at every login. The D-Bus service file also starts it on demand: a call
from the app or from `hid-lab` starts `hid-labd.service` if it is not
running (D-Bus activation), so a sandboxed app never has to start it. The `ReloadConfig` call makes the session bus read the new file now;
dbus-broker, Fedora's bus, otherwise reads it only at the next login
(found by test H5C-08). The udev rule from `docs/diagnosis.md` (T-001) must already
be installed; it is what lets your user open the mouse.

To stop it: `systemctl --user stop hid-labd.service`. To remove it:
`systemctl --user disable --now hid-labd.service`, then delete
`~/.config/systemd/user/hid-labd.service`,
`~/.local/share/dbus-1/services/dev.ozkkar.Cursiva.Daemon1.service` and the
two binaries. With the D-Bus file installed, a call to the daemon starts it
again even when it is disabled.

## Use

```bash
hid-lab status                         # what is wanted, what the mouse has
hid-lab set dpi 1600
hid-lab set smartshift-mode free-spin
hid-lab set smartshift-threshold 30
hid-lab set thumbwheel-invert on
hid-lab set smartshift-threshold off   # the wheel never switches by itself
hid-lab set smartshift-threshold on    # switching again, saved threshold
hid-lab set remap-gesture middle       # the gesture button middle-clicks
hid-lab set remap-gesture default      # its own task again
hid-lab set natural-scroll on          # GNOME setting, for every mouse
hid-lab set acceleration flat          # GNOME: default, flat or adaptive
hid-lab reset dpi                      # forget a setting, write the default
hid-lab reset all
hid-lab buttons                        # show presses for 30 s
hid-lab reapply                        # after editing config.toml
```

Every write to the mouse is read back. `status` reads the current values
from the mouse when you run it; it does not write.

`remap-<button>` takes `middle`, `back`, `forward`, `gesture` or `mode`
(the button behind the wheel). Targets are `left`, `right`, `middle`,
`back`, `forward`, `gesture`, `mode` or `default`; the mouse decides which
targets each button allows, and `status --json` lists them under
`capabilities`. A remap gives a button the task of another control: the
mouse itself does it, with no software running.

`reset` removes the setting from `config.toml`, so the daemon stops
applying it, and writes the profile default if the mouse is connected:
DPI 1000, threshold 10 with switching on, thumb wheel not inverted, every
button doing its own task (the values the mouse returns to after power-off,
P-033) and ratchet mode, which is also remembered in `state.toml`.

`natural-scroll` and `acceleration` are GNOME's own settings
(`org.gnome.desktop.peripherals.mouse`). The daemon reads and writes them
with `gsettings` so the app does not need access to dconf; GNOME stores
them and applies them to every mouse.

## Files

| Path | What |
|---|---|
| `~/.config/hid-lab/config.toml` | Configuration, see `docs/config-format.md` |
| `~/.local/state/hid-lab/daemon/state.toml` | Last SmartShift mode per device |
| `~/.config/systemd/user/hid-labd.service` | The unit, from `packaging/systemd/` |
| `~/.local/share/dbus-1/services/dev.ozkkar.Cursiva.Daemon1.service` | D-Bus activation, from `packaging/dbus/` |

## Logs

```bash
journalctl --user -u hid-labd.service
```

Mismatches and failures are logged at warning level. For raw HID++ bytes,
add `Environment=HIDLAB_LOG=debug` to the unit with
`systemctl --user edit hid-labd.service` and restart it.

## What it writes

Only the write functions in `CONFIRMED_WRITES`
(`crates/hidlab-features/src/allowlist.rs`): 0x2201 setSensorDpi, 0x2110
setRatchetControlMode and 0x2150 setThumbwheelReporting, confirmed by the
hardware tests H4 (`docs/protocol/README.md`, P-030 to P-032), and 0x1B04
setCidReporting, confirmed by H5 (P-036). The SmartShift threshold `off`
(255) was confirmed by H5 too (P-035). 0x1B04 is sent only to remap, with
the flags byte `00`, never to divert a button. A setting already at its
target is not written. Daemon requests use software ID `0x0C`; the CLI
uses `0x0B`.

Outside the mouse, the daemon writes only the two GNOME settings above,
and only when asked with `SetDesktop`.

## D-Bus API

Name `dev.ozkkar.Cursiva.Daemon1`, object `/dev/ozkkar/Cursiva/Daemon1`,
interface `dev.ozkkar.Cursiva.Daemon1`. Every answer is one JSON string,
except `WatchButtons`; the types are in `crates/hidlab-daemon/src/api.rs`.

| Member | Kind | Arguments | Returns |
|---|---|---|---|
| `GetStatus` | method | — | `hid-lab.daemon-status/3` |
| `Set` | method | setting, value (strings, as in `hid-lab set`) | `hid-lab.daemon-set/1` |
| `Reset` | method | setting name or `all` | `hid-lab.daemon-reset/1` |
| `SetDesktop` | method | `natural-scroll` or `acceleration`, value | `hid-lab.daemon-desktop/1` |
| `WatchButtons` | method | seconds, `u` (at most 60; 0 stops) | seconds granted, `u` |
| `Reapply` | method | — | `hid-lab.daemon-reapply/1` |
| `DeviceChanged` | signal | `hid-lab.daemon-device/1` | — |
| `Applied` | signal | apply record (trigger, report, times, second check); sent again when the second check finishes | — |
| `ModeChanged` | signal | `hid-lab.daemon-mode/1` | — |
| `BatteryChanged` | signal | `hid-lab.daemon-battery/1` | — |
| `ButtonPressed` | signal | `hid-lab.daemon-button/1`, only during a `WatchButtons` lease | — |

Status 3 adds, to status 2: `battery` (percent, level, status),
`capabilities` (DPI ranges, threshold range, remap targets per button,
reset defaults), `saved_threshold` (kept while SmartShift switching is
off, when `desired` holds 255) and `desktop` (the GNOME settings). The
remaps appear in `desired` and `current` like the other settings, as
`remap-<button>`.

Errors are `org.freedesktop.DBus.Error.Failed` with a message that says
what to do.

```bash
busctl --user call dev.ozkkar.Cursiva.Daemon1 /dev/ozkkar/Cursiva/Daemon1 \
  dev.ozkkar.Cursiva.Daemon1 GetStatus
```

## Other configurators

At start and before each apply, the daemon looks for running Solaar or
`logid` processes and logs a warning with how to stop them; `hid-lab
status` shows the same warning. Two programs configuring one mouse give
unpredictable results.
