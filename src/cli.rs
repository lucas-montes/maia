use clap::Parser;

use {ai, knowledge, finance, todo, food};


#[derive(Debug, Parser)]
#[command(name = "Maia")]
#[command(about = "Maia your (my) AI assistant")]
pub struct Cli {
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
            Commands::Food(cli) => cli.message(),
        }
    }
}

#[derive(Debug, Parser)]
pub enum Commands {
    Ai(ai::cli::Cli),
    Todo(todo::cli::Cli),
    Knowledge(knowledge::cli::Cli),
    Finance(finance::cli::Cli),
    Food(food::cli::Cli),
}
