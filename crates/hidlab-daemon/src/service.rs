//! The D-Bus object `dev.ozkkar.Cursiva.Daemon1` (see [`crate::api`]).

use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use zbus::fdo;
use zbus::interface;
use zbus::object_server::SignalEmitter;

use crate::engine::Engine;

pub struct Service {
    pub engine: Arc<Mutex<Engine>>,
}

fn json<T: Serialize>(v: &T) -> fdo::Result<String> {
    serde_json::to_string(v).map_err(|e| fdo::Error::Failed(e.to_string()))
}

impl Service {
    fn lock(&self) -> fdo::Result<MutexGuard<'_, Engine>> {
        self.engine
            .lock()
            .map_err(|_| fdo::Error::Failed("internal error: daemon state lock poisoned".into()))
    }
}

#[interface(name = "dev.ozkkar.Cursiva.Daemon1")]
impl Service {
    /// Status JSON (`hid-lab.daemon-status/3`).
    fn get_status(&self) -> fdo::Result<String> {
        json(&self.lock()?.status())
    }

    /// Applies and saves one setting; returns `hid-lab.daemon-set/1` JSON.
    async fn set(
        &self,
        setting: String,
        value: String,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let (reply, record) = {
            let mut e = self.lock()?;
            let reply = e.set(&setting, &value).map_err(fdo::Error::Failed)?;
            let record = reply.applied.as_ref().and(e.last_apply().cloned());
            (reply, record)
        };
        if let Some(rec) = &record {
            Self::applied(&emitter, &json(rec)?).await?;
        }
        json(&reply)
    }

    /// Forgets one setting (or `all`) and writes its default; returns
    /// `hid-lab.daemon-reset/1` JSON.
    async fn reset(
        &self,
        setting: String,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let (reply, record) = {
            let mut e = self.lock()?;
            let reply = e.reset(&setting).map_err(fdo::Error::Failed)?;
            let record = reply.applied.as_ref().and(e.last_apply().cloned());
            (reply, record)
        };
        if let Some(rec) = &record {
            Self::applied(&emitter, &json(rec)?).await?;
        }
        json(&reply)
    }

    /// Writes one GNOME pointer setting; returns `hid-lab.daemon-desktop/1`
    /// JSON.
    fn set_desktop(&self, setting: String, value: String) -> fdo::Result<String> {
        let reply = self
            .lock()?
            .set_desktop(&setting, &value)
            .map_err(fdo::Error::Failed)?;
        json(&reply)
    }

    /// Sends `ButtonPressed` for `seconds` (at most 60; 0 stops). Clients
    /// call it again before the lease ends while they show presses.
    fn watch_buttons(&self, seconds: u32) -> fdo::Result<u32> {
        Ok(self.lock()?.watch_buttons(seconds))
    }

    /// Reloads the config file and applies it; returns
    /// `hid-lab.daemon-reapply/1` JSON.
    async fn reapply(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let (reply, record) = {
            let mut e = self.lock()?;
            let reply = e.reapply().map_err(fdo::Error::Failed)?;
            (reply, e.last_apply().cloned())
        };
        if let Some(rec) = &record {
            Self::applied(&emitter, &json(rec)?).await?;
        }
        json(&reply)
    }

    /// The managed mouse connected or disconnected (`hid-lab.daemon-device/1`).
    #[zbus(signal)]
    pub async fn device_changed(emitter: &SignalEmitter<'_>, event: &str) -> zbus::Result<()>;

    /// Settings were applied (`ApplyRecord`). For a reconnect it is sent
    /// again with the second check filled in.
    #[zbus(signal)]
    pub async fn applied(emitter: &SignalEmitter<'_>, record: &str) -> zbus::Result<()>;

    /// The SmartShift mode was changed on the mouse and remembered
    /// (`hid-lab.daemon-mode/1`).
    #[zbus(signal)]
    pub async fn mode_changed(emitter: &SignalEmitter<'_>, event: &str) -> zbus::Result<()>;

    /// A button went down or up (`hid-lab.daemon-button/1`), while a
    /// `WatchButtons` lease lasts.
    #[zbus(signal)]
    pub async fn button_pressed(emitter: &SignalEmitter<'_>, event: &str) -> zbus::Result<()>;

    /// The mouse battery changed (`hid-lab.daemon-battery/1`).
    #[zbus(signal)]
    pub async fn battery_changed(emitter: &SignalEmitter<'_>, event: &str) -> zbus::Result<()>;
}
