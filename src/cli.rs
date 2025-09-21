use clap::Parser;

use {ai, knowledge, finance, todo, health};

/// Maia - your (my) AI assistant
#[derive(Debug, Parser)]
#[command(name = "Maia")]
#[command(about = "Maia your (my) AI assistant")]
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
            Commands::Ai(cli) => cli.message(),
            Commands::Todo(cli) => cli.message(),
            Commands::Knowledge(cli) => cli.message(),
            Commands::Finance(cli) => cli.message(),
            Commands::Health(cli) => cli.message(),
        }
    }
}

/// Top-level commands
#[derive(Debug, Parser)]
pub enum Commands {
    /// AI features
    Ai(ai::cli::Cli),
    /// To-do management
    Todo(todo::cli::Cli),
    /// Knowledge management
    Knowledge(knowledge::cli::Cli),
    /// Finance management
    Finance(finance::cli::Cli),
    /// Health tracking
    Health(health::cli::Cli),
}
