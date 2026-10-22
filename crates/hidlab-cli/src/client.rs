//! D-Bus client of hid-labd (`dev.ozkkar.Cursiva.Daemon1`).

use hidlab_daemon::api::{BUS_NAME, INTERFACE, OBJECT_PATH, UNIT};
use serde::Serialize;
use serde::de::DeserializeOwned;
use zbus::blocking::Connection;
use zbus::zvariant::DynamicType;

fn not_running() -> String {
    format!(
        "hid-labd is not running.\n\
         Try: systemctl --user start {UNIT}\n\
         If it is not installed yet, see docs/daemon.md."
    )
}

fn session() -> Result<Connection, String> {
    Connection::session().map_err(|e| {
        format!(
            "cannot connect to the D-Bus session bus: {e}\n\
             Run hid-lab from your desktop session, not over ssh or with sudo."
        )
    })
}

/// Calls `method` and decodes its JSON answer.
pub fn call<T, B>(method: &str, body: &B) -> Result<T, String>
where
    T: DeserializeOwned,
    B: Serialize + DynamicType,
{
    let conn = session()?;
    let reply = conn
        .call_method(Some(BUS_NAME), OBJECT_PATH, Some(INTERFACE), method, body)
        .map_err(|e| match e {
            zbus::Error::MethodError(name, detail, _) => match name.as_str() {
                "org.freedesktop.DBus.Error.ServiceUnknown"
                | "org.freedesktop.DBus.Error.NameHasNoOwner" => not_running(),
                _ => detail.unwrap_or_else(|| name.to_string()),
            },
            other => format!("D-Bus call {method} failed: {other}"),
        })?;
    let text: String = reply
        .body()
        .deserialize()
        .map_err(|e| format!("hid-labd answered {method} with an unexpected body: {e}"))?;
    serde_json::from_str(&text)
        .map_err(|e| format!("hid-labd answered {method} with JSON this hid-lab cannot read: {e}"))
}

/// `Some(true)` if hid-labd owns its name, `None` if the bus is unreachable.
pub fn daemon_running() -> Option<bool> {
    let conn = Connection::session().ok()?;
    let reply = conn
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "NameHasOwner",
            &(BUS_NAME,),
        )
        .ok()?;
    reply.body().deserialize::<bool>().ok()
}
