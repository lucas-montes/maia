//! `GET /agenda` -- what needs attention right now.
//!
//! Waybar's whole integration is one authenticated poll of this endpoint:
//! overdue and due-today tasks, plus the next upcoming "meeting" (there is no
//! calendar sync yet, so a time-blocked task -- one with `start_time_minutes`
//! set -- stands in for it). Nothing here mutates state; the due-task
//! notifier in [`crate::notifier`] is the only thing that writes
//! `tasks.notified_at`.

use axum::{extract::State, response::IntoResponse, Json};
use rusqlite::Connection;
use serde::Serialize;

use crate::{db, server::AppState};

/// Milliseconds in a day. `due_date` / `date` are stored as local-midnight
/// epoch ms (the client's own `DateTime(y, m, d)`), so this is both the
/// width of "today" and the fallback span for a date-only deadline.
pub(crate) const DAY_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Serialize, PartialEq)]
pub struct AgendaTask {
    pub id: String,
    pub title: String,
    #[serde(rename = "dueDate")]
    pub due_date: i64,
    #[serde(rename = "dueTimeMinutes")]
    pub due_time_minutes: Option<i64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct NextMeeting {
    pub id: String,
    pub title: String,
    pub date: i64,
    #[serde(rename = "startTimeMinutes")]
    pub start_time_minutes: i64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct AgendaResponse {
    pub overdue: Vec<AgendaTask>,
    #[serde(rename = "dueToday")]
    pub due_today: Vec<AgendaTask>,
    #[serde(rename = "nextMeeting")]
    pub next_meeting: Option<NextMeeting>,
}

pub async fn get_agenda(State(state): State<AppState>) -> impl IntoResponse {
    let now_ms = db::server_time_ms();
    let conn = state.db.get().unwrap();
    Json(build_agenda(&conn, now_ms))
}

/// Instant (epoch ms) a due date is actually due at.
///
/// A date-only deadline (`due_time_minutes` unset) is due for the *whole*
/// local day it names -- it becomes overdue only once that day has fully
/// elapsed, not the instant midnight ticks over. A task due "tomorrow" with
/// no time attached should still read as due *today* at 23:59, not overdue
/// at 00:00.
pub(crate) fn due_instant_ms(due_date: i64, due_time_minutes: Option<i64>) -> i64 {
    match due_time_minutes {
        Some(minutes) => due_date + minutes * 60_000,
        None => due_date + DAY_MS - 1,
    }
}

/// Local midnight for the day `now_ms` falls in, in epoch ms.
///
/// `due_date` / `date` are anchored to local midnight on the client, so
/// "today" has to be computed in the same local timezone the server is
/// running in -- a day boundary taken in UTC would shift by the server's
/// offset and either drop late-night due tasks from "due today" too early or
/// hold onto them a day too long.
pub(crate) fn local_day_start_ms(now_ms: i64) -> i64 {
    let now = chrono::DateTime::from_timestamp_millis(now_ms).unwrap_or_default();
    let local = now.with_timezone(&chrono::Local);
    local
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .and_then(|naive| naive.and_local_timezone(chrono::Local).single())
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(now_ms)
}

/// Builds the agenda for `now_ms` from `conn`. Split out from the handler so
/// tests can seed a database and call this directly without going through
/// HTTP.
pub(crate) fn build_agenda(conn: &Connection, now_ms: i64) -> AgendaResponse {
    let day_start = local_day_start_ms(now_ms);
    let day_end = day_start + DAY_MS;

    let mut overdue = Vec::new();
    let mut due_today = Vec::new();

    let mut stmt = conn
        .prepare(
            "SELECT id, title, due_date, due_time_minutes FROM tasks \
             WHERE due_date IS NOT NULL AND done = 0 AND deleted_at IS NULL",
        )
        .unwrap();
    let rows: Vec<(String, String, i64, Option<i64>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    for (id, title, due_date, due_time_minutes) in rows {
        let instant = due_instant_ms(due_date, due_time_minutes);
        let task = AgendaTask {
            id,
            title,
            due_date,
            due_time_minutes,
        };
        if instant < now_ms {
            overdue.push(task);
        } else if due_date >= day_start && due_date < day_end {
            due_today.push(task);
        }
    }
    overdue.sort_by_key(|t| due_instant_ms(t.due_date, t.due_time_minutes));
    due_today.sort_by_key(|t| due_instant_ms(t.due_date, t.due_time_minutes));

    let next_meeting = find_next_meeting(conn, now_ms);

    AgendaResponse {
        overdue,
        due_today,
        next_meeting,
    }
}

/// Earliest not-done, time-blocked task starting at or after `now_ms`.
///
/// Approximates "next meeting" until real calendar sync exists: any task
/// with `start_time_minutes` set is treated as a meeting-shaped block on its
/// `date`.
fn find_next_meeting(conn: &Connection, now_ms: i64) -> Option<NextMeeting> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, date, start_time_minutes FROM tasks \
             WHERE start_time_minutes IS NOT NULL AND done = 0 AND deleted_at IS NULL",
        )
        .unwrap();
    let rows: Vec<(String, String, i64, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    rows.into_iter()
        .filter_map(|(id, title, date, start_time_minutes)| {
            let start = date + start_time_minutes * 60_000;
            (start >= now_ms).then(|| {
                (
                    start,
                    NextMeeting {
                        id,
                        title,
                        date,
                        start_time_minutes,
                    },
                )
            })
        })
        .min_by_key(|(start, _)| *start)
        .map(|(_, meeting)| meeting)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn conn_with_tasks() -> r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager> {
        let pool = crate::db::init_memory().unwrap();
        pool.get().unwrap()
    }

    fn insert_task(
        conn: &Connection,
        id: &str,
        title: &str,
        due_date: Option<i64>,
        due_time_minutes: Option<i64>,
        date: i64,
        start_time_minutes: Option<i64>,
        done: i64,
    ) {
        conn.execute(
            "INSERT INTO tasks (id, date, title, done, carry_over, sort_order, due_date, due_time_minutes, start_time_minutes, created_at) \
             VALUES (?1, ?2, ?3, ?4, 1, 0, ?5, ?6, ?7, 0)",
            params![id, date, title, done, due_date, due_time_minutes, start_time_minutes],
        )
        .unwrap();
    }

    #[test]
    fn overdue_and_due_today_are_split_correctly() {
        let conn = conn_with_tasks();
        let today_start = local_day_start_ms(1_800_000_000_000); // fixed instant, any timezone
        let now_ms = today_start + 12 * 60 * 60 * 1000; // noon today

        // Overdue: due yesterday at noon.
        insert_task(
            &conn,
            "overdue-1",
            "Pay rent",
            Some(today_start - DAY_MS),
            Some(12 * 60),
            today_start - DAY_MS,
            None,
            0,
        );
        // Overdue: due today, but at 6am -- already passed relative to noon.
        insert_task(
            &conn,
            "overdue-2",
            "Standup",
            Some(today_start),
            Some(6 * 60),
            today_start,
            None,
            0,
        );
        // Due today: later today (11pm), not yet passed.
        insert_task(
            &conn,
            "today-1",
            "Ship the report",
            Some(today_start),
            Some(23 * 60),
            today_start,
            None,
            0,
        );
        // Due today: date-only deadline (no time attached) -- due for the
        // whole day, so not overdue even though "today" started 12h ago.
        insert_task(
            &conn,
            "today-2",
            "Water the plants",
            Some(today_start),
            None,
            today_start,
            None,
            0,
        );
        // Due tomorrow: must not appear anywhere.
        insert_task(
            &conn,
            "future-1",
            "Renew passport",
            Some(today_start + DAY_MS),
            Some(9 * 60),
            today_start + DAY_MS,
            None,
            0,
        );
        // Done tasks never appear even if overdue.
        insert_task(
            &conn,
            "done-1",
            "Already finished",
            Some(today_start - DAY_MS),
            Some(9 * 60),
            today_start - DAY_MS,
            None,
            1,
        );

        let agenda = build_agenda(&conn, now_ms);

        let overdue_ids: Vec<&str> = agenda.overdue.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(overdue_ids, vec!["overdue-1", "overdue-2"]);

        let today_ids: Vec<&str> = agenda.due_today.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(today_ids, vec!["today-1", "today-2"]);
    }

    #[test]
    fn empty_lists_are_returned_rather_than_an_error_when_nothing_is_due() {
        let conn = conn_with_tasks();
        let agenda = build_agenda(&conn, 1_800_000_000_000);
        assert!(agenda.overdue.is_empty());
        assert!(agenda.due_today.is_empty());
        assert!(agenda.next_meeting.is_none());
    }

    #[test]
    fn next_meeting_picks_the_earliest_upcoming_time_blocked_task() {
        let conn = conn_with_tasks();
        let now_ms: i64 = 1_800_000_000_000;
        let today_start = local_day_start_ms(now_ms);

        // A past time-blocked task must not be picked.
        insert_task(&conn, "past", "Already happened", None, None, today_start, Some(0), 0);
        // Two upcoming candidates; the earlier one wins.
        insert_task(&conn, "later", "Later meeting", None, None, today_start, Some(20 * 60), 0);
        insert_task(&conn, "sooner", "Sooner meeting", None, None, today_start, Some(14 * 60), 0);
        // A done task is never picked even if it is time-blocked and upcoming.
        insert_task(&conn, "done", "Done meeting", None, None, today_start, Some(10 * 60), 1);

        let agenda = build_agenda(&conn, today_start + 13 * 60 * 60 * 1000);
        let meeting = agenda.next_meeting.expect("expected a next meeting");
        assert_eq!(meeting.id, "sooner");
    }
}
