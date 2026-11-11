import { api } from "../lib/api";
import { addDaysMs, buildOccurrences, startOfDayMs, type CalendarOccurrence } from "../lib/recurrence";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { calendarAtom, taskDetailAtom, type CalendarGranularity } from "../lib/stores";
import { backToDashboard, emptyState, panelClose, panelOpen, statusBanner } from "./shared";
import { closeTaskDetail, openTaskDetail, taskDetailHtml } from "./taskDetail";

/**
 * Read-only month-grid calendar with week/day drill-down, plus an "up next"
 * panel showing the nearest upcoming time-blocked task. No create/edit here
 * by design (see context/plans/desktop-task-calendar.md) — editing a task
 * still goes through the Tasks list/form.
 *
 * Recurring tasks are expanded client-side for whatever range is on screen
 * (`buildOccurrences`, see lib/recurrence.ts) — the backend only returns raw
 * rows (anchors + one-offs + already-materialized occurrences).
 */

// ---- Date helpers (Monday-start weeks, all local-time) ----

function monthFirstDay(ms: number): number {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), 1).getTime();
}

function monthLastDay(ms: number): number {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth() + 1, 0).getTime();
}

function addMonths(ms: number, n: number): number {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth() + n, 1).getTime();
}

/** Monday=0 .. Sunday=6 offset of the day `ms` falls in. */
function mondayOffset(ms: number): number {
  const js = new Date(ms).getDay(); // 0=Sun..6=Sat
  return (js + 6) % 7;
}

function weekStart(ms: number): number {
  return addDaysMs(startOfDayMs(ms), -mondayOffset(ms));
}

function weekEnd(ms: number): number {
  return addDaysMs(weekStart(ms), 6);
}

function isSameDay(a: number, b: number): boolean {
  return startOfDayMs(a) === startOfDayMs(b);
}

function minutesToClock(minutes: number): string {
  const h = Math.floor(minutes / 60) % 24;
  const m = minutes % 60;
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
}

interface TimedLike {
  startTimeMinutes: number | null;
  endTimeMinutes: number | null;
  dueDate: number | null;
}

function timeLabel(occ: TimedLike): string {
  if (occ.startTimeMinutes != null) {
    return occ.endTimeMinutes != null
      ? `${minutesToClock(occ.startTimeMinutes)}–${minutesToClock(occ.endTimeMinutes)}`
      : minutesToClock(occ.startTimeMinutes);
  }
  if (occ.dueDate != null) return "Due";
  return "Anytime";
}

function formatMonthHeading(ms: number): string {
  return new Date(ms).toLocaleDateString(undefined, { month: "long", year: "numeric" });
}

function formatWeekHeading(ms: number): string {
  const s = weekStart(ms);
  const e = weekEnd(ms);
  const sameMonth = new Date(s).getMonth() === new Date(e).getMonth();
  const startFmt = new Date(s).toLocaleDateString(undefined, { month: "short", day: "numeric" });
  const endFmt = new Date(e).toLocaleDateString(undefined, sameMonth ? { day: "numeric" } : { month: "short", day: "numeric" });
  const year = new Date(e).getFullYear();
  return `${startFmt} – ${endFmt}, ${year}`;
}

function formatDayHeading(ms: number): string {
  return new Date(ms).toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric", year: "numeric" });
}

function formatShortDay(ms: number): string {
  return new Date(ms).toLocaleDateString(undefined, { weekday: "short", day: "numeric" });
}

const WEEKDAY_LABELS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/** The fetch range for whichever granularity is on screen — bounds the
 *  recurrence expansion so an endless series never gets walked past what's
 *  actually visible. */
function visibleRange(granularity: CalendarGranularity, selectedDate: number): { start: number; end: number } {
  if (granularity === "month") {
    return { start: weekStart(monthFirstDay(selectedDate)), end: weekEnd(monthLastDay(selectedDate)) };
  }
  if (granularity === "week") {
    return { start: weekStart(selectedDate), end: weekEnd(selectedDate) };
  }
  return { start: startOfDayMs(selectedDate), end: startOfDayMs(selectedDate) };
}

