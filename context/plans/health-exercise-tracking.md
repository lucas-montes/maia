# Plan: Exercise Tracking

**Status:** Proposed
**Depends on:** system-daemon-wiring

## Summary
Build exercise/workout tracking on top of existing `Exercise`, `Seance`, `SeanceSet`, `BodyWeight` models. CLI-driven workout logging with progress tracking.

## Tasks

### T01 — Move exercise models to `health` crate
- Move `shared/src/exercise/` module into `health/src/models/`
- Update all `shared` re-exports
- Ensure `Crud` derive still works after move
- Keep `met.rs` with the module

### T02 — Exercise library CLI
- CLI: `exercise add <name> <category> [met]`, `exercise list`, `exercise delete <id>`
- Category enum: Chest, Back, Legs, Shoulders, Arms, Core, Cardio, Other
- Wire through socket to daemon CRUD handlers

### T03 — Workout session (seance) CLI
- CLI: `seance start`, `seance end`, `seance list`, `seance show <id>`
- Daemon handler: create/update/find/list seances
- Timer support: auto-calculate duration from start_at/end_at

### T04 — Set logging CLI
- CLI: `set add <seance_id> <exercise_id> <sets> <reps> <weight> [notes]`, `set list <seance_id>`
- Wire through socket to daemon CRUD on `seance_set` table
- Show running total volume (sets × reps × weight)

### T05 — Body weight tracking CLI
- CLI: `weight add <kg> [fat%] [muscle%]`, `weight list`, `weight chart` (ASCII sparklines)
- Wire through socket to daemon CRUD on `body_weights` table

### T06 — Progress view
- CLI: `progress <exercise_id>` — show weight/reps over time
- Calculate estimated 1RM using Epley formula
- Track volume progression per exercise per week

### T07 — Validation & cleanup
- Test all CLI commands end-to-end
- Test seance timing (start/end flow)
- Test set CRUD within a seance
- Clean up old exercise module location after migration

## Done checks
- `exercise`, `seance`, `set`, `weight` CLI commands all work
- Workout sessions can be started, ended, and reviewed
- Progress view shows historical data for any exercise
- Body weight entries stored and retrievable
