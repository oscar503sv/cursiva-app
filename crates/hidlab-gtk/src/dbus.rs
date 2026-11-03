//! Client of hid-labd over the session bus, with gio on the GTK main loop.
//! Calls to the daemon's name start it through D-Bus activation when it is
//! not running.

use gettextrs::gettext;
use gtk::gio;
use gtk::glib;
use gtk::glib::prelude::ToVariant;
use hidlab_api::daemon::{
    ApplyReport, BUS_NAME, BatteryEvent, ButtonEvent, INTERFACE, ItemResult, OBJECT_PATH,
    ResetReply, SetReply, StartReply, Status, UNIT,
};

use crate::changes::Op;
use crate::model::fill;

/// Longest wait for the daemon; the first call may start it.
const TIMEOUT_MS: i32 = 25_000;

/// A signal from the daemon that the window cares about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Something changed that `GetStatus` shows: device, applied settings
    /// or SmartShift mode.
    Changed,
    Battery(BatteryEvent),
    /// A button went down or up, while a `WatchButtons` lease lasts.
    Button(ButtonEvent),
    /// The daemon's name got an owner.
    DaemonStarted,
    /// The daemon's name lost its owner.
    DaemonStopped,
}

pub async fn session() -> Result<gio::DBusConnection, String> {
    gio::bus_get_future(gio::BusType::Session)
        .await
        .map_err(|e| format!("cannot connect to the D-Bus session bus: {e}"))
}

/// Calls `GetStatus` and parses its JSON.
pub async fn status(conn: &gio::DBusConnection) -> Result<Status, String> {
    // Development aid, debug builds only: show a saved status instead
    // (screenshots of states the real mouse is not in; see CONTRIBUTING.md).
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("CURSIVA_DEV_STATUS") {
        let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        return parse_status(&json);
    }
    let reply = conn
        .call_future(
            Some(BUS_NAME),
            OBJECT_PATH,
            INTERFACE,
            "GetStatus",
            None,
            Some(glib::VariantTy::new("(s)").expect("valid type")),
            gio::DBusCallFlags::NONE,
            TIMEOUT_MS,
        )
        .await
        .map_err(|e| call_error(&e))?;
    let json: String = reply.child_value(0).get().unwrap_or_default();
    parse_status(&json)
}

pub fn parse_status(json: &str) -> Result<Status, String> {
    serde_json::from_str(json).map_err(|e| {
        fill(
            &gettext(
                // TRANSLATORS: {error} is a technical parse error; {unit} is
                // hid-labd.service. Keep the command between backticks as is.
                "The Cursiva service sent a status this app does not understand ({error}). \
                 The service and the app are probably different versions: install both \
                 from the same build, then run `systemctl --user restart {unit}`",
            ),
            &[("error", &e.to_string()), ("unit", UNIT)],
        )
    })
}

/// Turns a D-Bus error into an actionable message.
fn call_error(e: &glib::Error) -> String {
    let remote = gio::DBusError::remote_error(e).unwrap_or_default();
    let text = match remote.as_str() {
        "org.freedesktop.DBus.Error.ServiceUnknown" => gettext(
            // TRANSLATORS: {unit} is hid-labd.service. Keep commands as is.
            "The Cursiva service (hid-labd) is not installed for this user, or the session \
             bus has not read its service file yet. Install it as the README says, then run \
             `systemctl --user enable --now {unit}`",
        ),
        "org.freedesktop.DBus.Error.Spawn.ChildExited"
        | "org.freedesktop.DBus.Error.Spawn.Failed"
        | "org.freedesktop.systemd1.LoadFailed" => gettext(
            // TRANSLATORS: {unit} is hid-labd.service. Keep the command as is.
            "The Cursiva service could not start. See why with \
             `journalctl --user -u {unit} -n 50`",
        ),
        _ => gettext(
            // TRANSLATORS: {error} is the D-Bus error message; {unit} is
            // hid-labd.service. Keep the command as is.
            "The Cursiva service did not answer ({error}). Check it with \
             `systemctl --user status {unit}`",
        ),
    };
    fill(&text, &[("unit", UNIT), ("error", e.message())])
}

/// Asks the daemon to send `ButtonPressed` for `seconds` (0 stops); returns
/// the seconds granted.
pub async fn watch_buttons(conn: &gio::DBusConnection, seconds: u32) -> Result<u32, String> {
    let reply = conn
        .call_future(
            Some(BUS_NAME),
            OBJECT_PATH,
            INTERFACE,
            "WatchButtons",
            Some(&(seconds,).to_variant()),
            Some(glib::VariantTy::new("(u)").expect("valid type")),
            gio::DBusCallFlags::NONE,
            TIMEOUT_MS,
        )
        .await
        .map_err(|e| refused(&e))?;
    Ok(reply.child_value(0).get().unwrap_or_default())
}

