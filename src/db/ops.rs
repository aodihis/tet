use anyhow::{Context, Result};
use rusqlite::Connection;
use crate::models::Snippet;

pub fn insert_snippet(
    conn: &Connection,
    name: &str,
    group_name: &str,
    commands: &[String],
) -> Result<()> {
    conn.execute(
        "INSERT INTO snippets (name, group_name, commands) VALUES (?1, ?2, ?3)",
        (name, group_name, serde_json::to_string(commands)?),
    )
    .map(|_| ())
    .map_err(|e| {
        if let rusqlite::Error::SqliteFailure(ref err, _) = e
            && err.code == rusqlite::ErrorCode::ConstraintViolation
        {
            return anyhow::anyhow!(
                "Snippet '{}' already exists{}",
                name,
                if group_name.is_empty() { String::new() } else { format!(" in group '{}'", group_name) }
            );
        }
        anyhow::anyhow!("Failed to insert snippet: {}", e)
    })
}

pub fn get_snippet(conn: &Connection, group_name: &str, name: &str) -> Result<Option<Snippet>> {
    match conn.query_row(
        "SELECT id, name, group_name, commands, created_at \
         FROM snippets WHERE group_name = ?1 AND name = ?2",
        [group_name, name],
        map_row,
    ) {
        Ok(snippet) => Ok(Some(snippet)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e).context("Failed to get snippet"),
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Snippet> {
    let commands_raw: String = row.get(3)?;
    let commands = serde_json::from_str::<Vec<String>>(&commands_raw)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
            3, rusqlite::types::Type::Text, Box::new(e),
        ))?;
    Ok(Snippet {
        id: row.get::<_, u16>(0)?,
        name: row.get(1)?,
        group_name: row.get(2)?,
        commands,
        created_at: row.get(4)?,
    })
}

pub fn list_all(conn: &Connection) -> Result<Vec<Snippet>> {
    // empty group_name ("") sorts before named groups in ASC, giving ungrouped-first ordering
    let mut stmt = conn.prepare(
        "SELECT id, name, group_name, commands, created_at \
         FROM snippets ORDER BY group_name, name",
    )?;
    stmt.query_map([], map_row)?.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn list_by_group(conn: &Connection, group: &str) -> Result<Vec<Snippet>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, group_name, commands, created_at \
         FROM snippets WHERE group_name = ?1 ORDER BY name",
    )?;
    stmt.query_map([group], map_row)?.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn delete_snippet(conn: &Connection, id: u16) -> Result<bool> {
    conn.execute("DELETE FROM snippets WHERE id = ?1", [id])
        .map(|n| n > 0)
        .context("Failed to delete snippet")
}

pub fn delete_by_group(conn: &Connection, group_name: &str) -> Result<usize> {
    conn.execute("DELETE FROM snippets WHERE group_name = ?1", [group_name])
        .context("Failed to delete group snippets")
}

pub fn delete_all(conn: &Connection) -> Result<usize> {
    conn.execute("DELETE FROM snippets", [])
        .context("Failed to delete all snippets")
}

