//! Phase 3 persistence: Saved Searches, Sender Screener, VIP Senders, Labels, and Mail Rules.

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::CoreError;
use crate::ids::{LabelId, RuleId, SavedSearchId, ThreadId, UnixMs};
use crate::rules::{matches_rule, RuleAction, RuleCondition, RuleEvalTarget};
use crate::store::{
    from_json, id_to_blob, query_all, query_one, row_id, to_json, SqlResultExt,
};
use crate::types::{
    InboxCategory, LabelBadge, LabelView, MailRuleView, SaveLabelInput, SaveMailRuleInput,
    SaveSavedSearchInput, SavedSearchView, ScreenerDecision, ScreenerSenderView, SpaceColor,
    VipSenderView,
};

// ---------------------------------------------------------------- saved searches

pub(crate) fn list_saved_searches(conn: &mut Connection) -> Result<Vec<SavedSearchView>, CoreError> {
    query_all(
        conn,
        "SELECT id, name, query FROM saved_search ORDER BY position ASC, created_at ASC",
        [],
        |row| {
            Ok(SavedSearchView {
                id: row_id(row, 0, SavedSearchId::from_bytes)?,
                name: row.get(1).sq()?,
                query: row.get(2).sq()?,
            })
        },
    )
}

pub(crate) fn save_saved_search(
    conn: &mut Connection,
    input: &SaveSavedSearchInput,
    now: UnixMs,
) -> Result<SavedSearchView, CoreError> {
    let name = input.name.trim();
    let query = input.query.trim();
    if name.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "name".to_string(),
            reason: "saved search name is required".to_string(),
        });
    }
    if query.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "query".to_string(),
            reason: "search query is required".to_string(),
        });
    }
    let id = input.id.unwrap_or_else(SavedSearchId::fresh);
    let updated = conn
        .execute(
            "UPDATE saved_search SET name = ?2, query = ?3 WHERE id = ?1",
            params![id_to_blob(id.as_bytes()), name, query],
        )
        .sq()?;
    if updated == 0 {
        let pos: i64 = conn
            .query_row("SELECT COALESCE(MAX(position), -1) + 1 FROM saved_search", [], |r| r.get(0))
            .sq()?;
        conn.execute(
            "INSERT INTO saved_search (id, name, query, position, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id_to_blob(id.as_bytes()), name, query, pos, now.0],
        )
        .sq()?;
    }
    Ok(SavedSearchView {
        id,
        name: name.to_string(),
        query: query.to_string(),
    })
}

pub(crate) fn delete_saved_search(
    conn: &mut Connection,
    id: SavedSearchId,
) -> Result<bool, CoreError> {
    let changed = conn
        .execute(
            "DELETE FROM saved_search WHERE id = ?1",
            params![id_to_blob(id.as_bytes())],
        )
        .sq()?;
    Ok(changed > 0)
}

// ---------------------------------------------------------------- sender screener

pub(crate) fn list_screener_senders(
    conn: &mut Connection,
) -> Result<Vec<ScreenerSenderView>, CoreError> {
    query_all(
        conn,
        "SELECT lower(t.sender_email) AS addr,
                MAX(t.sender_name) AS name,
                MAX(t.subject) AS sample_subject,
                COUNT(*) AS thread_count,
                MAX(t.last_at) AS latest_at
         FROM thread t
         WHERE t.trashed = 0 AND t.spam = 0
           AND lower(t.sender_email) != ''
           AND lower(t.sender_email) NOT IN (SELECT lower(address) FROM identity)
           AND lower(t.sender_email) NOT IN (SELECT address FROM sender_decision)
         GROUP BY lower(t.sender_email)
         ORDER BY latest_at DESC
         LIMIT 100",
        [],
        |row| {
            Ok(ScreenerSenderView {
                address: row.get(0).sq()?,
                display_name: row.get(1).sq()?,
                sample_subject: row.get(2).sq()?,
                thread_count: row.get::<_, i64>(3).sq()?.max(0) as u32,
                latest_at: UnixMs(row.get(4).sq()?),
            })
        },
    )
}