/// Calls a method of the daemon that answers one string.
async fn call_text(
    conn: &gio::DBusConnection,
    method: &str,
    args: Option<&glib::Variant>,
) -> Result<String, String> {
    let reply = conn
        .call_future(
            Some(BUS_NAME),
            OBJECT_PATH,
            INTERFACE,
            method,
            args,
            Some(glib::VariantTy::new("(s)").expect("valid type")),
            gio::DBusCallFlags::NONE,
            TIMEOUT_MS,
        )
        .await
        .map_err(|e| refused(&e))?;
    Ok(reply.child_value(0).get().unwrap_or_default())
}

/// The daemon's sanitized diagnostics report.
pub async fn diagnostics(conn: &gio::DBusConnection) -> Result<String, String> {
    call_text(conn, "GetDiagnostics", None).await
}

/// Turns starting at login on or off; returns the state systemd reports
/// afterwards.
pub async fn set_start_at_login(
    conn: &gio::DBusConnection,
    on: bool,
) -> Result<Option<bool>, String> {
    let json = call_text(conn, "SetStartAtLogin", Some(&(on,).to_variant())).await?;
    serde_json::from_str::<StartReply>(&json)
        .map(|r| r.start_at_login)
        .map_err(|e| e.to_string())
}

/// Opens the Bluetooth panel of GNOME Settings through its
/// `launch-panel` action.
pub async fn open_bluetooth_settings(conn: &gio::DBusConnection) -> Result<(), String> {
    let panel = ("bluetooth", Vec::<glib::Variant>::new()).to_variant();
    let args = (
        "launch-panel",
        vec![glib::Variant::from_variant(&panel)],
        std::collections::HashMap::<String, glib::Variant>::new(),
    )
        .to_variant();
    conn.call_future(
        Some("org.gnome.Settings"),
        "/org/gnome/Settings",
        "org.gtk.Actions",
        "Activate",
        Some(&args),
        None,
        gio::DBusCallFlags::NONE,
        TIMEOUT_MS,
    )
    .await
    .map(|_| ())
    .map_err(|e| e.message().to_owned())
}

/// Runs one change call and checks the daemon's answer.
pub async fn run(conn: &gio::DBusConnection, op: &Op) -> Result<(), String> {
    let (method, args) = op.call();
    let args: Vec<glib::Variant> = args
        .iter()
        .map(glib::prelude::ToVariant::to_variant)
        .collect();
    let reply = conn
        .call_future(
            Some(BUS_NAME),
            OBJECT_PATH,
            INTERFACE,
            method,
            Some(&glib::Variant::tuple_from_iter(args)),
            Some(glib::VariantTy::new("(s)").expect("valid type")),
            gio::DBusCallFlags::NONE,
            TIMEOUT_MS,
        )
        .await
        .map_err(|e| refused(&e))?;
    let json: String = reply.child_value(0).get().unwrap_or_default();
    outcome(method, &json)
}

/// The daemon's own message for a refused call (an `fdo::Error::Failed`).
fn refused(e: &glib::Error) -> String {
    if gio::DBusError::remote_error(e).as_deref() == Some("org.freedesktop.DBus.Error.Failed") {
        let mut e = e.clone();
        gio::DBusError::strip_remote_error(&mut e);
        e.message().to_owned()
    } else {
        call_error(e)
    }
}

/// Reads a `Set` or `Reset` reply: an error when the mouse refused the
/// value or the read-back did not match. Other replies are fine.
pub fn outcome(method: &str, json: &str) -> Result<(), String> {
    let applied = match method {
        "Set" => {
            let r: SetReply = serde_json::from_str(json).map_err(|e| e.to_string())?;
            if r.saved_to.is_none() {
                return Err(r.note.unwrap_or_else(|| gettext("Nothing was saved")));
            }
            r.applied
        }
        "Reset" => {
            let r: ResetReply = serde_json::from_str(json).map_err(|e| e.to_string())?;
            r.applied
        }
        _ => None,
    };
    applied.as_ref().map_or(Ok(()), report_error)
}

fn report_error(report: &ApplyReport) -> Result<(), String> {
    if report.ok {
        return Ok(());
    }
    let item = report
        .items
        .iter()
        .find(|i| !matches!(i.result, ItemResult::Unchanged | ItemResult::Written));
    Err(match item {
        Some(i) => match &i.error {
            Some(e) => e.clone(),
            None => fill(
                // TRANSLATORS: {setting} is a setting name such as `dpi`.
                &gettext("The mouse did not keep {setting}"),
                &[("setting", i.setting.name())],
            ),
        },
        None => gettext("The mouse did not keep the change"),
    })
}

/// Handles that keep the subscriptions alive; dropping them unsubscribes.
pub struct Subscriptions {
    _signals: Vec<gio::SignalSubscription>,
    /// Stops the name watch. A closure, because gio exports two types
    /// named `WatcherId` and the name-watcher one cannot be named.
    unwatch: Option<Box<dyn FnOnce()>>,
}

impl Drop for Subscriptions {
    fn drop(&mut self) {
        if let Some(unwatch) = self.unwatch.take() {
            unwatch();
        }
    }
}

