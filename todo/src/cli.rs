use clap::{Args, Subcommand};

use crate::{goals, tasks};

/// To-do CLI
#[derive(Debug, Args)]
pub struct Cli {
    /// To-do command group
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

/// To-do commands
#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Manage goals
    Goal(goals::Cli),
    /// Manage tasks
    Task(tasks::Cli),
    /// Free-form text
    Free {
        /// Free-form text
        text: String,
    },
}
