use super::Task;
use crate::todo::utils::FileSaver;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct TasksFile {
    pub objects: HashMap<i16, Task>,
    pub seen: HashSet<i16>,
    pub last_check: String,
}

impl TasksFile {
    pub fn add_many(tasks: Vec<Task>) -> i16 {
        let mut manager = Self::get_or_create();
        for mut task in tasks {
            task.id = manager.get_latest_id();
            //println!("{task:?}");
            manager.objects.entry(task.id).or_insert(task);
        }
        //manager.save_changes();
        0
    }
}

impl FileSaver for TasksFile {
    type ObjectStored = Task;

    fn delete_by_title(&mut self, title: String) -> i16 {
        match self.objects.iter().find(|(_, t)| t.title == title) {
            Some((id, _)) => self.delete_by_id(*id),
            None => 1,
        }
    }

    fn objects(&mut self) -> &mut HashMap<i16, Task> {
        &mut self.objects
    }
}
