//! Thread and message persistence: mailbox views, search, actions, and sync ingest.

use rusqlite::types::Value;
use rusqlite::{params, params_from_iter, Connection, Row};

use crate::error::CoreError;
use crate::ids::{AccountId, MessageId, ThreadId, UnixMs};
use crate::sanitize::detect_trackers;
use crate::store::{
    account_exists, blob_to_id, fts_insert, id_to_blob, query_all, query_one, row_id, to_json,
    SqlResultExt,
};
use crate::sync::IngestMessage;
use crate::types::{
    ActionOutcome, InboxCategory, MailAction, MailboxCounts, MailboxView, Scope, SpaceColor,
    ThreadPage, ThreadQuery, ThreadSummary, UnsubscribeInfo,
};

pub(crate) struct ThreadRow {
    pub id: ThreadId,
    pub account_id: AccountId,
    pub subject: String,
    pub starred: bool,
}

pub(crate) struct MessageRow {
    pub id: MessageId,
    pub message_id_hdr: String,
    pub from_name: Option<String>,
    pub from_email: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub date: UnixMs,
    pub snippet: String,
    pub body_text: String,
    pub raw_html: Option<String>,
    pub unsub: Option<UnsubscribeInfo>,
}

pub(crate) struct NewThread<'a> {
    pub id: ThreadId,
    pub account_id: AccountId,
    pub remote_key: &'a str,
    pub subject: &'a str,
    pub snippet: &'a str,
    pub sender_name: &'a str,
    pub sender_email: &'a str,
    pub last_at: UnixMs,
    pub category: InboxCategory,
}

pub(crate) struct NewMessage {
    pub id: MessageId,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub message_id_hdr: String,
    pub in_reply_to: Option<String>,
    pub from_name: Option<String>,
    pub from_email: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub date: UnixMs,
    pub snippet: String,
    pub is_read: bool,
    pub body_text: String,
    pub raw_html: Option<String>,
    pub unsub: Option<UnsubscribeInfo>,
    pub trackers: u32,
}

const SNOOZED_ARM: &str = "EXISTS (SELECT 1 FROM job j WHERE j.thread_id = t.id AND j.kind = 'snooze' AND j.live = 1 AND json_extract(j.state_json, '$.kind') = 'armed')";
const FOLLOW_UP_LIVE: &str = "EXISTS (SELECT 1 FROM job j WHERE j.thread_id = t.id AND j.kind = 'follow_up' AND j.live = 1 AND json_extract(j.state_json, '$.kind') IN ('armed','fired'))";
const SEND_PENDING: &str = "EXISTS (SELECT 1 FROM job j WHERE j.thread_id = t.id AND j.kind = 'send' AND j.live = 1 AND json_extract(j.state_json, '$.kind') IN ('scheduled','retryWait'))";

fn map_thread(row: &Row<'_>) -> Result<ThreadRow, CoreError> {
    Ok(ThreadRow {
        id: row_id(row, 0, ThreadId::from_bytes)?,
        account_id: row_id(row, 1, AccountId::from_bytes)?,
        subject: row.get(2).sq()?,
        starred: row.get::<_, i64>(3).sq()? != 0,
    })
}

/// Columns for the thread-list projection (see `list_threads`).
const THREAD_COLS: &str =
    "t.id, t.account_id, t.subject, t.snippet, t.sender_name, t.sender_email, t.last_at, t.unread_count, t.message_count, t.starred, t.archived, t.category, t.trashed";

/// Columns `open_thread` needs for one thread.
const THREAD_COLS_SINGLE: &str = "t.id, t.account_id, t.subject, t.starred";

pub(crate) fn get_thread(
    conn: &mut Connection,
    id: ThreadId,
) -> Result<Option<ThreadRow>, CoreError> {
    let sql = format!("SELECT {THREAD_COLS_SINGLE} FROM thread t WHERE t.id = ?1");
    query_one(conn, &sql, params![id_to_blob(id.as_bytes())], map_thread)
}