// Up-next looks ahead independently of whatever range is on screen.
const UP_NEXT_LOOKAHEAD_DAYS = 60;

function occurrenceBadge(occ: CalendarOccurrence, compact: boolean): string {
  const doneClass = occ.done ? "opacity-50 line-through" : "";
  const time = timeLabel(occ);
  // The whole entry opens the task: `taskId` is the row the occurrence came
  // from, which for a synthesized occurrence is the series anchor.
  const open = `data-occurrence-task-id="${escapeHtml(occ.taskId)}" data-occurrence-date="${occ.date}" data-occurrence-virtual="${occ.isVirtual}"`;
  if (compact) {
    return `<div class="cursor-pointer truncate rounded px-1.5 py-0.5 text-xs hover:bg-base-300 ${occ.done ? "bg-base-300/50" : "bg-primary/15"} ${doneClass}" title="${escapeHtml(occ.title)}" ${open}>
      ${occ.startTimeMinutes != null ? `<span class="font-mono opacity-70">${escapeHtml(minutesToClock(occ.startTimeMinutes))}</span> ` : ""}${escapeHtml(occ.title)}
    </div>`;
  }
  return `<button class="list-row w-full items-start text-left" type="button" ${open}>
    <span class="mt-1 flex h-4 w-4 items-center justify-center text-sm ${occ.done ? "text-ok-400" : "text-ink-600"}">${occ.done ? "✓" : "○"}</span>
    <div class="min-w-0 flex-1">
      <div class="flex items-center gap-2">
        <span class="font-medium ${doneClass}">${escapeHtml(occ.title)}</span>
        ${occ.isVirtual ? `<i data-lucide="repeat" class="h-3 w-3 opacity-50" title="Recurring"></i>` : ""}
      </div>
      <div class="mt-1 flex flex-wrap items-center gap-2 text-sm text-ink-400">
        <span class="font-mono">${escapeHtml(time)}</span>
        ${occ.notes ? `<span class="truncate">${escapeHtml(occ.notes)}</span>` : ""}
      </div>
    </div>
  </button>`;
}

