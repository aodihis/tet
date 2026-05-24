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

pub(crate) const BASH_HOOK: &str = r#"# tet shell hook — add to ~/.bashrc
_tet_save_last() {
    echo "$1" > "${TMPDIR:-/tmp}/tet_last_cmd"
}
PROMPT_COMMAND='_tet_save_last "$(history 1 | sed "s/^ *[0-9]* *//")"'
"#;

pub(crate) const ZSH_HOOK: &str = r#"# tet shell hook — add to ~/.zshrc
_tet_save_last() {
    echo "$1" > "${TMPDIR:-/tmp}/tet_last_cmd"
}
autoload -Uz add-zsh-hook
add-zsh-hook preexec _tet_save_last
"#;

pub(crate) const PWSH_HOOK: &str = r#"# tet shell hook — add to $PROFILE
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_hook_bash_returns_ok() {
        assert!(print_hook("bash").is_ok());
    }

    #[test]
    fn print_hook_zsh_returns_ok() {
        assert!(print_hook("zsh").is_ok());
    }

    #[test]
    fn print_hook_pwsh_returns_ok() {
        assert!(print_hook("pwsh").is_ok());
    }

    #[test]
    fn print_hook_powershell_alias_returns_ok() {
        assert!(print_hook("powershell").is_ok());
    }

    #[test]
    fn print_hook_unknown_shell_returns_err() {
        let err = print_hook("fish").unwrap_err();
        assert!(err.to_string().contains("Unknown shell"));
    }

    #[test]
    fn last_cmd_path_ends_with_tet_last_cmd() {
        let path = last_cmd_path();
        assert_eq!(path.file_name().unwrap(), "tet_last_cmd");
    }

    #[test]
    fn bash_hook_writes_to_tet_last_cmd() {
        assert!(BASH_HOOK.contains("tet_last_cmd"));
    }

    #[test]
    fn zsh_hook_writes_to_tet_last_cmd() {
        assert!(ZSH_HOOK.contains("tet_last_cmd"));
    }

    #[test]
    fn pwsh_hook_writes_to_tet_last_cmd() {
        assert!(PWSH_HOOK.contains("tet_last_cmd"));
    }

    #[test]
    fn bash_hook_uses_prompt_command() {
        assert!(BASH_HOOK.contains("PROMPT_COMMAND"));
    }

    #[test]
    fn zsh_hook_uses_add_zsh_hook() {
        assert!(ZSH_HOOK.contains("add-zsh-hook"));
    }

    #[test]
    fn pwsh_hook_uses_psreadline() {
        assert!(PWSH_HOOK.contains("PSReadLine"));
    }
}
