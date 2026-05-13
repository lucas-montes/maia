# Plan: Data Visualization (Web UI)

**Status:** Proposed
**Depends on:** system-daemon-wiring (for socket to query data)

## Summary
Add a local web UI to visualize calories, expenses, tasks, and other tracked data. Embed a lightweight HTTP server in the daemon serving a single-page application.

## Tasks

### T01 — HTTP server in daemon
- Add a lightweight HTTP dependency (e.g. `axum` or `tiny_http`)
- Create `src/http.rs` module serving on a configurable port (default: 8765)
- Serve static files (HTML/JS/CSS) from an embedded directory
- Add `http_port` to config

### T02 — REST API endpoints
- `GET /api/health` — daemon status
- `GET /api/calories/today` — today's meals + goal
- `GET /api/calories/week` — daily totals for past 7 days
- `GET /api/finance/month` — monthly expense/income summary
- `GET /api/tasks/upcoming` — next 10 tasks
- `GET /api/tasks/overdue` — overdue tasks
- `GET /api/workouts/recent` — last 5 workout sessions

### T03 — Web frontend (SPA)
- Simple HTML page with vanilla JS or a minimal framework (no build step)
- Dashboard view: summary cards (today's calories, week's spending, next task, last workout)
- Charts: use a lightweight charting library (Chart.js from CDN or canvas-based)
- Theme: dark mode, consistent with the Chrome extension style

### T04 — Calorie visualization
- Bar chart: daily calories vs goal for past week
- Pie/donut: macro breakdown (protein/carbs/fats) for today
- Simple number display: calories remaining today

### T05 — Finance visualization
- Bar chart: daily expenses for past 7/30 days
- Pie: spending by category
- Line: cumulative spending vs budget over month
- Income/expense summary bar

### T06 — Task visualization
- Kanban-like view or simple list: overdue, today, upcoming
- Goal progress bars
- Calendar heatmap of task completion (GitHub-style)

### T07 — Authentication & security
- Since it's localhost-only by default: simple
- Optionally: basic auth or token-based if binding to 0.0.0.0
- CORS restricted to same origin

### T08 — Validation & cleanup
- Test all API endpoints return correct JSON
- Test frontend loads and displays data
- Test with empty database (graceful empty states)
- Test port conflicts (daemon should log and fail gracefully)
- Verify no new security warnings

## Done checks
- Daemon serves HTTP on configurable port
- REST API returns data from all domains (calories, finance, tasks, workouts)
- Web UI shows dashboard with summary cards and charts
- All views handle empty data gracefully
