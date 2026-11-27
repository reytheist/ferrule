//! Account, identity, sync-state, and folder-cursor persistence.

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::auth::{ProviderAuth, ProviderKind};
use crate::error::CoreError;
use crate::ids::UnixMs;
use crate::ids::{AccountId, IdentityId};
use crate::store::{
    from_json, fts_delete, id_to_blob, query_all, query_one, row_id, to_json, SqlResultExt,
};
use crate::types::{AccountView, IdentityView, SpaceColor, SyncState};

const ACCOUNT_COLS: &str =
    "id, display_name, provider, color, email, position, auth_json, sync_state_json";

pub(crate) struct AccountRow {
    pub id: AccountId,
    pub display_name: String,
    pub provider: ProviderKind,
    pub color: SpaceColor,
    pub email: String,
    pub position: u32,
    pub auth: ProviderAuth,
    pub sync: SyncState,
}

pub(crate) struct IdentityRow {
    pub id: IdentityId,
    pub account_id: AccountId,
    pub address: String,
    pub display_name: Option<String>,
    pub is_primary: bool,
}

impl IdentityRow {
    pub(crate) fn to_view(&self) -> IdentityView {
        IdentityView {
            id: self.id,
            address: self.address.clone(),
            display_name: self.display_name.clone(),
            is_primary: self.is_primary,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct FolderCursorRow {
    pub uid_validity: i64,
    pub high_uid: i64,
    pub low_uid: i64,
    pub window_floor: UnixMs,
    pub messages_count: u32,
    pub unseen_count: u32,
    pub uid_next: u32,
    pub backfill_done: bool,
    pub last_full_flag_pass: UnixMs,
}

fn provider_db(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Gmail => "gmail",
        ProviderKind::Outlook => "outlook",
        ProviderKind::Imap => "imap",
    }
}

fn map_account(row: &Row<'_>) -> Result<AccountRow, CoreError> {
    let provider_raw: String = row.get(2).sq()?;
    let color_raw: String = row.get(3).sq()?;
    let auth_raw: String = row.get(6).sq()?;
    let sync_raw: String = row.get(7).sq()?;
    Ok(AccountRow {
        id: row_id(row, 0, AccountId::from_bytes)?,
        display_name: row.get(1).sq()?,
        provider: match provider_raw.as_str() {
            "gmail" => ProviderKind::Gmail,
            "outlook" => ProviderKind::Outlook,
            _ => ProviderKind::Imap,
        },
        color: SpaceColor::from_db(&color_raw),
        email: row.get(4).sq()?,
        position: row.get::<_, i64>(5).sq()?.max(0) as u32,
        auth: from_json(&auth_raw)?,
        sync: from_json(&sync_raw)?,
    })
}

pub(crate) fn account_count(conn: &mut Connection) -> Result<u64, CoreError> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM account", [], |r| r.get(0))
        .sq()?;
    Ok(count.max(0) as u64)
}

pub(crate) fn account_exists(conn: &mut Connection, id: AccountId) -> Result<bool, CoreError> {
    let found: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM account WHERE id = ?1",
            params![id_to_blob(id.as_bytes())],
            |r| r.get(0),
        )
        .sq()?;
    Ok(found > 0)
}

pub(crate) fn get_account(
    conn: &mut Connection,
    id: AccountId,
) -> Result<Option<AccountRow>, CoreError> {
    let sql = format!("SELECT {ACCOUNT_COLS} FROM account WHERE id = ?1");
    query_one(conn, &sql, params![id_to_blob(id.as_bytes())], map_account)
}

pub(crate) fn list_account_rows(conn: &mut Connection) -> Result<Vec<AccountRow>, CoreError> {
    let sql = format!("SELECT {ACCOUNT_COLS} FROM account ORDER BY position ASC, rowid ASC");
    query_all(conn, &sql, [], map_account)
}

pub(crate) fn insert_account(
    conn: &mut Connection,
    display_name: &str,
    color: SpaceColor,
    auth: &ProviderAuth,
    sync: &SyncState,
    created_at: UnixMs,
) -> Result<AccountId, CoreError> {
    auth.validate()?;
    let position: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM account",
            [],
            |r| r.get(0),
        )
        .sq()?;
    let id = AccountId::fresh();
    conn.execute(
        "INSERT INTO account (id, display_name, provider, color, email, position, auth_json, sync_state_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            id_to_blob(id.as_bytes()),
            display_name,
            provider_db(auth.provider_kind()),
            color.as_str(),
            auth.address(),
            position,
            to_json(auth)?,
            to_json(sync)?,
            created_at.0,
        ],
    )
    .sq()?;
    Ok(id)
}

pub(crate) fn insert_identity(
    conn: &mut Connection,
    account_id: AccountId,
    address: &str,
    display_name: Option<&str>,
    is_primary: bool,
) -> Result<IdentityId, CoreError> {
    let id = IdentityId::fresh();
    conn.execute(
        "INSERT INTO identity (id, account_id, address, display_name, is_primary)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            id_to_blob(id.as_bytes()),
            id_to_blob(account_id.as_bytes()),
            address,
            display_name,
            is_primary as i64,
        ],
    )
    .sq()?;
    Ok(id)
}