fn map_message(row: &Row<'_>) -> Result<MessageRow, CoreError> {
    let to_json_raw: String = row.get(4).sq()?;
    let cc_json_raw: String = row.get(5).sq()?;
    let unsub_raw: Option<String> = row.get(10).sq()?;
    Ok(MessageRow {
        id: row_id(row, 0, MessageId::from_bytes)?,
        message_id_hdr: row.get(1).sq()?,
        from_name: row.get(2).sq()?,
        from_email: row.get(3).sq()?,
        to: crate::store::from_json(&to_json_raw)?,
        cc: crate::store::from_json(&cc_json_raw)?,
        date: UnixMs(row.get(6).sq()?),
        snippet: row.get(7).sq()?,
        body_text: row.get(8).sq()?,
        raw_html: row.get(9).sq()?,
        unsub: match unsub_raw {
            Some(raw) => Some(crate::store::from_json(&raw)?),
            None => None,
        },
    })
}

const MESSAGE_COLS: &str =
    "id, message_id_hdr, from_name, from_email, to_json, cc_json, date, snippet, body_text, raw_html, unsub_json";

pub(crate) fn load_messages(
    conn: &mut Connection,
    thread_id: ThreadId,
) -> Result<Vec<MessageRow>, CoreError> {
    let sql = format!(
        "SELECT {MESSAGE_COLS} FROM message WHERE thread_id = ?1 ORDER BY date ASC, rowid ASC"
    );
    query_all(
        conn,
        &sql,
        params![id_to_blob(thread_id.as_bytes())],
        map_message,
    )
}

pub(crate) fn last_message(
    conn: &mut Connection,
    thread_id: ThreadId,
) -> Result<Option<MessageRow>, CoreError> {
    let sql = format!(
        "SELECT {MESSAGE_COLS} FROM message WHERE thread_id = ?1 ORDER BY date DESC, rowid DESC LIMIT 1"
    );
    query_one(
        conn,
        &sql,
        params![id_to_blob(thread_id.as_bytes())],
        map_message,
    )
}

/// Insert or update a thread keyed by `(account_id, remote_key)`; returns the thread id.
pub(crate) fn upsert_thread(
    conn: &mut Connection,
    t: &NewThread<'_>,
) -> Result<ThreadId, CoreError> {
    let sql = "INSERT INTO thread (id, account_id, remote_key, subject, snippet, sender_name, sender_email, last_at, unread_count, message_count, has_attachments, starred, archived, trashed, category, trackers_blocked)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, 0, 0, 0, 0, 0, ?9, 0)
         ON CONFLICT(account_id, remote_key) DO UPDATE SET
           subject = excluded.subject,
           snippet = excluded.snippet,
           sender_name = excluded.sender_name,
           sender_email = excluded.sender_email,
           last_at = MAX(thread.last_at, excluded.last_at)
         RETURNING id";
    let id: Vec<u8> = conn
        .query_row(
            sql,
            params![
                id_to_blob(t.id.as_bytes()),
                id_to_blob(t.account_id.as_bytes()),
                t.remote_key,
                t.subject,
                t.snippet,
                t.sender_name,
                t.sender_email,
                t.last_at.0,
                t.category.as_str(),
            ],
            |r| r.get(0),
        )
        .sq()?;
    blob_to_id(&id, ThreadId::from_bytes)
}

