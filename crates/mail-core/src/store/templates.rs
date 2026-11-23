//! Snippet/template persistence.

use rusqlite::{params, Connection, Row};

use crate::error::CoreError;
use crate::ids::TemplateId;
use crate::store::{id_to_blob, query_all, query_one, row_id, SqlResultExt};
use crate::types::{SaveTemplateInput, TemplateView};

fn map_template(row: &Row<'_>) -> Result<TemplateView, CoreError> {
    Ok(TemplateView {
        id: row_id(row, 0, TemplateId::from_bytes)?,
        name: row.get(1).sq()?,
        shortcut: row.get(2).sq()?,
        subject: row.get(3).sq()?,
        body_text: row.get(4).sq()?,
    })
}

pub(crate) fn list_templates(conn: &mut Connection) -> Result<Vec<TemplateView>, CoreError> {
    query_all(
        conn,
        "SELECT id, name, shortcut, subject, body_text FROM template ORDER BY name ASC, rowid ASC",
        [],
        map_template,
    )
}

pub(crate) fn save_template(
    conn: &mut Connection,
    input: &SaveTemplateInput,
) -> Result<TemplateView, CoreError> {
    let name = input.name.trim();
    if name.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "name".to_string(),
            reason: "template name is required".to_string(),
        });
    }
    let id = match input.id {
        Some(existing) => {
            let changed = conn
                .execute(
                    "UPDATE template SET name = ?2, shortcut = ?3, subject = ?4, body_text = ?5
                     WHERE id = ?1",
                    params![
                        id_to_blob(existing.as_bytes()),
                        name,
                        input.shortcut,
                        input.subject,
                        input.body_text
                    ],
                )
                .sq()?;
            if changed == 0 {
                return Err(CoreError::NotFound {
                    entity: "template".to_string(),
                });
            }
            existing
        }
        None => {
            let id = TemplateId::fresh();
            conn.execute(
                "INSERT INTO template (id, name, shortcut, subject, body_text) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    id_to_blob(id.as_bytes()),
                    name,
                    input.shortcut,
                    input.subject,
                    input.body_text
                ],
            )
            .sq()?;
            id
        }
    };

    query_one(
        conn,
        "SELECT id, name, shortcut, subject, body_text FROM template WHERE id = ?1",
        params![id_to_blob(id.as_bytes())],
        map_template,
    )?
    .ok_or(CoreError::NotFound {
        entity: "template".to_string(),
    })
}

pub(crate) fn delete_template(conn: &mut Connection, id: TemplateId) -> Result<bool, CoreError> {
    let changed = conn
        .execute(
            "DELETE FROM template WHERE id = ?1",
            params![id_to_blob(id.as_bytes())],
        )
        .sq()?;
    Ok(changed > 0)
}
