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
  first. See [Translations](#translations).
- **Write code.** Open an issue first for anything larger than a fix, so we
  can agree on the approach.

## Building and checking

You need Rust stable (1.92 or newer; the daemon and CLI alone build with
1.85) and, on Fedora:

```bash
sudo dnf install systemd-devel gtk4-devel libadwaita-devel gettext
```

`systemd-devel` is for libudev; the other three are for the app
(`hidlab-gtk`, GNOME 48 or newer).

Before every commit:

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

The test suite needs no hardware: it uses a simulated HID++ device. Test
fixtures that are not real captures are labeled SYNTHETIC. The app's tests
need no display either; one renders the mouse illustration offscreen when
a display is available, and saves it with
`CURSIVA_RENDER_DIR=<dir> cargo test -p hidlab-gtk render`.

To look at a whole page without a desktop session, debug builds of the
app save a screenshot and quit when `CURSIVA_DEV_SCREENSHOT` is set. With
GTK's Broadway backend and a private session bus (start `hid-labd` on
that bus too, with `XDG_CONFIG_HOME` and `XDG_STATE_HOME` pointing at an
empty test directory, to see the connected pages):

```bash
gtk4-broadwayd :5 &
GDK_BACKEND=broadway BROADWAY_DISPLAY=:5 CURSIVA_DEV_SCREENSHOT=/tmp/pointer.png \
  CURSIVA_DEV_PAGE=pointer CURSIVA_DEV_SIZE=1000x1400 cargo run -p hidlab-gtk
```

`CURSIVA_DEV_PAGE` is `home`, `pointer`, `pointer:wheel`, `pointer:thumb`,
`buttons` or `buttons:<button>` (`middle`, `back`, `forward`, `gesture`,
`mode`: the page with that button's action picker open), `preferences` or
`about` (that dialog over Home); `CURSIVA_DEV_DELAY_MS` (default 3000) is the wait before the
capture. `CURSIVA_DEV_STATUS=<file>` makes the app read the status from a
saved `hid-lab status --json` instead of the daemon, to see states the
mouse is not in (edit `device.access` to `permission_denied`, set
`device` to null, or add a `conflicts` entry). Keep such files out of the
repository: they hold your device's data. Broadway's socket lives in `$XDG_RUNTIME_DIR`; a path longer than
about 100 characters makes it fail with "address already in use".

## Architecture rules

The dependency direction is fixed:

```text
gtk / cli ─► D-Bus ─► daemon ─► devices ─► features ─► hidpp ─► transport
```

- The daemon is the only program that talks to the mouse. The CLI and the
  app call it over D-Bus; the app contains no device logic.
- `hidlab-api` is the daemon's public contract (D-Bus names, JSON types,
  setting names). It depends on serde only; change it with care and bump
  the schema string of any JSON document whose shape changes.
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

## Translations

The app's text is in English in the code, marked with `gettext("...")`,
and translated with gettext (domain `cursiva`). The files are in `po/`:
`POTFILES` lists the source files, `cursiva.pot` is the template, and
`<lang>.po` is each translation. `cargo build` compiles them, so
`cargo run -p hidlab-gtk` shows your translation; try another language with
`LANGUAGE=<lang> cargo run -p hidlab-gtk`.

After changing text in the code, refresh the template and the translations:

```bash
xgettext --language=Rust --from-code=UTF-8 --keyword=gettext \
  --add-comments=TRANSLATORS --package-name=Cursiva \
  --msgid-bugs-address=https://github.com/oscar503sv/cursiva-app/issues \
  --files-from=po/POTFILES -o po/cursiva.pot
for po in po/*.po; do msgmerge --update --backup=none "$po" po/cursiva.pot; done
```

To add a language, for example French:

```bash
msginit --input=po/cursiva.pot --locale=fr --output-file=po/fr.po --no-translator
echo fr >> po/LINGUAS
```

Then translate the entries (Gtranslator or Poedit work well) and check
with `msgfmt --check --statistics -o /dev/null po/fr.po`. Keep `{name}`
placeholders and commands between backticks exactly as they are; the
`TRANSLATORS:` notes in the template say what each one means. The
`.desktop` file and the AppStream metainfo in `data/` carry their
translations inline (`Name[fr]=`, `xml:lang="fr"`).

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