fn map_identity(row: &Row<'_>) -> Result<IdentityRow, CoreError> {
    Ok(IdentityRow {
        id: row_id(row, 0, IdentityId::from_bytes)?,
        account_id: row_id(row, 1, AccountId::from_bytes)?,
        address: row.get(2).sq()?,
        display_name: row.get(3).sq()?,
        is_primary: row.get::<_, i64>(4).sq()? != 0,
    })
}

pub(crate) fn identities(
    conn: &mut Connection,
    account_id: AccountId,
) -> Result<Vec<IdentityRow>, CoreError> {
    query_all(
        conn,
        "SELECT id, account_id, address, display_name, is_primary
         FROM identity WHERE account_id = ?1 ORDER BY is_primary DESC, rowid ASC",
        params![id_to_blob(account_id.as_bytes())],
        map_identity,
    )
}

pub(crate) fn primary_identity(
    conn: &mut Connection,
    account_id: AccountId,
) -> Result<Option<IdentityRow>, CoreError> {
    Ok(identities(conn, account_id)?.into_iter().next())
}

pub(crate) fn identity(
    conn: &mut Connection,
    identity_id: IdentityId,
) -> Result<Option<IdentityRow>, CoreError> {
    query_one(
        conn,
        "SELECT id, account_id, address, display_name, is_primary FROM identity WHERE id = ?1",
        params![id_to_blob(identity_id.as_bytes())],
        map_identity,
    )
}

pub(crate) fn delete_account_inner(conn: &Connection, id: AccountId) -> Result<bool, CoreError> {
    // Contentless FTS5 rows have no foreign-key cascade: purge them first so search
    // never points at messages that no longer exist.
    let blob = id_to_blob(id.as_bytes());
    let mut stmt = conn
        .prepare("SELECT rowid FROM message WHERE account_id = ?1")
        .sq()?;
    let mut rows = stmt.query(params![blob]).sq()?;
    let mut rowids = Vec::new();
    while let Some(row) = rows.next().sq()? {
        rowids.push(row.get::<_, i64>(0).sq()?);
    }
    drop(rows);
    drop(stmt);

    for rowid in rowids {
        fts_delete(conn, rowid)?;
    }
    let changed = conn
        .execute(
            "DELETE FROM account WHERE id = ?1",
            params![id_to_blob(id.as_bytes())],
        )
        .sq()?;
    Ok(changed > 0)
}

pub(crate) fn delete_account(conn: &mut Connection, id: AccountId) -> Result<bool, CoreError> {
    let tx = conn.transaction().sq()?;
    let deleted = delete_account_inner(&tx, id)?;
    tx.commit().sq()?;
    Ok(deleted)
}

pub(crate) fn update_account_meta(
    conn: &mut Connection,
    id: AccountId,
    display_name: &str,
    color: SpaceColor,
) -> Result<bool, CoreError> {
    let changed = conn
        .execute(
            "UPDATE account SET display_name = ?2, color = ?3 WHERE id = ?1",
            params![id_to_blob(id.as_bytes()), display_name, color.as_str()],
        )
        .sq()?;
    Ok(changed > 0)
}

pub(crate) fn set_sync_state(
    conn: &mut Connection,
    id: AccountId,
    sync: &SyncState,
) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE account SET sync_state_json = ?2 WHERE id = ?1",
        params![id_to_blob(id.as_bytes()), to_json(sync)?],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn upsert_folder_cursor(
    conn: &mut Connection,
    account_id: AccountId,
    folder_name: &str,
    window_floor: UnixMs,
    backfill_target: UnixMs,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO folder_cursor (account_id, folder_name, uid_validity, high_uid, window_floor, backfill_target)
         VALUES (?1, ?2, 0, 0, ?3, ?4)
         ON CONFLICT(account_id, folder_name) DO UPDATE SET
           window_floor = excluded.window_floor,
           backfill_target = excluded.backfill_target",
        params![
            id_to_blob(account_id.as_bytes()),
            folder_name,
            window_floor.0,
            backfill_target.0,
        ],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn folder_cursor(
    conn: &mut Connection,
    account_id: AccountId,
    folder_name: &str,
) -> Result<Option<FolderCursorRow>, CoreError> {
    conn.query_row(
        "SELECT uid_validity, high_uid, low_uid, window_floor,
                messages_count, unseen_count, uid_next, backfill_done, last_full_flag_pass
         FROM folder_cursor WHERE account_id = ?1 AND folder_name = ?2",
        params![id_to_blob(account_id.as_bytes()), folder_name],
        |row| {
            Ok(FolderCursorRow {
                uid_validity: row.get(0)?,
                high_uid: row.get(1)?,
                low_uid: row.get(2)?,
                window_floor: UnixMs(row.get(3)?),
                messages_count: row.get::<_, i64>(4)?.max(0) as u32,
                unseen_count: row.get::<_, i64>(5)?.max(0) as u32,
                uid_next: row.get::<_, i64>(6)?.max(0) as u32,
                backfill_done: row.get::<_, i64>(7)? != 0,
                last_full_flag_pass: UnixMs(row.get(8)?),
            })
        },
    )
    .optional()
    .sq()
}

pub(crate) fn update_folder_high_uid(
    conn: &mut Connection,
    account_id: AccountId,
    folder_name: &str,
    uid_validity: i64,
    high_uid: i64,
) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE folder_cursor SET uid_validity = ?3, high_uid = MAX(high_uid, ?4)
         WHERE account_id = ?1 AND folder_name = ?2",
        params![
            id_to_blob(account_id.as_bytes()),
            folder_name,
            uid_validity,
            high_uid
        ],
    )
    .sq()?;
    Ok(())
}