pub fn get_by_name(conn: &Connection, name: &str) -> Result<Vec<Snippet>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, group_name, commands, created_at \
         FROM snippets WHERE name = ?1 ORDER BY group_name",
    )?;
    stmt.query_map([name], map_row)?.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn update_snippet(conn: &Connection, snippet: &Snippet) -> Result<()> {
    conn.execute(
        "UPDATE snippets SET name = ?1, group_name = ?2, commands = ?3 WHERE id = ?4",
        (&snippet.name, &snippet.group_name, serde_json::to_string(&snippet.commands)?, snippet.id),
    )
    .map(|_| ())
    .context("Failed to update snippet")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::schema::CREATE_SCHEMA).unwrap();
        conn
    }

    fn cmds(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }

    // ── insert / get ──────────────────────────────────────────────────────────

    #[test]
    fn insert_and_get_basic() {
        let conn = setup();
        insert_snippet(&conn, "ping", "", &cmds(&["ping 8.8.8.8"])).unwrap();
        let s = get_snippet(&conn, "", "ping").unwrap().expect("should exist");
        assert_eq!(s.name, "ping");
        assert_eq!(s.group_name, "");
        assert_eq!(s.commands, vec!["ping 8.8.8.8"]);
    }

    #[test]
    fn insert_with_group() {
        let conn = setup();
        insert_snippet(&conn, "dns", "home", &cmds(&["nslookup google.com"])).unwrap();
        let s = get_snippet(&conn, "home", "dns").unwrap().expect("should exist");
        assert_eq!(s.group_name, "home");
    }

    #[test]
    fn insert_multiple_commands_roundtrip() {
        let conn = setup();
        let commands = cmds(&["echo first", "echo second", "echo third"]);
        insert_snippet(&conn, "multi", "", &commands).unwrap();
        let s = get_snippet(&conn, "", "multi").unwrap().expect("should exist");
        assert_eq!(s.commands, commands);
    }

    #[test]
    fn insert_100_commands() {
        let conn = setup();
        let commands: Vec<String> = (0..100).map(|i| format!("echo cmd{}", i)).collect();
        insert_snippet(&conn, "big", "", &commands).unwrap();
        let s = get_snippet(&conn, "", "big").unwrap().expect("should exist");
        assert_eq!(s.commands.len(), 100);
        assert_eq!(s.commands[99], "echo cmd99");
    }

    #[test]
    fn insert_long_command() {
        let conn = setup();
        let long_cmd = "x".repeat(50_000);
        insert_snippet(&conn, "longcmd", "", &cmds(&[&long_cmd])).unwrap();
        let s = get_snippet(&conn, "", "longcmd").unwrap().expect("should exist");
        assert_eq!(s.commands[0].len(), 50_000);
    }

    #[test]
    fn insert_long_name() {
        let conn = setup();
        let long_name = "a".repeat(200);
        insert_snippet(&conn, &long_name, "", &cmds(&["echo hi"])).unwrap();
        let s = get_snippet(&conn, "", &long_name).unwrap().expect("should exist");
        assert_eq!(s.name.len(), 200);
    }

    #[test]
    fn insert_long_group_name() {
        let conn = setup();
        let long_group = "g".repeat(200);
        insert_snippet(&conn, "mysnip", &long_group, &cmds(&["echo hi"])).unwrap();
        let s = get_snippet(&conn, &long_group, "mysnip").unwrap().expect("should exist");
        assert_eq!(s.group_name.len(), 200);
    }

    #[test]
    fn insert_unicode_in_command() {
        let conn = setup();
        insert_snippet(&conn, "uni", "", &cmds(&["echo '日本語 émojis 🚀'"])).unwrap();
        let s = get_snippet(&conn, "", "uni").unwrap().expect("should exist");
        assert_eq!(s.commands[0], "echo '日本語 émojis 🚀'");
    }

    #[test]
    fn insert_special_shell_chars() {
        let conn = setup();
        let cmd = r#"echo "hello" | grep 'world' ; rm -rf /tmp/$VAR && true || false"#;
        insert_snippet(&conn, "shell", "", &cmds(&[cmd])).unwrap();
        let s = get_snippet(&conn, "", "shell").unwrap().expect("should exist");
        assert_eq!(s.commands[0], cmd);
    }

    #[test]
    fn insert_command_with_backslash_and_quotes() {
        let conn = setup();
        let cmd = r#"awk '{print $1}' file.txt | sed 's/\n/ /g'"#;
        insert_snippet(&conn, "awk", "", &cmds(&[cmd])).unwrap();
        let s = get_snippet(&conn, "", "awk").unwrap().expect("should exist");
        assert_eq!(s.commands[0], cmd);
    }

    #[test]
    fn insert_command_with_newline() {
        let conn = setup();
        let cmd = "line1\nline2";
        insert_snippet(&conn, "nl", "", &cmds(&[cmd])).unwrap();
        let s = get_snippet(&conn, "", "nl").unwrap().expect("should exist");
        assert_eq!(s.commands[0], cmd);
    }

    #[test]
    fn insert_duplicate_same_group_returns_error() {
        let conn = setup();
        insert_snippet(&conn, "ping", "home", &cmds(&["ping 1.1.1.1"])).unwrap();
        let err = insert_snippet(&conn, "ping", "home", &cmds(&["ping 8.8.8.8"])).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("already exists"), "error was: {}", msg);
        assert!(msg.contains("ping"), "error was: {}", msg);
    }

    #[test]
    fn insert_duplicate_ungrouped_returns_error() {
        let conn = setup();
        insert_snippet(&conn, "ping", "", &cmds(&["ping 1.1.1.1"])).unwrap();
        let err = insert_snippet(&conn, "ping", "", &cmds(&["ping 8.8.8.8"])).unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn insert_same_name_different_groups_ok() {
        let conn = setup();
        insert_snippet(&conn, "ping", "home", &cmds(&["ping 1.1.1.1"])).unwrap();
        insert_snippet(&conn, "ping", "work", &cmds(&["ping 10.0.0.1"])).unwrap();
        let home = get_snippet(&conn, "home", "ping").unwrap().expect("should exist");
        let work = get_snippet(&conn, "work", "ping").unwrap().expect("should exist");
        assert_eq!(home.commands[0], "ping 1.1.1.1");
        assert_eq!(work.commands[0], "ping 10.0.0.1");
    }

    #[test]
    fn insert_same_group_different_names_ok() {
        let conn = setup();
        insert_snippet(&conn, "ping", "home", &cmds(&["ping 1.1.1.1"])).unwrap();
        insert_snippet(&conn, "dns", "home", &cmds(&["nslookup google.com"])).unwrap();
        let snippets = list_by_group(&conn, "home").unwrap();
        assert_eq!(snippets.len(), 2);
    }

    // ── get ───────────────────────────────────────────────────────────────────

    #[test]
    fn get_snippet_not_found() {
        let conn = setup();
        assert!(get_snippet(&conn, "", "nope").unwrap().is_none());
    }

    #[test]
    fn get_snippet_wrong_group() {
        let conn = setup();
        insert_snippet(&conn, "ping", "home", &cmds(&["ping 8.8.8.8"])).unwrap();
        // exists in "home" but not in ""
        assert!(get_snippet(&conn, "", "ping").unwrap().is_none());
        assert!(get_snippet(&conn, "work", "ping").unwrap().is_none());
    }

    // ── list_all ──────────────────────────────────────────────────────────────

    #[test]
    fn list_all_empty() {
        let conn = setup();
        assert!(list_all(&conn).unwrap().is_empty());
    }

    #[test]
    fn list_all_ungrouped_first() {
        let conn = setup();
        insert_snippet(&conn, "baz", "zzz", &cmds(&["baz"])).unwrap();
        insert_snippet(&conn, "foo", "", &cmds(&["foo"])).unwrap();
        insert_snippet(&conn, "bar", "aaa", &cmds(&["bar"])).unwrap();
        let all = list_all(&conn).unwrap();
        assert_eq!(all[0].group_name, "");   // ungrouped first
        assert_eq!(all[1].group_name, "aaa"); // then alpha
        assert_eq!(all[2].group_name, "zzz");
    }

    #[test]
    fn list_all_within_group_by_name() {
        let conn = setup();
        insert_snippet(&conn, "z-snap", "home", &cmds(&["z"])).unwrap();
        insert_snippet(&conn, "a-snap", "home", &cmds(&["a"])).unwrap();
        insert_snippet(&conn, "m-snap", "home", &cmds(&["m"])).unwrap();
        let all = list_all(&conn).unwrap();
        assert_eq!(all[0].name, "a-snap");
        assert_eq!(all[1].name, "m-snap");
        assert_eq!(all[2].name, "z-snap");
    }

    #[test]
    fn list_all_groups_alphabetical() {
        let conn = setup();
        insert_snippet(&conn, "s", "zebra", &cmds(&["z"])).unwrap();
        insert_snippet(&conn, "s", "alpha", &cmds(&["a"])).unwrap();
        insert_snippet(&conn, "s", "mango", &cmds(&["m"])).unwrap();
        let all = list_all(&conn).unwrap();
        assert_eq!(all[0].group_name, "alpha");
        assert_eq!(all[1].group_name, "mango");
        assert_eq!(all[2].group_name, "zebra");
    }

    // ── list_by_group ─────────────────────────────────────────────────────────

    #[test]
    fn list_by_group_basic() {
        let conn = setup();
        insert_snippet(&conn, "ping", "home", &cmds(&["ping 8.8.8.8"])).unwrap();
        insert_snippet(&conn, "dns", "home", &cmds(&["nslookup"])).unwrap();
        insert_snippet(&conn, "other", "work", &cmds(&["other"])).unwrap();
        let home = list_by_group(&conn, "home").unwrap();
        assert_eq!(home.len(), 2);
        assert!(home.iter().all(|s| s.group_name == "home"));
    }

    #[test]
    fn list_by_group_ungrouped() {
        let conn = setup();
        insert_snippet(&conn, "free", "", &cmds(&["echo free"])).unwrap();
        insert_snippet(&conn, "other", "home", &cmds(&["echo other"])).unwrap();
        let ungrouped = list_by_group(&conn, "").unwrap();
        assert_eq!(ungrouped.len(), 1);
        assert_eq!(ungrouped[0].name, "free");
    }

    #[test]
    fn list_by_group_nonexistent_returns_empty() {
        let conn = setup();
        insert_snippet(&conn, "ping", "home", &cmds(&["ping"])).unwrap();
        assert!(list_by_group(&conn, "ghost").unwrap().is_empty());
    }

    // ── delete ────────────────────────────────────────────────────────────────

    #[test]
    fn delete_existing_returns_true() {
        let conn = setup();
        insert_snippet(&conn, "ping", "", &cmds(&["ping 8.8.8.8"])).unwrap();
        let s = get_snippet(&conn, "", "ping").unwrap().unwrap();
        assert!(delete_snippet(&conn, s.id).unwrap());
        assert!(get_snippet(&conn, "", "ping").unwrap().is_none());
    }

    #[test]
    fn delete_nonexistent_returns_false() {
        let conn = setup();
        assert!(!delete_snippet(&conn, 9999).unwrap());
    }

    #[test]
    fn delete_removes_only_target() {
        let conn = setup();
        insert_snippet(&conn, "a", "", &cmds(&["a"])).unwrap();
        insert_snippet(&conn, "b", "", &cmds(&["b"])).unwrap();
        let a = get_snippet(&conn, "", "a").unwrap().unwrap();
        delete_snippet(&conn, a.id).unwrap();
        assert!(get_snippet(&conn, "", "a").unwrap().is_none());
        assert!(get_snippet(&conn, "", "b").unwrap().is_some());
    }

    #[test]
    fn delete_same_id_twice_second_returns_false() {
        let conn = setup();
        insert_snippet(&conn, "ping", "", &cmds(&["ping"])).unwrap();
        let s = get_snippet(&conn, "", "ping").unwrap().unwrap();
        assert!(delete_snippet(&conn, s.id).unwrap());
        assert!(!delete_snippet(&conn, s.id).unwrap());
    }

    // ── delete_by_group / delete_all ─────────────────────────────────────────

    #[test]
    fn delete_by_group_removes_all_in_group() {
        let conn = setup();
        insert_snippet(&conn, "a", "home", &cmds(&["a"])).unwrap();
        insert_snippet(&conn, "b", "home", &cmds(&["b"])).unwrap();
        insert_snippet(&conn, "c", "work", &cmds(&["c"])).unwrap();
        let removed = delete_by_group(&conn, "home").unwrap();
        assert_eq!(removed, 2);
        assert!(list_by_group(&conn, "home").unwrap().is_empty());
        assert_eq!(list_by_group(&conn, "work").unwrap().len(), 1);
    }

    #[test]
    fn delete_by_group_ungrouped() {
        let conn = setup();
        insert_snippet(&conn, "free1", "", &cmds(&["a"])).unwrap();
        insert_snippet(&conn, "free2", "", &cmds(&["b"])).unwrap();
        insert_snippet(&conn, "kept", "home", &cmds(&["c"])).unwrap();
        let removed = delete_by_group(&conn, "").unwrap();
        assert_eq!(removed, 2);
        assert!(list_by_group(&conn, "").unwrap().is_empty());
        assert_eq!(list_by_group(&conn, "home").unwrap().len(), 1);
    }

    #[test]
    fn delete_by_group_nonexistent_returns_zero() {
        let conn = setup();
        insert_snippet(&conn, "ping", "home", &cmds(&["ping"])).unwrap();
        assert_eq!(delete_by_group(&conn, "ghost").unwrap(), 0);
        assert_eq!(list_all(&conn).unwrap().len(), 1);
    }

    #[test]
    fn delete_all_clears_everything() {
        let conn = setup();
        insert_snippet(&conn, "a", "",     &cmds(&["a"])).unwrap();
        insert_snippet(&conn, "b", "home", &cmds(&["b"])).unwrap();
        insert_snippet(&conn, "c", "work", &cmds(&["c"])).unwrap();
        let removed = delete_all(&conn).unwrap();
        assert_eq!(removed, 3);
        assert!(list_all(&conn).unwrap().is_empty());
    }

    #[test]
    fn delete_all_on_empty_db_returns_zero() {
        let conn = setup();
        assert_eq!(delete_all(&conn).unwrap(), 0);
    }

    // ── update ────────────────────────────────────────────────────────────────

    #[test]
    fn update_snippet_name() {
        let conn = setup();
        insert_snippet(&conn, "old", "", &cmds(&["echo hi"])).unwrap();
        let mut s = get_snippet(&conn, "", "old").unwrap().unwrap();
        s.name = "new".to_string();
        update_snippet(&conn, &s).unwrap();
        assert!(get_snippet(&conn, "", "old").unwrap().is_none());
        assert!(get_snippet(&conn, "", "new").unwrap().is_some());
    }

    #[test]
    fn update_snippet_commands() {
        let conn = setup();
        insert_snippet(&conn, "snap", "", &cmds(&["old cmd"])).unwrap();
        let mut s = get_snippet(&conn, "", "snap").unwrap().unwrap();
        s.commands = cmds(&["new cmd 1", "new cmd 2"]);
        update_snippet(&conn, &s).unwrap();
        let updated = get_snippet(&conn, "", "snap").unwrap().unwrap();
        assert_eq!(updated.commands, vec!["new cmd 1", "new cmd 2"]);
    }

    #[test]
    fn update_snippet_group() {
        let conn = setup();
        insert_snippet(&conn, "snap", "old-group", &cmds(&["cmd"])).unwrap();
        let mut s = get_snippet(&conn, "old-group", "snap").unwrap().unwrap();
        s.group_name = "new-group".to_string();
        update_snippet(&conn, &s).unwrap();
        assert!(get_snippet(&conn, "old-group", "snap").unwrap().is_none());
        assert!(get_snippet(&conn, "new-group", "snap").unwrap().is_some());
    }

    // ── large data ────────────────────────────────────────────────────────────

    #[test]
    fn insert_and_list_1000_snippets() {
        let conn = setup();
        for i in 0..1000u16 {
            insert_snippet(
                &conn,
                &format!("snap{:04}", i),
                if i % 2 == 0 { "even" } else { "odd" },
                &cmds(&[&format!("echo {}", i)]),
            ).unwrap();
        }
        let all = list_all(&conn).unwrap();
        assert_eq!(all.len(), 1000);
        let even = list_by_group(&conn, "even").unwrap();
        assert_eq!(even.len(), 500);
    }
}
