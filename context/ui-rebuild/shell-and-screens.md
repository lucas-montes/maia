# UI Shell and Core Screens

Implemented a sober, responsive desktop UI with collapsible sidebar and simplified core screens.

## Shell Structure

The app shell consists of:
- **app-shell**: Main container with responsive grid layout
- **app-sidebar**: Collapsible sidebar with navigation (Dashboard, Todos, Receipts, Notes, Settings)
- **app-main**: Main content area that fills available space
- **app-main-scroll**: Scrollable container for content

## Sidebar Features
- Collapsible/expandable via toggle button
- Shows compact icons when collapsed, icons+labels when expanded
- Displays badge counters for todos and receipts in footer
- Responsive behavior: collapses to icon-only on narrow screens

## Core Screens Implemented

### Dashboard
- Overview panel with key metrics (Todos, Receipts, Notes, Goals)
- Quick action buttons for each core feature
- Clean, readable layout with appropriate spacing

### Todos / Tasks
- Full CRUD backed by SQLite `tasks` table with title, description, due date, priority, tags, goal linkage
- Create form with title, description, due date, priority selector, tags input
- Filter controls: All / Active / Completed (based on `completed_at` timestamp)
- Checkbox toggle for completion (sets `completed_at` timestamp)
- Delete button per task
- Priority badges (high=red, medium=yellow, low=green)
- Real counts displayed in sidebar footer

### Goals
- Full CRUD backed by SQLite `goals` table with title, description, status, deadline, progress
- Create form with title, description, status selector (active/on-hold/done), deadline date picker, progress number input
- Goal list cards showing: title, description, status badge, deadline, task count (computed from loaded tasks), progress bar with range slider
- Progress slider sets 0-100% inline per goal
- Delete button per goal
- Status badges: active (sky), on-hold (amber), done (emerald)

### Saved URLs
- Full CRUD backed by SQLite `urls` table with title, url, source, tags, is_new, created_at
- Create form with URL, title, source selector (manual/chrome-extension/import), tags input
- URL list showing: title/URL, new badge (blue dot + "new" label), source, tags, created date
- "Open in browser" button (↗) using `tauri-plugin-opener`'s `openUrl`
- "Mark as read" button (✓) clears the `is_new` flag
- Delete button per URL
- New URLs notification banner at top of list
- No snapshot files, no note/goal linking — URLs are self-contained records

### Receipts
- List receipts from SQLite with merchant/ID, total, date, status badge, and current file path
- Click a receipt to view full detail: receipt ID, merchant, total, date, category, status
- File Paths section shows original, current, archived, and parsed JSON paths for provenance tracking
- Timestamps section shows created and processed dates
- Status badges: pending (slate), reviewed (emerald), archived (amber)
- Image display deferred (paths shown as text for now; visual preview can be added with Tauri asset protocol)

### Notes
- List all notes from SQLite with title, tags, and updated timestamp
- Click a note to open in‑line editor with textarea for Markdown content
- Title and tags inputs for metadata editing
- Save writes back to the underlying `.md` file and updates SQLite metadata
- New note button creates a `.md` file in `./notes/` and a matching SQLite record
- Tags stored as JSON array in SQLite, displayed and edited as comma‑separated text
- Editor shows the note's file path in the subtitle

### Settings
- Form-based editor for `maia.json` config with labeled fields for each key: `database`, `receipts_path`, `nutriments_path`, `bank_statements_path`, `investments_statements_path`, `model_api`, `receipts.archived_dir`
- Loads current config from disk on view mount via Tauri command and populates form fields
- Saves constructs a JSON object from form fields and writes with automatic backup (`maia.json.bak`)
- Handles nullable fields (empty → `null`) and nested keys (`receipts.archived_dir` → `{ "receipts": { "archived_dir": "..." } }`)
- Returns a default config when `maia.json` does not exist (T02)
- Status indicators for load/save/success/error states
- Reload from file button to discard unsaved changes

## Styling Approach
- Uses Tailwind CSS for utility-first styling
- Consistent spacing and visual hierarchy
- Subtle elevation and color variations for depth
- Responsive design that adapts to different screen sizes
- Focus on readability and minimal visual noise

## Key UI Principles Applied
- Each screen shows only essential information first
- Avoided dense nested panels through thoughtful layout
- Clear visual hierarchy with appropriate typography
- Consistent interaction patterns across screens
- Progressive disclosure for advanced features
- Mobile-first responsive design

## Files Modified
- `maia-ui/src/main.ts` - Complete UI implementation with shell and screens (settings form editor T03/T04, datepicker T01, notes list+editor T05, receipts list+detail+archive T06/T07, tasks+goals screens T08, saved URLs screen T09)
- `maia-ui/src/styles/tailwind.css` - Added custom utility classes (buttons T04, note list/editor T05, receipt status badges T06)
- `maia-ui/src-tauri/src/lib.rs` - Tauri commands (config T04; SQLite init + notes CRUD T05; receipts table + list/read + archive T06/T07; tasks+goals tables + CRUD + get_counts T08; urls table + CRUD + mark_read T09)
- `maia-ui/src-tauri/Cargo.toml` - Added `rusqlite` and `chrono` dependencies (T05)
- `maia-ui/index.html` - Updated to reference the new main.ts entry point

## Acceptance Criteria Met
- App shell fills full window and resizes fluidly
- Sidebar is compact, collapsible, and visually understated
- Sidebar shows Goals and URLs nav items with real task and receipt counts in footer
- Main content is not trapped inside decorative cards
- Each screen is readable at a glance and avoids dense nested panels
- Navigation works correctly between all core screens
- Settings can edit `maia.json` with JSON validation and automatic backup (T04)
- Notes display path and attributes; editing writes back to the underlying Markdown file (T05)
- Receipts display image path, receipt ID, metadata, parse state, and processed destination; original and current paths are shown preserving a traceable link (T06)
- Receipts can be archived with file move to archive dir, DB path/status update (T07)
- Tasks have title, description, due date, tags, priority, completed state — CRUD via Tauri + UI filter/sort (T08)
- Goals have title, description, status, deadline, progress, linked tasks — CRUD via Tauri + progress slider (T08)
- Dashboard shows real task, goal, receipt, note, and URL counts via `get_counts` command (T08/T09)
- URLs can be added, listed with new badge, opened externally via `tauri-plugin-opener`, marked read, and deleted (T09)

See also: [overview.md](../overview.md), [context-map.md](../context-map.md)