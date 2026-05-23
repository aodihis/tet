pub mod ops;
pub mod schema;

use anyhow::Result;
use rusqlite::Connection;

pub fn open() -> Result<Connection> {
    let dir = if let Ok(override_dir) = std::env::var("TET_DATA_DIR") {
        std::path::PathBuf::from(override_dir)
    } else {
        let mut p = dirs::data_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot find data directory"))?;
        p.push("tet");
        p
    };
    open_at(&dir)
}

pub fn open_at(dir: &std::path::Path) -> Result<Connection> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join("tet.db");
    let conn = Connection::open(&path)?;
    // Allow up to 5 s of retries when another writer holds the lock
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch(schema::CREATE_SCHEMA)?;
    Ok(conn)
}