/// Insert a message (idempotent on `(account_id, message_id_hdr)`), index it in FTS5,
/// and roll its tracker count into the thread badge. Returns true when newly inserted.
pub(crate) fn insert_message(conn: &mut Connection, m: &NewMessage) -> Result<bool, CoreError> {
    let exists: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM message WHERE account_id = ?1 AND message_id_hdr = ?2",
            params![id_to_blob(m.account_id.as_bytes()), m.message_id_hdr],
            |r| r.get(0),
        )
        .sq()?;
    if exists > 0 {
        return Ok(false);
    }
    conn.execute(
        "INSERT INTO message (id, account_id, thread_id, message_id_hdr, in_reply_to, from_name, from_email, to_json, cc_json, subject, date, snippet, is_read, body_state, body_text, raw_html, unsub_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'full', ?14, ?15, ?16)",
        params![
            id_to_blob(m.id.as_bytes()),
            id_to_blob(m.account_id.as_bytes()),
            id_to_blob(m.thread_id.as_bytes()),
            m.message_id_hdr,
            m.in_reply_to,
            m.from_name,
            m.from_email,
            to_json(&m.to)?,
            to_json(&m.cc)?,
            m.subject,
            m.date.0,
            m.snippet,
            m.is_read as i64,
            m.body_text,
            m.raw_html,
            m.unsub.as_ref().map(to_json).transpose()?,
        ],
    )
    .sq()?;

    let rowid: i64 = conn
        .query_row(
            "SELECT rowid FROM message WHERE account_id = ?1 AND message_id_hdr = ?2",
            params![id_to_blob(m.account_id.as_bytes()), m.message_id_hdr],
            |r| r.get(0),
        )
        .sq()?;
    let sender = match &m.from_name {
        Some(name) => format!("{name} {}", m.from_email),
        None => m.from_email.clone(),
    };
    fts_insert(conn, rowid, &m.subject, &sender, &m.body_text)?;

    if m.trackers > 0 {
        conn.execute(
            "UPDATE thread SET trackers_blocked = trackers_blocked + ?2 WHERE id = ?1",
            params![id_to_blob(m.thread_id.as_bytes()), m.trackers as i64],
        )
        .sq()?;
    }
    refresh_thread_summary(conn, m.thread_id)?;
    Ok(true)
}

/// Recompute derived thread columns from its messages.
pub(crate) fn refresh_thread_summary(
    conn: &mut Connection,
    thread_id: ThreadId,
) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE thread SET
           last_at = COALESCE((SELECT MAX(date) FROM message WHERE thread_id = ?1), last_at),
           message_count = (SELECT COUNT(*) FROM message WHERE thread_id = ?1),
           unread_count = (SELECT COUNT(*) FROM message WHERE thread_id = ?1 AND is_read = 0),
           snippet = COALESCE((SELECT snippet FROM message WHERE thread_id = ?1 ORDER BY date DESC, rowid DESC LIMIT 1), snippet),
           sender_name = COALESCE((SELECT COALESCE(from_name, '') FROM message WHERE thread_id = ?1 ORDER BY date DESC, rowid DESC LIMIT 1), sender_name),
           sender_email = COALESCE((SELECT from_email FROM message WHERE thread_id = ?1 ORDER BY date DESC, rowid DESC LIMIT 1), sender_email)
         WHERE id = ?1",
        params![id_to_blob(thread_id.as_bytes())],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn mark_thread_read(
    conn: &mut Connection,
    thread_id: ThreadId,
) -> Result<bool, CoreError> {
    let changed = conn
        .execute(
            "UPDATE message SET is_read = 1 WHERE thread_id = ?1 AND is_read = 0",
            params![id_to_blob(thread_id.as_bytes())],
        )
        .sq()?;
    if changed > 0 {
        conn.execute(
            "UPDATE thread SET unread_count = 0 WHERE id = ?1",
            params![id_to_blob(thread_id.as_bytes())],
        )
        .sq()?;
    }
    Ok(changed > 0)
}

fn in_clause(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(",")
}

fn id_values(ids: &[ThreadId]) -> Vec<Value> {
    ids.iter()
        .map(|id| Value::Blob(id_to_blob(id.as_bytes())))
        .collect()
}

