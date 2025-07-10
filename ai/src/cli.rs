use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Chat(Chat),
    OneShot {message: String}
}

#[derive(Debug, Args)]
struct Chat{
    message: String,
}
