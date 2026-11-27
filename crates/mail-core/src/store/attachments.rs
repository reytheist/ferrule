//! Attachment metadata & byte cache for received messages and outgoing drafts.

use rusqlite::{params, Connection, Row};

use crate::error::CoreError;
use crate::ids::{AccountId, AttachmentId, DraftId, MessageId};
use crate::store::{id_to_blob, query_all, query_one, row_id, SqlResultExt};
use crate::types::{AttachmentMeta, DraftAttachmentView};

#[derive(Debug, Clone)]
pub(crate) struct NewAttachment {
    pub id: AttachmentId,
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub filename: String,
    pub mime: String,
    pub size: u64,
    pub content_id: Option<String>,
    pub is_inline: bool,
    pub part_id: Option<String>,
    pub data: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub(crate) struct AttachmentRow {
    pub id: AttachmentId,
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub filename: String,
    pub mime: String,
    pub size: u64,
    pub content_id: Option<String>,
    pub is_inline: bool,
    pub part_id: Option<String>,
    pub data: Option<Vec<u8>>,
}

impl AttachmentRow {
    pub(crate) fn to_meta(&self) -> AttachmentMeta {
        AttachmentMeta {
            id: self.id,
            filename: self.filename.clone(),
            mime: self.mime.clone(),
            size: self.size,
            cached: self.data.is_some(),
            is_inline: self.is_inline,
            content_id: self.content_id.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DraftAttachmentRow {
    pub id: AttachmentId,
    pub filename: String,
    pub mime: String,
    pub size: u64,
    pub data: Vec<u8>,
}

impl DraftAttachmentRow {
    pub(crate) fn to_view(&self) -> DraftAttachmentView {
        DraftAttachmentView {
            id: self.id,
            filename: self.filename.clone(),
            mime: self.mime.clone(),
            size: self.size,
        }
    }
}

fn map_attachment(row: &Row<'_>) -> Result<AttachmentRow, CoreError> {
    Ok(AttachmentRow {
        id: row_id(row, 0, AttachmentId::from_bytes)?,
        message_id: row_id(row, 1, MessageId::from_bytes)?,
        account_id: row_id(row, 2, AccountId::from_bytes)?,
        filename: row.get(3).sq()?,
        mime: row.get(4).sq()?,
        size: row.get::<_, i64>(5).sq()?.max(0) as u64,
        content_id: row.get(6).sq()?,
        is_inline: row.get::<_, i64>(7).sq()? != 0,
        part_id: row.get(8).sq()?,
        data: row.get(9).sq()?,
    })
}

pub(crate) fn insert_attachments(
    conn: &mut Connection,
    attachments: &[NewAttachment],
) -> Result<(), CoreError> {
    if attachments.is_empty() {
        return Ok(());
    }
    for att in attachments {
        conn.execute(
            "INSERT INTO attachment
               (id, message_id, account_id, filename, mime, size, content_id, is_inline, part_id, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id_to_blob(att.id.as_bytes()),
                id_to_blob(att.message_id.as_bytes()),
                id_to_blob(att.account_id.as_bytes()),
                att.filename,
                att.mime,
                att.size as i64,
                att.content_id,
                att.is_inline as i64,
                att.part_id,
                att.data,
            ],
        )
        .sq()?;
    }
    Ok(())
}

pub(crate) fn list_message_attachments(
    conn: &mut Connection,
    message_id: MessageId,
) -> Result<Vec<AttachmentRow>, CoreError> {
    query_all(
        conn,
        "SELECT id, message_id, account_id, filename, mime, size, content_id, is_inline, part_id, data
         FROM attachment WHERE message_id = ?1 ORDER BY is_inline ASC, rowid ASC",
        params![id_to_blob(message_id.as_bytes())],
        map_attachment,
    )
}

pub(crate) fn get_attachment(
    conn: &mut Connection,
    id: AttachmentId,
) -> Result<Option<AttachmentRow>, CoreError> {
    query_one(
        conn,
        "SELECT id, message_id, account_id, filename, mime, size, content_id, is_inline, part_id, data
         FROM attachment WHERE id = ?1",
        params![id_to_blob(id.as_bytes())],
        map_attachment,
    )
}

pub(crate) fn cache_attachment_data(
    conn: &mut Connection,
    id: AttachmentId,
    data: &[u8],
) -> Result<(), CoreError> {
    conn.execute(
        "UPDATE attachment SET data = ?2, size = ?3 WHERE id = ?1",
        params![id_to_blob(id.as_bytes()), data, data.len() as i64],
    )
    .sq()?;
    Ok(())
}

pub(crate) fn insert_draft_attachments(
    conn: &mut Connection,
    draft_id: DraftId,
    items: &[(String, String, Vec<u8>)],
) -> Result<Vec<DraftAttachmentView>, CoreError> {
    let mut views = Vec::with_capacity(items.len());
    for (filename, mime, data) in items {
        let id = AttachmentId::fresh();
        let size = data.len() as u64;
        conn.execute(
            "INSERT INTO draft_attachment (id, draft_id, filename, mime, size, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                id_to_blob(id.as_bytes()),
                id_to_blob(draft_id.as_bytes()),
                filename,
                mime,
                size as i64,
                data,
            ],
        )
        .sq()?;
        views.push(DraftAttachmentView {
            id,
            filename: filename.clone(),
            mime: mime.clone(),
            size,
        });
    }
    Ok(views)
}

pub(crate) fn list_draft_attachments(
    conn: &mut Connection,
    draft_id: DraftId,
) -> Result<Vec<DraftAttachmentRow>, CoreError> {
    query_all(
        conn,
        "SELECT id, filename, mime, size, data FROM draft_attachment WHERE draft_id = ?1 ORDER BY rowid ASC",
        params![id_to_blob(draft_id.as_bytes())],
        |row| {
            Ok(DraftAttachmentRow {
                id: row_id(row, 0, AttachmentId::from_bytes)?,
                filename: row.get(1).sq()?,
                mime: row.get(2).sq()?,
                size: row.get::<_, i64>(3).sq()?.max(0) as u64,
                data: row.get(4).sq()?,
            })
        },
    )
}

/// Look up the `(folder_name, imap_uid)` of the message owning an attachment so uncached
/// attachments can be downloaded on demand from IMAP.
pub(crate) fn message_imap_location(
    conn: &mut Connection,
    message_id: MessageId,
) -> Result<Option<(String, u32)>, CoreError> {
    query_one(
        conn,
        "SELECT folder_name, imap_uid FROM message WHERE id = ?1 AND imap_uid IS NOT NULL AND imap_uid > 0",
        params![id_to_blob(message_id.as_bytes())],
        |row| {
            let folder: String = row.get(0).sq()?;
            let uid: i64 = row.get(1).sq()?;
            Ok((folder, uid as u32))
        },
    )
}
