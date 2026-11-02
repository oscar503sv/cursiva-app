# Cursiva

**English** · [Español](README.es.md)

Configure your Logitech MX Master 3S on Linux once and forget about it, the
way Logi Options+ works on Windows.

Cursiva is a native GNOME app plus a small background service. The service
applies your settings to the mouse and puts them back by itself after the
mouse is switched off, sleeps, reconnects, changes Easy-Switch channel, or
the computer reboots or wakes up.

> **Status: early development.** The service and the command-line tool work
> today over Bluetooth. The desktop app is being built: it shows the mouse
> and its state; its settings pages follow the
> [mockups](docs/mockups/index.html). Nothing here is packaged yet.

## What works today

Tested on Fedora 44, GNOME on Wayland, MX Master 3S over Bluetooth:

| Setting | How |
|---|---|
| Pointer speed (DPI, 200 to 8000) | written to the mouse |
| SmartShift: ratchet or free spin, how hard a push switches it, or never | written to the mouse |
| Thumb-wheel direction | written to the mouse |
| Button remapping: middle, back, forward, gesture and mode buttons can do another button's job | written to the mouse |
| Scroll direction and pointer acceleration | GNOME's settings, changed through the service |
| Battery level and charging state | shown by `hid-lab status` |
| Button presses, live | shown by `hid-lab buttons` |

After every reconnect, wake-up or reboot the service re-applies the mouse
settings and checks them twice, within 2 seconds of the event. The SmartShift
mode follows the mode button: whatever you last chose is restored.

Planned, in order: the desktop app (the first release), then button
actions such as key combinations, gestures and thumb-wheel actions, per-app
profiles, Logi Bolt receiver support, and RPM packaging. The full scope is in
[docs/requirements.md](docs/requirements.md).

## How it is built

- **Evidence first.** Every byte sent to the mouse is backed by a recorded
  observation on the real device ([docs/protocol](docs/protocol/README.md)).
  The service only uses write functions confirmed by hardware tests.
- **No root at runtime.** A udev rule gives your login session access to
  Logitech devices; the service runs as your user.
- **One owner.** The service is the only program that talks to the mouse.
  The app and the CLI ask it over D-Bus, so closing the app changes nothing.
- **Coexists with the kernel.** The `hid-logitech-hidpp` driver stays in
  charge of what it already handles, such as high-resolution scrolling and
  battery reporting.

```text
cursiva (GTK) ─┐
hid-lab (CLI) ─┴─► D-Bus ─► hid-labd ─► devices ─► features ─► hidpp ─► transport ─► /dev/hidraw
```

## Try it from source

You need Rust (stable, 1.92 or newer), the systemd development headers and,
for the app, GTK 4 and Libadwaita (GNOME 48 or newer). On Fedora:

```bash
sudo dnf install cargo rust systemd-devel gtk4-devel libadwaita-devel gettext
git clone https://github.com/oscar503sv/cursiva-app.git
cd cursiva-app
```

1. Install the udev rule, once. It lets the user at the local session open
   Logitech devices; it adds nothing else.

   ```bash
   sudo cp packaging/udev/70-hid-lab-logitech.rules /etc/udev/rules.d/
   sudo udevadm control --reload
   sudo udevadm trigger --subsystem-match=hidraw --action=change
   ```

2. Install the service and the CLI in `~/.local`, without root:

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

3. Use it:

   ```bash
   hid-lab status
   hid-lab set dpi 1600
   hid-lab set smartshift-mode free-spin
   hid-lab set smartshift-threshold 30
   hid-lab set thumbwheel-invert on
   hid-lab set remap-gesture middle
   hid-lab set natural-scroll on
   hid-lab reset all
   ```

   `hid-lab set --help` lists every setting.

4. Try the app (early: it shows the mouse, its battery and the active
   settings; changing settings from the app comes next). From the source
   folder:

   ```bash
   cargo run -p hidlab-gtk
   ```

   Or install it with its menu entry and icon:

   ```bash
   CURSIVA_LOCALEDIR=$HOME/.local/share/locale cargo install --path crates/hidlab-gtk --root ~/.local
   for po in po/*.po; do
     lang=$(basename "$po" .po)
     install -d ~/.local/share/locale/$lang/LC_MESSAGES
     msgfmt -o ~/.local/share/locale/$lang/LC_MESSAGES/cursiva.mo "$po"
   done
   sed "s|^Exec=cursiva|Exec=$HOME/.local/bin/cursiva|" data/dev.ozkkar.Cursiva.desktop \
     > ~/.local/share/applications/dev.ozkkar.Cursiva.desktop
   install -Dm644 data/dev.ozkkar.Cursiva.metainfo.xml ~/.local/share/metainfo/dev.ozkkar.Cursiva.metainfo.xml
   install -Dm644 data/icons/hicolor/scalable/apps/dev.ozkkar.Cursiva.svg \
     ~/.local/share/icons/hicolor/scalable/apps/dev.ozkkar.Cursiva.svg
   install -Dm644 data/icons/hicolor/symbolic/apps/dev.ozkkar.Cursiva-symbolic.svg \
     ~/.local/share/icons/hicolor/symbolic/apps/dev.ozkkar.Cursiva-symbolic.svg
   ```

Close Solaar or logiops first: two programs configuring one mouse give
unpredictable results. `hid-lab status` warns you if one is running.

More: [the service](docs/daemon.md), [config file](docs/config-format.md),
[hardware tests](docs/hardware-test.md).

## Contributing

Bug reports, captures from other devices, translations and code are all
welcome, in English or Spanish. Please read [CONTRIBUTING.md](CONTRIBUTING.md)
first: it explains the evidence rules that keep the mouse safe.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

Logitech, MX Master, Logi Bolt and Logi Options+ are trademarks of Logitech.
Cursiva is an independent project, not affiliated with or endorsed by
Logitech. The mouse illustration is original artwork.