/// Apply an idempotent mail action; returns the requested ids that exist (input order).
pub(crate) fn apply_mail_action(
    conn: &mut Connection,
    action: &MailAction,
    ids: &[ThreadId],
) -> Result<ActionOutcome, CoreError> {
    if ids.is_empty() {
        return Ok(ActionOutcome {
            updated_ids: Vec::new(),
            undo_action: None,
        });
    }

    let mut existing: Vec<ThreadId> = Vec::new();
    for chunk in ids.chunks(400) {
        let values = id_values(chunk);
        let placeholders = in_clause(chunk.len());

        let (sql, params): (String, Vec<Value>) = match action {
            MailAction::Archive => (
                format!("UPDATE thread SET archived = 1, spam = 0 WHERE id IN ({placeholders})"),
                values.clone(),
            ),
            MailAction::Unarchive => (
                format!("UPDATE thread SET archived = 0, trashed = 0, spam = 0 WHERE id IN ({placeholders})"),
                values.clone(),
            ),
            MailAction::Trash => (
                format!("UPDATE thread SET trashed = 1, spam = 0 WHERE id IN ({placeholders})"),
                values.clone(),
            ),
            MailAction::ReportSpam => (
                format!("UPDATE thread SET spam = 1, archived = 0, trashed = 0 WHERE id IN ({placeholders})"),
                values.clone(),
            ),
            MailAction::MarkRead { read } => (
                format!("UPDATE message SET is_read = {read} WHERE thread_id IN ({placeholders})"),
                values.clone(),
            ),
            MailAction::Star { on } => (
                format!("UPDATE thread SET starred = {on} WHERE id IN ({placeholders})"),
                values.clone(),
            ),
        };
        conn.execute(&sql, params_from_iter(params.iter())).sq()?;

        if matches!(action, MailAction::MarkRead { .. }) {
            conn.execute(
                &format!(
                    "UPDATE thread SET unread_count = (
                        SELECT COUNT(*) FROM message m WHERE m.thread_id = thread.id AND m.is_read = 0
                     ) WHERE id IN ({placeholders})"
                ),
                params_from_iter(values.iter()),
            )
            .sq()?;
        }

        let select_sql = format!("SELECT t.id FROM thread t WHERE t.id IN ({placeholders})");
        let mut stmt = conn.prepare(&select_sql).sq()?;
        let rows = stmt
            .query_map(params_from_iter(values.iter()), |r| r.get::<_, Vec<u8>>(0))
            .sq()?;
        for row in rows {
            let bytes = row.sq()?;
            existing.push(blob_to_id(&bytes, ThreadId::from_bytes)?);
        }
    }

    let seen: std::collections::HashSet<ThreadId> = existing.iter().copied().collect();
    let updated_ids: Vec<ThreadId> = ids.iter().filter(|id| seen.contains(id)).copied().collect();

    let undo_action = match action {
        MailAction::Archive => Some(MailAction::Unarchive),
        MailAction::Unarchive => Some(MailAction::Archive),
        // Restoring from trash clears both flags; trashing an already-archived thread
        // therefore lands in the inbox on undo, which matches user expectation.
        MailAction::Trash => Some(MailAction::Unarchive),
        MailAction::ReportSpam => Some(MailAction::Unarchive),
        MailAction::MarkRead { read } => Some(MailAction::MarkRead { read: !read }),
        MailAction::Star { on } => Some(MailAction::Star { on: !on }),
    };

    Ok(ActionOutcome {
        updated_ids,
        undo_action,
    })
}

