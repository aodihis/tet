use anyhow::Result;
use rusqlite::Connection;

use crate::commands::save;
use crate::shell;

pub fn run(conn: &Connection) -> Result<()> {
    save::run_interactive(conn, Some(last_command()?))
}

fn last_command() -> Result<String> {
    // Hook file takes priority (set up by `tet shell <shell>`)
    let path = shell::last_cmd_path();
    if path.exists() {
        let cmd = std::fs::read_to_string(&path)?.trim().to_string();
        if !cmd.is_empty() {
            return Ok(cmd);
        }
    }

    // On Windows fall back to PSReadLine history file
    #[cfg(target_os = "windows")]
    if let Some(cmd) = read_psreadline_history() {
        return Ok(cmd);
    }

    anyhow::bail!(
        "No last command found. Install the shell hook first:\n  tet shell pwsh >> $PROFILE  (PowerShell)\n  tet shell bash >> ~/.bashrc  (bash)\n  tet shell zsh  >> ~/.zshrc   (zsh)"
    )
}

#[cfg(target_os = "windows")]
fn read_psreadline_history() -> Option<String> {
    let appdata = std::env::var("APPDATA").ok()?;
    let path = std::path::Path::new(&appdata)
        .join("Microsoft")
        .join("Windows")
        .join("PowerShell")
        .join("PSReadLine")
        .join("ConsoleHost_history.txt");

    let content = std::fs::read_to_string(path).ok()?;
    content
        .lines()
        .rev()
        .find(|line| {
            let l = line.trim().to_lowercase();
            !l.is_empty()
                && !l.starts_with("tet save-last")
                && !l.starts_with("tet save_last")
                && !l.contains("cargo run")
        })
        .map(|s| s.trim().to_string())
}
