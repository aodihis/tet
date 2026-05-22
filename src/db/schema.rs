pub const CREATE_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS snippets (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    shortcut     TEXT UNIQUE NOT NULL,
    group_name   TEXT,
    description  TEXT,
    commands     TEXT NOT NULL,
    created_at   DATETIME DEFAULT CURRENT_TIMESTAMP,
    last_used_at DATETIME,
    use_count    INTEGER DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_group    ON snippets(group_name);
CREATE INDEX IF NOT EXISTS idx_shortcut ON snippets(shortcut);
";
