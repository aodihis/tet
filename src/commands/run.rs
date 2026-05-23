use anyhow::Result;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use rusqlite::Connection;

use crate::db::ops;
use crate::models::Snippet;
use crate::tui;

pub fn run(conn: &Connection, group: &str, name: &str) -> Result<()> {
    let snippet = match exact_match(conn, group, name)? {
        Some(s) => s,
        None => {
            let all = ops::list_all(conn)?;
            if all.is_empty() {
                anyhow::bail!("No snippets saved yet");
            }
            let query = build_query(group, name);
            let matcher = SkimMatcherV2::default();
            let mut scored: Vec<(i64, usize)> = all
                .iter()
                .enumerate()
                .filter_map(|(i, s)| {
                    matcher
                        .fuzzy_match(&composite(s), &query)
                        .map(|score| (score, i))
                })
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            match scored.as_slice() {
                [] => anyhow::bail!("No snippet found for '{}'", query),
                [(_, i)] => all
                    .into_iter()
                    .nth(*i)
                    .ok_or_else(|| anyhow::anyhow!("internal: fuzzy index out of bounds"))?,
                _ => match tui::list_view::run(conn, all, Some(query))? {
                    Some(s) => s,
                    None => return Ok(()),
                },
            }
        }
    };

    execute(&snippet)
}

fn exact_match(conn: &Connection, group: &str, name: &str) -> Result<Option<Snippet>> {
    ops::get_snippet(conn, group, name)
}

fn execute(snippet: &Snippet) -> Result<()> {
    for cmd in &snippet.commands {
        #[cfg(target_os = "windows")]
        let status = std::process::Command::new("cmd").args(["/C", cmd]).status()?;
        #[cfg(not(target_os = "windows"))]
        let status = std::process::Command::new("sh").args(["-c", cmd]).status()?;
        if !status.success() {
            anyhow::bail!("command exited with {}", status);
        }
    }
    Ok(())
}

fn build_query(group: &str, name: &str) -> String {
    if group.is_empty() {
        name.to_string()
    } else {
        format!("{} {}", group, name)
    }
}

fn composite(s: &Snippet) -> String {
    if s.group_name.is_empty() {
        format!("{} {}", s.name, s.commands.join(" "))
    } else {
        format!("{} {} {}", s.group_name, s.name, s.commands.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::schema::CREATE_SCHEMA).unwrap();
        conn
    }

    fn make_snippet(group: &str, name: &str, commands: &[&str]) -> Snippet {
        Snippet {
            id: 0,
            name: name.to_string(),
            group_name: group.to_string(),
            commands: commands.iter().map(|s| s.to_string()).collect(),
            created_at: String::new(),
        }
    }

    // ── build_query ───────────────────────────────────────────────────────────

    #[test]
    fn build_query_no_group() {
        assert_eq!(build_query("", "ping"), "ping");
    }

    #[test]
    fn build_query_with_group() {
        assert_eq!(build_query("home", "ping"), "home ping");
    }

    #[test]
    fn build_query_both_empty() {
        assert_eq!(build_query("", ""), "");
    }

    #[test]
    fn build_query_group_only() {
        assert_eq!(build_query("mygroup", ""), "mygroup ");
    }

    // ── composite ─────────────────────────────────────────────────────────────

    #[test]
    fn composite_ungrouped_single_command() {
        let s = make_snippet("", "ping", &["ping 8.8.8.8"]);
        assert_eq!(composite(&s), "ping ping 8.8.8.8");
    }

    #[test]
    fn composite_ungrouped_multiple_commands() {
        let s = make_snippet("", "deploy", &["git pull", "cargo build"]);
        assert_eq!(composite(&s), "deploy git pull cargo build");
    }

    #[test]
    fn composite_grouped_single_command() {
        let s = make_snippet("home", "dns", &["nslookup google.com"]);
        assert_eq!(composite(&s), "home dns nslookup google.com");
    }

    #[test]
    fn composite_grouped_multiple_commands() {
        let s = make_snippet("work", "setup", &["npm install", "npm run build"]);
        assert_eq!(composite(&s), "work setup npm install npm run build");
    }

    #[test]
    fn composite_ungrouped_no_commands() {
        let s = make_snippet("", "empty", &[]);
        assert_eq!(composite(&s), "empty ");
    }

    #[test]
    fn composite_grouped_no_commands() {
        let s = make_snippet("grp", "empty", &[]);
        assert_eq!(composite(&s), "grp empty ");
    }

    // ── exact_match ───────────────────────────────────────────────────────────

    #[test]
    fn exact_match_finds_ungrouped_snippet() {
        let conn = setup();
        ops::insert_snippet(&conn, "ping", "", &["ping 8.8.8.8".to_string()]).unwrap();
        let result = exact_match(&conn, "", "ping").unwrap();
        assert!(result.is_some());
        let s = result.unwrap();
        assert_eq!(s.name, "ping");
        assert_eq!(s.group_name, "");
    }

    #[test]
    fn exact_match_finds_grouped_snippet() {
        let conn = setup();
        ops::insert_snippet(&conn, "dns", "home", &["nslookup google.com".to_string()]).unwrap();
        let result = exact_match(&conn, "home", "dns").unwrap();
        assert!(result.is_some());
        let s = result.unwrap();
        assert_eq!(s.name, "dns");
        assert_eq!(s.group_name, "home");
    }

    #[test]
    fn exact_match_returns_none_when_missing() {
        let conn = setup();
        assert!(exact_match(&conn, "", "ghost").unwrap().is_none());
    }

    #[test]
    fn exact_match_wrong_group_returns_none() {
        let conn = setup();
        ops::insert_snippet(&conn, "ping", "home", &["ping 8.8.8.8".to_string()]).unwrap();
        assert!(exact_match(&conn, "work", "ping").unwrap().is_none());
    }

    #[test]
    fn exact_match_ungrouped_not_found_by_group() {
        let conn = setup();
        ops::insert_snippet(&conn, "ping", "", &["ping 8.8.8.8".to_string()]).unwrap();
        assert!(exact_match(&conn, "home", "ping").unwrap().is_none());
    }

    #[test]
    fn exact_match_preserves_commands() {
        let conn = setup();
        let cmds = vec!["echo first".to_string(), "echo second".to_string()];
        ops::insert_snippet(&conn, "multi", "grp", &cmds).unwrap();
        let s = exact_match(&conn, "grp", "multi").unwrap().unwrap();
        assert_eq!(s.commands, cmds);
    }
}