pub(crate) struct FolderStatusUpdate<'a> {
    pub folder_name: &'a str,
    pub uid_validity: i64,
    pub uid_next: u32,
    pub messages_count: u32,
    pub unseen_count: u32,
}

pub(crate) fn update_folder_status(
    conn: &mut Connection,
    account_id: AccountId,
    update: &FolderStatusUpdate<'_>,
    now: UnixMs,
) -> Result<(), CoreError> {
    conn.execute(
        "INSERT INTO folder_cursor
           (account_id, folder_name, uid_validity, high_uid, low_uid, window_floor, backfill_target,
            messages_count, unseen_count, uid_next, backfill_done, last_full_flag_pass)
         VALUES (?1, ?2, ?3, 0, 0, ?7, ?7, ?5, ?6, ?4, 0, 0)
         ON CONFLICT(account_id, folder_name) DO UPDATE SET
           uid_validity = CASE WHEN excluded.uid_validity > 0 THEN excluded.uid_validity ELSE folder_cursor.uid_validity END,
           high_uid = CASE WHEN excluded.uid_validity > 0 AND folder_cursor.uid_validity > 0 AND excluded.uid_validity != folder_cursor.uid_validity THEN 0 ELSE folder_cursor.high_uid END,
           low_uid = CASE WHEN excluded.uid_validity > 0 AND folder_cursor.uid_validity > 0 AND excluded.uid_validity != folder_cursor.uid_validity THEN 0 ELSE folder_cursor.low_uid END,
           backfill_done = CASE WHEN excluded.uid_validity > 0 AND folder_cursor.uid_validity > 0 AND excluded.uid_validity != folder_cursor.uid_validity THEN 0 ELSE folder_cursor.backfill_done END,
           uid_next = excluded.uid_next,
           messages_count = excluded.messages_count,
           unseen_count = excluded.unseen_count",
        params![
            id_to_blob(account_id.as_bytes()),
            update.folder_name,
            update.uid_validity,
            i64::from(update.uid_next),
            i64::from(update.messages_count),
            i64::from(update.unseen_count),
            now.0,
        ],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn update_folder_backfill(
    conn: &mut Connection,
    account_id: AccountId,
    folder_name: &str,
    low_uid: i64,
    backfill_done: bool,
) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE folder_cursor SET low_uid = ?3, backfill_done = ?4
         WHERE account_id = ?1 AND folder_name = ?2",
        params![
            id_to_blob(account_id.as_bytes()),
            folder_name,
            low_uid,
            backfill_done as i64,
        ],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn mark_folder_flag_pass(
    conn: &mut Connection,
    account_id: AccountId,
    folder_name: &str,
    at: UnixMs,
) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE folder_cursor SET last_full_flag_pass = ?3
         WHERE account_id = ?1 AND folder_name = ?2",
        params![id_to_blob(account_id.as_bytes()), folder_name, at.0],
    )
    .sq()?;
    Ok(())
}

/// Unread badge for the account's inbox (mirrors the inbox thread predicate, including
/// hiding snoozed threads).
pub(crate) fn account_unread(
    conn: &mut Connection,
    account_id: AccountId,
) -> Result<u32, CoreError> {
    let count: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(t.unread_count), 0) FROM thread t
             WHERE t.account_id = ?1 AND t.archived = 0 AND t.trashed = 0 AND t.spam = 0
               AND NOT EXISTS (
                 SELECT 1 FROM job j WHERE j.thread_id = t.id AND j.kind = 'snooze'
                   AND j.live = 1 AND json_extract(j.state_json, '$.kind') = 'armed'
               )",
            params![id_to_blob(account_id.as_bytes())],
            |r| r.get(0),
        )
        .sq()?;
    Ok(count.max(0) as u32)
}

pub(crate) fn account_views(conn: &mut Connection) -> Result<Vec<AccountView>, CoreError> {
    let rows = list_account_rows(conn)?;
    let mut views = Vec::with_capacity(rows.len());
    for row in rows {
        let identities = identities(conn, row.id)?;
        let unread = account_unread(conn, row.id)?;
        views.push(AccountView {
            id: row.id,
            display_name: row.display_name,
            provider: row.provider,
            color: row.color,
            email: row.email,
            identities: identities.iter().map(IdentityRow::to_view).collect(),
            sync: row.sync,
            unread,
            position: row.position,
        });
    }
    Ok(views)
}
