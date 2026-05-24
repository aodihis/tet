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

    if let Some(snippet) = tui::list_view::run(conn, snippets, None)? {
        crate::commands::run::execute(&snippet)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::schema::CREATE_SCHEMA).unwrap();
        conn
    }

    #[test]
    fn run_returns_ok_when_db_empty() {
        let conn = setup();
        assert!(run(&conn, None).is_ok());
    }

    #[test]
    fn run_returns_ok_when_group_filter_matches_nothing() {
        let conn = setup();
        crate::db::ops::insert_snippet(&conn, "ping", "", &["echo ping".to_string()]).unwrap();
        // Filter for a group that has no snippets
        assert!(run(&conn, Some("nonexistent".to_string())).is_ok());
    }

    #[test]
    fn run_returns_ok_when_ungrouped_filter_matches_nothing() {
        let conn = setup();
        crate::db::ops::insert_snippet(&conn, "dns", "home", &["echo dns".to_string()]).unwrap();
        // Filter for ungrouped when only grouped snippets exist
        assert!(run(&conn, Some("".to_string())).is_ok());
    }
}
