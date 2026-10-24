# Context Map

## Overview
This file indexes all durable context artifacts for the Maia project.

## Core Context Files
| File | Purpose |
|---|---|
| `overview.md` | Project overview, architecture, key decisions |
| `glossary.md` | Domain-specific terms and definitions |
| `context-map.md` | This file — index of all context |

## Domain Context Files
| File | Purpose |
|---|---|
| `nix/packaging.md` | How the `server` flake packages (`sync-server`, `frontend`, `maia-desktop`) are built: crane deps caching, pnpm frontend, crane-tauri app, utoipa-swagger-ui download vendoring, build-environment gotchas |
| `ui-rebuild/shell-and-screens.md` | UI shell, core screen implementations, settings editor, and notes file-backed records |
| `sync/sync-contract.md` | FitFat offline-first sync contract (mirrors `~/Projects/fitfat/docs/api/openapi.yaml` and `context/sync/sync-contract.md`) |

## Plans (proposed)
| Plan | Status | Description |
|---|---|---|
| `health-calorie-tracking` | proposed | AI receipt → nutrition, meal logging, nutrition goals |
| `health-exercise-tracking` | proposed | Workout sessions, set logging, body weight, progress charts |
| `finance-expenses-income` | proposed | Tx/investment CLI, bank/investment CSV parsing, receipt→expense link |
| `finance-budget-simulation` | proposed | Budget tracking, auto-categorization, Monte Carlo sim integration |
| `system-daemon-wiring` | proposed | DB init, main loop, socket handler, file watcher fixes (FOUNDATIONAL) |
| `system-waybar` | proposed | Waybar JSON output for tasks, calendar, calories, finance |
| `system-remote-storage` | proposed | S3/local sync for DB and watched files |
| `tasks-sqlite-port` | proposed | Replace JSON todo storage with SQLite, wire new CLI, notifications |
| `todo-txt-import` | proposed | Watch todo.txt files, parse tasks, save them, and notify on due dates |
| `calendar-google` | proposed | Google Calendar OAuth, event fetch/create, reminders |
| `personal-meetings` | proposed | Meeting start/stop, notes, audio recording, markdown export |
| `ui-visualization` | proposed | Embedded HTTP server + web dashboard with charts |
| `ui-rebuild` | completed | Sober responsive desktop UI rebuild with files, SQLite indexing, and saved URLs |
| `kb-chrome-extension` | proposed | Wire Chrome → native host → daemon → SQLite pipeline |
| `kb-search` | proposed | FTS5 full-text search, vector embeddings, knowledge graph |
| `fitfat-sync-server` | completed | Standalone `sync-server` crate (Axum+rusqlite) + full FitFat OpenAPI + Tauri 0.0.0.0:3030 deterministic LAN embedding |
| `maia-headless-server` | completed | Headless `sync-server` binary (`serve`/`migrate`/`status`) + `PRAGMA user_version` migrations with snapshots + env config + NixOS service `services.maia` |
| `maia-desktop-split` | completed | Desktop app stops hosting the server and becomes a client of it; config moves from `MAIA_*` env vars to a Nix-generated `/etc/maia/config.json` plus a per-user overlay; `maia-desktop` packaged and installed by `services.maia.desktop.enable` |
| `desktop-entry-and-state-dir` | completed | Ship the `.desktop` inside the package (`/etc/applications` is off `XDG_DATA_DIRS`, so rofi never saw it); drop the fatal `ReadWritePaths` for a not-yet-created `StateDirectory` |
| `service-user-ownership` | completed | Service runs as the desktop user instead of a `maia` account; drops the group, setgid directory and umask coordination, and reverts an `ExecStartPre` reorder that deadlocked the unit |
| `fitfat-visualization` | completed | HTTP-leveraged `GET /workouts|/meals|/body-metrics` + Chart.js 4 canvases + per-chart filters + Settings QR auth token |
| `fix-fitfat-posts` | completed | Fix all 12 FitFat POSTs (camelCase + `deleted[]`), DB nuke to v30 single `maia.db`, UUID v7, missing GET pulls (templates/notes/tasks/goals/transactions/budget-accounts/receipts/tags), utoipa at `/docs`, WebKit WebVTT gst-plugins-bad |
| `ui-unified-tabs` | completed | Grouped collapsible sidebar (Training/Nutrition/Health/Planning/Finance) + 13 tabs, virtual scroll, lazy media, per-tab sync |
| `ui-vanilla-rewrite` | completed | `maia-ui` monolith rewritten as modular vanilla TS (daisyUI dashboard shell, `shell/` + `views/` per screen, no framework) |
| `sync-ui-bugfixes` | completed | Desktop reads TEXT-uuid notes + `done/taskStatus` filtering, clickable workout detail, labeled set columns; `POST /tags` + `GET /templates` with nested sets; mobile pushes tags/stores/foods |

## Decisions
| Decision | Date | Summary |
|---|---|---|

## External References
- `ROADMAP.md` — High-level feature roadmap
- `STORYLINE.md` — Historical development notes
- `README.md` — Usage guide
