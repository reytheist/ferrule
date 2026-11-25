//! Private SQLite + FTS5 persistence. Nothing in this module crosses the crate boundary.

mod accounts;
mod demo;
mod drafts;
mod jobs;
mod templates;
mod threads;

pub(crate) use accounts::*;
pub(crate) use demo::*;
pub(crate) use drafts::*;
pub(crate) use jobs::*;
pub(crate) use templates::*;
pub(crate) use threads::*;

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::CoreError;

const MIGRATION_0001: &str = include_str!("0001_init.sql");
/// Adds the frozen outbox payload columns. Applied once, guarded by a column check,
/// because `ALTER TABLE … ADD COLUMN` is not idempotent.
const MIGRATION_0002_JOB_PAYLOAD: &str =
    "ALTER TABLE job ADD COLUMN payload TEXT;\nALTER TABLE job ADD COLUMN message_id_hdr TEXT;";
/// Adds `space_kind` to existing `account` tables so Pigment Staff spaces are distinguished
/// from personal spaces (which have Staff Sharing disabled by default).
const MIGRATION_0003_ACCOUNT_SPACE_KIND: &str =
    "ALTER TABLE account ADD COLUMN space_kind TEXT NOT NULL DEFAULT 'personal' CHECK (space_kind IN ('personal', 'pigment_staff'));";

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
        // Runs every open: idempotent DDL plus connection-scoped pragmas (WAL, FKs, mmap).
        conn.execute_batch(MIGRATION_0001)
            .map_err(|e| CoreError::Storage {
                reason: format!("run migration 0001: {e}"),
            })?;
        if !column_exists(&conn, "job", "payload")? {
            conn.execute_batch(MIGRATION_0002_JOB_PAYLOAD)
                .map_err(|e| CoreError::Storage {
                    reason: format!("run migration 0002: {e}"),
                })?;
        }
        if !column_exists(&conn, "account", "space_kind")? {
            conn.execute_batch(MIGRATION_0003_ACCOUNT_SPACE_KIND)
                .map_err(|e| CoreError::Storage {
                    reason: format!("run migration 0003: {e}"),
                })?;
        }
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

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, CoreError> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(sq_err)?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(sq_err)?;
    for name in names {
        if name.map_err(sq_err)? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn sq_err(e: rusqlite::Error) -> CoreError {
    CoreError::Storage {
        reason: e.to_string(),
    }
}

/// Convenience conversion from any rusqlite error into a user-safe [`CoreError::Storage`].
pub(crate) trait SqlResultExt<T> {
    fn sq(self) -> Result<T, CoreError>;
}

impl<T> SqlResultExt<T> for rusqlite::Result<T> {
    fn sq(self) -> Result<T, CoreError> {
        self.map_err(sq_err)
    }
}

/// Row mapper-friendly `query_row`: closures may return [`CoreError`].
pub(crate) fn query_one<T>(
    conn: &mut Connection,
    sql: &str,
    params: impl rusqlite::Params,
    map: impl FnOnce(&rusqlite::Row<'_>) -> Result<T, CoreError>,
) -> Result<Option<T>, CoreError> {
    let mut stmt = conn.prepare(sql).sq()?;
    let mut rows = stmt.query(params).sq()?;
    match rows.next().sq()? {
        Some(row) => Ok(Some(map(row)?)),
        None => Ok(None),
    }
}

/// Row mapper-friendly `query_map` over a whole result set.
pub(crate) fn query_all<T>(
    conn: &mut Connection,
    sql: &str,
    params: impl rusqlite::Params,
    mut map: impl FnMut(&rusqlite::Row<'_>) -> Result<T, CoreError>,
) -> Result<Vec<T>, CoreError> {
    let mut stmt = conn.prepare(sql).sq()?;
    let mut rows = stmt.query(params).sq()?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().sq()? {
        out.push(map(row)?);
    }
    Ok(out)
}

pub(crate) fn storage_err(reason: impl Into<String>) -> CoreError {
    CoreError::Storage {
        reason: reason.into(),
    }
}

pub(crate) fn id_to_blob(id: &[u8; 16]) -> Vec<u8> {
    id.to_vec()
}

pub(crate) fn blob_to_id<T>(bytes: &[u8], map: impl FnOnce([u8; 16]) -> T) -> Result<T, CoreError> {
    let arr: [u8; 16] = bytes
        .try_into()
        .map_err(|_| storage_err("invalid id blob in database"))?;
    Ok(map(arr))
}

/// Read a branded id out of a row column.
pub(crate) fn row_id<T>(
    row: &rusqlite::Row<'_>,
    idx: usize,
    map: impl FnOnce([u8; 16]) -> T,
) -> Result<T, CoreError> {
    let bytes: Vec<u8> = row.get(idx).sq()?;
    blob_to_id(&bytes, map)
}

pub(crate) fn to_json<T: Serialize>(value: &T) -> Result<String, CoreError> {
    serde_json::to_string(value).map_err(|e| CoreError::Internal {
        reason: format!("serialize domain state: {e}"),
    })
}

pub(crate) fn from_json<T: DeserializeOwned>(raw: &str) -> Result<T, CoreError> {
    serde_json::from_str(raw).map_err(|e| CoreError::Storage {
        reason: format!("corrupt stored JSON: {e}"),
    })
}

/// Contentless FTS5 rows are keyed by the `message.rowid` so they can be deleted on update.
pub(crate) fn fts_insert(
    conn: &mut Connection,
    rowid: i64,
    subject: &str,
    sender: &str,
    body: &str,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO message_fts(rowid, subject, sender, body_text) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![rowid, subject, sender, body],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn fts_delete(conn: &mut Connection, rowid: i64) -> Result<(), CoreError> {
    conn.execute(
        "DELETE FROM message_fts WHERE rowid = ?1",
        rusqlite::params![rowid],
    )
    .sq()?;
    Ok(())
}

const APP_SETTINGS_KEY: &str = "app_settings";

pub(crate) fn get_app_settings(conn: &mut Connection) -> Result<crate::types::AppSettings, CoreError> {
    let raw: Option<String> = query_one(
        conn,
        "SELECT value_json FROM app_setting WHERE key = ?1",
        rusqlite::params![APP_SETTINGS_KEY],
        |row| row.get(0).sq(),
    )?;
    match raw {
        Some(json) => from_json(&json),
        None => Ok(crate::types::AppSettings::default()),
    }
}

pub(crate) fn save_app_settings(
    conn: &mut Connection,
    settings: &crate::types::AppSettings,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO app_setting (key, value_json) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
        rusqlite::params![APP_SETTINGS_KEY, to_json(settings)?],
    )
    .sq()?;
    Ok(())
}
