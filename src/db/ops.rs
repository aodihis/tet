use anyhow::{Context, Result};
use rusqlite::Connection;
use crate::models::Snippet;

pub fn insert_snippet(
    conn: &Connection,
    name: &str,
    group_name: &str,
    commands: &[String],
) -> Result<()> {
    conn.execute(
        "INSERT INTO snippets (name, group_name, commands) VALUES (?1, ?2, ?3)",
        (name, group_name, serde_json::to_string(commands)?),
    )
    .map(|_| ())
    .map_err(|e| {
        if let rusqlite::Error::SqliteFailure(ref err, _) = e {
            if err.code == rusqlite::ErrorCode::ConstraintViolation {
                return anyhow::anyhow!(
                    "Snippet '{}' already exists{}",
                    name,
                    if group_name.is_empty() { String::new() } else { format!(" in group '{}'", group_name) }
                );
            }
        }
        anyhow::anyhow!("Failed to insert snippet: {}", e)
    })
}

pub fn get_snippet(conn: &Connection, group_name: &str, name: &str) -> Result<Option<Snippet>> {
    match conn.query_row(
        "SELECT id, name, group_name, commands, created_at \
         FROM snippets WHERE group_name = ?1 AND name = ?2",
        [group_name, name],
        map_row,
    ) {
        Ok(snippet) => Ok(Some(snippet)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e).context("Failed to get snippet"),
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Snippet> {
    let commands_raw: String = row.get(3)?;
    let commands = serde_json::from_str::<Vec<String>>(&commands_raw)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
            3, rusqlite::types::Type::Text, Box::new(e),
        ))?;
    Ok(Snippet {
        id: row.get::<_, u16>(0)?,
        name: row.get(1)?,
        group_name: row.get(2)?,
        commands,
        created_at: row.get(4)?,
    })
}

pub fn list_all(conn: &Connection) -> Result<Vec<Snippet>> {
    // empty group_name ("") sorts before named groups in ASC, giving ungrouped-first ordering
    let mut stmt = conn.prepare(
        "SELECT id, name, group_name, commands, created_at \
         FROM snippets ORDER BY group_name, name",
    )?;
    stmt.query_map([], map_row)?.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_by_group(conn: &Connection, group: &str) -> Result<Vec<Snippet>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, group_name, commands, created_at \
         FROM snippets WHERE group_name = ?1 ORDER BY name",
    )?;
    stmt.query_map([group], map_row)?.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn delete_snippet(conn: &Connection, id: u16) -> Result<bool> {
    conn.execute("DELETE FROM snippets WHERE id = ?1", [id])
        .map(|n| n > 0)
        .context("Failed to delete snippet")
}

pub fn update_snippet(conn: &Connection, snippet: &Snippet) -> Result<()> {
    conn.execute(
        "UPDATE snippets SET name = ?1, group_name = ?2, commands = ?3 WHERE id = ?4",
        (&snippet.name, &snippet.group_name, serde_json::to_string(&snippet.commands)?, snippet.id),
    )
    .map(|_| ())
    .context("Failed to update snippet")
}
