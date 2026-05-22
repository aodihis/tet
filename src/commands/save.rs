// Phase 2: non-interactive save
// Phase 4: add run_interactive(conn, prefill)
use anyhow::Result;
use rusqlite::Connection;

pub fn run_noninteractive(
    _conn: &Connection,
    _group: Option<String>,
    _shortcut: String,
    _cmd: Vec<String>,
) -> Result<()> {
    todo!("Phase 2")
}