/// Subscribes to the daemon's signals and to its name owner.
pub fn subscribe(
    conn: &gio::DBusConnection,
    on_event: impl Fn(Event) + Clone + 'static,
) -> Subscriptions {
    let signals = [
        "DeviceChanged",
        "Applied",
        "ModeChanged",
        "BatteryChanged",
        "ButtonPressed",
    ]
    .into_iter()
    .map(|member| {
        let on_event = on_event.clone();
        conn.subscribe_to_signal(
            Some(BUS_NAME),
            Some(INTERFACE),
            Some(member),
            Some(OBJECT_PATH),
            None,
            gio::DBusSignalFlags::NONE,
            move |signal| {
                let json: Option<String> =
                    signal.parameters.try_child_value(0).and_then(|v| v.get());
                if let Some(event) = decode(signal.signal_name, json.as_deref()) {
                    on_event(event);
                }
            },
        )
    })
    .collect();
    let (appeared, vanished) = (on_event.clone(), on_event);
    let watcher = gio::bus_watch_name_on_connection(
        conn,
        BUS_NAME,
        gio::BusNameWatcherFlags::NONE,
        move |_, _, _| appeared(Event::DaemonStarted),
        move |_, _| vanished(Event::DaemonStopped),
    );
    Subscriptions {
        _signals: signals,
        unwatch: Some(Box::new(move || gio::bus_unwatch_name(watcher))),
    }
}

/// Maps a signal to an event; a battery document that does not parse
/// becomes a plain status refresh.
pub fn decode(member: &str, json: Option<&str>) -> Option<Event> {
    match member {
        "DeviceChanged" | "Applied" | "ModeChanged" => Some(Event::Changed),
        "BatteryChanged" => Some(
            json.and_then(|j| serde_json::from_str(j).ok())
                .map_or(Event::Changed, Event::Battery),
        ),
        "ButtonPressed" => json
            .and_then(|j| serde_json::from_str(j).ok())
            .map(Event::Button),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hidlab_api::daemon::Battery;

    #[test]
    fn battery_signal_carries_the_battery() {
        // SYNTHETIC event.
        let json = r#"{"schema":"hid-lab.daemon-battery/1",
            "battery":{"percent":40,"level":"Normal","status":"Discharging"}}"#;
        let Some(Event::Battery(e)) = decode("BatteryChanged", Some(json)) else {
            panic!("not a battery event");
        };
        assert_eq!(
            e.battery,
            Some(Battery {
                percent: Some(40),
                level: Some("Normal".into()),
                status: Some("Discharging".into()),
            })
        );
        assert_eq!(decode("BatteryChanged", Some("{")), Some(Event::Changed));
        assert_eq!(decode("ButtonPressed", Some("{}")), None);
        let press =
            r#"{"schema":"hid-lab.daemon-button/1","number":4,"control":"back","pressed":true}"#;
        let Some(Event::Button(b)) = decode("ButtonPressed", Some(press)) else {
            panic!("not a button event");
        };
        assert_eq!((b.control.as_deref(), b.pressed), (Some("back"), true));
        assert_eq!(decode("Other", Some(press)), None);
    }

    #[test]
    fn replies_that_did_not_stick_are_errors() {
        // SYNTHETIC replies.
        let rejected = r#"{"schema":"hid-lab.daemon-set/1","value":{"setting":"dpi","value":1625},
            "saved_to":null,"note":"the mouse cannot take this value; nothing was saved",
            "applied":{"items":[{"setting":"dpi","target":{"setting":"dpi","value":1625},
            "before":null,"result":"rejected","checks":[],"error":"1625 is not in the list"}],
            "ok":false,"elapsed_ms":3,"ignored_reports":0}}"#;
        assert_eq!(
            outcome("Set", rejected),
            Err("the mouse cannot take this value; nothing was saved".into())
        );
        let saved_offline = r#"{"schema":"hid-lab.daemon-set/1","value":{"setting":"dpi","value":1600},
            "saved_to":"~/.config/hid-lab/config.toml","applied":null,"note":null}"#;
        assert_eq!(outcome("Set", saved_offline), Ok(()));
        let mismatch = r#"{"schema":"hid-lab.daemon-reset/1","values":[],"saved_to":[],"note":null,
            "applied":{"items":[{"setting":"dpi","target":{"setting":"dpi","value":1000},
            "before":null,"result":"mismatch","checks":[],"error":null}],
            "ok":false,"elapsed_ms":3,"ignored_reports":0}}"#;
        assert_eq!(
            outcome("Reset", mismatch),
            Err("The mouse did not keep dpi".into())
        );
        assert_eq!(outcome("SetDesktop", "{}"), Ok(()));
    }

    #[test]
    fn unknown_status_says_what_to_do() {
        let err = parse_status("{}").unwrap_err();
        assert!(
            err.contains("systemctl --user restart hid-labd.service"),
            "{err}"
        );
    }
}
