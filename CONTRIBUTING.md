# Contributing to Cursiva

Thanks for helping. Issues and pull requests are welcome in English or
Spanish (*se aceptan issues y pull requests en español o en inglés*). Code,
comments, commit messages and documentation are written in English.

This project sends commands to real hardware. Most of the rules below exist
so that no change can ever put an unknown byte on someone's mouse.

## Ways to help

- **Report a bug.** Include your distribution, kernel (`uname -r`), desktop,
  connection (Bluetooth or receiver), and the output of `hid-lab status`.
  Remove anything personal first (see [Privacy](#privacy)).
- **Test on your hardware.** Run the checklists in
  [docs/hardware-test.md](docs/hardware-test.md) and report what you saw.
- **Share captures from other devices.** `hid-lab lab features --json` and
  `hid-lab lab read --json` are read-only and redact identifiers by default.
- **Translate.** The desktop app uses gettext; English and Spanish come
  first. Translation files will live in `po/` when the app lands.
- **Write code.** Open an issue first for anything larger than a fix, so we
  can agree on the approach.

## Building and checking

You need Rust stable (1.85 or newer) and `systemd-devel` (for libudev).

Before every commit:

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

The test suite needs no hardware: it uses a simulated HID++ device. Test
fixtures that are not real captures are labeled SYNTHETIC.

## Architecture rules

The dependency direction is fixed:

```text
gtk / cli ─► D-Bus ─► daemon ─► devices ─► features ─► hidpp ─► transport
```

- The daemon is the only program that talks to the mouse. The CLI and the
  app call it over D-Bus; the app contains no device logic.
- `hidlab-hidpp` and `hidlab-features` are generic HID++ 2.0. Anything
  specific to one mouse goes in `hidlab-devices`.
- No root at runtime, ever. Do not suggest `sudo` as a workaround.
- Disconnects are normal events: no panics on expected failures.
- Errors say what happened, the likely cause and the exact command to try.

The full requirements are in [docs/requirements.md](docs/requirements.md);
its decision log records why things are the way they are.

## Protocol evidence

Every protocol fact goes in [docs/protocol/README.md](docs/protocol/README.md)
with a status, a source and evidence:

| Status | Meaning |
|---|---|
| CONFIRMED | Verified on the real device, with more than one value where possible, and reproducible |
| LIKELY | Consistent with a reference implementation and one observation |
| HYPOTHESIS | Inferred, not tested |
| UNKNOWN | Not known |

- **The daemon only writes with CONFIRMED functions.** A new write function
  is confirmed with `hid-lab lab write` (explicit `--write`, original value
  saved, every byte logged, read-back, `--restore`) and a physical test.
- A function that no source documents is never called: it could be a write.
- References are the Linux kernel driver (`drivers/hid/hid-logitech-hidpp.c`,
  GPL-2.0-only) and Solaar (GPL-2.0-or-later). Read them and describe the
  behavior in your own words; never copy their code.
- Never fabricate device data.

## Privacy

The Bluetooth address (`HID_UNIQ`) and the firmware unit ID are redacted by
default in every output. Before attaching a capture or log to an issue or a
pull request, check it for serial numbers, your username and home paths
(replace them with `~`), and say in the file what you removed.

## Commits and pull requests

- [Conventional Commits](https://www.conventionalcommits.org/) in English:
  `feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`, with an optional
  scope such as `feat(daemon):`.
- One concern per commit; keep them small.
- Update `docs/` in the same pull request when behavior changes.

## License

By contributing you agree that your contribution is licensed under
GPL-3.0-or-later, the license of this project.
