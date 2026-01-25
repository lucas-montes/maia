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
        todo!()
    }
}

/// Top-level commands
#[derive(Debug, Parser)]
pub enum Commands {
    /// To-do management
    Todo(todo::cli::Cli),
    /// Knowledge management
    Knowledge(knowledge::cli::Cli),
    /// Command to add information for new food eaten or related to diet
    Food(food::Cli),
}

mod food {
    use std::str::FromStr;

    use serde::{Deserialize, Serialize};
use shared::protocol::MessageProtocol;

    #[derive(Debug, clap::Args)]
    pub struct Cli {
        /// Knowledge command group
        #[command(subcommand)]
        command: Commands,
    }
    impl Cli {
        pub fn message(self) -> Vec<u8> {
            self.command
                .to_bytes()
                .expect("msgage serialization failed")
        }
    }

    #[derive(Debug, Deserialize, Serialize, clap::Subcommand)]
    pub enum Commands {
        Add(Add),
    }

    impl MessageProtocol for Commands {}

    #[derive(Debug, Deserialize, Serialize, clap::Args)]
    pub struct Add {
        meals: Vec<Meal>
    }

    #[derive(Debug, Deserialize, Serialize, Clone)]
    struct Meal {
        name: String,
        amount: f32,
    }

    impl FromStr for Meal {
        type Err = String;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            let parts: Vec<&str> = s.split(',').collect();
            if parts.len() != 2 {
                return Err("Invalid meal format. Use 'name,amount'".to_string());
            }
            let name = parts[0].to_string();
            let amount = parts[1].parse::<f32>().map_err(|_| "Invalid amount".to_string())?;
            Ok(Meal { name, amount })
        }
    }
}
