//! D-Bus client of hid-labd (`dev.ozkkar.Cursiva.Daemon1`).

use std::sync::mpsc::{self, Receiver};

use hidlab_api::daemon::{BUS_NAME, INTERFACE, OBJECT_PATH, UNIT};
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

/// Receives the JSON body of every `signal` hid-labd emits, on a thread.
/// The receiver closes if the bus connection ends.
pub fn subscribe(signal: &'static str) -> Result<Receiver<String>, String> {
    let conn = session()?;
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface(INTERFACE)
        .and_then(|b| b.path(OBJECT_PATH))
        .and_then(|b| b.member(signal))
        .map_err(|e| format!("cannot build the {signal} match rule: {e}"))?
        .build();
    let iter = zbus::blocking::MessageIterator::for_match_rule(rule, &conn, Some(256))
        .map_err(|e| format!("cannot subscribe to {signal}: {e}"))?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for msg in iter {
            let Ok(msg) = msg else { break };
            if let Ok(body) = msg.body().deserialize::<String>() {
                if tx.send(body).is_err() {
                    break;
                }
            }
        }
    });
    Ok(rx)
}

fn call_raw<B>(method: &str, body: &B) -> Result<zbus::Message, String>
where
    B: Serialize + DynamicType,
{
    let conn = session()?;
    conn.call_method(Some(BUS_NAME), OBJECT_PATH, Some(INTERFACE), method, body)
        .map_err(|e| match e {
            zbus::Error::MethodError(name, detail, _) => match name.as_str() {
                "org.freedesktop.DBus.Error.ServiceUnknown"
                | "org.freedesktop.DBus.Error.NameHasNoOwner" => not_running(),
                _ => detail.unwrap_or_else(|| name.to_string()),
            },
            other => format!("D-Bus call {method} failed: {other}"),
        })
}

/// Calls `method` and decodes its JSON answer.
pub fn call<T, B>(method: &str, body: &B) -> Result<T, String>
where
    T: DeserializeOwned,
    B: Serialize + DynamicType,
{
    let text: String = call_value(method, body)?;
    serde_json::from_str(&text)
        .map_err(|e| format!("hid-labd answered {method} with JSON this hid-lab cannot read: {e}"))
}

/// Calls `method` and decodes a plain D-Bus answer (for example a `u`).
pub fn call_value<T, B>(method: &str, body: &B) -> Result<T, String>
where
    T: DeserializeOwned + zbus::zvariant::Type,
    B: Serialize + DynamicType,
{
    call_raw(method, body)?
        .body()
        .deserialize()
        .map_err(|e| format!("hid-labd answered {method} with an unexpected body: {e}"))
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
