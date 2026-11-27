//! Per-account named email signatures (rich HTML + plain text).

use rusqlite::{params, Connection, Row};

use crate::error::CoreError;
use crate::ids::{AccountId, SignatureId, UnixMs};
use crate::store::{account_exists, id_to_blob, query_all, row_id, SqlResultExt};
use crate::types::{SaveSignatureInput, SignatureView};

fn map_signature(row: &Row<'_>) -> Result<SignatureView, CoreError> {
    Ok(SignatureView {
        id: row_id(row, 0, SignatureId::from_bytes)?,
        account_id: row_id(row, 1, AccountId::from_bytes)?,
        name: row.get(2).sq()?,
        body_html: row.get(3).sq()?,
        body_text: row.get(4).sq()?,
        is_default: row.get::<_, i64>(5).sq()? != 0,
    })
}

pub(crate) fn list_signatures(
    conn: &mut Connection,
    account_id: Option<AccountId>,
) -> Result<Vec<SignatureView>, CoreError> {
    match account_id {
        Some(acct) => query_all(
            conn,
            "SELECT id, account_id, name, body_html, body_text, is_default
             FROM signature WHERE account_id = ?1
             ORDER BY is_default DESC, name ASC",
            params![id_to_blob(acct.as_bytes())],
            map_signature,
        ),
        None => query_all(
            conn,
            "SELECT id, account_id, name, body_html, body_text, is_default
             FROM signature
             ORDER BY is_default DESC, name ASC",
            [],
            map_signature,
        ),
    }
}

pub(crate) fn save_signature(
    conn: &mut Connection,
    input: &SaveSignatureInput,
    now: UnixMs,
) -> Result<SignatureView, CoreError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "name".to_string(),
            reason: "signature name is required".to_string(),
        });
    }
    if !account_exists(conn, input.account_id)? {
        return Err(CoreError::NotFound {
            entity: "account".to_string(),
        });
    }

    let id = input.id.unwrap_or_else(SignatureId::fresh);
    let tx = conn.transaction().sq()?;
    if input.is_default {
        tx.execute(
            "UPDATE signature SET is_default = 0 WHERE account_id = ?1",
            params![id_to_blob(input.account_id.as_bytes())],
        )
        .sq()?;
    }

    let updated = tx
        .execute(
            "UPDATE signature
             SET name = ?2, body_html = ?3, body_text = ?4, is_default = ?5, updated_at = ?6
             WHERE id = ?1 AND account_id = ?7",
            params![
                id_to_blob(id.as_bytes()),
                name,
                input.body_html,
                input.body_text,
                input.is_default as i64,
                now.0,
                id_to_blob(input.account_id.as_bytes()),
            ],
        )
        .sq()?;

    if updated == 0 {
        tx.execute(
            "INSERT INTO signature
               (id, account_id, name, body_html, body_text, is_default, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            params![
                id_to_blob(id.as_bytes()),
                id_to_blob(input.account_id.as_bytes()),
                name,
                input.body_html,
                input.body_text,
                input.is_default as i64,
                now.0,
            ],
        )
        .sq()?;
    }
    tx.commit().sq()?;

    Ok(SignatureView {
        id,
        account_id: input.account_id,
        name: name.to_string(),
        body_html: input.body_html.clone(),
        body_text: input.body_text.clone(),
        is_default: input.is_default,
    })
}

pub(crate) fn delete_signature(
    conn: &mut Connection,
    id: SignatureId,
) -> Result<bool, CoreError> {
    let changed = conn
        .execute(
            "DELETE FROM signature WHERE id = ?1",
            params![id_to_blob(id.as_bytes())],
        )
        .sq()?;
    Ok(changed > 0)
}
