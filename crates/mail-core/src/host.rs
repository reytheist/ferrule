use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::auth::{SecretKey, SecretStore, SecretString};
use crate::error::CoreError;
use crate::ids::UnixMs;

pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> UnixMs;
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> UnixMs {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        UnixMs(ms)
    }
}

pub struct ManualClock {
    now_ms: AtomicI64,
}

impl ManualClock {
    pub fn new(start: UnixMs) -> Self {
        Self {
            now_ms: AtomicI64::new(start.0),
        }
    }

    pub fn advance_secs(&self, secs: i64) {
        self.now_ms
            .fetch_add(secs.saturating_mul(1000), Ordering::SeqCst);
    }

    pub fn set(&self, at: UnixMs) {
        self.now_ms.store(at.0, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now(&self) -> UnixMs {
        UnixMs(self.now_ms.load(Ordering::SeqCst))
    }
}

pub struct OsKeychain {
    service: String,
}

impl Default for OsKeychain {
    fn default() -> Self {
        Self {
            service: "ferrule-mail".to_string(),
        }
    }
}

impl SecretStore for OsKeychain {
    fn put(&self, key: &SecretKey, value: &SecretString) -> Result<(), CoreError> {
        let entry = keyring::Entry::new(&self.service, &key.storage_id()).map_err(|e| {
            CoreError::KeychainUnavailable {
                reason: e.to_string(),
            }
        })?;
        entry
            .set_password(value.expose())
            .map_err(|e| CoreError::KeychainUnavailable {
                reason: e.to_string(),
            })
    }

    fn get(&self, key: &SecretKey) -> Result<Option<SecretString>, CoreError> {
        let entry = keyring::Entry::new(&self.service, &key.storage_id()).map_err(|e| {
            CoreError::KeychainUnavailable {
                reason: e.to_string(),
            }
        })?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(SecretString::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(CoreError::KeychainUnavailable {
                reason: e.to_string(),
            }),
        }
    }

    fn delete(&self, key: &SecretKey) -> Result<(), CoreError> {
        let entry = keyring::Entry::new(&self.service, &key.storage_id()).map_err(|e| {
            CoreError::KeychainUnavailable {
                reason: e.to_string(),
            }
        })?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(CoreError::KeychainUnavailable {
                reason: e.to_string(),
            }),
        }
    }
}
