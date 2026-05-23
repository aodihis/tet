use anyhow::Result;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use rusqlite::Connection;

use crate::db::ops;
use crate::models::Snippet;
use crate::tui;

pub fn run(conn: &Connection, group: &str, name: &str) -> Result<()> {
    let snippet = match exact_match(conn, group, name)? {
        ExactResult::One(s) => s,
        ExactResult::Many(_candidates) => {
            let query = build_query(group, name);
            let all = ops::list_all(conn)?;
            match tui::list_view::run(conn, all, Some(query))? {
                Some(s) => s,
                None => return Ok(()),
            }
        }
        ExactResult::None => {
            let all = ops::list_all(conn)?;
            if all.is_empty() {
                anyhow::bail!("No snippets saved yet");
            }
            let query = build_query(group, name);
            let mut scored: Vec<(i64, usize)> = all
                .iter()
                .enumerate()
                .filter_map(|(i, s)| {
                    SkimMatcherV2::default()
                        .fuzzy_match(&composite(s), &query)
                        .map(|score| (score, i))
                })
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            match scored.as_slice() {
                [] => anyhow::bail!("No snippet found for '{}'", query),
                [(_, i)] => all.into_iter().nth(*i).unwrap(),
                _ => match tui::list_view::run(conn, all, Some(query))? {
                    Some(s) => s,
                    None => return Ok(()),
                },
            }
        }
    };

    execute(&snippet)
}

enum ExactResult {
    One(Snippet),
    Many(Vec<Snippet>),
    None,
}

fn exact_match(conn: &Connection, group: &str, name: &str) -> Result<ExactResult> {
    if !group.is_empty() {
        return Ok(match ops::get_snippet(conn, group, name)? {
            Some(s) => ExactResult::One(s),
            None => ExactResult::None,
        });
    }
    // No group specified — prefer ungrouped, then any group
    if let Some(s) = ops::get_snippet(conn, "", name)? {
        return Ok(ExactResult::One(s));
    }
    let candidates = ops::get_by_name(conn, name)?;
    Ok(match candidates.len() {
        0 => ExactResult::None,
        1 => ExactResult::One(candidates.into_iter().next().unwrap()),
        _ => ExactResult::Many(candidates),
    })
}

fn execute(snippet: &Snippet) -> Result<()> {
    for cmd in &snippet.commands {
        #[cfg(target_os = "windows")]
        std::process::Command::new("cmd").args(["/C", cmd]).status()?;
        #[cfg(not(target_os = "windows"))]
        std::process::Command::new("sh").args(["-c", cmd]).status()?;
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
