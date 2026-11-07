//! Background due-task desktop notifier.
//!
//! Shaped like `notes_mirror::watch` / `statements::watch` -- a loop spawned
//! from `start_server_with_shutdown` with its own lifetime tied to the
//! server process -- but simpler: there is no filesystem event to react to
//! for "a due time just passed", so a fixed-interval tick is both the
//! trigger and the sweep, rather than an inotify stream with a sweep
//! backstop.
//!
//! Every tick asks the database which not-yet-notified tasks have a due
//! instant at or before now, fires one `notify-send` per task, and marks
//! `tasks.notified_at` in the same pass. That marker is what makes "once per
//! crossing" true: a task already marked is excluded from the next tick's
//! query, and a service restart sees the same marker, so it never replays
//! every already-overdue task as a fresh notification.

use rusqlite::{Connection, params};

use crate::db;
use crate::handlers::agenda::due_instant_ms;
use crate::server::AppState;

/// How often to check for newly-due tasks. `due_time_minutes` only has
/// minute resolution, so there is nothing to gain from polling more often
/// than that.
const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

/// Starts the notifier. Lives for as long as the server process.
pub fn spawn(state: AppState) {
    tokio::spawn(main_loop(state));
}

async fn main_loop(state: AppState) {
    let mut interval = tokio::time::interval(TICK_INTERVAL);
    loop {
        // `interval`'s first tick completes immediately, so a task that
        // crossed its due time while the service was down is caught on
        // startup -- the same "catch up on boot" behaviour the file
        // watchers get from their startup scan.
        interval.tick().await;
        tick(&state).await;
    }
}

async fn tick(state: &AppState) {
    let due = {
        let conn = match state.db.get() {
            Ok(conn) => conn,
            Err(error) => {
                tracing::warn!(error = %error, "due-task notifier: database unavailable");
                return;
            }
        };
        match select_and_mark_due_tasks(&conn, db::server_time_ms()) {
            Ok(due) => due,
            Err(error) => {
                tracing::warn!(error = %error, "due-task notifier: query failed");
                return;
            }
        }
    };
    for task in due {
        send_notification(&task.title);
    }
}

/// A task whose due instant has passed and that has just been marked
/// notified by this call.
#[derive(Debug, Clone, PartialEq)]
pub struct DueTask {
    pub id: String,
    pub title: String,
}

/// Selects every not-yet-notified task whose due instant is at or before
/// `now_ms`, and marks each one's `notified_at` before returning it.
///
/// Marking happens in the same call that selects, not after `notify-send`
/// runs: the caller's job (actually showing the notification) can fail or
/// be skipped in a test without the task becoming selectable again on the
/// next tick, which is what keeps a crossing "fired exactly once" even if
/// `notify-send` itself is flaky.
///
/// `updated_at` is deliberately left untouched -- `notified_at` is
/// server-only (`pull_tasks` selects an explicit column list that omits
/// it), and bumping `updated_at` would make every phone re-pull a task that
/// did not actually change.
pub(crate) fn select_and_mark_due_tasks(
    conn: &Connection,
    now_ms: i64,
) -> anyhow::Result<Vec<DueTask>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, due_date, due_time_minutes FROM tasks \
         WHERE due_date IS NOT NULL AND done = 0 AND deleted_at IS NULL AND notified_at IS NULL",
    )?;
    let candidates: Vec<(String, String, i64, Option<i64>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .filter_map(|r| r.ok())
        .collect();

    let mut due = Vec::new();
    for (id, title, due_date, due_time_minutes) in candidates {
        if due_instant_ms(due_date, due_time_minutes) <= now_ms {
            conn.execute(
                "UPDATE tasks SET notified_at = ?1 WHERE id = ?2",
                params![now_ms, id],
            )?;
            due.push(DueTask { id, title });
        }
    }
    Ok(due)
}

/// Fires one desktop notification. Logged rather than propagated: a single
/// unreachable notification daemon should not stop the tick from marking
/// (and thus not re-trying forever) the other due tasks in the same batch.
fn send_notification(title: &str) {
    match std::process::Command::new("notify-send")
        .arg("Task due")
        .arg(title)
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => {
            tracing::warn!(status = %status, task = %title, "notify-send exited non-zero")
        }
        Err(error) => {
            tracing::warn!(error = %error, task = %title, "cannot invoke notify-send")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::agenda::local_day_start_ms;

    fn insert_task(
        conn: &Connection,
        id: &str,
        title: &str,
        due_date: i64,
        due_time_minutes: Option<i64>,
        notified_at: Option<i64>,
    ) {
        conn.execute(
            "INSERT INTO tasks (id, date, title, done, carry_over, sort_order, due_date, due_time_minutes, notified_at, created_at) \
             VALUES (?1, ?2, ?3, 0, 1, 0, ?2, ?4, ?5, 0)",
            params![id, due_date, title, due_time_minutes, notified_at],
        )
        .unwrap();
    }

    /// A task whose due time has newly passed fires exactly once: the tick
    /// that crosses it returns it and marks it, and the very next tick --
    /// even though the due time is still in the past -- does not return it
    /// again.
    #[test]
    fn a_newly_crossed_due_task_is_returned_exactly_once() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        let today_start = local_day_start_ms(1_800_000_000_000);
        let due_time = today_start + 9 * 60 * 60 * 1000; // 9am

        insert_task(&conn, "t1", "Submit report", today_start, Some(9 * 60), None);

        // Before the due time: not yet due.
        let before = select_and_mark_due_tasks(&conn, due_time - 1).unwrap();
        assert!(before.is_empty(), "must not fire before the due instant");

        // The tick that crosses the due time: fires once.
        let crossing = select_and_mark_due_tasks(&conn, due_time).unwrap();
        assert_eq!(crossing, vec![DueTask { id: "t1".into(), title: "Submit report".into() }]);

        // The next tick, due time still in the past: does not fire again.
        let after = select_and_mark_due_tasks(&conn, due_time + 60_000).unwrap();
        assert!(after.is_empty(), "must not re-fire once notified");
    }

    /// A restart (modelled here as simply calling the selector again on the
    /// same database) with several already-notified, already-overdue tasks
    /// must not replay any of them.
    #[test]
    fn a_restart_does_not_renotify_already_notified_overdue_tasks() {
        let pool = db::init_memory().unwrap();
        let conn = pool.get().unwrap();
        let today_start = local_day_start_ms(1_800_000_000_000);

        use crate::handlers::agenda::DAY_MS;

        // Already overdue and already notified before the "restart".
        insert_task(&conn, "old-1", "Stale reminder one", today_start - 2 * DAY_MS, Some(0), Some(today_start));
        insert_task(&conn, "old-2", "Stale reminder two", today_start - DAY_MS, Some(0), Some(today_start));
        // A genuinely new, never-notified overdue task must still fire.
        insert_task(&conn, "new-1", "Fresh reminder", today_start - DAY_MS, Some(0), None);

        let due = select_and_mark_due_tasks(&conn, today_start + 60_000).unwrap();
        assert_eq!(due, vec![DueTask { id: "new-1".into(), title: "Fresh reminder".into() }]);
    }
}
