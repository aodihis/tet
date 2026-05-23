use anyhow::Result;
use clap::{CommandFactory, Parser};
use tet::cli::{Args, Command};

fn main() -> Result<()> {
    let args = Args::parse();
    let conn = tet::db::open()?;

    match args.command {
        Some(Command::Save { group, name: Some(name), cmd }) => {
            tet::commands::save::run_noninteractive(&conn, group, name, cmd)?;
        }
        Some(Command::Save { name: None, .. }) => {
            tet::commands::save::run_interactive(&conn, None)?;
        }
        Some(Command::List { group }) => {
            tet::commands::list::run(&conn, group)?;
        }
        Some(Command::Delete { group, name }) => {
            tet::commands::delete::run(&conn, &group, &name)?;
        }
        Some(Command::Last) => {
            todo!("tet last — Phase 6")
        }
        Some(Command::Shell { .. }) => {
            todo!("tet shell — Phase 6")
        }
        None => match args.run_args.as_slice() {
            [name] => tet::commands::run::run(&conn, "", name)?,
            [group, name] => tet::commands::run::run(&conn, group.as_str(), name.as_str())?,
            [] => Args::command().print_help()?,
            _ => anyhow::bail!("Too many arguments"),
        },
    }

    Ok(())
}
