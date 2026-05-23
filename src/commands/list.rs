use anyhow::Result;
use rusqlite::Connection;

use crate::db::ops;

pub fn run(conn: &Connection, group: Option<String>) -> Result<()> {
    let snippets = match group.as_deref() {
        None    => ops::list_all(conn)?,
        Some(g) => ops::list_by_group(conn, g)?,
    };

    if snippets.is_empty() {
        println!("No snippets found.");
        return Ok(());
    }

    let (name_w, group_w) = snippets.iter().fold((4usize, 5usize), |(nw, gw), s| {
        (nw.max(s.name.len()), gw.max(s.group_name.len()))
    });

    println!("{:<name_w$}  {:<group_w$}  COMMANDS", "NAME", "GROUP");
    println!("{}", "-".repeat(name_w + group_w + 20));

    let mut current_group: Option<&str> = None;
    for s in &snippets {
        let grp = s.group_name.as_str();
        if group.is_none() && Some(grp) != current_group {
            let label = if grp.is_empty() { "ungrouped" } else { grp };
            println!("\n  — {} —", label);
            current_group = Some(grp);
        }
        println!("{:<name_w$}  {:<group_w$}  {}", s.name, s.group_name, s.commands.join(" | "));
    }

    Ok(())
}
