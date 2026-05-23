use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_ungrouped() {
        assert_eq!(display_name("", "ping"), "ping");
    }

    #[test]
    fn display_name_grouped() {
        assert_eq!(display_name("home", "ping"), "home/ping");
    }

    #[test]
    fn display_name_empty_both() {
        assert_eq!(display_name("", ""), "");
    }

    #[test]
    fn display_name_unicode() {
        assert_eq!(display_name("grüp", "名前"), "grüp/名前");
    }

    #[test]
    fn display_name_group_only() {
        assert_eq!(display_name("mygroup", ""), "mygroup/");
    }
}
