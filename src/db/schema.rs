pub const CREATE_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS snippets (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    name         TEXT NOT NULL,
    group_name   TEXT,
    commands     TEXT NOT NULL,
    created_at   DATETIME DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_group ON snippets(group_name);
CREATE UNIQUE INDEX IF NOT EXISTS idx_group_name ON snippets(group_name, name);
";
