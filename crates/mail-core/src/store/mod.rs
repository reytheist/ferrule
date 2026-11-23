use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::error::CoreError;

const MIGRATION_0001: &str = include_str!("0001_init.sql");

pub(crate) struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub(crate) fn open(path: &Path) -> Result<Self, CoreError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|e| CoreError::Storage {
                    reason: format!("create db directory: {e}"),
                })?;
            }
        }
        let conn = Connection::open(path).map_err(|e| CoreError::Storage {
            reason: format!("open sqlite: {e}"),
        })?;
        Self::init(conn)
    }

    pub(crate) fn open_in_memory() -> Result<Self, CoreError> {
        let conn = Connection::open_in_memory().map_err(|e| CoreError::Storage {
            reason: format!("open in-memory sqlite: {e}"),
        })?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, CoreError> {
        conn.execute_batch(MIGRATION_0001)
            .map_err(|e| CoreError::Storage {
                reason: format!("run migration 0001: {e}"),
            })?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub(crate) fn with_conn<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let mut guard = self.conn.lock().map_err(|_| CoreError::Internal {
            reason: "sqlite mutex poisoned".to_string(),
        })?;
        f(&mut guard)
    }
}
