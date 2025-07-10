use clap::{Args, Subcommand};

use crate::utils::{Day, Priority};

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,

}

#[derive(Debug, Subcommand)]
enum Commands {
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

#[derive(Debug, Args, Clone)]
struct Create {
    #[arg(short, long)]
    title: String,
    #[arg(short, long)]
    description: Option<String>,
    #[arg(short, long, value_name = "Start date")]
    start: Option<String>,
    #[arg(short, long, value_name = "End date")]
    end: Option<String>,
    #[arg(
        short,
        long,
        default_value_t = Priority::Low,
        default_missing_value = "Low",
        value_enum,
        required = false
    )]
    priority: Priority,
    #[arg(
        short,
        long,
        default_value_t = true,
        required = false,
        help = "Is a  meant to be done only once"
    )]
    one_off: bool,
    #[arg(long, num_args = 0..=7, value_enum, required = false)]
    days: Option<Vec<Day>>,
    #[arg(short, long)]
    after: Option<i16>,
}

#[derive(Debug, Args, Clone)]
struct Read {
    #[arg(short, long, default_value_t = true, required = false)]
    all: bool,
    #[arg(short, long, required = false)]
    id: Option<i16>,
    #[arg(short, long, required = false)]
    title: Option<String>,
    #[arg(short, long, required = false)]
    start: Option<String>,
    #[arg(short, long, required = false)]
    end: Option<String>,
    #[arg(short, long, value_enum, required = false)]
    priority: Option<Priority>,
    #[arg(long, num_args = 0..=7, value_enum, required=false)]
    days: Option<Vec<Day>>,
    #[arg(short, long, required = false)]
    done: Option<bool>,
}

#[derive(Debug, Args, Clone)]
struct Update {
    #[arg(short, long)]
    id: i16,
    #[arg(short, long)]
    title: Option<String>,
    #[arg(short, long)]
    description: Option<String>,
    #[arg(short, long)]
    start: Option<String>,
    #[arg(short, long)]
    end: Option<String>,
    #[arg(short, long, value_enum)]
    priority: Option<Priority>,
    #[arg(short, long)]
    done: Option<bool>,
    #[arg(long, num_args = 0..=7, value_enum)]
    days: Option<Vec<Day>>,
    #[arg(short, long, default_value = "0", required = false)]
    after: Option<i16>,
}

#[derive(Debug, Args, Clone)]
struct Delete {
    #[arg(short, long)]
    id: Option<i16>,
    #[arg(short, long)]
    title: Option<String>,
}