pub(crate) fn screener_pending_count(conn: &mut Connection) -> Result<u32, CoreError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(DISTINCT lower(t.sender_email))
             FROM thread t
             WHERE t.trashed = 0 AND t.spam = 0
               AND lower(t.sender_email) != ''
               AND lower(t.sender_email) NOT IN (SELECT lower(address) FROM identity)
               AND lower(t.sender_email) NOT IN (SELECT address FROM sender_decision)",
            [],
            |r| r.get(0),
        )
        .sq()?;
    Ok(count.max(0) as u32)
}

pub(crate) fn get_sender_decision(
    conn: &mut Connection,
    address: &str,
) -> Result<Option<ScreenerDecision>, CoreError> {
    let addr = address.trim().to_ascii_lowercase();
    let raw: Option<String> = conn
        .query_row(
            "SELECT decision FROM sender_decision WHERE address = ?1",
            params![addr],
            |r| r.get(0),
        )
        .optional()
        .sq()?;
    Ok(raw.as_deref().map(ScreenerDecision::from_db))
}

pub(crate) fn screen_sender(
    conn: &mut Connection,
    address: &str,
    display_name: Option<&str>,
    decision: ScreenerDecision,
    now: UnixMs,
) -> Result<u32, CoreError> {
    let addr = address.trim().to_ascii_lowercase();
    if addr.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "address".to_string(),
            reason: "sender email address is required".to_string(),
        });
    }
    let tx = conn.transaction().sq()?;
    tx.execute(
        "INSERT INTO sender_decision (address, decision, display_name, decided_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(address) DO UPDATE SET
           decision = excluded.decision,
           display_name = COALESCE(excluded.display_name, sender_decision.display_name),
           decided_at = excluded.decided_at",
        params![addr, decision.as_str(), display_name, now.0],
    )
    .sq()?;

    let affected = match decision {
        ScreenerDecision::Inbox => tx
            .execute(
                "UPDATE thread SET category = 'primary', spam = 0 WHERE lower(sender_email) = ?1",
                params![addr],
            )
            .sq()?,
        ScreenerDecision::PaperTrail => tx
            .execute(
                "UPDATE thread SET category = 'receipts', spam = 0 WHERE lower(sender_email) = ?1",
                params![addr],
            )
            .sq()?,
        ScreenerDecision::Feed => tx
            .execute(
                "UPDATE thread SET category = 'newsletters', spam = 0 WHERE lower(sender_email) = ?1",
                params![addr],
            )
            .sq()?,
        ScreenerDecision::Blocked => tx
            .execute(
                "UPDATE thread SET spam = 1, archived = 0, trashed = 0 WHERE lower(sender_email) = ?1",
                params![addr],
            )
            .sq()?,
    };
    tx.commit().sq()?;
    Ok(affected as u32)
}

// ---------------------------------------------------------------- VIP senders

pub(crate) fn list_vip_senders(conn: &mut Connection) -> Result<Vec<VipSenderView>, CoreError> {
    query_all(
        conn,
        "SELECT address, display_name, added_at FROM vip_sender ORDER BY added_at DESC, address ASC",
        [],
        |row| {
            Ok(VipSenderView {
                address: row.get(0).sq()?,
                display_name: row.get(1).sq()?,
                added_at: UnixMs(row.get(2).sq()?),
            })
        },
    )
}

pub(crate) fn add_vip_sender(
    conn: &mut Connection,
    address: &str,
    display_name: Option<&str>,
    now: UnixMs,
) -> Result<VipSenderView, CoreError> {
    let addr = address.trim().to_ascii_lowercase();
    if addr.is_empty() || !addr.contains('@') {
        return Err(CoreError::InvalidInput {
            field: "address".to_string(),
            reason: "valid email address is required".to_string(),
        });
    }
    conn.execute(
        "INSERT INTO vip_sender (address, display_name, added_at)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(address) DO UPDATE SET
           display_name = COALESCE(excluded.display_name, vip_sender.display_name)",
        params![addr, display_name, now.0],
    )
    .sq()?;
    Ok(VipSenderView {
        address: addr,
        display_name: display_name.map(str::to_string),
        added_at: now,
    })
}

