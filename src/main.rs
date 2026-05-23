mod cli;
mod commands;
mod db;
mod models;
mod tui;

use anyhow::Result;
use clap::{CommandFactory, Parser};
use cli::{Args, Command};

fn main() -> Result<()> {
    let args = Args::parse();
    let conn = db::open()?;

    match args.command {
        Some(Command::Save { group, name: Some(name), cmd }) => {
            commands::save::run_noninteractive(&conn, group, name, cmd)?;
        }
        Some(Command::Save { .. }) => {
            // Phase 4: interactive save form
            todo!("Interactive save — Phase 4")
        }
        Some(Command::List { group }) => {
            commands::list::run(&conn, group)?;
        }
        Some(Command::Delete { group, name }) => {
            commands::delete::run(&conn, &group, &name)?;
        }
        Some(Command::Last) => {
            todo!("tet last — Phase 6")
        }
        Some(Command::Shell { .. }) => {
            todo!("tet shell — Phase 6")
        }
        None => match args.run_args.as_slice() {
            [name] => commands::run::run(&conn, "", name)?,
            [group, name] => commands::run::run(&conn, group.as_str(), name.as_str())?,
            [] => Args::command().print_help()?,
            _ => anyhow::bail!("Too many arguments"),
        },
    }

    Ok(())
}
