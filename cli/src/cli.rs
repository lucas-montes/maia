use clap::Parser;

use {knowledge, todo};

#[derive(Debug, Parser)]
#[command(name = "Maia")]
#[command(about = "Maia your AI assistant")]
pub struct Cli {
    /// Main command group
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub fn handle() -> Self {
        Cli::parse()
    }

    pub fn message(self) -> Vec<u8> {
        match self.command {
            Commands::Todo(cli) => cli.message(),
            Commands::Knowledge(cli) => cli.message(),
        }
    }
}

/// Top-level commands
#[derive(Debug, Parser)]
pub enum Commands {
    /// To-do management
    Todo(todo::cli::Cli),
    /// Knowledge management
    Knowledge(knowledge::cli::Cli),
}