pub(crate) fn remove_vip_sender(conn: &mut Connection, address: &str) -> Result<bool, CoreError> {
    let addr = address.trim().to_ascii_lowercase();
    let changed = conn
        .execute("DELETE FROM vip_sender WHERE address = ?1", params![addr])
        .sq()?;
    Ok(changed > 0)
}

// ---------------------------------------------------------------- labels

pub(crate) fn list_labels(conn: &mut Connection) -> Result<Vec<LabelView>, CoreError> {
    query_all(
        conn,
        "SELECT l.id, l.name, l.color,
                (SELECT COUNT(*) FROM thread_label tl JOIN thread t ON t.id = tl.thread_id
                 WHERE tl.label_id = l.id AND t.trashed = 0 AND t.spam = 0) AS thread_count
         FROM label l
         ORDER BY l.name ASC",
        [],
        |row| {
            let color_raw: String = row.get(2).sq()?;
            Ok(LabelView {
                id: row_id(row, 0, LabelId::from_bytes)?,
                name: row.get(1).sq()?,
                color: SpaceColor::from_db(&color_raw),
                thread_count: row.get::<_, i64>(3).sq()?.max(0) as u32,
            })
        },
    )
}

pub(crate) fn save_label(
    conn: &mut Connection,
    input: &SaveLabelInput,
    now: UnixMs,
) -> Result<LabelView, CoreError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "name".to_string(),
            reason: "label name is required".to_string(),
        });
    }
    let id = input.id.unwrap_or_else(LabelId::fresh);
    let updated = conn
        .execute(
            "UPDATE label SET name = ?2, color = ?3 WHERE id = ?1",
            params![id_to_blob(id.as_bytes()), name, input.color.as_str()],
        )
        .sq()?;
    if updated == 0 {
        conn.execute(
            "INSERT INTO label (id, name, color, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id_to_blob(id.as_bytes()), name, input.color.as_str(), now.0],
        )
        .sq()?;
    }
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM thread_label WHERE label_id = ?1",
            params![id_to_blob(id.as_bytes())],
            |r| r.get(0),
        )
        .sq()?;
    Ok(LabelView {
        id,
        name: name.to_string(),
        color: input.color,
        thread_count: count.max(0) as u32,
    })
}

pub(crate) fn delete_label(conn: &mut Connection, id: LabelId) -> Result<bool, CoreError> {
    let changed = conn
        .execute(
            "DELETE FROM label WHERE id = ?1",
            params![id_to_blob(id.as_bytes())],
        )
        .sq()?;
    Ok(changed > 0)
}

pub(crate) fn set_thread_label(
    conn: &mut Connection,
    label_id: LabelId,
    thread_ids: &[ThreadId],
    add: bool,
) -> Result<(), CoreError> {
    let tx = conn.transaction().sq()?;
    for thread_id in thread_ids {
        if add {
            tx.execute(
                "INSERT OR IGNORE INTO thread_label (thread_id, label_id) VALUES (?1, ?2)",
                params![
                    id_to_blob(thread_id.as_bytes()),
                    id_to_blob(label_id.as_bytes())
                ],
            )
            .sq()?;
        } else {
            tx.execute(
                "DELETE FROM thread_label WHERE thread_id = ?1 AND label_id = ?2",
                params![
                    id_to_blob(thread_id.as_bytes()),
                    id_to_blob(label_id.as_bytes())
                ],
            )
            .sq()?;
        }
    }
    tx.commit().sq()?;
    Ok(())
}

pub(crate) fn labels_for_thread(
    conn: &mut Connection,
    thread_id: ThreadId,
) -> Result<Vec<LabelBadge>, CoreError> {
    query_all(
        conn,
        "SELECT l.id, l.name FROM label l
         JOIN thread_label tl ON tl.label_id = l.id
         WHERE tl.thread_id = ?1
         ORDER BY l.name ASC",
        params![id_to_blob(thread_id.as_bytes())],
        |row| {
            Ok(LabelBadge {
                id: row_id(row, 0, LabelId::from_bytes)?,
                name: row.get(1).sq()?,
            })
        },
    )
}

