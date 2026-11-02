//! Client of hid-labd over the session bus, with gio on the GTK main loop.
//! Calls to the daemon's name start it through D-Bus activation when it is
//! not running.

use gettextrs::gettext;
use gtk::gio;
use gtk::glib;
use hidlab_api::daemon::{BUS_NAME, BatteryEvent, INTERFACE, OBJECT_PATH, Status, UNIT};

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
    let signals = ["DeviceChanged", "Applied", "ModeChanged", "BatteryChanged"]
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
