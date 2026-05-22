mod cli;
mod commands;
mod db;
mod models;
mod tui;

use anyhow::Result;

fn main() -> Result<()> {
    println!("tet — snippet manager");
    Ok(())
}
