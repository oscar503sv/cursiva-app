# hid-labd, the user daemon

`hid-labd` applies the configuration to the mouse and serves a D-Bus API on
the session bus. It runs as your user, never as root. While it runs it is
the only program that changes the mouse's settings (requirements §5);
`hid-lab lab write --write` refuses to run alongside it.

Milestone 4b status: the daemon applies the configuration when it starts,
on `hid-lab set` and on `hid-lab reapply`. It notices when the mouse
connects and disconnects, but **does not yet re-apply after a reconnect**;
that is phase 4c (requirements §7).

## Install (no root)

From the source directory:

```bash
cargo install --path crates/hidlab-daemon --root ~/.local
cargo install --path crates/hidlab-cli --root ~/.local
install -Dm644 packaging/systemd/hid-labd.service ~/.config/systemd/user/hid-labd.service
systemctl --user daemon-reload
systemctl --user enable --now hid-labd.service
```

This puts `hid-labd` and `hid-lab` in `~/.local/bin` and starts the daemon
at every login. The udev rule from `docs/diagnosis.md` (T-001) must already
be installed; it is what lets your user open the mouse.

To stop it: `systemctl --user stop hid-labd.service`. To remove it:
`systemctl --user disable --now hid-labd.service`, then delete
`~/.config/systemd/user/hid-labd.service` and the two binaries.

## Use

```bash
hid-lab status                         # what is wanted, what the mouse has
hid-lab set dpi 1600
hid-lab set smartshift-mode free-spin
hid-lab set smartshift-threshold 30
hid-lab set thumbwheel-invert on
hid-lab reapply                        # after editing config.toml
```

Every write is read back. `status` reads the current values from the mouse
when you run it; it does not write.

## Files

| Path | What |
|---|---|
| `~/.config/hid-lab/config.toml` | Configuration, see `docs/config-format.md` |
| `~/.local/state/hid-lab/daemon/state.toml` | Last SmartShift mode per device |
| `~/.config/systemd/user/hid-labd.service` | The unit, from `packaging/systemd/` |

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
setRatchetControlMode and 0x2150 setThumbwheelReporting, each confirmed by
the hardware tests H4 (`docs/protocol/README.md`, P-030 to P-032). A setting
already at its target is not written. Daemon requests use software ID
`0x0C`; the CLI uses `0x0B`.

## D-Bus API

Name `dev.ozkkar.Cursiva.Daemon1`, object `/dev/ozkkar/Cursiva/Daemon1`,
interface `dev.ozkkar.Cursiva.Daemon1`. Every answer is one JSON string;
the types are in `crates/hidlab-daemon/src/api.rs`.

| Member | Kind | Arguments | Returns |
|---|---|---|---|
| `GetStatus` | method | — | `hid-lab.daemon-status/1` |
| `Set` | method | setting, value (strings, as in `hid-lab set`) | `hid-lab.daemon-set/1` |
| `Reapply` | method | — | `hid-lab.daemon-reapply/1` |
| `DeviceChanged` | signal | `hid-lab.daemon-device/1` | — |
| `Applied` | signal | apply report | — |

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
