use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use shared::protocol::MessageProtocol;

use crate::utils::Priority;


#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub fn message(self) -> Vec<u8> {
        self.command.to_bytes().expect("msgage serialization failed")
    }
}

#[derive(Debug, Subcommand, Serialize, Deserialize)]
pub enum Commands {
    /// Create a new object
    #[command(arg_required_else_help = true)]
    Create(Create),

    /// Update an actual object
    #[command(arg_required_else_help = true)]
    Update(Update),

    /// Delete one object
    #[command(arg_required_else_help = true)]
    Delete(Delete),

    /// Read one or more objects
    #[command(arg_required_else_help = true)]
    Read(Read),
}

impl MessageProtocol for Commands {}

#[derive(Debug, Args, Clone, Serialize, Deserialize)]
struct Create {
    #[arg(short, long)]
    title: String,
    #[arg(short, long)]
    why: Option<String>,
    #[arg(long)]
    how: Option<String>,
    #[arg(short, long)]
    notes: Option<String>,
    #[arg(
        short,
        long,
        default_value_t = Priority::Low,
        default_missing_value = "Low",
        value_enum,
        required = false
    )]
    priority: Priority,
    #[arg(short = 'f', long)]
    horizon: i8,
}
#[derive(Debug, Args, Clone, Serialize, Deserialize)]
struct Delete {
    #[arg(short, long)]
    id: Option<i16>,
    #[arg(short, long)]
    title: Option<String>,
}
#[derive(Debug, Args, Clone, Serialize, Deserialize)]
struct Read {
    #[arg(short, long, default_value_t = true, required = false)]
    all: bool,
    #[arg(short, long, required = false)]
    id: Option<i16>,
    #[arg(short, long, required = false)]
    title: Option<String>,
    #[arg(short, long, value_enum, required = false)]
    priority: Option<Priority>,
}
#[derive(Debug, Args, Clone, Serialize, Deserialize)]
struct Update {
    #[arg(short, long)]
    id: i16,
    #[arg(short, long)]
    title: Option<String>,
    #[arg(short, long)]
    why: Option<String>,
    #[arg(long)]
    how: Option<String>,
    #[arg(short, long)]
    notes: Option<String>,
    #[arg(short, long, value_enum)]
    priority: Option<Priority>,
    #[arg(short = 'f', long)]
    horizon: Option<i8>,
}
