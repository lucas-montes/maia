use std::{collections::HashSet, thread::sleep, time};

use crate::todo::tasks::{Task, TasksFile};
use crate::todo::utils::FileSaver;
use crate::todo::ProjectsFile;

use chrono::Local;
use chrono::{prelude::*, Duration};

pub async fn clean_seen() -> i16 {
    let now: NaiveDateTime = Local::now().naive_local();
    let mut task_file = TasksFile::get_or_create();
    let last_date = get_last_date(&task_file.last_check).await;
    if (last_date - now) < Duration::hours(20) {
        task_file.seen = HashSet::new();
        task_file.last_check = now.to_string();
        task_file.save_changes();
    };
    1
}

async fn get_last_date(last_check: &str) -> NaiveDateTime {
    match NaiveDateTime::parse_from_str(last_check, "%Y-%m-%d %H:%M:%S%.9f") {
        Ok(value) => value,
        Err(err) => panic!("oupsi, {:?}", err),
    }
}

pub async fn check_all() {
    let now: NaiveDateTime = Local::now().naive_local();
    let mut task_file = TasksFile::get_or_create();
    let all_tasks = task_file.objects();
    for task in get_all_to_do_tasks(&get_all_active_tasks(all_tasks).await, now).await {
        if task_file.seen.contains(&task.id) {
            continue;
        }
        task.to_notification().await;
        task_file.seen.insert(task.id);
        sleep(time::Duration::from_secs(5));
    }

    task_file.save_changes()
}

async fn get_all_to_do_tasks(
    actives_tasks: &Vec<Task>,
    now: NaiveDateTime,
) -> Vec<&Task> {
    let mut to_do_tasks: Vec<&Task> = vec![];
    for task in actives_tasks {
        if !task.done && (task.is_one_off() || check_repetitif(task, now).await) {
            to_do_tasks.push(task);
        }
    }
    to_do_tasks
}

async fn get_all_active_tasks(
    all_tasks: &mut std::collections::HashMap<i16, Task>,
) -> Vec<Task> {
    for project in ProjectsFile::get_all() {
        if project.in_stand_by() {
            for task in project.tasks.iter() {
                all_tasks.remove(task);
            }
        }
    }
    for (k, v) in all_tasks.clone().into_iter() {
        match v.after {
            Some(task_id) => {
                let in_stand_by = task_id == 0;
                let previous_unfinished = match all_tasks.get(&task_id) {
                    Some(prev_task) => !prev_task.done,
                    None => false,
                };
                if in_stand_by || previous_unfinished {
                    all_tasks.remove(&k);
                };
            },
            None => continue,
        };
    }
    all_tasks.values().cloned().collect()
}

async fn check_repetitif(task: &Task, now: NaiveDateTime) -> bool {
    task.days
        .iter()
        .any(|day| day.get_digit() as u32 == Local::now().weekday().number_from_monday())
        && is_due_today(&task.start, now).await
}

async fn is_due_today(date_str: &str, now: NaiveDateTime) -> bool {
    if let Ok(date) = NaiveTime::parse_from_str(date_str, "%H:%M") {
        return date.hour() == now.hour() && date.minute() == now.minute();
    }
    if let Ok(datetime) = NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M") {
        return datetime == now;
    }
    if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        return date == now.date();
    }
    false
}
