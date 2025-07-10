use clap::{Args, Subcommand};

use crate::{goals, tasks};

#[derive(Debug, Args)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Goal(goals::Cli),
    Task(tasks::Cli),
    Free { text: String },
}
