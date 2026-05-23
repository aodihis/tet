use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Snippet {
    pub id: u16,
    pub name: String,
    pub group_name: String,
    pub commands: Vec<String>,
    pub created_at: String,
}

pub fn display_name(group: &str, name: &str) -> String {
    if group.is_empty() {
        name.to_string()
    } else {
        format!("{}/{}", group, name)
    }
}
