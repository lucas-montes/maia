use super::{Task, TasksFile};
use crate::todo::utils::{ColorWhen, Day, FileSaver, Priority};

use clap::{Args, Subcommand};

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct TaskArgs {
    #[command(subcommand)]
    pub command: TaskCommands,
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
pub enum TaskCommands {
    /// Create a new object
    #[command(arg_required_else_help = true)]
    Create(CreateTask),

    /// Update an actual object
    #[command(arg_required_else_help = true)]
    Update(UpdateTask),

    /// Delete one object
    #[command(arg_required_else_help = true)]
    Delete(DeleteTask),

    /// Read one or more objects
    #[command(arg_required_else_help = true)]
    Read(ReadTask),
}

impl TaskCommands {
    pub fn handle_commands(self) -> i16 {
        match self {
            TaskCommands::Create(args) => args.run(),
            TaskCommands::Update(args) => args.run(),
            TaskCommands::Delete(args) => args.run(),
            TaskCommands::Read(args) => args.run(),
        };
        0
    }
}

#[derive(Debug, Args, Clone)]
pub struct CreateTask {
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
        help = "Is a task meant to be done only once"
    )]
    one_off: bool,
    #[arg(long, num_args = 0..=7, value_enum, required = false)]
    days: Option<Vec<Day>>,
    #[arg(short, long)]
    after: Option<i16>,
}

impl CreateTask {
    fn run(&self) -> i16 {
        let mut item_saver = TasksFile::get_or_create();
        let mut item = Task::new(
            self.title.clone(),
            self.description.clone().unwrap_or_default(),
            self.start.clone().unwrap_or_default(),
            self.end.clone().unwrap_or_default(),
            self.priority,
            self.after,
            self.days.clone().unwrap_or_default(),
        );
        let title = item.title.clone();
        let id = item_saver.get_latest_id();
        item.set_id(id);

        item_saver.objects.entry(item.id).or_insert(item);
        item_saver.save_changes();
        println!("New Task {id} - {title} created successfully");
        0
    }
}

#[derive(Debug, Args, Clone)]
pub struct ReadTask {
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

impl ReadTask {
    fn run(&self) -> i16 {
        let mut todos_file = TasksFile::get_or_create();
        let tasks = todos_file.objects();
        let tasks_to_show: std::collections::HashMap<i16, super::Task> =
            if let Some(v) = self.done {
                tasks
                    .iter()
                    .filter(|&(_, task)| task.done == v)
                    .map(|(&k, v)| (k, v.clone()))
                    .collect()
            } else {
                tasks.clone()
            };
        todos_file.read(&tasks_to_show);
        0
    }
}

#[derive(Debug, Args, Clone)]
pub struct UpdateTask {
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

impl UpdateTask {
    fn run(self) -> i16 {
        let mut item_saver = TasksFile::get_or_create();
        item_saver.objects.entry(self.id).and_modify(|item| {
            if let Some(title) = &self.title {
                item.title = title.to_string();
            }
            if let Some(description) = &self.description {
                item.description = description.to_string();
            }
            if let Some(start) = &self.start {
                item.start = start.to_string();
            }
            if let Some(end) = &self.end {
                item.end = end.to_string();
            }
            if let Some(priority) = &self.priority {
                item.priority = *priority;
            }
            if let Some(done) = &self.done {
                item.done = *done;
            }
            if let Some(days) = &self.days {
                item.days = days.to_owned();
            }
        });

        item_saver.save_changes();
        0
    }
}

#[derive(Debug, Args, Clone)]
pub struct DeleteTask {
    #[arg(short, long)]
    id: Option<i16>,
    #[arg(short, long)]
    title: Option<String>,
}

impl DeleteTask {
    fn run(&self) -> i16 {
        let mut todos_file = TasksFile::get_or_create();
        todos_file.delete(self.id, self.title.clone())
    }
}
