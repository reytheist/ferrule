//! Durable SQLite offline action queue processed in bounded fair slices so bulk
//! mutations never starve incoming mail sync or outgoing SMTP sends.

use rusqlite::{params, Connection, Row};

use crate::error::CoreError;
use crate::ids::{AccountId, OfflineActionId, ThreadId, UnixMs};
use crate::store::{from_json, id_to_blob, query_all, row_id, to_json, SqlResultExt};
use crate::types::MailAction;

/// Default fair-slice cap: process at most 20 queued mutations per actor turn.
pub(crate) const DEFAULT_SLICE_MAX_ACTIONS: usize = 20;
/// Maximum retry attempts before an offline action is marked failed.
pub(crate) const MAX_ACTION_ATTEMPTS: u32 = 8;

#[derive(Debug, Clone)]
pub(crate) struct OfflineActionRow {
    pub id: OfflineActionId,
    pub thread_id: Option<ThreadId>,
    pub action: MailAction,
    pub attempts: u32,
}

fn map_action(row: &Row<'_>) -> Result<OfflineActionRow, CoreError> {
    let action_raw: String = row.get(2).sq()?;
    Ok(OfflineActionRow {
        id: row_id(row, 0, OfflineActionId::from_bytes)?,
        thread_id: match row.get::<_, Option<Vec<u8>>>(1).sq()? {
            Some(bytes) => Some(crate::store::blob_to_id(&bytes, ThreadId::from_bytes)?),
            None => None,
        },
        action: from_json(&action_raw)?,
        attempts: row.get::<_, i64>(3).sq()?.max(0) as u32,
    })
}

/// Enqueue one durable offline action per `(account_id, thread_id)` pair.
pub(crate) fn enqueue_offline_actions(
    conn: &mut Connection,
    entries: &[(AccountId, ThreadId)],
    action: &MailAction,
    now: UnixMs,
) -> Result<usize, CoreError> {
    if entries.is_empty() {
        return Ok(0);
    }
    let action_json = to_json(action)?;
    let tx = conn.transaction().sq()?;
    let mut inserted = 0usize;
    for &(account_id, thread_id) in entries {
        let id = OfflineActionId::fresh();
        tx.execute(
            "INSERT INTO offline_action
               (id, account_id, thread_id, action_json, status, attempts, next_retry_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', 0, 0, ?5, ?5)",
            params![
                id_to_blob(id.as_bytes()),
                id_to_blob(account_id.as_bytes()),
                id_to_blob(thread_id.as_bytes()),
                action_json,
                now.0,
            ],
        )
        .sq()?;
        inserted += 1;
    }
    tx.commit().sq()?;
    Ok(inserted)
}

/// Reset any actions stranded in `'in_flight'` after an unclean shutdown back to `'pending'`.
pub(crate) fn recover_in_flight_actions(conn: &mut Connection) -> Result<usize, CoreError> {
    let changed = conn
        .execute(
            "UPDATE offline_action SET status = 'pending' WHERE status = 'in_flight'",
            [],
        )
        .sq()?;
    Ok(changed)
}

/// Claim up to `limit` due pending actions for `account_id` in FIFO order (a single fair slice).
pub(crate) fn claim_action_slice(
    conn: &mut Connection,
    account_id: AccountId,
    now: UnixMs,
    limit: usize,
) -> Result<Vec<OfflineActionRow>, CoreError> {
    let cap = limit.clamp(1, 100) as i64;
    let rows = query_all(
        conn,
        "SELECT id, thread_id, action_json, attempts
         FROM offline_action
         WHERE account_id = ?1 AND status = 'pending' AND next_retry_at <= ?2
         ORDER BY created_at ASC, rowid ASC
         LIMIT ?3",
        params![id_to_blob(account_id.as_bytes()), now.0, cap],
        map_action,
    )?;
    if rows.is_empty() {
        return Ok(rows);
    }
    let tx = conn.transaction().sq()?;
    for row in &rows {
        tx.execute(
            "UPDATE offline_action SET status = 'in_flight', updated_at = ?2 WHERE id = ?1",
            params![id_to_blob(row.id.as_bytes()), now.0],
        )
        .sq()?;
    }
    tx.commit().sq()?;
    Ok(rows)
}

/// Mark an offline action completed and prune it from the queue.
pub(crate) fn complete_offline_action(
    conn: &mut Connection,
    id: OfflineActionId,
) -> Result<(), CoreError> {
    conn.execute(
        "DELETE FROM offline_action WHERE id = ?1",
        params![id_to_blob(id.as_bytes())],
    )
    .sq()?;
    Ok(())
}

/// Re-queue an offline action with exponential backoff, or mark `'failed'` once
/// `MAX_ACTION_ATTEMPTS` is reached.
pub(crate) fn retry_or_fail_offline_action(
    conn: &mut Connection,
    id: OfflineActionId,
    attempts: u32,
    reason: &str,
    now: UnixMs,
) -> Result<bool, CoreError> {
    let next_attempts = attempts.saturating_add(1);
    if next_attempts >= MAX_ACTION_ATTEMPTS {
        conn.execute(
            "UPDATE offline_action
             SET status = 'failed', attempts = ?2, last_error = ?3, updated_at = ?4
             WHERE id = ?1",
            params![
                id_to_blob(id.as_bytes()),
                i64::from(next_attempts),
                reason,
                now.0,
            ],
        )
        .sq()?;
        return Ok(false);
    }
    let backoff_secs = (2_i64.saturating_pow(next_attempts.min(8))).min(300);
    let next_retry = now.add_secs(backoff_secs);
    conn.execute(
        "UPDATE offline_action
         SET status = 'pending', attempts = ?2, next_retry_at = ?3, last_error = ?4, updated_at = ?5
         WHERE id = ?1",
        params![
            id_to_blob(id.as_bytes()),
            i64::from(next_attempts),
            next_retry.0,
            reason,
            now.0,
        ],
    )
    .sq()?;
    Ok(true)
}

/// Return the number of pending actions queued for an account.
pub(crate) fn pending_action_count(
    conn: &mut Connection,
    account_id: AccountId,
) -> Result<u32, CoreError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM offline_action WHERE account_id = ?1 AND status IN ('pending', 'in_flight')",
            params![id_to_blob(account_id.as_bytes())],
            |r| r.get(0),
        )
        .sq()?;
    Ok(count.max(0) as u32)
}

/// Look up `(folder_name, imap_uid)` pairs for all messages in a thread so remote IMAP
/// actions can apply flags/moves to the corresponding server UIDs.
pub(crate) fn thread_imap_uids(
    conn: &mut Connection,
    thread_id: ThreadId,
) -> Result<Vec<(String, u32)>, CoreError> {
    query_all(
        conn,
        "SELECT folder_name, imap_uid FROM message
         WHERE thread_id = ?1 AND imap_uid IS NOT NULL AND imap_uid > 0",
        params![id_to_blob(thread_id.as_bytes())],
        |row| {
            let folder: String = row.get(0).sq()?;
            let uid: i64 = row.get(1).sq()?;
            Ok((folder, uid as u32))
        },
    )
}
