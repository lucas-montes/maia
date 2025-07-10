use super::Project;
use crate::todo::{
    tasks::TasksFile,
    utils::{FileSaver, RelationAction},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct ProjectsFile {
    pub objects: HashMap<i16, Project>,
}

impl ProjectsFile {
    pub async fn handle_relationships(
        from: i16,
        to: i16,
        action: &RelationAction,
    ) -> i16 {
        let mut objs = Self::get_or_create();
        let project = objs.objects().get_mut(&to).unwrap();
        match &action {
            RelationAction::Add => project.tasks.insert(from),
            RelationAction::Remove => project.tasks.remove(&from),
        };
        Self::update_hours_per_week(project).await;
        objs.save_changes();
        0
    }

    async fn update_hours_per_week(project: &mut Project) -> i16 {
        let tasks = TasksFile::get_or_create().objects;
        project.hours_per_week += project
            .tasks
            .iter()
            .map(|t| tasks.get(t).unwrap().duration())
            .sum::<f64>();
        0
    }
}

impl FileSaver for ProjectsFile {
    type ObjectStored = Project;

    fn delete_by_title(&mut self, title: String) -> i16 {
        match self.objects.iter().find(|(_, t)| t.title == title) {
            Some((id, _)) => self.delete_by_id(*id),
            None => 1,
        }
    }

    fn objects(&mut self) -> &mut HashMap<i16, Project> {
        &mut self.objects
    }
}
