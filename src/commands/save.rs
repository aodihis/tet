use anyhow::{bail, Result};
use rusqlite::Connection;

use crate::cli::RESERVED;
use crate::db::ops;
use crate::models::display_name;
use crate::tui::save_form;

pub fn run_interactive(conn: &Connection, prefill: Option<String>) -> Result<()> {
    match save_form::run(prefill)? {
        None => println!("Cancelled."),
        Some(result) => {
            run_noninteractive(conn, result.group, result.name, result.commands)?;
        }
    }
    Ok(())
}

pub fn run_noninteractive(
    conn: &Connection,
    group: String,
    name: String,
    cmd: Vec<String>,
) -> Result<()> {
    if name.is_empty() {
        bail!("Name cannot be empty");
    }
    for word in [name.as_str(), group.as_str()] {
        if RESERVED.contains(&word) {
            bail!("'{}' is a reserved word and cannot be used", word);
        }
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        bail!("Name must be a single word (letters, numbers, - and _ only)");
    }
    if !group.is_empty() && !group.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        bail!("Group must be a single word (letters, numbers, - and _ only)");
    }
    if cmd.is_empty() {
        bail!("Command cannot be empty");
    }

    ops::insert_snippet(conn, &name, &group, &cmd)?;
    println!("Saved: {}", display_name(&group, &name));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ops;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::schema::CREATE_SCHEMA).unwrap();
        conn
    }

    fn save(conn: &Connection, group: &str, name: &str, cmd: &str) -> Result<()> {
        run_noninteractive(
            conn,
            group.to_string(),
            name.to_string(),
            vec![cmd.to_string()],
        )
    }

    // ── valid inputs ──────────────────────────────────────────────────────────

    #[test]
    fn valid_ungrouped() {
        let conn = setup();
        assert!(save(&conn, "", "ping", "ping 8.8.8.8").is_ok());
        assert!(ops::get_snippet(&conn, "", "ping").unwrap().is_some());
    }

    #[test]
    fn valid_with_group() {
        let conn = setup();
        assert!(save(&conn, "home", "dns", "nslookup google.com").is_ok());
        assert!(ops::get_snippet(&conn, "home", "dns").unwrap().is_some());
    }

    #[test]
    fn valid_name_with_hyphen() {
        let conn = setup();
        assert!(save(&conn, "", "my-cmd", "echo hi").is_ok());
    }

    #[test]
    fn valid_name_with_underscore() {
        let conn = setup();
        assert!(save(&conn, "", "my_cmd", "echo hi").is_ok());
    }

    #[test]
    fn valid_name_numbers_only() {
        let conn = setup();
        assert!(save(&conn, "", "123", "echo 123").is_ok());
    }

    #[test]
    fn valid_name_mixed_alphanumeric() {
        let conn = setup();
        assert!(save(&conn, "", "cmd2x", "echo cmd").is_ok());
    }

    #[test]
    fn valid_multiple_commands() {
        let conn = setup();
        let result = run_noninteractive(
            &conn,
            "".to_string(),
            "multi".to_string(),
            vec!["echo a".to_string(), "echo b".to_string(), "echo c".to_string()],
        );
        assert!(result.is_ok());
        let s = ops::get_snippet(&conn, "", "multi").unwrap().unwrap();
        assert_eq!(s.commands.len(), 3);
    }

    #[test]
    fn valid_group_with_hyphen_and_underscore() {
        let conn = setup();
        assert!(save(&conn, "my-group_1", "snap", "echo hi").is_ok());
    }

    // ── name validation errors ────────────────────────────────────────────────

    #[test]
    fn error_empty_name() {
        let conn = setup();
        let err = save(&conn, "", "", "echo hi").unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    #[test]
    fn error_name_with_space() {
        let conn = setup();
        let err = save(&conn, "", "my cmd", "echo hi").unwrap_err();
        assert!(err.to_string().contains("single word"));
    }

    #[test]
    fn error_name_with_dot() {
        let conn = setup();
        let err = save(&conn, "", "my.cmd", "echo hi").unwrap_err();
        assert!(err.to_string().contains("single word"));
    }

    #[test]
    fn error_name_with_slash() {
        let conn = setup();
        let err = save(&conn, "", "my/cmd", "echo hi").unwrap_err();
        assert!(err.to_string().contains("single word"));
    }

    #[test]
    fn error_name_with_at() {
        let conn = setup();
        let err = save(&conn, "", "my@cmd", "echo hi").unwrap_err();
        assert!(err.to_string().contains("single word"));
    }

    // ── reserved word errors ──────────────────────────────────────────────────

    #[test]
    fn error_reserved_name_save() {
        let conn = setup();
        let err = save(&conn, "", "save", "echo hi").unwrap_err();
        assert!(err.to_string().contains("reserved"));
    }

    #[test]
    fn error_reserved_name_list() {
        let conn = setup();
        let err = save(&conn, "", "list", "echo hi").unwrap_err();
        assert!(err.to_string().contains("reserved"));
    }

    #[test]
    fn error_reserved_name_delete() {
        let conn = setup();
        let err = save(&conn, "", "delete", "echo hi").unwrap_err();
        assert!(err.to_string().contains("reserved"));
    }

    #[test]
    fn error_reserved_name_del() {
        let conn = setup();
        let err = save(&conn, "", "del", "echo hi").unwrap_err();
        assert!(err.to_string().contains("reserved"));
    }

    #[test]
    fn error_reserved_name_rm() {
        let conn = setup();
        let err = save(&conn, "", "rm", "echo hi").unwrap_err();
        assert!(err.to_string().contains("reserved"));
    }

    #[test]
    fn error_reserved_name_ls() {
        let conn = setup();
        let err = save(&conn, "", "ls", "echo hi").unwrap_err();
        assert!(err.to_string().contains("reserved"));
    }

    #[test]
    fn error_reserved_name_run() {
        let conn = setup();
        let err = save(&conn, "", "run", "echo hi").unwrap_err();
        assert!(err.to_string().contains("reserved"));
    }

    #[test]
    fn error_reserved_group() {
        let conn = setup();
        let err = save(&conn, "save", "mysnip", "echo hi").unwrap_err();
        assert!(err.to_string().contains("reserved"));
    }

    // ── group validation errors ───────────────────────────────────────────────

    #[test]
    fn error_group_with_space() {
        let conn = setup();
        let err = save(&conn, "my group", "snap", "echo hi").unwrap_err();
        assert!(err.to_string().contains("single word"));
    }

    #[test]
    fn error_group_with_slash() {
        let conn = setup();
        let err = save(&conn, "my/group", "snap", "echo hi").unwrap_err();
        assert!(err.to_string().contains("single word"));
    }

    // ── command errors ────────────────────────────────────────────────────────

    #[test]
    fn error_empty_commands() {
        let conn = setup();
        let err = run_noninteractive(
            &conn, "".to_string(), "snap".to_string(), vec![],
        ).unwrap_err();
        assert!(err.to_string().contains("empty"));
    }

    // ── duplicate ─────────────────────────────────────────────────────────────

    #[test]
    fn error_duplicate_same_group_and_name() {
        let conn = setup();
        save(&conn, "home", "ping", "ping 1.1.1.1").unwrap();
        let err = save(&conn, "home", "ping", "ping 8.8.8.8").unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn duplicate_different_groups_is_ok() {
        let conn = setup();
        save(&conn, "home", "ping", "ping 1.1.1.1").unwrap();
        assert!(save(&conn, "work", "ping", "ping 10.0.0.1").is_ok());
    }

    // ── all reserved words are covered ───────────────────────────────────────

    #[test]
    fn all_reserved_words_are_rejected() {
        let conn = setup();
        for &word in crate::cli::RESERVED {
            let err = save(&conn, "", word, "echo hi").unwrap_err();
            assert!(
                err.to_string().contains("reserved"),
                "'{}' should be rejected as reserved but got: {}",
                word, err
            );
        }
    }
}
