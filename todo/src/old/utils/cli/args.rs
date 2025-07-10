use super::ToolArgs;
use crate::todo::goals::GoalArgs;
use crate::todo::projects::ProjectArgs;
use crate::todo::tasks::TaskArgs;

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
pub struct ToDoCli {
    #[command(subcommand)]
    pub command: Commands,
}

impl ToDoCli {
    pub async fn handle_args(self) -> i16 {
        match self.command {
            Commands::Goal(args) => args.command.handle_commands(),
            Commands::Project(args) => args.command.handle_commands(),
            Commands::Task(args) => args.command.handle_commands(),
            Commands::Tool(args) => args.command.handle_commands().await,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Hanlde the goals
    #[command(arg_required_else_help = true)]
    Goal(GoalArgs),

    /// Handle the tasks
    #[command(arg_required_else_help = true)]
    Project(ProjectArgs),

    /// Handle the tasks
    #[command(arg_required_else_help = true)]
    Task(TaskArgs),

    /// Different commands to execute
    #[command(arg_required_else_help = true)]
    Tool(ToolArgs),
}