export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    <div data-slot="task-detail" class="hidden"></div>
    <div data-slot="calendar">
    ${panelOpen("Calendar", "Month and week view of your tasks — read-only; edit from Tasks")}
      <div data-slot="status"></div>
      <div class="flex flex-wrap items-center gap-3">
        <div class="join">
          <button class="btn btn-sm join-item" type="button" data-cal-nav="prev"><i data-lucide="chevron-left" class="h-4 w-4"></i></button>
          <button class="btn btn-sm join-item" type="button" data-cal-nav="today">Today</button>
          <button class="btn btn-sm join-item" type="button" data-cal-nav="next"><i data-lucide="chevron-right" class="h-4 w-4"></i></button>
        </div>
        <span class="text-lg font-semibold" data-slot="heading"></span>
        <div class="join ml-auto">
          <button class="btn btn-sm join-item" type="button" data-cal-granularity="month">Month</button>
          <button class="btn btn-sm join-item" type="button" data-cal-granularity="week">Week</button>
          <button class="btn btn-sm join-item" type="button" data-cal-granularity="day">Day</button>
        </div>
      </div>
      <div class="flex min-w-0 flex-col gap-4 lg:flex-row">
        <div class="min-w-0 flex-1" data-slot="grid"></div>
        <div class="w-full shrink-0 lg:w-72" data-slot="upnext"></div>
      </div>
      <div>${backToDashboard()}</div>
    ${panelClose}
    </div>
  `;

  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const headingEl = root.querySelector("[data-slot='heading']") as HTMLElement;
  const gridEl = root.querySelector("[data-slot='grid']") as HTMLElement;
  const upNextEl = root.querySelector("[data-slot='upnext']") as HTMLElement;
  const calendarEl = root.querySelector("[data-slot='calendar']") as HTMLElement;
  const detailEl = root.querySelector("[data-slot='task-detail']") as HTMLElement;

  /** The task detail replaces the calendar while a task is open; the
   *  calendar keeps its own state, so going back returns to the same
   *  granularity and day. */
  function renderTaskDetail(): void {
    const d = taskDetailAtom.get();
    const open = d.taskId != null;
    detailEl.classList.toggle("hidden", !open);
    calendarEl.classList.toggle("hidden", open);
    if (!open) return;
    setHtml(
      detailEl,
      `
      ${panelOpen(d.detail?.title || "Task", "Read-only — managed on mobile", "panel-fit")}
        ${statusBanner(d)}
        <button class="btn btn-ghost btn-sm self-start" type="button" data-action="task-back">← Back to calendar</button>
        <div class="min-h-0 flex-1 overflow-auto">
          ${d.detail ? taskDetailHtml(d.detail, { occurrenceDate: d.occurrenceDate, occurrenceVirtual: d.occurrenceVirtual }) : ""}
        </div>
      ${panelClose}`,
    );
    refreshIcons(detailEl);
  }

  // ---- Grid rendering ----

  function renderMonth(selectedDate: number, byDay: Map<number, CalendarOccurrence[]>): string {
    const gridStartMs = weekStart(monthFirstDay(selectedDate));
    const gridEndMs = weekEnd(monthLastDay(selectedDate));
    const monthIdx = new Date(selectedDate).getMonth();
    const todayKey = startOfDayMs(Date.now());

    const headerRow = WEEKDAY_LABELS.map((w) => `<div class="p-1 text-center text-xs font-medium uppercase text-ink-500">${w}</div>`).join("");

    const cells: string[] = [];
    let cur = gridStartMs;
    while (cur <= gridEndMs) {
      const occs = byDay.get(cur) ?? [];
      const inMonth = new Date(cur).getMonth() === monthIdx;
      const isToday = cur === todayKey;
      const visible = occs.slice(0, 3);
      const overflow = occs.length - visible.length;
      cells.push(`
        <button type="button" class="flex min-h-24 flex-col items-stretch gap-1 rounded border border-base-300 p-1.5 text-left transition-colors hover:bg-base-200 ${inMonth ? "" : "opacity-40"} ${isToday ? "ring-2 ring-primary" : ""}" data-cal-day="${cur}">
          <span class="text-xs font-semibold ${isToday ? "text-primary" : ""}">${new Date(cur).getDate()}</span>
          <div class="flex min-h-0 flex-1 flex-col gap-0.5 overflow-hidden">
            ${visible.map((o) => occurrenceBadge(o, true)).join("")}
            ${overflow > 0 ? `<span class="text-xs text-ink-500">+${overflow} more</span>` : ""}
          </div>
        </button>`);
      cur = addDaysMs(cur, 1);
    }

    return `<div class="grid grid-cols-7 gap-px">${headerRow}</div><div class="grid grid-cols-7 gap-1">${cells.join("")}</div>`;
  }

  function renderWeek(selectedDate: number, byDay: Map<number, CalendarOccurrence[]>): string {
    const start = weekStart(selectedDate);
    const todayKey = startOfDayMs(Date.now());
    const cols: string[] = [];
    for (let i = 0; i < 7; i++) {
      const day = addDaysMs(start, i);
      const occs = byDay.get(day) ?? [];
      const isToday = day === todayKey;
      cols.push(`
        <div class="flex min-w-0 flex-col gap-1 rounded border border-base-300 p-2 ${isToday ? "ring-2 ring-primary" : ""}">
          <button type="button" class="truncate text-left text-sm font-semibold ${isToday ? "text-primary" : ""}" data-cal-day="${day}">
            ${escapeHtml(formatShortDay(day))}
          </button>
          <div class="flex min-h-16 flex-col gap-1">
            ${occs.length ? occs.map((o) => occurrenceBadge(o, true)).join("") : `<span class="text-xs text-ink-500">—</span>`}
          </div>
        </div>`);
    }
    return `<div class="grid grid-cols-1 gap-2 sm:grid-cols-7">${cols.join("")}</div>`;
  }

  function renderDay(selectedDate: number, byDay: Map<number, CalendarOccurrence[]>): string {
    const day = startOfDayMs(selectedDate);
    const occs = (byDay.get(day) ?? []).slice().sort((a, b) => (a.startTimeMinutes ?? -1) - (b.startTimeMinutes ?? -1));
    return `<div class="space-y-2">${occs.length ? occs.map((o) => occurrenceBadge(o, false)).join("") : emptyState("Nothing scheduled this day.")}</div>`;
  }

  function renderUpNext(): string {
    const s = calendarAtom.get();
    if (s.upNextStatus === "loading") return `<div class="rounded border border-base-300 p-3 text-sm text-ink-400">Loading…</div>`;
    const next = s.upNext[0];
    if (!next) {
      return `<div class="rounded border border-base-300 p-3">
        <div class="mb-1 flex items-center gap-2 text-sm font-semibold"><i data-lucide="clock" class="h-4 w-4"></i> Up next</div>
        <div class="text-sm text-ink-500">Nothing time-blocked in the next ${UP_NEXT_LOOKAHEAD_DAYS} days.</div>
      </div>`;
    }
    const day = startOfDayMs(next.date);
    const dayLabel = isSameDay(day, Date.now()) ? "Today" : formatShortDay(day);
    return `<div class="rounded border border-base-300 p-3">
      <div class="mb-2 flex items-center gap-2 text-sm font-semibold"><i data-lucide="clock" class="h-4 w-4"></i> Up next</div>
      <div class="font-medium">${escapeHtml(next.title)}</div>
      <div class="mt-1 text-sm text-ink-400">${escapeHtml(dayLabel)} · <span class="font-mono">${escapeHtml(timeLabel(next))}</span></div>
      ${next.notes ? `<div class="mt-1 truncate text-sm text-ink-500">${escapeHtml(next.notes)}</div>` : ""}
    </div>`;
  }

  function render(): void {
    const s = calendarAtom.get();
    setHtml(statusEl, statusBanner(s));

    headingEl.textContent =
      s.granularity === "month"
        ? formatMonthHeading(s.selectedDate)
        : s.granularity === "week"
          ? formatWeekHeading(s.selectedDate)
          : formatDayHeading(s.selectedDate);

    root.querySelectorAll<HTMLElement>("[data-cal-granularity]").forEach((b) => {
      b.classList.toggle("btn-active", b.dataset.calGranularity === s.granularity);
    });

    const { start, end } = visibleRange(s.granularity, s.selectedDate);
    const occurrences = buildOccurrences(s.tasks, start, end);
    const byDay = new Map<number, CalendarOccurrence[]>();
    for (const occ of occurrences) {
      const list = byDay.get(occ.date);
      if (list) list.push(occ);
      else byDay.set(occ.date, [occ]);
    }
    for (const list of byDay.values()) {
      list.sort((a, b) => (a.startTimeMinutes ?? -1) - (b.startTimeMinutes ?? -1));
    }

    setHtml(
      gridEl,
      s.granularity === "month"
        ? renderMonth(s.selectedDate, byDay)
        : s.granularity === "week"
          ? renderWeek(s.selectedDate, byDay)
          : renderDay(s.selectedDate, byDay),
    );
    setHtml(upNextEl, renderUpNext());
    refreshIcons(root);
  }

  // ---- Data loading ----

  function loadRange(): void {
    const s = calendarAtom.get();
    const { start, end } = visibleRange(s.granularity, s.selectedDate);
    calendarAtom.set({ ...calendarAtom.get(), status: "loading" });
    api
      .listTasksRange({ start, end })
      .then((res) => {
        if (res.success && res.data) {
          calendarAtom.set({ ...calendarAtom.get(), tasks: res.data, status: "idle", message: "" });
        } else {
          calendarAtom.set({ ...calendarAtom.get(), status: "error", message: res.error ?? "Failed to load calendar" });
        }
      })
      .catch((e) => calendarAtom.set({ ...calendarAtom.get(), status: "error", message: String(e) }));
  }

  function loadUpNext(): void {
    const today = startOfDayMs(Date.now());
    const end = addDaysMs(today, UP_NEXT_LOOKAHEAD_DAYS);
    calendarAtom.set({ ...calendarAtom.get(), upNextStatus: "loading" });
    api
      .listTasksRange({ start: today, end })
      .then((res) => {
        if (!res.success || !res.data) {
          calendarAtom.set({ ...calendarAtom.get(), upNextStatus: "error" });
          return;
        }
        const occurrences = buildOccurrences(res.data, today, end);
        const now = Date.now();
        const candidates = occurrences
          .filter((o) => !o.done && o.startTimeMinutes != null)
          .map((o) => ({ occ: o, at: o.date + (o.startTimeMinutes as number) * 60_000 }))
          .filter((c) => c.at >= now)
          .sort((a, b) => a.at - b.at);
        const upNext = candidates.slice(0, 1).map((c) => ({
          id: c.occ.id,
          date: c.occ.date,
          title: c.occ.title,
          done: 0,
          taskStatus: null,
          dueDate: c.occ.dueDate,
          dueTimeMinutes: null,
          startTimeMinutes: c.occ.startTimeMinutes,
          endTimeMinutes: c.occ.endTimeMinutes,
          notes: c.occ.notes,
          recurrence: null,
          seriesId: c.occ.seriesId,
          createdAt: 0,
          updatedAt: 0,
        }));
        calendarAtom.set({ ...calendarAtom.get(), upNext, upNextStatus: "idle" });
      })
      .catch(() => calendarAtom.set({ ...calendarAtom.get(), upNextStatus: "error" }));
  }

  // ---- Interaction ----

  const onClick = (event: MouseEvent): void => {
    const target = event.target as HTMLElement;

    if (target.closest<HTMLElement>("[data-action='task-back']")) {
      closeTaskDetail();
      return;
    }

    // An entry opens its task. This is checked before the day cell, because a
    // month-grid entry sits inside the cell button and would otherwise drill
    // into the day instead.
    const occurrence = target.closest<HTMLElement>("[data-occurrence-task-id]");
    if (occurrence?.dataset.occurrenceTaskId) {
      void openTaskDetail(occurrence.dataset.occurrenceTaskId, {
        occurrenceDate: Number(occurrence.dataset.occurrenceDate),
        occurrenceVirtual: occurrence.dataset.occurrenceVirtual === "true",
      });
      return;
    }

    const navBtn = target.closest<HTMLElement>("[data-cal-nav]");
    if (navBtn?.dataset.calNav) {
      const s = calendarAtom.get();
      if (navBtn.dataset.calNav === "today") {
        calendarAtom.set({ ...s, selectedDate: Date.now() });
      } else {
        const dir = navBtn.dataset.calNav === "prev" ? -1 : 1;
        const selectedDate =
          s.granularity === "month"
            ? addMonths(s.selectedDate, dir)
            : s.granularity === "week"
              ? addDaysMs(s.selectedDate, dir * 7)
              : addDaysMs(s.selectedDate, dir);
        calendarAtom.set({ ...s, selectedDate });
      }
      loadRange();
      return;
    }

    const granBtn = target.closest<HTMLElement>("[data-cal-granularity]");
    if (granBtn?.dataset.calGranularity) {
      const s = calendarAtom.get();
      calendarAtom.set({ ...s, granularity: granBtn.dataset.calGranularity as CalendarGranularity });
      loadRange();
      return;
    }

    const dayBtn = target.closest<HTMLElement>("[data-cal-day]");
    if (dayBtn?.dataset.calDay) {
      const s = calendarAtom.get();
      // Drill into the day view for whichever cell was clicked — month and
      // week cells both funnel here.
      calendarAtom.set({ ...s, granularity: "day", selectedDate: Number(dayBtn.dataset.calDay) });
      loadRange();
    }
  };

  root.addEventListener("click", onClick);
  render();
  renderTaskDetail();
  loadRange();
  loadUpNext();

  const unsub = calendarAtom.subscribe(render);
  const unsubDetail = taskDetailAtom.subscribe(renderTaskDetail);
  return () => {
    root.removeEventListener("click", onClick);
    unsub();
    unsubDetail();
  };
}
