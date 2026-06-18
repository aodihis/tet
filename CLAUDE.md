# tet — Coding Rules

## Project

CLI command snippet manager written in Rust. Saves long commands with short nicknames and lets users run them instantly.

## General

- Rust 2024 edition. Keep dependencies minimal — check `Cargo.toml` before adding a new crate, prefer what's already there.
- Each module has one clear responsibility (CLI parsing, DB ops, a single command, a single TUI component). Don't mix concerns into one file.
- Keep `main.rs` thin — parsing args and dispatching only, no business logic.
- Each command module exposes exactly one entry point: `pub fn run(...) -> Result<()>`.
- Favor explicit, readable code over clever abstractions. No premature generalization.

## Coding Rules

### Errors
- All functions return `anyhow::Result<T>`
- Propagate with `?`
- User-visible errors: `anyhow::bail!("message")`
- Never use `panic!` or `unwrap()` in command handlers
- `main()` prints errors with `eprintln!("{:#}", e)` via `?` propagation

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

### File Headers
- Every source file starts with a short doc comment (`//!`) describing its single responsibility — one or two lines, no more
- State what the file owns, not how it works internally (the code already shows that)
- Example:
  ```rust
  //! CRUD operations against the snippets table.
  use anyhow::{Context, Result};
  ```
- Skip the header only for trivial re-export files (e.g. a `mod.rs` that's just `pub mod x;` lines)

## Testing

- Unit tests live in a `#[cfg(test)] mod tests` block at the bottom of the file they test (see `src/commands/save.rs` for the pattern). Cover validation logic, edge cases, and error paths directly.
- Integration tests live in `tests/integration.rs` and exercise the CLI/DB end-to-end (e.g. via a temp DB with `tempfile`), not internal functions.
- Every new command or validation rule needs at least one unit test for the happy path and one for each error case.
- Every bug fix gets a regression test that fails before the fix and passes after.

## Before Committing

- [ ] `cargo fmt` — formatting is consistent
- [ ] `cargo clippy --all-targets -- -D warnings` — no lint warnings
- [ ] `cargo test` — all unit and integration tests pass
- [ ] `cargo build` — builds cleanly
- [ ] New/changed behavior has corresponding tests
- [ ] No `panic!`/`unwrap()` introduced in command handlers
- [ ] Version bumped in `Cargo.toml` if this is a release-worthy change (see pre-push hook)
