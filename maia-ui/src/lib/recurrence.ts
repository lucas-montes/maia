/**
 * Port of mobile's `PlannerRecurrence` (lib/src/models/planner_recurrence.dart)
 * to TypeScript. Same semantics, same four rule types, evaluated purely
 * against a day — independent of the task's time-of-day fields.
 *
 * A recurring task stores one "anchor" row (`tasks.date` is the series
 * start, `tasks.recurrence` the rule, `tasks.series_id` the group id) plus
 * zero or more already-materialized occurrence rows sharing that
 * `series_id`. `expandRecurrence` synthesizes the occurrences that fall in a
 * given range but are not necessarily materialized as DB rows yet (the
 * desktop app is read-only, so it never writes these back — it just needs
 * something to render on the grid). `buildOccurrences` merges real rows with
 * synthesized ones, preferring the real row whenever one already exists for
 * a given (series, day) pair — mirroring mobile's materializer dedup.
 *
 * All timestamps here are epoch milliseconds representing a *day*, read in
 * the local timezone — matching how the Dart side stores
 * `DateTime(y, m, d).millisecondsSinceEpoch` (local midnight), and how the
 * rest of this frontend already treats date columns (see `formatDateMs` in
 * views/todos.ts). Day-count math is done via UTC-normalized calendar
 * fields so DST transitions never shift a day-difference by an hour.
 */

export type RecurrenceType = "daily" | "weekly" | "interval" | "monthly";

export interface RecurrenceRule {
  type: RecurrenceType;
  /** 1..7, Monday..Sunday (matches Dart's `DateTime.weekday`) — weekly only. */
  weekdays?: number[];
  /** >=1 — interval ("every N days") only. */
  intervalDays?: number;
  /** 1..31 — monthly only. */
  monthDay?: number;
  /** Epoch ms, inclusive last occurrence day. */
  endDate?: number;
  /** Max total occurrences, including the anchor (index 0). */
  count?: number;
  /** Epoch ms start-of-day values the materializer/UI should skip. */
  excludedDates?: number[];
}

/** A task row shape wide enough to expand — real Tauri rows satisfy this. */
export interface RecurrenceTaskLike {
  id: string;
  /** Series start / the task's own day. Epoch ms. */
  date: number;
  title: string;
  done: number;
  taskStatus?: number | null;
  dueDate?: number | null;
  startTimeMinutes?: number | null;
  endTimeMinutes?: number | null;
  notes?: string | null;
  recurrence?: string | null;
  seriesId?: string | null;
}

/** One calendar entry — either a real DB row or a synthesized occurrence. */
export interface CalendarOccurrence {
  id: string;
  taskId: string;
  seriesId: string | null;
  /** The occurrence's own day, epoch ms start-of-day (local). */
  date: number;
  title: string;
  done: boolean;
  dueDate: number | null;
  startTimeMinutes: number | null;
  endTimeMinutes: number | null;
  notes: string | null;
  /** True when this entry was synthesized by `expandRecurrence`, not read
   *  directly from a `tasks` row. */
  isVirtual: boolean;
}

/** Parses the `tasks.recurrence` JSON TEXT column; `null`/invalid -> null. */
export function parseRecurrence(json: string | null | undefined): RecurrenceRule | null {
  if (!json) return null;
  try {
    const raw = JSON.parse(json) as Record<string, unknown>;
    if (!raw || typeof raw.type !== "string") return null;
    const type = raw.type as RecurrenceType;
    if (type !== "daily" && type !== "weekly" && type !== "interval" && type !== "monthly") return null;
    return {
      type,
      weekdays: Array.isArray(raw.weekdays) ? raw.weekdays.map((v) => Number(v)) : undefined,
      intervalDays: typeof raw.intervalDays === "number" ? raw.intervalDays : undefined,
      monthDay: typeof raw.monthDay === "number" ? raw.monthDay : undefined,
      endDate: typeof raw.endDate === "number" ? raw.endDate : undefined,
      count: typeof raw.count === "number" ? raw.count : undefined,
      excludedDates: Array.isArray(raw.excludedDates) ? raw.excludedDates.map((v) => Number(v)) : undefined,
    };
  } catch {
    return null;
  }
}

interface YMD {
  y: number;
  m: number; // 1..12
  d: number;
}

function toYMD(ms: number): YMD {
  const dt = new Date(ms);
  return { y: dt.getFullYear(), m: dt.getMonth() + 1, d: dt.getDate() };
}

/** Local-midnight epoch ms for the calendar day `ms` falls in. */
export function startOfDayMs(ms: number): number {
  const { y, m, d } = toYMD(ms);
  return new Date(y, m - 1, d).getTime();
}

/** Local-midnight epoch ms, `n` days after `ms`'s day (n may be negative). */
export function addDaysMs(ms: number, n: number): number {
  const { y, m, d } = toYMD(ms);
  return new Date(y, m - 1, d + n).getTime();
}

// UTC-normalized day index — immune to local DST shifts, used only for
// counting whole days between two calendar dates.
function daysSinceEpoch(ms: number): number {
  const { y, m, d } = toYMD(ms);
  return Math.floor(Date.UTC(y, m - 1, d) / 86_400_000);
}

function daysBetween(aMs: number, bMs: number): number {
  return daysSinceEpoch(bMs) - daysSinceEpoch(aMs);
}

function monthsBetween(aMs: number, bMs: number): number {
  const a = toYMD(aMs);
  const b = toYMD(bMs);
  return (b.y - a.y) * 12 + (b.m - a.m);
}

/** 1..7, Monday..Sunday — matches Dart's `DateTime.weekday`. */
function weekdayOf(ms: number): number {
  const js = new Date(ms).getDay(); // 0=Sun..6=Sat, local
  return js === 0 ? 7 : js;
}