/// Build the `t.id IN (…)` search fragment: FTS5 MATCH first, LIKE fallback when FTS
/// cannot serve the token shape (too short, or zero hits).
fn search_fragment(conn: &mut Connection, text: &str) -> Result<(String, Vec<Value>), CoreError> {
    let tokens: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .take(12)
        .map(|t| t.to_lowercase())
        .collect();
    if tokens.is_empty() {
        return Ok(("0 = 1".to_string(), Vec::new()));
    }

    let fts_query = tokens
        .iter()
        .map(|t| format!("\"{t}\"*"))
        .collect::<Vec<_>>()
        .join(" AND ");

    let fts_hit: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM message m WHERE m.rowid IN (
                 SELECT rowid FROM message_fts WHERE message_fts MATCH ?1))",
            params![fts_query],
            |r| r.get(0),
        )
        .unwrap_or(false);

    if fts_hit {
        return Ok((
            "t.id IN (SELECT m.thread_id FROM message m WHERE m.rowid IN (
                 SELECT rowid FROM message_fts WHERE message_fts MATCH ?))"
                .to_string(),
            vec![Value::Text(fts_query)],
        ));
    }

    // Partial / short tokens: substring match across thread and message text.
    let mut conds = Vec::new();
    let mut values = Vec::new();
    for token in &tokens {
        conds.push(
            "(lower(m.subject) LIKE ? OR lower(m.from_email) LIKE ? OR lower(m.body_text) LIKE ?
              OR lower(t.sender_name) LIKE ? OR lower(t.sender_email) LIKE ?)",
        );
        let pattern = Value::Text(format!("%{token}%"));
        for _ in 0..5 {
            values.push(pattern.clone());
        }
    }
    Ok((
        format!(
            "t.id IN (SELECT m.thread_id FROM message m WHERE {})",
            conds.join(" AND ")
        ),
        values,
    ))
}

fn scope_clause(scope: &Scope) -> Result<(String, Vec<Value>), CoreError> {
    match scope {
        Scope::Unified => Ok(("1 = 1".to_string(), Vec::new())),
        Scope::Account { account_id } => Ok((
            "t.account_id = ?".to_string(),
            vec![Value::Blob(id_to_blob(account_id.as_bytes()))],
        )),
    }
}

fn sent_clause(scope: &Scope) -> (String, Vec<Value>) {
    match scope {
        Scope::Unified => (
            "EXISTS (SELECT 1 FROM message m WHERE m.thread_id = t.id
                 AND lower(m.from_email) IN (SELECT lower(address) FROM identity))"
                .to_string(),
            Vec::new(),
        ),
        Scope::Account { account_id } => (
            "EXISTS (SELECT 1 FROM message m WHERE m.thread_id = t.id
                 AND lower(m.from_email) IN (
                     SELECT lower(address) FROM identity WHERE account_id = ?))"
                .to_string(),
            vec![Value::Blob(id_to_blob(account_id.as_bytes()))],
        ),
    }
}

