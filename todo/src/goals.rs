use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use shared::protocol::MessageProtocol;

use crate::utils::Priority;

/// Goal management CLI
#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Cli {
    /// Goal command group
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub fn message(self) -> Vec<u8> {
        self.command.to_bytes().expect("msgage serialization failed")
    }
}

/// Goal commands
#[derive(Debug, Subcommand, Serialize, Deserialize)]
pub enum Commands {
    /// Create a new goal
    #[command(arg_required_else_help = true)]
    Create(Create),
    /// Update an existing goal
    #[command(arg_required_else_help = true)]
    Update(Update),
    /// Delete a goal
    #[command(arg_required_else_help = true)]
    Delete(Delete),
    /// Read one or more goals
    #[command(arg_required_else_help = true)]
    Read(Read),
}

impl MessageProtocol for Commands {}

#[derive(Debug, Args, Clone, Serialize, Deserialize)]
struct Create {
    /// Title of the goal
    #[arg(short, long)]
    title: String,
    /// Why this goal?
    #[arg(short, long)]
    why: Option<String>,
    /// How to achieve this goal
    #[arg(long)]
    how: Option<String>,
    /// Additional notes
    #[arg(short, long)]
    notes: Option<String>,
    /// Priority
    #[arg(
        short,
        long,
        default_value_t = Priority::Low,
        default_missing_value = "Low",
        value_enum,
        required = false
    )]
    priority: Priority,
    /// Horizon (time frame)
    #[arg(short = 'f', long)]
    horizon: i8,
}

#[derive(Debug, Args, Clone, Serialize, Deserialize)]
struct Delete {
    /// Goal ID
    #[arg(short, long)]
    id: Option<i16>,
    /// Goal title
    #[arg(short, long)]
    title: Option<String>,
}

#[derive(Debug, Args, Clone, Serialize, Deserialize)]
struct Read {
    /// Read all goals
    #[arg(short, long, default_value_t = true, required = false)]
    all: bool,
    /// Goal ID
    #[arg(short, long, required = false)]
    id: Option<i16>,
    /// Goal title
    #[arg(short, long, required = false)]
    title: Option<String>,
    /// Filter by priority
    #[arg(short, long, value_enum, required = false)]
    priority: Option<Priority>,
}

#[derive(Debug, Args, Clone, Serialize, Deserialize)]
struct Update {
    /// Goal ID
    #[arg(short, long)]
    id: i16,
    /// Goal title
    #[arg(short, long)]
    title: Option<String>,
    /// Why this goal?
    #[arg(short, long)]
    why: Option<String>,
    /// How to achieve this goal
    #[arg(long)]
    how: Option<String>,
    /// Additional notes
    #[arg(short, long)]
    notes: Option<String>,
    /// Priority
    #[arg(short, long, value_enum)]
    priority: Option<Priority>,
    /// Horizon (time frame)
    #[arg(short = 'f', long)]
    horizon: Option<i8>,
}
