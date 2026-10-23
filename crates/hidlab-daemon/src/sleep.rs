//! Watches logind's `PrepareForSleep` on the system bus (requirements §7,
//! system resume) in a thread, and wakes the main loop through a socket.

use std::io::Write as _;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::Sender;

use zbus::MatchRule;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepEvent {
    Suspending,
    Resumed,
}

fn rule() -> zbus::Result<MatchRule<'static>> {
    Ok(MatchRule::builder()
        .msg_type(Type::Signal)
        .sender("org.freedesktop.login1")?
        .path("/org/freedesktop/login1")?
        .interface("org.freedesktop.login1.Manager")?
        .member("PrepareForSleep")?
        .build())
}

/// Starts the watcher. Each event is sent on `tx` and one byte is written to
/// `wake`. Errors are returned to the caller to log; the daemon keeps
/// working without resume detection (reconnects still trigger re-applies).
pub fn spawn(tx: Sender<SleepEvent>, mut wake: UnixStream) -> Result<(), String> {
    let conn = Connection::system()
        .map_err(|e| format!("cannot connect to the system bus for logind: {e}"))?;
    let rule = rule().map_err(|e| e.to_string())?;
    let messages = MessageIterator::for_match_rule(rule, &conn, Some(8))
        .map_err(|e| format!("cannot subscribe to logind PrepareForSleep: {e}"))?;
    std::thread::Builder::new()
        .name("logind-sleep".into())
        .spawn(move || {
            let _conn = conn;
            for msg in messages.flatten() {
                let Ok(going_down) = msg.body().deserialize::<bool>() else {
                    continue;
                };
                let ev = if going_down {
                    SleepEvent::Suspending
                } else {
                    SleepEvent::Resumed
                };
                if tx.send(ev).is_err() || wake.write_all(&[1]).is_err() {
                    return;
                }
            }
        })
        .map(|_| ())
        .map_err(|e| format!("cannot start the logind thread: {e}"))
}
