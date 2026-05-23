# tet

A fast CLI command snippet manager. Save long commands with short nicknames and run them instantly.

## Install

```powershell
cargo install --path .
```

## Usage

### Save a snippet

Interactive (opens TUI form):
```
tet save
```

Non-interactive:
```
tet save [group] <shortcut> <command...>
```

Use `-` as group to save without a group:
```
tet save - ping "ping 8.8.8.8 -n 4"
tet save home dns "nslookup google.com 8.8.8.8"
```

### Run a snippet

```
tet <shortcut>
```

If the shortcut isn't found exactly, fuzzy search kicks in and shows a picker.

### List snippets

```
tet list              # all snippets
tet list home         # only "home" group
tet list -            # only ungrouped
```

The list opens a TUI with a live search bar. Type to filter by shortcut, group, or command content.

### Delete a snippet

```
tet delete <shortcut>
```

Aliases: `tet del`, `tet rm`

### Save last shell command

First, install the shell hook:
```
tet shell bash >> ~/.bashrc   # bash
tet shell zsh  >> ~/.zshrc    # zsh
tet shell pwsh >> $PROFILE    # powershell
```

Then:
```
tet last
```

This opens the save form with the previous command pre-filled.

## Data storage

Snippets are stored in SQLite at:
- **Windows:** `%APPDATA%\tet\tet.db`
- **Linux/Mac:** `~/.local/share/tet/tet.db`

## Reserved words

These cannot be used as shortcut names:
`save`, `last`, `list`, `ls`, `search`, `find`, `delete`, `del`, `rm`, `help`, `run`, `-`

## Development phases

| Phase | Status |
|-------|--------|
| 1 — Foundation | ✓ |
| 2 — Core CRUD (non-interactive) | ✓ |
| 3 — TUI list with search | ✓ |
| 4 — Interactive save form | ✓ |
| 5 — Run shortcuts + fuzzy fallback | pending |
| 6 — Advanced features | pending |
