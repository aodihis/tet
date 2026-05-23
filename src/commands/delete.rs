use anyhow::Result;
use rusqlite::Connection;

use crate::db::ops;
use crate::models::display_name;

pub fn run(conn: &Connection, group: &str, name: &str) -> Result<()> {
    let display = display_name(group, name);
    match ops::get_snippet(conn, group, name)? {
        None => println!("Not found: {}", display),
        Some(snippet) => {
            ops::delete_snippet(conn, snippet.id)?;
            println!("Deleted: {}", display);
        }
    }
    Ok(())
}