function intervalIndex(startMs: number, dayMs: number, step: number): number {
  const delta = daysBetween(startMs, dayMs);
  if (delta % step !== 0) return -1;
  return Math.floor(delta / step);
}

function monthlyIndex(startMs: number, dayMs: number, monthDay: number | undefined): number {
  const targetDay = monthDay ?? toYMD(startMs).d;
  if (toYMD(dayMs).d !== targetDay) return -1;
  return monthsBetween(startMs, dayMs);
}

function weekdayIndex(startMs: number, dayMs: number, weekdays: number[]): number {
  const set = new Set(weekdays);
  if (set.size === 0 || !set.has(weekdayOf(dayMs))) return -1;
  let count = 0;
  let cur = startMs;
  while (cur <= dayMs) {
    if (set.has(weekdayOf(cur))) count++;
    cur = addDaysMs(cur, 1);
  }
  return count - 1; // anchor is index 0
}

/**
 * 0-based index of `dayMs` among occurrences starting at `startMs`; -1 when
 * `dayMs` is not an occurrence under `rule` (ignoring `count`/`endDate`
 * bounds — see `isOccurrenceOn`).
 */
export function occurrenceIndex(startMs: number, dayMs: number, rule: RecurrenceRule): number {
  const d = startOfDayMs(dayMs);
  const s = startOfDayMs(startMs);
  if (d < s) return -1;
  switch (rule.type) {
    case "daily":
      return daysBetween(s, d);
    case "interval":
      return intervalIndex(s, d, rule.intervalDays ?? 1);
    case "weekly":
      return weekdayIndex(s, d, rule.weekdays ?? []);
    case "monthly":
      return monthlyIndex(s, d, rule.monthDay);
    default:
      return -1;
  }
}

/** True if `dayMs` is an occurrence of `rule`, whose series started at `startMs`. */
export function isOccurrenceOn(startMs: number, dayMs: number, rule: RecurrenceRule): boolean {
  const index = occurrenceIndex(startMs, dayMs, rule);
  if (index < 0) return false;
  if (rule.count != null && index >= rule.count) return false;
  if (rule.endDate != null && startOfDayMs(dayMs) > startOfDayMs(rule.endDate)) return false;
  return true;
}

/**
 * Synthesizes every occurrence of `task`'s recurrence rule that falls within
 * `[rangeStart, rangeEnd]` (inclusive, day granularity). Returns `[]` for a
 * non-recurring task. Bounded by the range regardless of `count`/`endDate`,
 * so an unbounded series never iterates further than the visible window.
 */
export function expandRecurrence(
  task: RecurrenceTaskLike,
  rangeStart: number,
  rangeEnd: number,
): CalendarOccurrence[] {
  const rule = parseRecurrence(task.recurrence ?? null);
  if (!rule) return [];

  const startDay = startOfDayMs(task.date);
  const rStart = startOfDayMs(rangeStart);
  const rEnd = startOfDayMs(rangeEnd);
  if (rEnd < rStart || rEnd < startDay) return [];

  const excluded = new Set(rule.excludedDates ?? []);
  const out: CalendarOccurrence[] = [];
  let cur = Math.max(startDay, rStart);
  while (cur <= rEnd) {
    if (!excluded.has(cur) && isOccurrenceOn(startDay, cur, rule)) {
      out.push({
        id: `${task.id}:${cur}`,
        taskId: task.id,
        seriesId: task.seriesId ?? task.id,
        date: cur,
        title: task.title,
        done: false,
        dueDate: task.dueDate != null ? cur : null,
        startTimeMinutes: task.startTimeMinutes ?? null,
        endTimeMinutes: task.endTimeMinutes ?? null,
        notes: task.notes ?? null,
        isVirtual: true,
      });
    }
    cur = addDaysMs(cur, 1);
  }
  return out;
}

function isDone(task: RecurrenceTaskLike): boolean {
  return task.done === 1 || task.taskStatus === 1;
}

/**
 * Merges raw task rows (anchors, one-offs, and whatever's already
 * materialized) with synthesized recurrence occurrences for `[rangeStart,
 * rangeEnd]`. A real row always wins over a synthesized one for the same
 * (series, day) — mirroring mobile's materializer, which skips generating an
 * occurrence that already exists.
 */
export function buildOccurrences(
  tasks: RecurrenceTaskLike[],
  rangeStart: number,
  rangeEnd: number,
): CalendarOccurrence[] {
  const rStart = startOfDayMs(rangeStart);
  const rEnd = startOfDayMs(rangeEnd);
  const real: CalendarOccurrence[] = [];
  const realKeys = new Set<string>();

  for (const t of tasks) {
    const day = startOfDayMs(t.dueDate ?? t.date);
    if (day < rStart || day > rEnd) continue;
    real.push({
      id: t.id,
      taskId: t.id,
      seriesId: t.seriesId ?? null,
      date: day,
      title: t.title,
      done: isDone(t),
      dueDate: t.dueDate ?? null,
      startTimeMinutes: t.startTimeMinutes ?? null,
      endTimeMinutes: t.endTimeMinutes ?? null,
      notes: t.notes ?? null,
      isVirtual: false,
    });
    realKeys.add(`${t.seriesId ?? t.id}:${day}`);
  }

  const virtual: CalendarOccurrence[] = [];
  for (const t of tasks) {
    if (!t.recurrence) continue;
    for (const occ of expandRecurrence(t, rangeStart, rangeEnd)) {
      const key = `${occ.seriesId}:${occ.date}`;
      if (realKeys.has(key)) continue;
      virtual.push(occ);
    }
  }

  return [...real, ...virtual].sort((a, b) => a.date - b.date);
}
