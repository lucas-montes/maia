import { api, type TaskDetail } from "../lib/api";
import { escapeHtml, formatEpoch } from "../lib/dom";
import { describeRecurrence, parseRecurrence } from "../lib/recurrence";
import { taskDetailAtom } from "../lib/stores";

/**
 * The read-only detail page for one task, shared by the Tasks view and the
 * Calendar view so a task opened from either reads the same.
 *
 * The desktop app opens the database read-only — the sync server is the only
 * writer — so nothing here edits anything. Tasks are managed on the phone.
 */

/** Opens a task's detail page and loads it. */
export async function openTaskDetail(
  id: string,
  opts: { occurrenceDate?: number | null; occurrenceVirtual?: boolean } = {},
): Promise<void> {
  taskDetailAtom.set({
    taskId: id,
    occurrenceDate: opts.occurrenceDate ?? null,
    occurrenceVirtual: opts.occurrenceVirtual ?? false,
    detail: null,
    status: "loading",
    message: "",
  });
  try {
    const res = await api.getTask(id);
    if (res.success && res.data) {
      taskDetailAtom.set({ ...taskDetailAtom.get(), detail: res.data, status: "idle", message: "" });
    } else {
      taskDetailAtom.set({ ...taskDetailAtom.get(), status: "error", message: res.error ?? "Failed to load task" });
    }
  } catch (e) {
    taskDetailAtom.set({ ...taskDetailAtom.get(), status: "error", message: `Failed to load task: ${e}` });
  }
}

/** Closes the detail page and returns to whatever opened it. */
export function closeTaskDetail(): void {
  taskDetailAtom.set({
    taskId: null,
    occurrenceDate: null,
    occurrenceVirtual: false,
    detail: null,
    status: "idle",
    message: "",
  });
}

/** Minutes since local midnight -> "14:30". */
export function formatClock(minutes: number | null | undefined): string {
  if (minutes == null) return "";
  const h = Math.floor(minutes / 60) % 24;
  const m = minutes % 60;
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
}

function formatDay(ms: number | null | undefined): string {
  if (ms == null) return "";
  return new Date(ms).toLocaleDateString(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
    year: "numeric",
  });
}

/** The task's lifecycle: 0 pending, 1 done, 2 cancelled. */
function taskState(d: TaskDetail): "pending" | "done" | "cancelled" {
  if (d.taskStatus != null) {
    if (d.taskStatus === 1) return "done";
    if (d.taskStatus === 2) return "cancelled";
    return "pending";
  }
  return d.done === 1 ? "done" : "pending";
}

function stateBadge(d: TaskDetail): string {
  const state = taskState(d);
  if (state === "done") return `<span class="badge badge-sm badge-ok">done</span>`;
  if (state === "cancelled") return `<span class="badge badge-sm badge-err">cancelled</span>`;
  return `<span class="badge badge-sm badge-muted">pending</span>`;
}

function metaRow(label: string, value: string): string {
  if (!value) return "";
  return `<div><span class="text-ink-500">${escapeHtml(label)}:</span> <span class="text-ink-300">${escapeHtml(value)}</span></div>`;
}

function section(title: string, body: string): string {
  if (!body) return "";
  return `<div>
    <h3 class="mb-2 text-sm font-medium text-ink-200">${escapeHtml(title)}</h3>
    ${body}
  </div>`;
}

