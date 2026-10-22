# Configuration file format

File: `$XDG_CONFIG_HOME/hid-lab/config.toml`, by default
`~/.config/hid-lab/config.toml`. Read by `hid-labd` at start and on
`hid-lab reapply`. This page is the final schema for `format_version = 1`;
requirements §15 shows an earlier illustration.

```toml
format_version = 1

[device."mx-master-3s"]
dpi = 1600
smartshift = { mode = "ratchet", threshold = 30 }
thumbwheel = { invert = false }
```

| Key | Values | Meaning |
|---|---|---|
| `format_version` | `1` | Required. |
| `[device."<key>"]` | `mx-master-3s` | One table per device profile in `hidlab-devices`. |
| `dpi` | a value from the mouse's DPI list (200 to 8000 in steps of 50 on the MX Master 3S) | Pointer resolution. A value outside the list is reported as rejected and not sent. |
| `smartshift.threshold` | `1` to `50` | How easily the wheel switches from clicks to free spin; lower is easier. |
| `smartshift.mode` | `"ratchet"` or `"free-spin"` | **Initial** mode only, see below. |
| `thumbwheel.invert` | `true` or `false` | Reverses the thumb wheel. |

Every key except `format_version` is optional. The daemon never writes a
setting that is not in the file.

Not here on purpose:

- Main-wheel scroll direction: use GNOME's natural-scrolling setting.
- The 0x2121 wheel mode (high resolution): the kernel driver owns it.

## SmartShift mode

The mode follows the user. The last mode chosen with `hid-lab set
smartshift-mode` (and, from phase 4c on, with the mode button) is kept in
`$XDG_STATE_HOME/hid-lab/daemon/state.toml`. `smartshift.mode` in this file
is used only while that state file has no mode. Each computer keeps its own
mode.

## Errors

The daemon checks the whole file: unknown keys, unknown devices, a missing
or different `format_version`, and out-of-range values are errors. On an
error the daemon logs it, keeps the last valid configuration (none at
start), and `hid-lab status` shows the message. `hid-lab set` refuses to
change a file that has an error, so your text is never overwritten.

## Changes made with `hid-lab set`

`hid-lab set` rewrites this file with the new value. Comments are not kept.
