# Plan: Google Calendar Integration

**Status:** Proposed
**Depends on:** system-daemon-wiring (for socket and daemon loop)

## Summary
Connect to Google Calendar API to fetch upcoming events, display them in CLI/Waybar, and later support event creation and reminders.

## Tasks

### T01 — Google Calendar API client
- Add Google Calendar API client module in a new `calendar/` crate or in the daemon
- OAuth 2.0 flow: store refresh token, auto-refresh access tokens
- Config: `google_client_id`, `google_client_secret` in `maia.json`
- First-time auth: print URL for user to authorize, save token

### T02 — Event fetching daemon task
- Add periodic fetch task in daemon main loop (e.g., every 5 minutes)
- Fetch events for next 7 days
- Store in local `calendar_events` table (id, summary, description, start, end, calendar_id, raw JSON)
- This gives a local cache and avoids hitting API on every CLI invocation

### T03 — Calendar CLI
- CLI: `calendar next` — show next upcoming event
- CLI: `calendar today` — show today's events
- CLI: `calendar week` — show this week's events
- CLI: `calendar sync` — force re-fetch from Google

### T04 — Calendar events in Task dashboard
- Add an "upcoming events" section to `task dashboard`
- Merge tasks and events in a unified timeline view
- Show events alongside tasks in the same chronological list

### T05 — Event creation from CLI
- CLI: `calendar add "Meeting title" [--at "2026-05-14 14:00"] [--duration 60] [--description "..."]`
- Creates event locally AND pushes to Google Calendar API
- Daemon handles the two-phase create (local + remote)

### T06 — Event reminders & notifications
- Daemon checks for events starting within configurable threshold (e.g., 15 minutes)
- Sends desktop notification: "Upcoming: Team standup at 14:00"
- Integrate with existing notifications module

### T07 — Validation & cleanup
- Test OAuth flow (first-time auth + token refresh)
- Test event fetching and caching
- Test event creation (local + Google)
- Test notifications fire at correct time
- Test offline mode: show cached events when API unreachable

## Done checks
- `calendar next` shows next event from Google Calendar
- Events are cached in local database
- Events appear in the unified task/event dashboard
- Desktop notifications fire for upcoming events
- Event creation syncs to Google Calendar
