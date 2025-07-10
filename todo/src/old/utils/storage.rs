use serde::Serialize;
use std::collections::HashMap;
use std::env;
use std::fmt::Debug;
use std::fs::{create_dir, File, OpenOptions};
use std::io::Write;
use std::path::Path;

use serde::Deserialize;
use std::io::BufReader;

pub trait FileSaver: Serialize + std::default::Default
where
    for<'de> Self: Deserialize<'de> + Sized,
{
    type ObjectStored: Debug + Serialize + Clone;

    fn read(&mut self, tasks: &HashMap<i16, Self::ObjectStored>) -> i16 {
        let mut sorted_tasks: Vec<(&i16, &Self::ObjectStored)> = tasks.iter().collect();
        sorted_tasks.sort_by_key(|&(key, _)| *key);
        println!("{}", serde_json::to_string_pretty(&sorted_tasks).unwrap());
        0
    }

    fn delete(&mut self, id: Option<i16>, title: Option<String>) -> i16 {
        let result = match id {
            Some(val) => self.delete_by_id(val),
            None => match title {
                Some(val) => self.delete_by_title(val),
                None => panic!(
                    "Error: Either `id` or `title` must be provided to delete a Task."
                ),
            },
        };

        if result > 0 {
            eprintln!("Error: Task not found.")
        }
        result
    }

    fn delete_by_id(&mut self, id: i16) -> i16 {
        match self.objects().remove(&id) {
            None => 1,
            Some(object) => {
                self.save_changes();
                println!("Deleted Task: {:?}", object);
                0
            },
        }
    }

    fn delete_by_title(&mut self, title: String) -> i16;

    fn get_or_create() -> Self {
        match serde_json::from_reader(BufReader::new(Self::open_or_create_file())) {
            Ok(value) => value,
            Err(err) => {
                println!("get_or_create {:?}", err);
                Self::default()
            },
        }
    }

    fn save_changes(&self) {
        let mut file = Self::open_or_create_file();
        file.set_len(0).unwrap();
        file.write_all(serde_json::to_string_pretty(self).unwrap().as_bytes())
            .expect("something");
    }

    fn open_or_create_file() -> File {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(Self::get_file_path())
        {
            Ok(value) => value,
            Err(e) => panic!("Problem creating the file: {:?}", e),
        }
    }

    fn get_file_path() -> String {
        let folder_path = format!("{}/.todors", home_dir());
        if !Path::new(&folder_path).exists() {
            if let Err(err) = create_dir(&folder_path) {
                panic!("Failed to create folder: {}", err);
            };
        };
        format!("{}/{}.json", folder_path, Self::file_name())
    }

    fn file_name() -> String {
        let entity_name = std::any::type_name::<Self::ObjectStored>()
            .rsplit("::")
            .next()
            .unwrap()
            .to_lowercase();
        format!("{}s", entity_name)
    }

    fn get_latest_id(&mut self) -> i16 {
        match self.objects().keys().max() {
            Some(result) => *result + 1,
            None => 1,
        }
    }

    fn objects(&mut self) -> &mut HashMap<i16, Self::ObjectStored>;

    fn get_all() -> Vec<Self::ObjectStored> {
        Self::get_or_create().objects().values().cloned().collect()
    }
}

pub fn home_dir() -> String {
    match env::var("HOME") {
        Ok(value) => value,
        Err(err) => panic!("Failed to retrieve home directory, {:?}", err),
    }
}
