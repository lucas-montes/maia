# Waybar + Desktop Notifications

Supersedes `system-waybar.md`, which targeted the deprecated socket-based daemon architecture (no
longer exists — `sync-server`/`maia` replaced it). This plan targets the current HTTP service.

## Change summary
A new authenticated `GET /agenda` endpoint on the `maia` service returns what needs attention right
now: due/overdue tasks and the next upcoming "meeting" (approximated as the next time-blocked task —
one with `start_time_minutes` set — until calendar sync exists). Waybar polls this endpoint directly
(`exec` + `curl` with the existing Bearer key) and renders it via Waybar's custom-module JSON protocol.
Separately, the `maia` service itself runs a background scheduler (same shape as the existing
statements/notes-mirror watchers) that checks for tasks crossing their due time and fires `notify-send`
directly from the server process — which means the systemd unit needs access to the desktop session's
D-Bus bus, not just a new code path.

## Success criteria
- [ ] `GET /agenda` (Bearer-authed, like every other route) returns: overdue + due-today tasks (id,
      title, due time), and the next upcoming time-blocked task as the "next meeting" proxy. Empty
      lists when there's nothing due — not an error.
- [ ] A documented example Waybar module config (in this repo's `context/` docs, not necessarily deployed
      by Nix) shows `exec` calling `curl` against `/agenda` with the Bearer key, piped into a small
      `jq` transform into Waybar's `{text, tooltip, class}` shape.
- [ ] The `maia` service runs a background scheduler (debounced tick, same shape as
      `notes_mirror::watch`/`statements::watch`) that, once per task whose due time is newly crossed,
      calls `notify-send` with the task's title — not a repeated notification every tick for the same
      task.
- [ ] The systemd unit (`nix/module.nix`) is updated with whatever's needed for `notify-send` to reach
      the logged-in user's session bus (environment/`DBUS_SESSION_BUS_ADDRESS` wiring) — verified on
      the actual deployed service, not just assumed from reading the unit file.
- [ ] `cargo test -p maia` covers the due-task query (overdue/due-today selection, "newly crossed" vs
      "already notified" so a restart doesn't re-fire every pending notification at once).

## Constraints and non-goals
**Constraints:**
- `notify-send` is called from the server process itself (not the Tauri GUI, not a separate user-level
  timer) — per explicit direction, even though this is the part with real deployment friction (a
  system service reaching a user session's notification bus).
- `/agenda` requires the same Bearer auth as every other route — no new unauthenticated surface.
- "Next meeting" has no real backing data yet (no calendar integration exists) — it's approximated
  from time-blocked tasks (`start_time_minutes` set) until the external-calendar-sync plan lands, at
  which point this endpoint should also consider real calendar events without changing its shape.

**Non-goals:**
- No actual Waybar module/CSS shipped via this repo's Nix config — the home-manager-side Waybar
  config is the user's own dotfiles; this plan only guarantees the data endpoint and a documented
  example.
- No finance/calorie/other Waybar "modes" — scoped to tasks + meetings only, per explicit direction.
- No notification snoozing/dismissal tracking beyond "don't re-fire the same crossing twice."

## Assumptions
- A task counts as "due" for `/agenda` when it has a `due_date` (and optional `due_time_minutes`) at or
  before now, and is not yet `done`.
- "Newly crossed" due-time tracking needs *some* persisted marker (a `notified_at` column on `tasks`,
  or a small separate table) so a service restart doesn't replay every currently-overdue task as a
  fresh notification — leaning toward a `tasks.notified_at` column since it's a 1:1 relationship and
  avoids a join.

## Task stack (T01..T04)

- [ ] T01: `GET /agenda` endpoint (status:open)
  - Goal: Query overdue + due-today tasks and the next time-blocked task; return as JSON.
  - Done when: seeded tasks with various due dates/times produce the expected overdue/due-today split
    and correct "next meeting" pick.

- [ ] T02: Background due-task notifier (status:open)
  - Goal: A tokio interval loop (debounced, sweep-backstop shape like the existing watchers) that finds
    tasks whose due time newly passed since the last tick, calls `notify-send`, and marks them
    (`notified_at`) so they're not re-fired.
  - Done when: a task whose due time passes between two ticks fires exactly one notification; a service
    restart with several already-overdue-and-notified tasks does not re-notify them.

- [ ] T03: systemd D-Bus session wiring (status:open)
  - Goal: Whatever `nix/module.nix` + the unit's `Environment=`/sandboxing needs so `notify-send` calls
    from the system service actually reach the logged-in user's notification daemon.
  - Done when: manually triggering a notification from the deployed service produces a visible desktop
    notification, confirmed on the real machine (not just a clean `systemctl status`).

- [ ] T04: Documented Waybar module example (status:open)
  - Done when: a doc snippet (module config + the curl/jq one-liner) in `context/` is copy-pasteable
    into a real Waybar config and renders something sensible.

## Risks / gotchas
- T03 is the real risk in this plan: a system service calling `notify-send` needs the user session's
  `DBUS_SESSION_BUS_ADDRESS` (and the right `XDG_RUNTIME_DIR`), which systemd system units don't get by
  default. If this turns out fragile/unreliable in practice, the fallback is a tiny user-level
  `systemd --user` timer hitting the same `/agenda` endpoint and calling `notify-send` from within the
  user session — architecturally simpler, worth keeping in mind if T03 proves too brittle.
- "Due today" needs a day-boundary definition (local timezone midnight) — get this wrong and tasks due
  late at night either vanish from the agenda too early or linger a day too long.
