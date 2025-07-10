use super::GoalsFile;
use crate::todo::{
    goals::Goal,
    utils::{ColorWhen, FileSaver, Priority},
};

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct GoalArgs {
    #[command(subcommand)]
    pub command: GoalCommands,
    #[arg(
        long,
        require_equals = true,
        value_name = "WHEN",
        num_args = 0..=1,
        default_value_t = ColorWhen::Auto,
        default_missing_value = "always",
        value_enum,
    )]
    color: ColorWhen,
}

#[derive(Debug, Subcommand)]
pub enum GoalCommands {
    /// Create a new object
    #[command(arg_required_else_help = true)]
    Create(CreateGoal),

    /// Update an actual object
    #[command(arg_required_else_help = true)]
    Update(UpdateGoal),

    /// Delete one object
    #[command(arg_required_else_help = true)]
    Delete(DeleteGoal),

    /// Read one or more objects
    #[command(arg_required_else_help = true)]
    Read(ReadGoal),
}

impl GoalCommands {
    pub fn handle_commands(self) -> i16 {
        match self {
            GoalCommands::Create(args) => args.run(),
            GoalCommands::Update(args) => args.run(),
            GoalCommands::Delete(args) => args.run(),
            GoalCommands::Read(args) => args.run(),
        };
        0
    }
}

#[derive(Debug, Args, Clone)]
pub struct CreateGoal {
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

impl CreateGoal {
    fn run(&self) -> i16 {
        let mut item_saver = GoalsFile::get_or_create();
        let mut item = Goal::new(
            self.title.clone(),
            self.why.clone().unwrap_or_default(),
            self.how.clone().unwrap_or_default(),
            self.notes.clone().unwrap_or_default(),
            self.priority,
            self.horizon,
        );
        let title = item.title.clone();
        let id = item_saver.get_latest_id();
        item.set_id(id);

        item_saver.objects.entry(item.id).or_insert(item);
        item_saver.save_changes();
        println!("New goal {id} - {title} created successfully");
        0
    }
}

#[derive(Debug, Args, Clone)]
pub struct DeleteGoal {
    #[arg(short, long)]
    id: Option<i16>,
    #[arg(short, long)]
    title: Option<String>,
}

impl DeleteGoal {
    fn run(&self) -> i16 {
        let mut goals_file = GoalsFile::get_or_create();
        goals_file.delete(self.id, self.title.clone())
    }
}

#[derive(Debug, Args, Clone)]
pub struct ReadGoal {
    #[arg(short, long, default_value_t = true, required = false)]
    all: bool,
    #[arg(short, long, required = false)]
    id: Option<i16>,
    #[arg(short, long, required = false)]
    title: Option<String>,
    #[arg(short, long, value_enum, required = false)]
    priority: Option<Priority>,
}

impl ReadGoal {
    fn run(&self) -> i16 {
        let mut goals_file = GoalsFile::get_or_create();
        let objs = goals_file.objects().clone();
        goals_file.read(&objs);
        0
    }
}

#[derive(Debug, Args, Clone)]
pub struct UpdateGoal {
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

impl UpdateGoal {
    fn run(self) -> i16 {
        let mut item_saver = GoalsFile::get_or_create();
        item_saver.objects.entry(self.id).and_modify(|item| {
            if let Some(title) = &self.title {
                item.title = title.to_string();
            }
            if let Some(why) = &self.why {
                item.why = why.to_string();
            }
            if let Some(how) = &self.how {
                item.how = how.to_string();
            }
            if let Some(notes) = &self.notes {
                item.notes = notes.to_string();
            }
            if let Some(priority) = &self.priority {
                item.priority = *priority;
            }
            if let Some(horizon) = &self.horizon {
                item.horizon = *horizon;
            }
        });

        item_saver.save_changes();
        0
    }
}
