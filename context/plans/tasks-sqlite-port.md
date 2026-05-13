# Plan: Port Tasks to SQLite

**Status:** Proposed
**Depends on:** system-daemon-wiring (for socket handling, DB init)

## Summary
Replace the legacy JSON-file-based todo system with SQLite-backed tasks and goals. Port the key logic (hierarchy, monitoring, notifications) while simplifying the schema. The new CLI structures in `todo/` already exist but have no backend — this plan wires them up.

## Tasks

### T01 — Design and create tasks/goals database tables
Design and implement these tables in the shared database schema:

- `goals`: id, title, why TEXT, how TEXT, notes TEXT, priority (enum), horizon TEXT, created_at, updated_at, completed_at (optional)
- `projects`: id, goal_id (FK), title, notes, created_at, completed_at (optional)  
- `tasks`: id, project_id (FK, optional), title, description, start_at (optional), end_at (optional), priority (enum), recurrence (JSON: days_of_week[], one_off boolean, after_*), completed_at (optional), created_at, updated_at
- Add `CREATE TABLE IF NOT EXISTS` in `initialize_schema()`

### T02 — Goal CRUD via new CLI structures
- The `todo/src/goals.rs` CLI structs already define Goal subcommands (Create, Read, Update, Delete) — wire them to actual daemon handlers
- Daemon handlers perform CRUD on the `goals` table
- Return serialized data via socket protocol

### T03 — Task CRUD via new CLI structures
- The `todo/src/tasks.rs` CLI structs already define Task subcommands — wire them to actual daemon handlers
- Daemon handlers perform CRUD on the `tasks` table
- Handle the `days` (recurrence), `priority`, `one_off`, `after` fields from the existing CLI

### T04 — Goal ↔ Task relationship
- The legacy system has a Goal → Project → Task hierarchy with hours_per_week tracking
- Simplify: Goal → Task directly (drop the Project layer unless needed)
- Implement: `task list --goal <goal_id>` and `goal show <id>` to list related tasks
- Track hours per goal per week (sum of task durations)

### T05 — Task monitoring & notifications
- Port the legacy monitoring logic (`old/utils/monitoring.rs`) to use SQLite queries
- On daemon start (or interval): check for tasks that are due soon, overdue, or starting
- Send desktop notifications via `notify-rust`
- Mark tasks as "seen" (new `notified_at` column on tasks) to avoid repeat notifications

### T06 — Task dashboard CLI
- CLI: `task dashboard` — grouped view: overdue, today, this week, later
- CLI: `goal dashboard` — progress per goal (completed tasks / total tasks)
- Format: clean table output with colors (clap + colored output)

### T07 — Remove legacy JSON storage
- After confirming all data is migrated: remove `todo/src/old/` module
- Or keep it as read-only fallback, clearly marked as deprecated
- Update `todo/Cargo.toml` to remove old dependencies

### T08 — Validation & cleanup
- Test full CRUD for both goals and tasks
- Test hierarchical queries (tasks under a goal)
- Test notification triggers (due date approaching)
- Test migration from JSON to SQLite (write a one-shot migration CLI command)
- Verify legacy code can be removed cleanly

## Done checks
- `goal create|read|update|delete` works end-to-end
- `task create|read|update|delete` works end-to-end
- Notifications fire for approaching due dates
- `task dashboard` shows meaningful grouped view
- Legacy JSON storage is either migrated or clearly deprecated
