use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Add(Add)
}

#[derive(Debug, Args)]
struct Add{
    file: PathBuf,
    symlink: bool
}
