use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Snippet {
    pub id: i64,
    pub shortcut: String,
    pub group_name: Option<String>,
    pub description: Option<String>,
    pub commands: Vec<String>,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub use_count: i64,
}
