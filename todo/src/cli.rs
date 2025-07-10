use clap::{Args, Subcommand};

use crate::{goals, tasks};

#[derive(Debug, Args)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub fn message(self) -> Vec<u8> {
        match self.command {
            Commands::Goal(cmd) => cmd.message(),
            Commands::Task(cmd) => cmd.message(),
            Commands::Free { text } => text.into_bytes(),
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Goal(goals::Cli),
    Task(tasks::Cli),
    Free { text: String },
}
