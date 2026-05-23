use anyhow::Result;
use rusqlite::Connection;

use crate::db::ops;
use crate::tui;

pub fn run(conn: &Connection, group: Option<String>) -> Result<()> {
    let snippets = match group.as_deref() {
        None    => ops::list_all(conn)?,
        Some(g) => ops::list_by_group(conn, g)?,
    };

    if snippets.is_empty() {
        println!("No snippets found.");
        return Ok(());
    }

    tui::list_view::run(conn, snippets)?;
    Ok(())
}
