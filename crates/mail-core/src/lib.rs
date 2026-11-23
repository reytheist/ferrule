pub mod agent;
pub mod auth;
pub mod error;
pub mod events;
pub mod host;
pub mod ids;
pub mod sanitize;
pub mod schedule;
pub mod types;

mod store;
mod sync;

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::broadcast;

use crate::agent::AgentPort;
use crate::auth::{ProviderAuth, SecretStore};
use crate::error::CoreError;
use crate::events::CoreEvent;
use crate::host::{Clock, OsKeychain, SystemClock};
use crate::ids::{AgentId, UnixMs};
use crate::store::Store;
use crate::sync::Dialect;

#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub db_path: Option<PathBuf>,
    pub sync_window_days: i64,
    pub default_undo_secs: u32,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            db_path: None,
            sync_window_days: 90,
            default_undo_secs: 7,
        }
    }
}

pub struct Host {
    pub secrets: Arc<dyn SecretStore>,
    pub clock: Arc<dyn Clock>,
}

impl Host {
    pub fn desktop() -> Self {
        Self {
            secrets: Arc::new(OsKeychain::default()),
            clock: Arc::new(SystemClock),
        }
    }
}

struct MailCoreInner {
    config: CoreConfig,
    store: Store,
    host: Host,
    events: broadcast::Sender<CoreEvent>,
}

#[derive(Clone)]
pub struct MailCore {
    inner: Arc<MailCoreInner>,
}

impl MailCore {
    pub fn open(config: CoreConfig, host: Host) -> Result<Self, CoreError> {
        let store = match &config.db_path {
            Some(path) => Store::open(path)?,
            None => Store::open_in_memory()?,
        };
        let (events, _) = broadcast::channel(128);
        Ok(Self {
            inner: Arc::new(MailCoreInner {
                config,
                store,
                host,
                events,
            }),
        })
    }

    pub fn config(&self) -> &CoreConfig {
        &self.inner.config
    }

    pub fn now(&self) -> UnixMs {
        self.inner.host.clock.now()
    }

    pub fn secrets(&self) -> &dyn SecretStore {
        self.inner.host.secrets.as_ref()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<CoreEvent> {
        self.inner.events.subscribe()
    }

    pub fn agent_port(&self, agent_id: AgentId) -> AgentPort {
        AgentPort::new(agent_id)
    }

    pub fn primary_folder_for(&self, auth: &ProviderAuth) -> &'static str {
        Dialect::from_auth(auth).primary_sync_folder()
    }

    pub fn db_health_check(&self) -> Result<(), CoreError> {
        self.inner.store.with_conn(|conn| {
            conn.execute_batch("SELECT 1;")
                .map_err(|e| CoreError::Storage {
                    reason: e.to_string(),
                })
        })
    }
}