pub(crate) fn list_threads(
    conn: &mut Connection,
    query: &ThreadQuery,
    _now: UnixMs,
) -> Result<ThreadPage, CoreError> {
    if let Scope::Account { account_id } = &query.scope {
        if !account_exists(conn, *account_id)? {
            return Err(CoreError::NotFound {
                entity: "account".to_string(),
            });
        }
    }

    let mut wheres: Vec<(String, Vec<Value>)> = Vec::new();
    wheres.push(scope_clause(&query.scope)?);

    let mut order = "t.last_at DESC, t.rowid DESC".to_string();
    match &query.view {
        MailboxView::Inbox { category } => {
            wheres.push(("t.archived = 0 AND t.trashed = 0 AND t.spam = 0".to_string(), Vec::new()));
            wheres.push((format!("NOT {SNOOZED_ARM}"), Vec::new()));
            if *category != InboxCategory::All {
                wheres.push((
                    "t.category = ?".to_string(),
                    vec![Value::Text(category.as_str().to_string())],
                ));
            }
        }
        MailboxView::Snoozed => {
            wheres.push(("t.trashed = 0 AND t.spam = 0".to_string(), Vec::new()));
            wheres.push((SNOOZED_ARM.to_string(), Vec::new()));
            order = "snoozed_until ASC, t.last_at DESC".to_string();
        }
        MailboxView::FollowUps => {
            wheres.push(("t.trashed = 0 AND t.spam = 0".to_string(), Vec::new()));
            wheres.push((FOLLOW_UP_LIVE.to_string(), Vec::new()));
            order = "follow_up_due ASC, t.last_at DESC".to_string();
        }
        MailboxView::Scheduled => {
            wheres.push(("t.trashed = 0 AND t.spam = 0".to_string(), Vec::new()));
            wheres.push((SEND_PENDING.to_string(), Vec::new()));
            order = "scheduled_at ASC, t.last_at DESC".to_string();
        }
        MailboxView::Sent => {
            wheres.push(("t.trashed = 0 AND t.spam = 0".to_string(), Vec::new()));
            wheres.push(sent_clause(&query.scope));
        }
        MailboxView::Drafts => {
            wheres.push(("t.trashed = 0 AND t.spam = 0".to_string(), Vec::new()));
            wheres.push((
                "EXISTS (SELECT 1 FROM draft d WHERE d.reply_to_thread = t.id
                     AND json_extract(d.state_json, '$.kind') IN ('editing','pendingApproval','queued'))"
                    .to_string(),
                Vec::new(),
            ));
        }
        MailboxView::Starred => {
            wheres.push((
                "t.archived = 0 AND t.trashed = 0 AND t.spam = 0 AND t.starred = 1".to_string(),
                Vec::new(),
            ));
        }
        MailboxView::Archive => {
            wheres.push(("t.archived = 1 AND t.trashed = 0 AND t.spam = 0".to_string(), Vec::new()));
        }
        MailboxView::Spam => {
            wheres.push(("t.spam = 1 AND t.trashed = 0".to_string(), Vec::new()));
        }
        MailboxView::Trash => {
            wheres.push(("t.trashed = 1".to_string(), Vec::new()));
        }
        MailboxView::Label { .. } => {
            // Labels arrive with Gmail label sync (Phase 2); no label can exist yet.
            return Err(CoreError::NotFound {
                entity: "label".to_string(),
            });
        }
        MailboxView::Search { text } => {
            wheres.push(("t.trashed = 0 AND t.spam = 0".to_string(), Vec::new()));
            wheres.push(search_fragment(conn, text)?);
        }
    }

    let where_sql = wheres
        .iter()
        .map(|(clause, _)| format!("({clause})"))
        .collect::<Vec<_>>()
        .join(" AND ");
    let mut params: Vec<Value> = wheres
        .iter()
        .flat_map(|(_, values)| values.iter().cloned())
        .collect();

    let count_sql = format!(
        "SELECT COUNT(*) FROM thread t JOIN account a ON a.id = t.account_id WHERE {where_sql}"
    );
    let total: i64 = conn
        .query_row(&count_sql, params_from_iter(params.iter()), |r| r.get(0))
        .sq()?;
    params.push(Value::Integer(query.limit.clamp(1, 200) as i64));
    params.push(Value::Integer(query.offset as i64));

    let list_sql = format!(
        "SELECT {THREAD_COLS},
                a.display_name, a.color, a.email,
                (SELECT MIN(j.due_at) FROM job j WHERE j.thread_id = t.id AND j.kind = 'snooze'
                     AND j.live = 1 AND json_extract(j.state_json, '$.kind') = 'armed') AS snoozed_until,
                (SELECT MIN(j.due_at) FROM job j WHERE j.thread_id = t.id AND j.kind = 'follow_up'
                     AND j.live = 1 AND json_extract(j.state_json, '$.kind') IN ('armed','fired')) AS follow_up_due,
                (SELECT MIN(j.due_at) FROM job j WHERE j.thread_id = t.id AND j.kind = 'send'
                     AND j.live = 1 AND json_extract(j.state_json, '$.kind') IN ('scheduled','retryWait')) AS scheduled_at
         FROM thread t JOIN account a ON a.id = t.account_id
         WHERE {where_sql}
         ORDER BY {order} LIMIT ? OFFSET ?"
    );

    let mut stmt = conn.prepare(&list_sql).sq()?;
    struct RawSummary {
        id: Vec<u8>,
        account_id: Vec<u8>,
        subject: String,
        snippet: String,
        sender_name: String,
        sender_email: String,
        last_at: i64,
        unread_count: i64,
        message_count: i64,
        has_attachments: i64,
        starred: i64,
        category: String,
        trackers_blocked: i64,
        account_label: String,
        account_color: String,
        account_email: String,
        snoozed_until: Option<i64>,
        follow_up_due: Option<i64>,
    }
    let rows = stmt
        .query_map(params_from_iter(params.iter()), |row| {
            Ok(RawSummary {
                id: row.get(0)?,
                account_id: row.get(1)?,
                subject: row.get(2)?,
                snippet: row.get(3)?,
                sender_name: row.get(4)?,
                sender_email: row.get(5)?,
                last_at: row.get(6)?,
                unread_count: row.get(7)?,
                message_count: row.get(8)?,
                has_attachments: row.get(9)?,
                starred: row.get(10)?,
                category: row.get(11)?,
                trackers_blocked: row.get(12)?,
                account_label: row.get(13)?,
                account_color: row.get(14)?,
                account_email: row.get(15)?,
                snoozed_until: row.get(16)?,
                follow_up_due: row.get(17)?,
            })
        })
        .sq()?;
    let mut items = Vec::new();
    for row in rows {
        let raw = row.sq()?;
        items.push(ThreadSummary {
            id: blob_to_id(&raw.id, ThreadId::from_bytes)?,
            account_id: blob_to_id(&raw.account_id, AccountId::from_bytes)?,
            subject: raw.subject,
            snippet: raw.snippet,
            sender_name: raw.sender_name,
            sender_email: raw.sender_email,
            last_at: UnixMs(raw.last_at),
            unread_count: raw.unread_count.max(0) as u32,
            message_count: raw.message_count.max(0) as u32,
            has_attachments: raw.has_attachments != 0,
            starred: raw.starred != 0,
            category: InboxCategory::from_db(&raw.category),
            trackers_blocked: raw.trackers_blocked.max(0) as u32,
            account_label: raw.account_label,
            account_color: SpaceColor::from_db(&raw.account_color),
            account_email: raw.account_email,
            snoozed_until: raw.snoozed_until.map(UnixMs),
            follow_up_due: raw.follow_up_due.map(UnixMs),
            labels: Vec::new(),
        });
    }

    Ok(ThreadPage {
        items,
        total: total.max(0) as u32,
    })
}