function tagChips(tags: TaskDetail["tags"]): string {
  if (!tags.length) return "";
  return `<div class="flex flex-wrap gap-1.5">${tags
    .map((t) => {
      const color = t.color != null ? `#${t.color.toString(16).padStart(6, "0")}` : null;
      const style = color ? ` style="color:${color};border-color:${color}55"` : "";
      return `<span class="chip"${style}>${escapeHtml(t.name)}</span>`;
    })
    .join("")}</div>`;
}

function seriesList(d: TaskDetail): string {
  if (!d.series.length) return "";
  const rows = d.series
    .map((s) => {
      const done = s.done === 1 || s.taskStatus === 1;
      const current = s.id === d.id;
      return `<li class="flex items-center gap-2 text-sm ${current ? "text-ink-100" : "text-ink-400"}">
        <span class="${done ? "text-ok-400" : "text-ink-600"}">${done ? "✓" : "○"}</span>
        <span class="${current ? "font-medium" : ""}">${escapeHtml(formatDay(s.date))}</span>
        ${current ? `<span class="badge badge-xs badge-muted">this task</span>` : ""}
      </li>`;
    })
    .join("");
  return `<ul class="space-y-1">${rows}</ul>`;
}

export interface TaskDetailOptions {
  /** The day of the calendar entry this was opened from, when it was. */
  occurrenceDate?: number | null;
  /** True when that entry is a synthesized occurrence with no row of its own. */
  occurrenceVirtual?: boolean;
}

/**
 * The task detail body. `d` may be null while the task is still loading.
 */
export function taskDetailHtml(d: TaskDetail | null, opts: TaskDetailOptions = {}): string {
  if (!d) return "";

  const rule = parseRecurrence(d.recurrence);
  const timeBlock =
    d.startTimeMinutes != null
      ? d.endTimeMinutes != null
        ? `${formatClock(d.startTimeMinutes)}–${formatClock(d.endTimeMinutes)}`
        : formatClock(d.startTimeMinutes)
      : "";
  const dueTime = d.dueTimeMinutes != null ? formatClock(d.dueTimeMinutes) : "";

  const occurrenceLine =
    opts.occurrenceDate != null
      ? `<div class="mt-1 flex items-center gap-2 text-sm text-ink-400">
          <i data-lucide="calendar-days" class="h-3.5 w-3.5"></i>
          <span>Occurrence on ${escapeHtml(formatDay(opts.occurrenceDate))}</span>
          ${opts.occurrenceVirtual ? `<span class="badge badge-xs badge-muted">not saved as a row</span>` : ""}
        </div>`
      : "";

  const meta = `<div class="grid grid-cols-2 gap-x-4 gap-y-2 text-sm">
    ${metaRow("Day", formatDay(d.date))}
    ${metaRow("Due", d.dueDate != null ? formatDay(d.dueDate) : "")}
    ${metaRow("Time", timeBlock)}
    ${metaRow("Due at", dueTime)}
    ${metaRow("Repeats", rule ? describeRecurrence(rule) : "")}
    ${metaRow("Carry over", d.carryOver ? "yes" : "no")}
    ${metaRow("Sort order", String(d.sortOrder))}
    ${metaRow("Created", formatEpoch(d.createdAt))}
    ${metaRow("Updated", formatEpoch(d.updatedAt))}
  </div>`;

  const notes = d.notes && d.notes.trim() ? `<p class="whitespace-pre-wrap text-sm text-ink-200">${escapeHtml(d.notes)}</p>` : "";

  const links: string[] = [];
  if (d.workoutName) links.push(`<div><span class="text-ink-500">Workout:</span> <span class="text-ink-300">${escapeHtml(d.workoutName)}</span></div>`);
  if (d.workoutTemplateName) links.push(`<div><span class="text-ink-500">Workout template:</span> <span class="text-ink-300">${escapeHtml(d.workoutTemplateName)}</span></div>`);
  if (d.linkedGoals.length) {
    links.push(
      `<div class="flex flex-wrap items-center gap-1.5">
        <span class="text-ink-500">Goals:</span>
        ${d.linkedGoals.map((g) => `<span class="chip">${escapeHtml(g.title)}</span>`).join("")}
      </div>`,
    );
  }
  if (d.linkedNotes.length) {
    links.push(
      `<div class="flex flex-wrap items-center gap-1.5">
        <span class="text-ink-500">Notes:</span>
        ${d.linkedNotes.map((n) => `<span class="chip">${escapeHtml(n.title || "Untitled")}</span>`).join("")}
      </div>`,
    );
  }

  return `
    <div class="rounded border border-surface-800 bg-base-100 p-4">
      <div class="mb-3 flex flex-wrap items-center gap-2">
        <h3 class="text-base font-medium text-ink-100">${escapeHtml(d.title)}</h3>
        ${stateBadge(d)}
      </div>
      ${occurrenceLine}
      <div class="mt-3">${meta}</div>
    </div>

    ${section("Notes", notes)}
    ${section("Tags", tagChips(d.tags))}
    ${section("Linked to", links.join(""))}
    ${section("Series occurrences", seriesList(d))}

    <div class="text-xs text-ink-600">ID: ${escapeHtml(d.id)}</div>
  `;
}
