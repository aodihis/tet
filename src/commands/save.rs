use anyhow::{bail, Result};
use rusqlite::Connection;

use crate::cli::RESERVED;
use crate::db::ops;
use crate::models::display_name;

pub fn run_noninteractive(
    conn: &Connection,
    group: String,
    name: String,
    cmd: Vec<String>,
) -> Result<()> {
    if name.is_empty() {
        bail!("Name cannot be empty");
    }
    for word in [name.as_str(), group.as_str()] {
        if RESERVED.contains(&word) {
            bail!("'{}' is a reserved word and cannot be used", word);
        }
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        bail!("Name must be a single word (letters, numbers, - and _ only)");
    }
    if !group.is_empty() && !group.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        bail!("Group must be a single word (letters, numbers, - and _ only)");
    }
    if cmd.is_empty() {
        bail!("Command cannot be empty");
    }

    ops::insert_snippet(conn, &name, &group, &cmd)?;
    println!("Saved: {}", display_name(&group, &name));
    Ok(())
}
