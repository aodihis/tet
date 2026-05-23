use anyhow::{bail, Result};

pub fn print_hook(shell: &str) -> Result<()> {
    match shell {
        "bash" => print!("{}", BASH_HOOK),
        "zsh" => print!("{}", ZSH_HOOK),
        "pwsh" | "powershell" => print!("{}", PWSH_HOOK),
        other => bail!("Unknown shell '{}'. Supported: bash, zsh, pwsh", other),
    }
    Ok(())
}

pub fn last_cmd_path() -> std::path::PathBuf {
    std::env::temp_dir().join("tet_last_cmd")
}

const BASH_HOOK: &str = r#"# tet shell hook — add to ~/.bashrc
_tet_save_last() {
    echo "$1" > "${TMPDIR:-/tmp}/tet_last_cmd"
}
PROMPT_COMMAND='_tet_save_last "$(history 1 | sed "s/^ *[0-9]* *//")"'
"#;

const ZSH_HOOK: &str = r#"# tet shell hook — add to ~/.zshrc
_tet_save_last() {
    echo "$1" > "${TMPDIR:-/tmp}/tet_last_cmd"
}
autoload -Uz add-zsh-hook
add-zsh-hook preexec _tet_save_last
"#;

const PWSH_HOOK: &str = r#"# tet shell hook — add to $PROFILE
function _TetSaveLast {
    param([string]$Line)
    $Line | Set-Content -Path "$env:TEMP\tet_last_cmd" -Encoding UTF8
}
Set-PSReadLineOption -AddToHistoryHandler {
    param([string]$line)
    _TetSaveLast $line
    return $true
}
"#;
