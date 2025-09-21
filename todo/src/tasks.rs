use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};
use shared::protocol::MessageProtocol;

use crate::utils::{Day, Priority};

/// Task management CLI
#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Cli {
    /// Task command group
    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    pub fn message(self) -> Vec<u8> {
        self.command.to_bytes().expect("msgage serialization failed")
    }
}

/// Task commands
#[derive(Debug, Subcommand, Serialize, Deserialize)]
pub enum Commands {
    /// Create a new task
    #[command(arg_required_else_help = true)]
    Create(Create),
    /// Update an existing task
    #[command(arg_required_else_help = true)]
    Update(Update),
    /// Delete a task
    #[command(arg_required_else_help = true)]
    Delete(Delete),
    /// Read one or more tasks
    #[command(arg_required_else_help = true)]
    Read(Read),
}

impl MessageProtocol for Commands {}

#[derive(Debug, Args, Clone, Serialize, Deserialize)]
struct Create {
    /// Title of the task
    #[arg(short, long)]
    title: String,
    /// Task description
    #[arg(short, long)]
    description: Option<String>,
    /// Start date
    #[arg(short, long, value_name = "Start date")]
    start: Option<String>,
    /// End date
    #[arg(short, long, value_name = "End date")]
    end: Option<String>,
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
    /// Is this a one-off task?
    #[arg(
        short,
        long,
        default_value_t = true,
        required = false,
        help = "Is a task meant to be done only once"
    )]
    one_off: bool,
    /// Days of the week
    #[arg(long, num_args = 0..=7, value_enum, required = false)]
    days: Option<Vec<Day>>,
    /// ID of the task this depends on
    #[arg(short, long)]
    after: Option<i16>,
}

#[derive(Debug, Args, Clone, Serialize, Deserialize)]
struct Read {
    /// Read all tasks
    #[arg(short, long, default_value_t = true, required = false)]
    all: bool,
    /// Task ID
    #[arg(short, long, required = false)]
    id: Option<i16>,
    /// Task title
    #[arg(short, long, required = false)]
    title: Option<String>,
    /// Start date
    #[arg(short, long, required = false)]
    start: Option<String>,
    /// End date
    #[arg(short, long, required = false)]
    end: Option<String>,
    /// Filter by priority
    #[arg(short, long, value_enum, required = false)]
    priority: Option<Priority>,
    /// Filter by days
    #[arg(long, num_args = 0..=7, value_enum, required=false)]
    days: Option<Vec<Day>>,
    /// Filter by completion status
    #[arg(short, long, required = false)]
    done: Option<bool>,
}

#[derive(Debug, Args, Clone, Serialize, Deserialize)]
struct Update {
    /// Task ID
    #[arg(short, long)]
    id: i16,
    /// Task title
    #[arg(short, long)]
    title: Option<String>,
    /// Task description
    #[arg(short, long)]
    description: Option<String>,
    /// Start date
    #[arg(short, long)]
    start: Option<String>,
    /// End date
    #[arg(short, long)]
    end: Option<String>,
    /// Priority
    #[arg(short, long, value_enum)]
    priority: Option<Priority>,
    /// Completion status
    #[arg(short, long)]
    done: Option<bool>,
    /// Days of the week
    #[arg(long, num_args = 0..=7, value_enum)]
    days: Option<Vec<Day>>,
    /// ID of the task this depends on
    #[arg(short, long, default_value = "0", required = false)]
    after: Option<i16>,
}

#[derive(Debug, Args, Clone, Serialize, Deserialize)]
struct Delete {
    /// Task ID
    #[arg(short, long)]
    id: Option<i16>,
    /// Task title
    #[arg(short, long)]
    title: Option<String>,
}
