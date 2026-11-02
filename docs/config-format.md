# Configuration file format

File: `$XDG_CONFIG_HOME/hid-lab/config.toml`, by default
`~/.config/hid-lab/config.toml`. Read by `hid-labd` at start and on
`hid-lab reapply`. This page is the final schema for `format_version = 1`;
requirements §15 shows an earlier illustration.

```toml
format_version = 1

[device."mx-master-3s"]
dpi = 1600
smartshift = { mode = "ratchet", threshold = 30, auto = false }
thumbwheel = { invert = false }

[device."mx-master-3s".buttons]
gesture = "middle"
back = "default"
```

| Key | Values | Meaning |
|---|---|---|
| `format_version` | `1` | Required. |
| `[device."<key>"]` | `mx-master-3s` | One table per device profile in `hidlab-devices`. |
| `dpi` | a value from the mouse's DPI list (200 to 8000 in steps of 50 on the MX Master 3S) | Pointer resolution. A value outside the list is reported as rejected and not sent. |
| `smartshift.threshold` | `1` to `50` | How easily the wheel switches from clicks to free spin; lower is easier. |
| `smartshift.auto` | `true` or `false` | `false`: the wheel never switches by itself (threshold 255 is sent, P-035) and `threshold` is kept for when it is turned back on. Missing or `true`: `threshold` is sent. |
| `smartshift.mode` | `"ratchet"` or `"free-spin"` | **Initial** mode only, see below. |
| `thumbwheel.invert` | `true` or `false` | Reverses the thumb wheel. |
| `buttons.<button>` | `"default"` or a target: `"left"`, `"right"`, `"middle"`, `"back"`, `"forward"`, `"gesture"`, `"mode"` | What the button does. Buttons: `middle`, `back`, `forward`, `gesture`, `mode` (the button behind the wheel). The mouse decides which targets each button allows; a target it refuses is reported as rejected and not sent. `"default"` is written too, so the button is put back if something else remapped it. |

Every key except `format_version` is optional. The daemon never writes a
setting that is not in the file.

Not here on purpose:

- Main-wheel scroll direction and pointer acceleration: GNOME settings,
  which `hid-lab set natural-scroll` and `hid-lab set acceleration` change
  through the daemon. GNOME stores them.
- The 0x2121 wheel mode (high resolution): the kernel driver owns it.

## SmartShift mode

The mode follows the user. The last mode chosen with `hid-lab set
smartshift-mode` or with the mouse's mode button is kept in
`$XDG_STATE_HOME/hid-lab/daemon/state.toml`. `smartshift.mode` in this file
is used only while that state file has no mode. Each computer keeps its own
mode.

## Errors

The daemon checks the whole file: unknown keys, unknown devices, a missing
or different `format_version`, and out-of-range values are errors. On an
error the daemon logs it, keeps the last valid configuration (none at
start), and `hid-lab status` shows the message. `hid-lab set` refuses to
change a file that has an error, so your text is never overwritten.

## Changes made with `hid-lab set` and `hid-lab reset`

`hid-lab set` rewrites this file with the new value; `hid-lab reset`
removes the key (for `smartshift.threshold`, also `auto`). Comments are
not kept. `hid-lab set smartshift-threshold off` sets `auto = false`;
`on` or a number removes `auto`.
