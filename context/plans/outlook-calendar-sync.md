# Outlook Calendar Sync (read-only)

## Change summary
Read-only Microsoft Graph calendar integration, first of the two providers the roadmap names (Google
follow-up is a separate, structurally identical plan once this one proves the pattern out). The server
handles the OAuth authorization-code exchange and refresh-token storage — same shape as the Trading 212
feed credentials (stored in the per-user config overlay, never the DB) — while the desktop app only
triggers the initial "Connect Outlook" browser flow and displays the result. A background task
periodically pulls upcoming events from Microsoft Graph into a new local `calendar_events` table, which
both the task-calendar plan's grid and the Waybar/agenda plan's "next meeting" query can read from going
forward.

## Success criteria
- [ ] A registered Azure/Microsoft Entra app (client id, no client secret needed — native/public client
      with PKCE) is a documented one-time external prerequisite, stored in the overlay config like a
      T212-style credential.
- [ ] `GET /calendar-feeds/outlook/authorize-url` returns the Microsoft OAuth consent URL (with PKCE
      challenge, `Calendars.Read offline_access` scope, redirect URI pointing at a new server callback
      route); the desktop "Connect Outlook" button opens it in the system browser.
- [ ] `GET /calendar-feeds/outlook/callback` receives the auth code, exchanges it for access + refresh
      tokens via Microsoft's token endpoint, and persists the refresh token in the overlay config.
  Never logs the token itself.
- [ ] A background task refreshes the access token as needed and periodically (default: every 15–30
      minutes) pulls upcoming events (e.g. next 30 days) via Microsoft Graph's calendar view API,
      upserting them into a new `calendar_events` table (source, external_id, title, start_at, end_at,
      location, updated_at) keyed by `(source, external_id)` so re-syncs don't duplicate.
- [ ] A desktop Settings section (mirroring the Trading 212 feed settings screen) shows connection
      status (connected/not connected, last sync time/error) with connect/disconnect actions.
- [ ] `GET /calendar-feeds/outlook/events?since=` (or however the existing pull-style endpoints are
      shaped) exposes synced events for the task-calendar and agenda features to consume.
- [ ] `cargo test -p maia` covers token refresh logic and event upsert idempotency (re-syncing the same
      events twice doesn't duplicate rows) using recorded/fake Graph responses — no live Microsoft
      account needed for the test suite.

## Constraints and non-goals
**Constraints:**
- Read-only — never creates, edits, or deletes anything on the user's actual Outlook calendar.
- OAuth tokens live in the overlay config file, never the database, matching the Trading 212 pattern
  (the deployment-owned config file is readable by the service manager; credentials must never land
  there).
- Server-side token exchange and refresh — the desktop app never sees or stores a token directly.

**Non-goals:**
- No Google Calendar in this plan — same architecture, separate follow-up plan once this one is proven.
- No two-way sync, no event creation/editing from maia.
- No recurring-event expansion logic of its own — Microsoft Graph's calendar view API already expands
  recurring events server-side when queried with a date range, so there's no need to replicate the
  task-calendar plan's recurrence port here.

## Assumptions
- Unlike the Trading 212 split (deployment owns "enabled", overlay owns credentials), both the
  enable/disable flag and the refresh token live in the overlay here — a calendar connection is
  inherently personal (whose calendar is it?), not a deployment-wide toggle.
- The OAuth redirect URI points at the `maia` service itself (`http://127.0.0.1:<port>/calendar-feeds/
  outlook/callback`), reachable because Microsoft's native-app flow allows loopback redirects — no need
  for a public HTTPS callback URL.
- "Upcoming" events means a rolling window (next 30 days) rather than full history — this is for
  display/agenda purposes, not calendar archival.

## Task stack (T01..T05)

- [ ] T01: Schema — `calendar_events` table (status:open)
  - Goal: Migration adding `calendar_events(id, source, external_id, title, start_at, end_at, location,
    created_at, updated_at, deleted_at)` with a unique constraint on `(source, external_id)`.
  - Done when: migration applies cleanly; `cargo test -p maia` green.

- [ ] T02: OAuth authorize + callback routes (status:open)
  - Goal: PKCE-based authorization URL generation; callback route exchanging the code for tokens and
    persisting the refresh token to the overlay.
  - Done when: a stubbed/fake token endpoint in tests confirms the exchange and overlay write happen
    correctly; manual end-to-end verified against a real Microsoft account once implemented.

- [ ] T03: Background sync task (status:open)
  - Goal: Interval task (same shape as the other background watchers in this codebase) that refreshes
    the access token when needed and upserts events from Graph's calendar view into `calendar_events`.
  - Done when: a fake Graph response with overlapping/duplicate events upserts idempotently; token
    refresh is exercised with a stubbed expiring token.

- [ ] T04: Events endpoint (status:open)
  - Goal: Expose synced events for consumers (task-calendar view, agenda endpoint) via a pull-style
    route.
  - Done when: returns the expected event set for a date range, respecting tombstones if any event is
    later removed from the source calendar (status becomes cancelled/deleted upstream).

- [ ] T05: Desktop Settings UI (status:open)
  - Goal: Connection status display, connect/disconnect actions, last-sync status — mirroring the
    Trading 212 feed settings screen's shape.
  - Done when: connecting from a clean state completes the browser round-trip and shows "connected";
    disconnecting clears the stored token and stops the background sync.

## Risks / gotchas
- The loopback OAuth redirect needs the server's actual bound port, which can vary — make sure the
  redirect URI registered in Azure matches exactly what the running server uses (port mismatches are a
  classic OAuth integration failure mode), and that the Azure app registration allows loopback
  redirects with a dynamic port if that's how it ends up being implemented.
- Microsoft Graph tokens expire frequently (typically ~1 hour for access tokens) — the refresh logic
  needs to be solid and tested with an actually-expired-token case, not just the happy path, since a
  background task silently failing to refresh would quietly stop syncing with no obvious symptom beyond
  a stale calendar.
- A cancelled/deleted event upstream needs to be reflected locally (tombstoned in `calendar_events`) or
  it'll linger on the calendar view forever — confirm what Graph's calendar view API actually returns
  for a cancelled recurring instance before assuming a simple upsert covers it.