fn count_where(
    conn: &mut Connection,
    scope_sql: &str,
    scope_values: &[Value],
    extra: &str,
) -> Result<u32, CoreError> {
    let sql = format!("SELECT COUNT(*) FROM thread t WHERE ({scope_sql}) AND ({extra})");
    let count: i64 = conn
        .query_row(&sql, params_from_iter(scope_values.iter()), |r| r.get(0))
        .sq()?;
    Ok(count.max(0) as u32)
}

pub(crate) fn mailbox_counts(
    conn: &mut Connection,
    scope: &Scope,
    _now: UnixMs,
) -> Result<MailboxCounts, CoreError> {
    if let Scope::Account { account_id } = scope {
        if !account_exists(conn, *account_id)? {
            return Err(CoreError::NotFound {
                entity: "account".to_string(),
            });
        }
    }
    let (scope_sql, scope_values) = scope_clause(scope)?;
    let inbox_base = format!("t.archived = 0 AND t.trashed = 0 AND t.spam = 0 AND NOT {SNOOZED_ARM}");

    Ok(MailboxCounts {
        inbox: count_where(conn, &scope_sql, &scope_values, &inbox_base)?,
        snoozed: count_where(conn, &scope_sql, &scope_values, SNOOZED_ARM)?,
        follow_ups: count_where(conn, &scope_sql, &scope_values, FOLLOW_UP_LIVE)?,
        scheduled: count_where(conn, &scope_sql, &scope_values, SEND_PENDING)?,
        primary: count_where(
            conn,
            &scope_sql,
            &scope_values,
            &format!("{inbox_base} AND t.category = 'primary'"),
        )?,
        receipts: count_where(
            conn,
            &scope_sql,
            &scope_values,
            &format!("{inbox_base} AND t.category = 'receipts'"),
        )?,
        newsletters: count_where(
            conn,
            &scope_sql,
            &scope_values,
            &format!("{inbox_base} AND t.category = 'newsletters'"),
        )?,
    })
}

