use clap::{Parser, Subcommand};

pub const RESERVED: &[&str] = &[
    "save", "save-last", "list", "ls", "search", "find",
    "delete", "del", "rm", "help", "run", "shell",
];

#[derive(Parser)]
#[command(name = "tet", about = "CLI command snippet manager")]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Run a snippet: `tet <name>` or `tet <group> <name>`
    pub run_args: Vec<String>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Save a new snippet (interactive if name/cmd omitted)
    Save {
        /// Group name (empty = ungrouped)
        #[arg(short = 'g', long, default_value = "")]
        group: String,
        name: Option<String>,
        #[arg(trailing_var_arg = true)]
        cmd: Vec<String>,
    },
    /// Save the last shell command as a snippet
    SaveLast,
    /// List snippets; optionally filter by group name
    List {
        group: Option<String>,
    },
    /// Delete a saved snippet
    #[command(aliases = ["del", "rm"])]
    Delete {
        /// Group name (empty = ungrouped)
        #[arg(short = 'g', long, default_value = "")]
        group: String,
        name: String,
    },
    /// Print shell hook script (bash, zsh, pwsh)
    Shell {
        shell: String,
    },
}