pub(crate) fn label_exists(conn: &mut Connection, id: LabelId) -> Result<bool, CoreError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM label WHERE id = ?1",
            params![id_to_blob(id.as_bytes())],
            |r| r.get(0),
        )
        .sq()?;
    Ok(count > 0)
}

// ---------------------------------------------------------------- client-side rules

fn map_rule(row: &Row<'_>) -> Result<MailRuleView, CoreError> {
    let conds_raw: String = row.get(4).sq()?;
    let acts_raw: String = row.get(5).sq()?;
    Ok(MailRuleView {
        id: row_id(row, 0, RuleId::from_bytes)?,
        name: row.get(1).sq()?,
        enabled: row.get::<_, i64>(2).sq()? != 0,
        match_all: row.get::<_, i64>(3).sq()? != 0,
        conditions: from_json(&conds_raw)?,
        actions: from_json(&acts_raw)?,
    })
}

pub(crate) fn list_rules(conn: &mut Connection) -> Result<Vec<MailRuleView>, CoreError> {
    query_all(
        conn,
        "SELECT id, name, enabled, match_all, conditions_json, actions_json
         FROM mail_rule ORDER BY position ASC, created_at ASC",
        [],
        map_rule,
    )
}

pub(crate) fn get_rule(
    conn: &mut Connection,
    id: RuleId,
) -> Result<Option<MailRuleView>, CoreError> {
    query_one(
        conn,
        "SELECT id, name, enabled, match_all, conditions_json, actions_json
         FROM mail_rule WHERE id = ?1",
        params![id_to_blob(id.as_bytes())],
        map_rule,
    )
}

pub(crate) fn save_rule(
    conn: &mut Connection,
    input: &SaveMailRuleInput,
    now: UnixMs,
) -> Result<MailRuleView, CoreError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "name".to_string(),
            reason: "rule name is required".to_string(),
        });
    }
    if input.conditions.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "conditions".to_string(),
            reason: "at least one condition is required".to_string(),
        });
    }
    if input.actions.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "actions".to_string(),
            reason: "at least one action is required".to_string(),
        });
    }
    let id = input.id.unwrap_or_else(RuleId::fresh);
    let conds_json = to_json(&input.conditions)?;
    let acts_json = to_json(&input.actions)?;
    let updated = conn
        .execute(
            "UPDATE mail_rule
             SET name = ?2, enabled = ?3, match_all = ?4, conditions_json = ?5, actions_json = ?6, updated_at = ?7
             WHERE id = ?1",
            params![
                id_to_blob(id.as_bytes()),
                name,
                input.enabled as i64,
                input.match_all as i64,
                conds_json,
                acts_json,
                now.0,
            ],
        )
        .sq()?;
    if updated == 0 {
        let pos: i64 = conn
            .query_row("SELECT COALESCE(MAX(position), -1) + 1 FROM mail_rule", [], |r| r.get(0))
            .sq()?;
        conn.execute(
            "INSERT INTO mail_rule
               (id, name, enabled, match_all, position, conditions_json, actions_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                id_to_blob(id.as_bytes()),
                name,
                input.enabled as i64,
                input.match_all as i64,
                pos,
                conds_json,
                acts_json,
                now.0,
            ],
        )
        .sq()?;
    }
    Ok(MailRuleView {
        id,
        name: name.to_string(),
        enabled: input.enabled,
        match_all: input.match_all,
        conditions: input.conditions.clone(),
        actions: input.actions.clone(),
    })
}

pub(crate) fn delete_rule(conn: &mut Connection, id: RuleId) -> Result<bool, CoreError> {
    let changed = conn
        .execute(
            "DELETE FROM mail_rule WHERE id = ?1",
            params![id_to_blob(id.as_bytes())],
        )
        .sq()?;
    Ok(changed > 0)
}

