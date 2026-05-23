/// Integration tests: full pipeline through library API + concurrency via threads.
/// Each test gets its own isolated temp directory — no shared state.
use tempfile::TempDir;

use tet::db::{self, ops};
use tet::commands::{save, delete};

// ── helpers ───────────────────────────────────────────────────────────────────

fn temp_conn() -> (TempDir, rusqlite::Connection) {
    let dir = TempDir::new().unwrap();
    let conn = db::open_at(dir.path()).unwrap();
    (dir, conn)
}

fn cmds(s: &[&str]) -> Vec<String> {
    s.iter().map(|s| s.to_string()).collect()
}

fn do_save(conn: &rusqlite::Connection, group: &str, name: &str, cmd: &str) {
    save::run_noninteractive(
        conn,
        group.to_string(),
        name.to_string(),
        vec![cmd.to_string()],
    ).unwrap();
}

// ── pipeline: save → get → delete ────────────────────────────────────────────

#[test]
fn save_and_retrieve_roundtrip() {
    let (_dir, conn) = temp_conn();
    do_save(&conn, "", "ping", "ping 8.8.8.8 -n 4");
    let s = ops::get_snippet(&conn, "", "ping").unwrap().expect("should exist");
    assert_eq!(s.name, "ping");
    assert_eq!(s.commands, vec!["ping 8.8.8.8 -n 4"]);
}

#[test]
fn save_grouped_and_retrieve() {
    let (_dir, conn) = temp_conn();
    do_save(&conn, "home", "dns", "nslookup google.com 8.8.8.8");
    let s = ops::get_snippet(&conn, "home", "dns").unwrap().expect("should exist");
    assert_eq!(s.group_name, "home");
}

#[test]
fn save_then_delete() {
    let (_dir, conn) = temp_conn();
    do_save(&conn, "", "ping", "ping 8.8.8.8");
    delete::run(&conn, "", "ping").unwrap();
    assert!(ops::get_snippet(&conn, "", "ping").unwrap().is_none());
}

#[test]
fn save_duplicate_returns_error() {
    let (_dir, conn) = temp_conn();
    do_save(&conn, "home", "ping", "ping 1.1.1.1");
    let err = save::run_noninteractive(
        &conn,
        "home".to_string(),
        "ping".to_string(),
        vec!["ping 8.8.8.8".to_string()],
    ).unwrap_err();
    assert!(err.to_string().contains("already exists"));
}

#[test]
fn delete_nonexistent_is_ok() {
    let (_dir, conn) = temp_conn();
    assert!(delete::run(&conn, "", "ghost").is_ok());
}

// ── list ordering ─────────────────────────────────────────────────────────────

#[test]
fn list_all_empty() {
    let (_dir, conn) = temp_conn();
    assert!(ops::list_all(&conn).unwrap().is_empty());
}

#[test]
fn list_all_correct_order() {
    let (_dir, conn) = temp_conn();
    do_save(&conn, "zebra", "s1", "z");
    do_save(&conn, "", "free", "free");
    do_save(&conn, "alpha", "s1", "a");
    let all = ops::list_all(&conn).unwrap();
    assert_eq!(all[0].group_name, "");       // ungrouped first
    assert_eq!(all[1].group_name, "alpha");  // then alphabetical
    assert_eq!(all[2].group_name, "zebra");
}

// ── edge cases: long/complex data ─────────────────────────────────────────────

#[test]
fn very_long_command_survives_roundtrip() {
    let (_dir, conn) = temp_conn();
    let long_cmd = "A".repeat(50_000);
    ops::insert_snippet(&conn, "bigcmd", "", &cmds(&[&long_cmd])).unwrap();
    let s = ops::get_snippet(&conn, "", "bigcmd").unwrap().unwrap();
    assert_eq!(s.commands[0].len(), 50_000);
    assert!(s.commands[0].chars().all(|c| c == 'A'));
}

#[test]
fn many_commands_survive_roundtrip() {
    let (_dir, conn) = temp_conn();
    let commands: Vec<String> = (0..50).map(|i| format!("step {}: do something", i)).collect();
    ops::insert_snippet(&conn, "pipeline", "", &commands).unwrap();
    let s = ops::get_snippet(&conn, "", "pipeline").unwrap().unwrap();
    assert_eq!(s.commands.len(), 50);
    assert_eq!(s.commands[49], "step 49: do something");
}

#[test]
fn special_chars_in_command_survive_roundtrip() {
    let (_dir, conn) = temp_conn();
    let cmd = r#"bash -c 'grep "pattern" file.txt | awk "{print $1}" | sort -u'"#;
    ops::insert_snippet(&conn, "grep", "", &cmds(&[cmd])).unwrap();
    let s = ops::get_snippet(&conn, "", "grep").unwrap().unwrap();
    assert_eq!(s.commands[0], cmd);
}

#[test]
fn unicode_in_command_and_name_survive_roundtrip() {
    let (_dir, conn) = temp_conn();
    ops::insert_snippet(&conn, "emoji", "grüp", &cmds(&["echo 🚀 日本語"])).unwrap();
    let s = ops::get_snippet(&conn, "grüp", "emoji").unwrap().unwrap();
    assert_eq!(s.commands[0], "echo 🚀 日本語");
}

// ── open_at creates directory ─────────────────────────────────────────────────

