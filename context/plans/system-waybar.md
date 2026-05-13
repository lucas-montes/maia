# Plan: Waybar Integration

**Status:** Proposed
**Depends on:** system-daemon-wiring (for socket communication)

## Summary
Add Waybar-compatible output from the daemon. Display glanceable information: next task, upcoming calendar event, daily calorie summary, or latest expense.

## Tasks

### T01 — Waybar output module in daemon
- Create `src/waybar.rs` module
- Implement a Waybar JSON formatter (follows the Waybar custom module protocol: `{"text": "...", "tooltip": "...", "class": "..."}`)
- Socket endpoint: send a `Waybar` message type, receive formatted JSON

### T02 — Daemon endpoint for Waybar queries
- Add `Waybar` variant to `ClientMessage` enum in socket protocol
- Handler aggregates data from multiple sources (tasks, calendar, nutrition, finance)
- Return formatted Waybar JSON based on a `?mode=` parameter (e.g., `mode=task`, `mode=calories`, `mode=finance`)

### T03 — CLI wrapper for Waybar
- Add `maia waybar [mode]` CLI command
- CLI connects to daemon socket, sends Waybar query, prints JSON to stdout
- This is what Waybar's `exec` or `interval` exec will call

### T04 — Task mode (next due task)
- Query the tasks table for the next uncompleted task (by start date or priority)
- Display: `"📋 Buy groceries"` with tooltip showing full details

### T05 — Calendar mode (next event)
- Query Google Calendar (if connected, see google-calendar plan) for next event
- Display: `"📅 Team standup in 15min"`

### T06 — Finance mode (today's spending)
- Query today's transactions, sum expenses
- Display: `"💳 -€42.50 today"` with tooltip showing breakdown

### T07 — Calorie mode (daily summary)
- Query today's meals, sum calories vs goal
- Display: `"🍎 1450/2000 kcal"`

### T08 — Waybar configuration example
- Document example Waybar config in README or a comment in the module
- Example `config.jsonc` snippet for each mode

### T09 — Validation & cleanup
- Test each mode returns valid Waybar JSON
- Test with Waybar running (or mock the exec)
- Ensure graceful degradation when data sources are empty
- Verify tooltip content is useful at a glance

## Done checks
- `maia waybar task` returns valid Waybar JSON with next task
- Each mode handles empty state gracefully
- CLI wrapper works in Waybar's `exec` directive
- Documented config snippet available
