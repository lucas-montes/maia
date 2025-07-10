use clap::{Args, Subcommand, ValueEnum};

use crate::todo::goals::GoalsFile;
use crate::todo::projects::ProjectsFile;

use crate::todo::utils::{
    check_all, clean_seen, dialog::DialogArgs, notify, RelationAction,
};

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct ToolArgs {
    #[command(subcommand)]
    pub command: ToolCommands,
}

#[derive(Debug, Subcommand)]
pub enum ToolCommands {
    /// Just to test the notification
    Notify,
    Clean,
    Dialog(DialogArgs),
    /// Looks for all the tasks that aren't done, and looks for the ones that are due now.
    /// Then for each of this tasks it launch a notification.
    #[command(arg_required_else_help = true)]
    Check(CheckArgs),

    #[command(arg_required_else_help = true)]
    Relate(RelateArgs),
}

impl ToolCommands {
    pub async fn handle_commands(&self) -> i16 {
        match self {
            ToolCommands::Clean => clean_seen().await,
            ToolCommands::Notify => {
                notify("Cool this test", "We are testing it", "dialog-error").await
            },
            ToolCommands::Dialog(args) => args.run().await,
            ToolCommands::Check(args) => args.run().await,
            ToolCommands::Relate(args) => args.run().await,
        }
    }
}

#[derive(Debug, Args, Clone)]
pub struct CheckArgs {
    #[arg(
        short,
        long,
        default_value_t = CheckOptions::All,
        default_missing_value = "All",
        value_enum,
        required = false
    )]
    work: CheckOptions,
}

impl CheckArgs {
    async fn run(&self) -> i16 {
        match &self.work {
            // CheckOptions::Goals => {check_goals(&mut GoalsFile::get_or_create()).await;},
            // CheckOptions::Projects => {check_projects(&mut ProjectsFile::get_or_create(), now).await;},
            // CheckOptions::Tasks => {check_tasks(&mut TasksFile::get_or_create(), now).await;},
            CheckOptions::All => check_all().await,
            _ => todo!("Not ready yet"),
        };
        0
    }
}

#[derive(ValueEnum, Clone, Debug)]
pub enum CheckOptions {
    Goals,
    Projects,
    Tasks,
    All,
}

#[derive(Debug, Args, Clone)]
pub struct RelateArgs {
    #[arg(short, long, value_enum)]
    from: Models,
    #[arg(short, long)]
    id: i16,
    #[arg(short, long)]
    to: i16,
    #[arg(short, long, value_enum)]
    action: RelationAction,
}

impl RelateArgs {
    async fn run(&self) -> i16 {
        match &self.from {
            Models::Projects => {
                GoalsFile::handle_relationships(self.id, self.to, &self.action).await
            },
            Models::Tasks => {
                ProjectsFile::handle_relationships(self.id, self.to, &self.action).await
            },
        }
    }
}

#[derive(ValueEnum, Clone, Debug)]
pub enum Models {
    Projects,
    Tasks,
}
