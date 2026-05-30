# Plan: Todo.txt Parser, Import, and Notifications

**Status:** Proposed
**Depends on:** system-daemon-wiring (watch loop, config loading)

## Summary
Watch configured directories for task files, parse and persist tasks once, then archive/delete the source files. After ingestion, all updates happen through CLI (or future interfaces), with completion represented by `completed_at`.

## Assumptions
- Support an import format inspired by todo.txt, but allow Maia-specific drift where needed.
- Parse dates only; ignore times even if they appear in the file.
- Ingestion is one-shot per file: parse → save tasks → move to archive dir or delete.
- After ingestion, task lifecycle operations are done from CLI/interfaces, not by re-reading source files.
- `completed_at IS NOT NULL` is the canonical completion signal.
- Deletes are logical completion/closure, not hard deletes.
- Resolve watched directories from configuration using XDG defaults, and pass the config path into the loader so tests can override it.
- Notifications run on a configurable periodic timer with anti-noise controls.

## Brainstorm decisions (locked)
- User can add one or more task files to watched directories.
- Daemon parses and saves tasks, then moves or deletes the source files.
- Future updates/deletes are handled through CLI or future interfaces.
- `completed_at` is the primary completion field; empty means active.
- "Delete task" behavior maps to completion (logical close), not physical removal.
- Notification cadence and repetition are configurable to avoid noise.

## Tasks

### T01 — Define the ingestion model
- Decide how import lines map to Maia task records, including first-class fields and preserved raw metadata.
- Define ingest metadata needed for audit/debug (`source_file`, `ingested_at`, `archived_path` if moved).
- Specify storage for due dates, priorities, projects, contexts, and `completed_at` semantics.

### T02 — Implement todo.txt parser
- Parse supported task-file line variants into a structured internal model.
- Handle invalid lines deterministically and keep the original raw text for diagnostics.
- Extract date fields as date-only values and ignore time-of-day components.
- Add parser coverage for open tasks, priorities, dates, projects, contexts, and `due:` metadata.

### T03 — Load watched directories from config
- Load the todo.txt watch configuration from a path argument so the caller can inject alternate locations in tests.
- Use XDG config directories as the default lookup path for production runs.
- Surface the configured watched directories cleanly to the daemon watcher setup.

### T04 — Persist imported tasks and finalize source files
- Store parsed items in the existing task storage path used by the daemon.
- Set `completed_at` when provided by input; otherwise keep it null.
- After successful save, move files to a configured archive directory or delete them, based on config.

### T05 — Evaluate due items and notify
- Query imported tasks for due today, soon, and overdue windows on a configurable periodic timer.
- Send desktop notifications once per due-state transition and avoid repeat alerts with notification state tracking and configurable cooldown/repeat settings.
- Include enough task context in the notification body to identify the source file and due date.

### T06 — Implement completion lifecycle rules
- Implement CLI/interface operations so "delete" marks `completed_at` (or equivalent closed state) instead of removing rows.
- Ensure task update operations never require source-file rewrites after ingestion.
- Define optional restore/reopen behavior by clearing `completed_at`.

### T07 — Wire file ingestion flow
- Attach watched directories so file create events trigger ingest.
- Parse and save tasks from each new file exactly once, then archive/delete the source file.
- Log ingest outcomes clearly (ingested count, invalid lines, archive/delete result).

### T08 — Visibility and troubleshooting
- Surface parser errors, sync errors, and notification errors in daemon logs without crashing the watcher.
- Add a simple inspection path for stored active/completed tasks so the pipeline is easy to verify.
- Keep the behavior aligned with the existing daemon logging style.

### T09 — Validation and cleanup
- Test parsing against representative todo.txt samples, including malformed lines.
- Verify config loading works with both default XDG paths and an injected test path.
- Verify new files are ingested and then archived/deleted per config.
- Verify delete operations mark tasks completed (no physical delete).
- Verify due and overdue notifications fire exactly once per state change.

## Done checks
- todo.txt files are ingested from the watched directory.
- Source files are archived/deleted after successful ingest.
- Configuration can be loaded from XDG defaults or an injected path.
- Task completion is represented by `completed_at` and delete maps to completion.
- Due and overdue tasks trigger desktop notifications.
- Invalid lines are reported without taking down the daemon.
