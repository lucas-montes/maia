use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Expense(Expense)
}

#[derive(Debug, Args)]
struct Expense{
    ticket: PathBuf,

}
