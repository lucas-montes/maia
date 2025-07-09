use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "Maia", version = "0.0.1")]
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
    Model(ModelCli),
    Todo(ToDoCli)
}

#[derive(Debug, Args)]
struct ModelCli {
    #[command(subcommand)]
    command: ModelCommands,
}

#[derive(Debug, Subcommand)]
enum ModelCommands {
    Ask {
        #[arg(short, long, help = "The question to ask")]
        question: String,
    },
    Chat {
        #[arg(short, long, help = "The initial message for the chat")]
        initial_message: String,
    },
}


#[derive(Debug, Args)]
pub struct ToDoCli {
    #[command(subcommand)]
    command: ToDoCommands,
}


#[derive(Debug, Subcommand)]
pub enum ToDoCommands {
    #[command(arg_required_else_help = true)]
    Create,
    #[command(arg_required_else_help = true)]
    Delete,
    #[command(arg_required_else_help = true)]
    Update,
    #[command(arg_required_else_help = true)]
    Read,
}
