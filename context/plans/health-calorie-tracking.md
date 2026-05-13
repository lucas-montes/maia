# Plan: Calorie Tracking (Health & Nutrition)

**Status:** Proposed
**Depends on:** system-daemon-wiring (for socket handling), finance-expenses (for shared receipt pipeline)

## Summary
Build the calorie tracking system: AI-powered receipt parsing for nutritional data, manual meal logging, and nutrition goal setting.

## Tasks

### T01 — Create `health` crate scaffold
- Create `health/Cargo.toml` with `shared` dependency
- Add `health` to workspace members in root `Cargo.toml`
- Set up module structure: `cli/`, `daemon/`, `models/`
- Wire feature flags: `cli`, `daemon`

### T02 — Add nutrition goals to database schema
- Create goals/macros table: `nutrition_goals` (daily_calories, proteins_g, carbs_g, fats_g, fiber_g, start_date, end_date)
- Add migration in `shared/src/database.rs` init
- Derive `Crud` for the new model

### T03 — Meal logging CLI
- Add CLI commands: `meal add <aliment_id> <quantity> [date]`, `meal list [date]`, `meal delete <id>`
- Wire through socket protocol to daemon
- Daemon handler: CRUD on `meals` table

### T04 — Wire AI receipt → nutrition pipeline
- Extend receipt AI extraction to include product-level nutritional fields
- On receipt save, upsert into `aliments` table (match by barcode or product name)
- Add config option for nutriment source URL patterns (e.g. Intermarché)

### T05 — Nutrition plan/goal tracking CLI
- CLI: `goal set-nutrition`, `goal show-nutrition`, `goal progress [date]`
- Show daily actual vs target (calories, macros)
- Integrate with the goals system from tasks plan

### T06 — Daily nutrition summary
- Daemon endpoint: daily macro report (calories in vs out, protein/carb/fat breakdown)
- Format for CLI output and later Waybar display

### T07 — Validation & cleanup
- Test all CLI commands end-to-end
- Test receipt → aliment upsert flow
- Verify database schema migrations are idempotent
- Clean up: remove hardcoded paths, verify config-driven paths work

## Done checks
- `health` crate compiles with both `cli` and `daemon` features
- `meal add/list/delete` works via CLI through daemon
- Receipts with nutrition data populate `aliments` table
- `goal show-nutrition` displays daily progress vs targets
