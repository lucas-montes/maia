use super::ProjectsFile;
use crate::todo::{
    utils::{ColorWhen, FileSaver, Priority},
    Project,
};

use chrono::Local;
use clap::{Args, Subcommand};

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct ProjectArgs {
    #[command(subcommand)]
    pub command: ProjectCommands,
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
pub enum ProjectCommands {
    /// Create a new object
    #[command(arg_required_else_help = true)]
    Create(CreateProject),

    /// Update an actual object
    #[command(arg_required_else_help = true)]
    Update(UpdateProject),

    /// Delete one object
    #[command(arg_required_else_help = true)]
    Delete(DeleteProject),

    /// Read one or more objects
    #[command(arg_required_else_help = true)]
    Read(ReadProject),
}

impl ProjectCommands {
    pub fn handle_commands(self) -> i16 {
        match self {
            ProjectCommands::Create(args) => args.run(),
            ProjectCommands::Update(args) => args.run(),
            ProjectCommands::Delete(args) => args.run(),
            ProjectCommands::Read(args) => args.run(),
        };
        0
    }
}

#[derive(Debug, Args, Clone)]
pub struct CreateProject {
    #[arg(short, long)]
    title: String,
    #[arg(short, long)]
    description: Option<String>,
    #[arg(short, long)]
    start: Option<String>,
    #[arg(short, long)]
    end: Option<String>,
    #[arg(short, long)]
    notes: Option<String>,
    #[arg(
        short,
        long,
        require_equals = true,
        default_value_t = Priority::Low,
        default_missing_value = "Low",
        value_enum,
        required = false
    )]
    priority: Priority,
}

impl CreateProject {
    fn run(&self) -> i16 {
        let mut item_saver = ProjectsFile::get_or_create();

        let start = if self.start.as_ref().is_some_and(|f| f.eq("now")) {
            Local::now().naive_local().to_string()
        } else {
            self.start.clone().unwrap()
        };
        let mut item = Project::new(
            self.title.clone(),
            self.description.clone().unwrap_or_default(),
            start,
            self.end.clone().unwrap_or_default(),
            self.notes.clone().unwrap_or_default(),
            self.priority,
        );
        let title = item.title.clone();
        let id = item_saver.get_latest_id();
        item.set_id(id);

        item_saver.objects.entry(item.id).or_insert(item);
        item_saver.save_changes();
        println!("New project {id} - {title} created successfully");
        0
    }
}

#[derive(Debug, Args, Clone)]
pub struct DeleteProject {
    #[arg(short, long)]
    id: Option<i16>,
    #[arg(short, long)]
    title: Option<String>,
}

impl DeleteProject {
    fn run(&self) -> i16 {
        let mut projects_file = ProjectsFile::get_or_create();
        projects_file.delete(self.id, self.title.clone())
    }
}

#[derive(Debug, Args, Clone)]
pub struct ReadProject {
    #[arg(short, long, default_value_t = true, required = false)]
    all: bool,
    #[arg(short, long, required = false)]
    id: Option<i16>,
    #[arg(short, long, required = false)]
    title: Option<String>,
    #[arg(short, long, value_enum, required = false)]
    priority: Option<Priority>,
}

impl ReadProject {
    fn run(&self) -> i16 {
        let mut projects_file = ProjectsFile::get_or_create();
        let objs = projects_file.objects().clone();
        projects_file.read(&objs);
        0
    }
}

#[derive(Debug, Args, Clone)]
pub struct UpdateProject {
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
    #[arg(short, long)]
    notes: Option<String>,
    #[arg(short, long, value_enum)]
    priority: Option<Priority>,
    #[arg(short, long)]
    accomplished: Option<bool>,
}

impl UpdateProject {
    fn run(self) -> i16 {
        let mut item_saver = ProjectsFile::get_or_create();
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
            if let Some(notes) = &self.notes {
                item.notes = notes.to_string();
            }
            if let Some(priority) = &self.priority {
                item.priority = *priority;
            }
            if let Some(accomplished) = &self.accomplished {
                item.accomplished = *accomplished;
            }
        });

        item_saver.save_changes();
        0
    }
}
