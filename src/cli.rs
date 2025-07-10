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

    pub fn message(&self) -> &[u8] {
        "self.message".as_bytes()
    }
}

#[derive(Debug, Parser)]
enum Commands {
    Ai(ai::cli::Cli),
    Todo(todo::cli::Cli),
    Knowledge(knowledge::cli::Cli),
    Finance(finance::cli::Cli),
    Food(food::cli::Cli),
}