/// Apply all enabled rules to a newly ingested message/thread.
pub(crate) fn apply_enabled_rules_to_thread(
    conn: &mut Connection,
    thread_id: ThreadId,
    target: &RuleEvalTarget<'_>,
) -> Result<(), CoreError> {
    let rules = list_rules(conn)?;
    for rule in rules.into_iter().filter(|r| r.enabled) {
        if matches_rule(&rule.conditions, rule.match_all, target) {
            apply_rule_actions(conn, thread_id, &rule.actions)?;
        }
    }
    Ok(())
}

/// Retroactively run one rule against all non-trashed threads in the store; returns count matched.
pub(crate) fn run_rule_retroactively(
    conn: &mut Connection,
    conditions: &[RuleCondition],
    match_all: bool,
    actions: &[RuleAction],
) -> Result<u32, CoreError> {
    struct Candidate {
        thread_id: ThreadId,
        from_email: String,
        from_name: Option<String>,
        subject: String,
        body_text: String,
        has_attachments: bool,
    }
    let candidates = query_all(
        conn,
        "SELECT t.id, t.sender_email, t.sender_name, t.subject,
                COALESCE((SELECT m.body_text FROM message m WHERE m.thread_id = t.id ORDER BY m.date DESC LIMIT 1), ''),
                t.has_attachments
         FROM thread t
         WHERE t.trashed = 0",
        [],
        |row| {
            let sender_name: String = row.get(2).sq()?;
            Ok(Candidate {
                thread_id: row_id(row, 0, ThreadId::from_bytes)?,
                from_email: row.get(1).sq()?,
                from_name: if sender_name.is_empty() {
                    None
                } else {
                    Some(sender_name)
                },
                subject: row.get(3).sq()?,
                body_text: row.get(4).sq()?,
                has_attachments: row.get::<_, i64>(5).sq()? != 0,
            })
        },
    )?;

    let mut matched = 0u32;
    for cand in candidates {
        let target = RuleEvalTarget {
            from_email: &cand.from_email,
            from_name: cand.from_name.as_deref(),
            subject: &cand.subject,
            body_text: &cand.body_text,
            has_attachments: cand.has_attachments,
        };
        if matches_rule(conditions, match_all, &target) {
            apply_rule_actions(conn, cand.thread_id, actions)?;
            matched += 1;
        }
    }
    Ok(matched)
}

fn apply_rule_actions(
    conn: &mut Connection,
    thread_id: ThreadId,
    actions: &[RuleAction],
) -> Result<(), CoreError> {
    let tid_blob = id_to_blob(thread_id.as_bytes());
    for action in actions {
        match action {
            RuleAction::ApplyLabel { label_id } => {
                if label_exists(conn, *label_id)? {
                    conn.execute(
                        "INSERT OR IGNORE INTO thread_label (thread_id, label_id) VALUES (?1, ?2)",
                        params![tid_blob, id_to_blob(label_id.as_bytes())],
                    )
                    .sq()?;
                }
            }
            RuleAction::Archive => {
                conn.execute(
                    "UPDATE thread SET archived = 1, spam = 0 WHERE id = ?1",
                    params![tid_blob],
                )
                .sq()?;
            }
            RuleAction::Trash => {
                conn.execute(
                    "UPDATE thread SET trashed = 1, spam = 0 WHERE id = ?1",
                    params![tid_blob],
                )
                .sq()?;
            }
            RuleAction::MarkRead => {
                conn.execute(
                    "UPDATE message SET is_read = 1 WHERE thread_id = ?1",
                    params![tid_blob],
                )
                .sq()?;
                conn.execute(
                    "UPDATE thread SET unread_count = 0 WHERE id = ?1",
                    params![tid_blob],
                )
                .sq()?;
            }
            RuleAction::Star => {
                conn.execute(
                    "UPDATE thread SET starred = 1 WHERE id = ?1",
                    params![tid_blob],
                )
                .sq()?;
            }
            RuleAction::SetCategory { category } => {
                let cat = match category {
                    InboxCategory::All => InboxCategory::Primary,
                    other => *other,
                };
                conn.execute(
                    "UPDATE thread SET category = ?2 WHERE id = ?1",
                    params![tid_blob, cat.as_str()],
                )
                .sq()?;
            }
        }
    }
    Ok(())
}
