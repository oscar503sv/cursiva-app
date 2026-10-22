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
    /// Status JSON (`hid-lab.daemon-status/1`).
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
        let reply = self
            .lock()?
            .set(&setting, &value)
            .map_err(fdo::Error::Failed)?;
        if let Some(report) = &reply.applied {
            Self::applied(&emitter, &json(report)?).await?;
        }
        json(&reply)
    }

    /// Reloads the config file and applies it; returns
    /// `hid-lab.daemon-reapply/1` JSON.
    async fn reapply(
        &self,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<String> {
        let reply = self.lock()?.reapply().map_err(fdo::Error::Failed)?;
        Self::applied(&emitter, &json(&reply.report)?).await?;
        json(&reply)
    }

    /// The managed mouse connected or disconnected (`hid-lab.daemon-device/1`).
    #[zbus(signal)]
    pub async fn device_changed(emitter: &SignalEmitter<'_>, event: &str) -> zbus::Result<()>;

    /// Settings were applied (an apply report).
    #[zbus(signal)]
    pub async fn applied(emitter: &SignalEmitter<'_>, report: &str) -> zbus::Result<()>;
}