/// Distinct accounts owning any of the given threads (for per-account change events).
pub(crate) fn thread_account_ids(
    conn: &mut Connection,
    ids: &[ThreadId],
) -> Result<Vec<AccountId>, CoreError> {
    let mut accounts: Vec<AccountId> = Vec::new();
    for chunk in ids.chunks(400) {
        let values = id_values(chunk);
        let sql = format!(
            "SELECT DISTINCT t.account_id FROM thread t WHERE t.id IN ({})",
            in_clause(chunk.len())
        );
        let mut stmt = conn.prepare(&sql).sq()?;
        let rows = stmt
            .query_map(params_from_iter(values.iter()), |r| r.get::<_, Vec<u8>>(0))
            .sq()?;
        for row in rows {
            let bytes = row.sq()?;
            let account_id = blob_to_id(&bytes, AccountId::from_bytes)?;
            if !accounts.contains(&account_id) {
                accounts.push(account_id);
            }
        }
    }
    Ok(accounts)
}

/// Ingest parsed IMAP messages: upsert threads, insert messages + FTS rows, refresh
/// derived columns. Message-level dedup makes re-sync idempotent.
pub(crate) fn ingest_messages(
    conn: &mut Connection,
    account_id: AccountId,
    messages: Vec<IngestMessage>,
    now: UnixMs,
) -> Result<Vec<ThreadId>, CoreError> {
    let mut touched: Vec<ThreadId> = Vec::new();
    for incoming in messages {
        let date = incoming.date.unwrap_or(now);
        let thread_id = upsert_thread(
            conn,
            &NewThread {
                id: ThreadId::fresh(),
                account_id,
                remote_key: &incoming.thread_key,
                subject: &incoming.subject,
                snippet: &incoming.snippet,
                sender_name: &incoming.from_name.clone().unwrap_or_default(),
                sender_email: &incoming.from_email,
                last_at: date,
                category: incoming.category,
            },
        )?;
        let msg_id = MessageId::fresh();
        let inserted = insert_message(
            conn,
            &NewMessage {
                id: msg_id,
                account_id,
                thread_id,
                message_id_hdr: incoming.message_id_hdr.clone(),
                in_reply_to: incoming.in_reply_to.clone(),
                from_name: incoming.from_name.clone(),
                from_email: incoming.from_email.clone(),
                to: incoming.to.clone(),
                cc: incoming.cc.clone(),
                subject: incoming.subject.clone(),
                date,
                snippet: incoming.snippet.clone(),
                is_read: incoming.is_read,
                body_text: incoming.body_text.clone(),
                raw_html: incoming.raw_html.clone(),
                unsub: incoming.unsub.clone(),
                trackers: incoming
                    .raw_html
                    .as_deref()
                    .map(|html| detect_trackers(html).len() as u32)
                    .unwrap_or(0),
            },
        )?;
        if inserted {
            for payload in &incoming.ics_payloads {
                for parsed in crate::calendar::parse_ics(payload) {
                    crate::store::upsert_message_invite(conn, msg_id, &parsed)?;
                }
            }
            if !touched.contains(&thread_id) {
                touched.push(thread_id);
            }
        }
    }
    Ok(touched)
}
