pub mod ops;
pub mod schema;

use anyhow::Result;
use rusqlite::Connection;

pub fn open() -> Result<Connection> {
    let mut path = dirs::data_dir()
        .ok_or_else(|| anyhow::anyhow!("Cannot find data directory"))?;
    path.push("tet");
    std::fs::create_dir_all(&path)?;
    path.push("tet.db");
    let conn = Connection::open(&path)?;
    conn.execute_batch(schema::CREATE_SCHEMA)?;
    Ok(conn)
}
