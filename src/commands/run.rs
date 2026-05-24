use anyhow::Result;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use rusqlite::Connection;

use crate::db::ops;
use crate::models::Snippet;

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
            if scored.is_empty() {
                anyhow::bail!("No snippet found for '{}'", query);
            }
            println!("'{}' not found. Did you mean:", query);
            for line in suggestions(&all, &scored) {
                println!("  {}", line);
            }
            return Ok(());
        }
    };

    execute(&snippet)
}

fn exact_match(conn: &Connection, group: &str, name: &str) -> Result<Option<Snippet>> {
    ops::get_snippet(conn, group, name)
}

pub fn execute(snippet: &Snippet) -> Result<()> {
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

fn suggestions(all: &[Snippet], scored: &[(i64, usize)]) -> Vec<String> {
    scored
        .iter()
        .take(2)
        .map(|(_, i)| {
            let s = &all[*i];
            if s.group_name.is_empty() {
                format!("tet {}", s.name)
            } else {
                format!("tet {} {}", s.group_name, s.name)
            }
        })
        .collect()
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

    // ── suggestions ───────────────────────────────────────────────────────────

    #[test]
    fn suggestions_ungrouped_uses_name_only() {
        let s = make_snippet("", "ping", &["ping 8.8.8.8"]);
        let scored = vec![(10, 0usize)];
        assert_eq!(suggestions(&[s], &scored), vec!["tet ping"]);
    }

    #[test]
    fn suggestions_grouped_includes_group() {
        let s = make_snippet("home", "dns", &["nslookup google.com"]);
        let scored = vec![(10, 0usize)];
        assert_eq!(suggestions(&[s], &scored), vec!["tet home dns"]);
    }

    #[test]
    fn suggestions_capped_at_two() {
        let snippets = vec![
            make_snippet("", "alpha", &["echo a"]),
            make_snippet("", "beta", &["echo b"]),
            make_snippet("", "gamma", &["echo c"]),
        ];
        let scored = vec![(30, 0usize), (20, 1), (10, 2)];
        let result = suggestions(&snippets, &scored);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "tet alpha");
        assert_eq!(result[1], "tet beta");
    }

    #[test]
    fn suggestions_single_match_returns_one() {
        let s = make_snippet("", "ping", &["ping 8.8.8.8"]);
        let scored = vec![(10, 0usize)];
        let result = suggestions(&[s], &scored);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn suggestions_empty_when_scored_empty() {
        let result = suggestions(&[], &[]);
        assert!(result.is_empty());
    }

    // ── run end-to-end ────────────────────────────────────────────────────────

    #[test]
    fn run_errors_on_empty_db() {
        let conn = setup();
        let err = run(&conn, "", "anything").unwrap_err();
        assert!(err.to_string().contains("No snippets saved yet"));
    }

    #[test]
    fn run_returns_ok_on_fuzzy_match() {
        let conn = setup();
        ops::insert_snippet(&conn, "ping", "", &["echo ping".to_string()]).unwrap();
        // "pin" fuzzy-matches "ping" but is not an exact match
        let result = run(&conn, "", "pin");
        assert!(result.is_ok());
    }

    #[test]
    fn run_errors_when_no_fuzzy_match() {
        let conn = setup();
        ops::insert_snippet(&conn, "ping", "", &["echo ping".to_string()]).unwrap();
        let err = run(&conn, "", "xyzzy").unwrap_err();
        assert!(err.to_string().contains("No snippet found"));
    }

    #[test]
    fn run_returns_ok_on_fuzzy_match_with_group() {
        let conn = setup();
        ops::insert_snippet(&conn, "dns", "home", &["echo dns".to_string()]).unwrap();
        // "hom dn" fuzzy-matches "home dns" composite but won't exact-match
        let result = run(&conn, "home", "dn");
        assert!(result.is_ok());
    }
}