#[test]
fn open_at_creates_nested_directory() {
    let base = TempDir::new().unwrap();
    let nested = base.path().join("a").join("b").join("c");
    assert!(!nested.exists());
    let _conn = db::open_at(&nested).unwrap();
    assert!(nested.join("tet.db").exists());
}

// ── multiple connections to same file (simulates concurrent CLI invocations) ──

#[test]
fn two_connections_see_same_data() {
    let dir = TempDir::new().unwrap();
    let conn1 = db::open_at(dir.path()).unwrap();
    let conn2 = db::open_at(dir.path()).unwrap();
    ops::insert_snippet(&conn1, "ping", "", &cmds(&["ping 8.8.8.8"])).unwrap();
    let s = ops::get_snippet(&conn2, "", "ping").unwrap();
    assert!(s.is_some(), "conn2 should see data written by conn1");
}

// ── concurrency: different snippets ──────────────────────────────────────────

#[test]
fn concurrent_inserts_of_different_snippets_all_succeed() {
    let dir = TempDir::new().unwrap();
    // Initialise DB before threads start
    let _ = db::open_at(dir.path()).unwrap();

    let dir_path = dir.path().to_owned();
    let handles: Vec<_> = (0..20u32)
        .map(|i| {
            let path = dir_path.clone();
            std::thread::spawn(move || {
                let conn = db::open_at(&path).unwrap();
                ops::insert_snippet(
                    &conn,
                    &format!("snap{:02}", i),
                    "",
                    &[format!("echo snippet{}", i)],
                )
            })
        })
        .collect();

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let failures: Vec<_> = results.iter().filter(|r| r.is_err()).collect();
    assert!(
        failures.is_empty(),
        "{} out of 20 inserts failed: {:?}",
        failures.len(),
        failures
    );

    let conn = db::open_at(dir.path()).unwrap();
    assert_eq!(ops::list_all(&conn).unwrap().len(), 20);
}

// ── concurrency: same snippet — only one write wins ──────────────────────────

#[test]
fn concurrent_inserts_of_same_snippet_exactly_one_wins() {
    let dir = TempDir::new().unwrap();
    let _ = db::open_at(dir.path()).unwrap();

    let dir_path = dir.path().to_owned();
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let path = dir_path.clone();
            std::thread::spawn(move || {
                let conn = db::open_at(&path).unwrap();
                ops::insert_snippet(&conn, "ping", "", &["ping 8.8.8.8".to_string()])
            })
        })
        .collect();

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let successes = results.iter().filter(|r| r.is_ok()).count();
    assert_eq!(successes, 1, "exactly one insert should succeed; got {}", successes);

    let conn = db::open_at(dir.path()).unwrap();
    let all = ops::list_all(&conn).unwrap();
    assert_eq!(all.len(), 1, "DB should contain exactly one snippet");
}

// ── concurrency: mixed reads and writes ──────────────────────────────────────

#[test]
fn concurrent_reads_and_writes_are_consistent() {
    let dir = TempDir::new().unwrap();
    // Pre-populate
    let conn = db::open_at(dir.path()).unwrap();
    for i in 0..5u32 {
        ops::insert_snippet(
            &conn,
            &format!("existing{}", i),
            "",
            &[format!("echo {}", i)],
        ).unwrap();
    }
    drop(conn);

    let dir_path = dir.path().to_owned();

    // 5 writer threads + 5 reader threads
    let writers: Vec<_> = (0..5u32)
        .map(|i| {
            let path = dir_path.clone();
            std::thread::spawn(move || -> anyhow::Result<()> {
                let conn = db::open_at(&path)?;
                ops::insert_snippet(
                    &conn,
                    &format!("new{}", i),
                    "concurrent",
                    &[format!("echo new{}", i)],
                )?;
                Ok(())
            })
        })
        .collect();

    let readers: Vec<_> = (0..5)
        .map(|_| {
            let path = dir_path.clone();
            std::thread::spawn(move || -> anyhow::Result<usize> {
                let conn = db::open_at(&path)?;
                Ok(ops::list_all(&conn)?.len())
            })
        })
        .collect();

    for h in writers {
        h.join().unwrap().unwrap();
    }
    for h in readers {
        // Just assert no panic — reads can observe any snapshot between 5 and 10
        let count = h.join().unwrap().unwrap();
        assert!((5..=10).contains(&count), "unexpected count: {}", count);
    }

    // After all threads finish, total should be exactly 10
    let conn = db::open_at(dir.path()).unwrap();
    assert_eq!(ops::list_all(&conn).unwrap().len(), 10);
}

// ── TET_DATA_DIR env var ──────────────────────────────────────────────────────

#[test]
fn tet_data_dir_env_var_overrides_default_location() {
    let dir = TempDir::new().unwrap();
    // Set env var, open, insert, verify file is in the expected location
    // Safety: test binary is single-threaded at this point; no other thread reads the env.
    unsafe { std::env::set_var("TET_DATA_DIR", dir.path()); }
    let conn = db::open().unwrap();
    ops::insert_snippet(&conn, "envtest", "", &["echo env".to_string()]).unwrap();
    unsafe { std::env::remove_var("TET_DATA_DIR"); }

    assert!(dir.path().join("tet.db").exists());
    let s = ops::get_snippet(&conn, "", "envtest").unwrap();
    assert!(s.is_some());
}
