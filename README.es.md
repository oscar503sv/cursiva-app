# Cursiva

[English](README.md) · **Español**

Configura tu Logitech MX Master 3S en Linux una vez y olvídate, como funciona
Logi Options+ en Windows.

Cursiva es una app nativa de GNOME más un pequeño servicio en segundo plano.
El servicio aplica tus ajustes al ratón y los vuelve a poner solo cuando el
ratón se apaga, se duerme, se reconecta o cambia de canal Easy-Switch, y
cuando la computadora se reinicia o sale de la suspensión.

> **Estado: desarrollo temprano.** El servicio y la herramienta de línea de
> comandos ya funcionan por Bluetooth. La app de escritorio está en
> construcción: muestra el ratón y su estado; sus páginas de ajustes siguen
> las [maquetas](docs/mockups/index.html). Todavía no hay paquetes.

## Qué funciona hoy

Probado en Fedora 44, GNOME en Wayland, MX Master 3S por Bluetooth:

| Ajuste | Cómo |
|---|---|
| Velocidad del puntero (DPI, de 200 a 8000) | se escribe en el ratón |
| SmartShift: con clics o libre, con qué empujón cambia, o que nunca cambie | se escribe en el ratón |
| Dirección de la rueda del pulgar | se escribe en el ratón |
| Reasignación de botones: central, atrás, adelante, gestos y modo pueden hacer la tarea de otro botón | se escribe en el ratón |
| Dirección del desplazamiento y aceleración del puntero | ajustes de GNOME, cambiados a través del servicio |
| Nivel de batería y carga | lo muestra `hid-lab status` |
| Pulsaciones de botones, en vivo | las muestra `hid-lab buttons` |

Tras cada reconexión, despertar o reinicio, el servicio vuelve a aplicar
los ajustes del ratón y los verifica dos veces, en menos de 2 segundos desde el
evento. El modo de SmartShift sigue al botón de modo: se restaura lo último
que elegiste.

Lo que viene, en orden: la app de escritorio (la primera versión), luego
acciones de botones como combinaciones de teclas, gestos y acciones de la
rueda del pulgar, perfiles por aplicación, soporte del receptor Logi Bolt
y paquetes RPM. El alcance completo está en
[docs/requirements.md](docs/requirements.md) (en inglés).

## Cómo está hecho

- **Primero la evidencia.** Cada byte que se envía al ratón está respaldado
  por una observación registrada en el dispositivo real
  ([docs/protocol](docs/protocol/README.md)). El servicio solo usa funciones
  de escritura confirmadas con pruebas en el hardware.
- **Sin root al usarlo.** Una regla udev da a tu sesión acceso a los
  dispositivos Logitech; el servicio corre como tu usuario.
- **Un solo dueño.** El servicio es el único programa que habla con el ratón.
  La app y la línea de comandos le piden las cosas por D-Bus, así que cerrar
  la app no cambia nada.
- **Convive con el kernel.** El driver `hid-logitech-hidpp` sigue a cargo de
  lo que ya maneja, como el desplazamiento de alta resolución y la batería.

```text
cursiva (GTK) ─┐
hid-lab (CLI) ─┴─► D-Bus ─► hid-labd ─► devices ─► features ─► hidpp ─► transport ─► /dev/hidraw
```

## Pruébalo desde el código

Necesitas Rust (estable, 1.92 o más nuevo), las cabeceras de desarrollo de
systemd y, para la app, GTK 4 y Libadwaita (GNOME 48 o más nuevo). En Fedora:

```bash
sudo dnf install cargo rust systemd-devel gtk4-devel libadwaita-devel gettext
git clone https://github.com/oscar503sv/cursiva-app.git
cd cursiva-app
```

1. Instala la regla udev, una sola vez. Permite que el usuario de la sesión
   local abra dispositivos Logitech; no añade nada más.

   ```bash
   sudo cp packaging/udev/70-hid-lab-logitech.rules /etc/udev/rules.d/
   sudo udevadm control --reload
   sudo udevadm trigger --subsystem-match=hidraw --action=change
   ```

2. Instala el servicio y la línea de comandos en `~/.local`, sin root:

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

3. Úsalo:

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

   `hid-lab set --help` muestra todos los ajustes.

4. Prueba la app (temprana: muestra el ratón, su batería y los ajustes
   activos; cambiar ajustes desde la app llega después). Desde la carpeta
   del código:

   ```bash
   cargo run -p hidlab-gtk
   ```

   O instálala con su entrada en el menú y su ícono:

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

Cierra Solaar o logiops antes: dos programas configurando el mismo ratón dan
resultados impredecibles. `hid-lab status` te avisa si alguno está corriendo.

Más información (en inglés): [el servicio](docs/daemon.md),
[archivo de configuración](docs/config-format.md),
[pruebas en el hardware](docs/hardware-test.md).

## Contribuir

Reportes de errores, capturas de otros dispositivos, traducciones y código
son bienvenidos, en español o en inglés. Lee primero
[CONTRIBUTING.md](CONTRIBUTING.md) (en inglés): explica las reglas de
evidencia que protegen al ratón.

## Licencia

GPL-3.0 o posterior. Mira [LICENSE](LICENSE).

Logitech, MX Master, Logi Bolt y Logi Options+ son marcas de Logitech.
Cursiva es un proyecto independiente, sin relación con Logitech ni respaldo
suyo. El dibujo del ratón es original.
