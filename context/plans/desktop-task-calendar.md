# Desktop Task Calendar + Upcoming Digest

## Change summary
A new read-only month-grid calendar view in the desktop app — the existing `todos.ts` is a flat list
with no date-grid visualization at all. Click a day to drill into week/day detail. Recurring tasks
(`tasks.recurrence`/`series_id`, already fully modeled — mobile's `PlannerRecurrence` evaluates
daily/weekly/interval/monthly rules against each day) get expanded into individual calendar entries for
every occurrence, not just their anchor row. An "up next" panel inside the calendar view surfaces the
next upcoming time-blocked task as a lightweight "next meeting" digest (reusing the same due/upcoming
query the Waybar `/agenda` endpoint uses, once that plan lands — or its own equivalent query if built
first).

## Success criteria
- [ ] A new desktop sidebar entry/view: month grid, click a day to see a week or day drill-down with
      that day's tasks in detail.
- [ ] Recurring tasks are expanded client-side for the visible date range (port of mobile's
      `PlannerRecurrence.isOccurrenceOn`/`occurrenceIndex` logic to TypeScript — same
      daily/weekly/interval/monthly rules, same `excludedDates` skip behavior), not just their anchor
      row.
- [ ] The calendar is read-only: no create/edit from calendar cells. Editing a task still goes through
      the existing task list/form.
- [ ] An "up next" panel within the calendar view shows the next upcoming time-blocked task (by
      `start_time_minutes`, nearest in the future from now).
- [ ] `npx tsc --noEmit`/`vite build` clean; a TS unit test or two covers the recurrence-expansion port
      against the same cases mobile's `PlannerRecurrence` already handles (daily, weekly-by-weekday,
      interval-every-N-days, monthly-by-day-of-month, an excluded date, a `count`/`endDate` bound).

## Constraints and non-goals
**Constraints:**
- Read-only for now — no click-to-create/edit, per explicit direction. A later plan can add it once the
  view exists and is used.
- Recurrence expansion logic must stay behaviorally identical to mobile's `PlannerRecurrence` — the same
  task series should produce the same occurrence dates whether viewed on the phone or desktop. Port the
  algorithm, don't reinvent it.
- The "up next" panel lives inside the calendar view, not the dashboard — per explicit direction.

**Non-goals:**
- No real calendar-event integration yet (Google/Outlook sync is a separate plan) — this calendar shows
  tasks only. Once external events exist, they can be layered onto the same grid as a later addition
  without changing this plan's structure.
- No recurring-task editing UI changes — whatever already creates/edits recurrence rules (mobile,
  presumably) is unchanged; this plan only consumes and displays them.

## Assumptions
- The visible date range for expansion is bounded to the currently-displayed month/week (plus a small
  buffer for week/month boundary overlap) — not expanding every recurring series to infinity.
- "Next time-blocked task" for the up-next panel means a task with `start_time_minutes` set and a
  due/scheduled date at or after today, nearest first — tasks with no time block are excluded from this
  specific panel (they can still show in the day drill-down).

## Task stack (T01..T04)

- [ ] T01: Port recurrence expansion to TypeScript (status:open)
  - Goal: A `expandRecurrence(task, rangeStart, rangeEnd)` function mirroring
    `PlannerRecurrence`'s daily/weekly/interval/monthly logic and `excludedDates`/`count`/`endDate`
    bounds, producing concrete occurrence dates within a range.
  - Done when: unit tests covering each recurrence type (plus an excluded date and a bounded count)
    match hand-computed expected occurrence lists.

- [ ] T02: Month grid + day/week drill-down (status:open)
  - Goal: New view: month grid pulling tasks for the visible range (anchor tasks + expanded
    occurrences), click-through to a week or day view listing that day's/week's tasks in detail.
  - Done when: a recurring weekly task shows up on every matching weekday across the visible month; a
    one-off task shows exactly once on its due date.

- [ ] T03: Up-next panel (status:open)
  - Goal: Query (or reuse the Waybar plan's `/agenda` endpoint if it exists by the time this is built)
    for the nearest upcoming time-blocked task; render it in a small panel within the calendar view.
  - Done when: the panel shows the correct next task given several candidates at different future
    times, and shows a sensible empty state when nothing is scheduled.

- [ ] T04: Sidebar wiring + navigation (status:open)
  - Goal: New sidebar entry, routing for month/week/day views.
  - Done when: navigating in and switching grid granularity works without losing the selected date.

## Risks / gotchas
- Porting date-math between Dart and TypeScript is exactly the kind of place off-by-one bugs hide
  (month boundaries, week-start convention, timezone handling for `start_time_minutes` which is
  minutes-since-midnight-local, not an absolute timestamp) — lean on direct unit tests against the same
  cases mobile already covers rather than trusting the port by inspection.
- A task series with no `endDate`/`count` expands indefinitely in principle — make sure the range bound
  (T01) is actually enforced before this ships, or a far-future month view could end up iterating a lot
  of occurrences for a `daily` series with no end.
