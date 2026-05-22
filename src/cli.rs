use clap::{Parser, Subcommand};

pub const RESERVED: &[&str] = &[
    "save", "last", "list", "ls", "search", "find",
    "delete", "del", "rm", "help", "run", "-",
];

#[derive(Parser)]
#[command(name = "tet", about = "CLI command snippet manager")]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Run a saved shortcut directly (e.g. tet ping)
    pub shortcut: Option<String>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Save a new snippet (interactive if args omitted)
    Save {
        /// Group name, or "-" for ungrouped
        group: Option<String>,
        shortcut: Option<String>,
        #[arg(trailing_var_arg = true)]
        cmd: Vec<String>,
    },
    /// Save the last shell command as a snippet
    Last,
    /// List snippets; optionally filter by group ("-" for ungrouped)
    List {
        group: Option<String>,
    },
    /// Delete a saved snippet
    #[command(aliases = ["del", "rm"])]
    Delete {
        shortcut: String,
    },
    /// Print shell hook script (bash, zsh, pwsh)
    Shell {
        shell: String,
    },
}
