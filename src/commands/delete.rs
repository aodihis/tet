use anyhow::Result;
use rusqlite::Connection;

use crate::db::ops;
use crate::models::display_name;

pub fn run(conn: &Connection, group: &str, name: &str) -> Result<()> {
    let display = display_name(group, name);
    match ops::get_snippet(conn, group, name)? {
        None => println!("Not found: {}", display),
        Some(snippet) => {
            ops::delete_snippet(conn, snippet.id)?;
            println!("Deleted: {}", display);
        }
    }
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

    fn insert(conn: &Connection, group: &str, name: &str) {
        ops::insert_snippet(conn, name, group, &["echo hi".to_string()]).unwrap();
    }

    #[test]
    fn delete_existing_returns_ok_and_removes_from_db() {
        let conn = setup();
        insert(&conn, "", "ping");
        assert!(run(&conn, "", "ping").is_ok());
        assert!(ops::get_snippet(&conn, "", "ping").unwrap().is_none());
    }

    #[test]
    fn delete_nonexistent_returns_ok() {
        let conn = setup();
        // "Not found" is a normal user outcome, not an error
        assert!(run(&conn, "", "ghost").is_ok());
    }

    #[test]
    fn delete_grouped_snippet() {
        let conn = setup();
        insert(&conn, "home", "dns");
        assert!(run(&conn, "home", "dns").is_ok());
        assert!(ops::get_snippet(&conn, "home", "dns").unwrap().is_none());
    }

    #[test]
    fn delete_does_not_remove_sibling_in_same_group() {
        let conn = setup();
        insert(&conn, "home", "dns");
        insert(&conn, "home", "ping");
        run(&conn, "home", "dns").unwrap();
        assert!(ops::get_snippet(&conn, "home", "ping").unwrap().is_some());
    }

    #[test]
    fn delete_wrong_group_leaves_snippet_intact() {
        let conn = setup();
        insert(&conn, "home", "ping");
        // delete from "work" group — snippet lives in "home"
        run(&conn, "work", "ping").unwrap();
        assert!(ops::get_snippet(&conn, "home", "ping").unwrap().is_some());
    }

    #[test]
    fn delete_ungrouped_does_not_affect_grouped() {
        let conn = setup();
        insert(&conn, "", "ping");
        insert(&conn, "home", "ping");
        run(&conn, "", "ping").unwrap();
        assert!(ops::get_snippet(&conn, "", "ping").unwrap().is_none());
        assert!(ops::get_snippet(&conn, "home", "ping").unwrap().is_some());
    }

    #[test]
    fn double_delete_second_is_ok() {
        let conn = setup();
        insert(&conn, "", "ping");
        run(&conn, "", "ping").unwrap();
        // second delete is "Not found" — still Ok(())
        assert!(run(&conn, "", "ping").is_ok());
    }
}
