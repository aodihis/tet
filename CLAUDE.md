# tet — Coding Rules

## Project

CLI command snippet manager written in Rust. Saves long commands with short nicknames and lets users run them instantly.

## Module Structure

```
src/
  main.rs          — CLI parse + dispatch only, stays thin
  cli.rs           — clap structs, RESERVED words list
  models.rs        — Snippet struct (serde derives)
  db/
    mod.rs         — open() → Connection
    schema.rs      — CREATE TABLE SQL constant
    ops.rs         — all CRUD functions
  commands/        — one file per subcommand, each exports pub fn run(...)
  tui/
    list_view.rs   — full-screen list + live search bar
    save_form.rs   — 3-field interactive save form
  shell/mod.rs     — hook scripts for bash/zsh/powershell
```

## Coding Rules

### Errors
- All functions return `anyhow::Result<T>`
- Propagate with `?`
- User-visible errors: `anyhow::bail!("message")`
- Never use `panic!` or `unwrap()` in command handlers
- `main()` prints errors with `eprintln!("{:#}", e)` via `?` propagation

### Commands
- Each command module exposes exactly one entry point: `pub fn run(...) -> Result<()>`
- `main.rs` only parses CLI args and dispatches — no logic there

### Database
- `commands` column is always a JSON array (`Vec<String>`), even for a single command
- Serialize: `serde_json::to_string(&cmds)?`
- Deserialize: `serde_json::from_str::<Vec<String>>(&raw)?`
- Group = `None` (NULL) means ungrouped; group = `"-"` in CLI maps to NULL in DB

### Reserved Words
- Defined once in `src/cli.rs` as `pub const RESERVED: &[&str]`
- Validated in `commands/save.rs` before any DB write
- Users cannot use these as shortcut names: save, last, list, ls, search, find, delete, del, rm, help, run, -

### TUI
- Every TUI component wraps terminal setup/teardown in a `TerminalGuard` (Drop impl)
- This ensures terminal is restored even if the component panics
- TUI components return `Result<Option<T>>` — `None` means user cancelled (Esc)

### Fuzzy Search
- Match target per snippet: `"{group} {shortcut} {commands_joined} {description}"`
- For ungrouped snippets (group = None): omit the group prefix
- Use `fuzzy_matcher::skim::SkimMatcherV2`

### Display Order (list view)
- Ungrouped snippets appear first under "— ungrouped —" header
- Named groups sorted alphabetically below

### Comments
- No comments explaining what code does
- Only add a comment when the WHY is non-obvious: a hidden constraint, a workaround, a subtle invariant

## Development Phases

| Phase | Scope |
|-------|-------|
| 1 | Foundation: Cargo.toml, stubs, CLAUDE.md, README.md |
| 2 | Core CRUD: non-interactive save, static list, delete |
| 3 | TUI list view with live search bar |
| 4 | Interactive save form |
| 5 | Run shortcuts + fuzzy fallback |
| 6 | Advanced: last, multi-command, export/import |
