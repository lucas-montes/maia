import { api, type Todo } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { taskDetailAtom, todosAtom } from "../lib/stores";
import { backToDashboard, emptyState, panelClose, panelOpen, statusBanner } from "./shared";
import { closeTaskDetail, openTaskDetail, taskDetailHtml } from "./taskDetail";

function formatDateMs(ms: number): string {
  const d = new Date(ms);
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function todoRow(t: Todo): string {
  const done = t.done === 1 || t.taskStatus === 1;
  return `
    <button class="list-row w-full text-left ${done ? "opacity-60" : ""}" type="button" data-task-id="${escapeHtml(t.id)}">
      <span class="mt-1 flex h-4 w-4 items-center justify-center text-sm ${done ? "text-ok-400" : "text-ink-600"}">
        ${done ? "✓" : "○"}
      </span>
      <div class="min-w-0 flex-1">
        <span class="block font-medium ${done ? "text-ink-500 line-through" : "text-ink-100"}">${escapeHtml(t.title)}</span>
        <div class="mt-1 flex flex-wrap items-center gap-2 text-sm text-ink-400">
          ${t.dueDate ? `<span>Due: ${escapeHtml(formatDateMs(t.dueDate))}</span>` : ""}
          ${t.carryOver ? `<span class="badge badge-sm badge-muted">carry-over</span>` : ""}
          ${t.notes ? `<span class="truncate">${escapeHtml(t.notes)}</span>` : ""}
        </div>
      </div>
    </button>`;
}

/** Tasks — read-only list with filters, and a detail page for one task
 *  (tasks are managed on mobile). */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    <div data-slot="content"></div>
  `;
  const content = root.querySelector("[data-slot='content']") as HTMLElement;

  const render = (): void => {
    const detail = taskDetailAtom.get();
    if (detail.taskId) {
      setHtml(
        content,
        `
        ${panelOpen(detail.detail?.title || "Task", "Read-only — managed on mobile", "panel-fit")}
          ${statusBanner(detail)}
          <button class="btn btn-ghost btn-sm self-start" type="button" data-action="task-back">← Back to tasks</button>
          <div class="min-h-0 flex-1 overflow-auto">
            ${detail.detail ? taskDetailHtml(detail.detail, { occurrenceDate: detail.occurrenceDate, occurrenceVirtual: detail.occurrenceVirtual }) : ""}
          </div>
        ${panelClose}`,
      );
      refreshIcons(content);
      return;
    }

    const s = todosAtom.get();
    const done = (t: Todo): boolean => t.done === 1 || t.taskStatus === 1;
    const active = s.items.filter((t) => !done(t)).length;

    let filtered = s.items;
    if (s.filter === "active") filtered = s.items.filter((t) => !done(t));
    else if (s.filter === "completed") filtered = s.items.filter(done);

    setHtml(
      content,
      `
      ${panelOpen("Tasks", "Manage your tasks and priorities", "panel-fit")}
        ${statusBanner(s)}
        <div class="flex items-center gap-3">
          <div class="join">
            <button class="btn btn-sm join-item" type="button" data-filter="all">All</button>
            <button class="btn btn-sm join-item" type="button" data-filter="active">Active</button>
            <button class="btn btn-sm join-item" type="button" data-filter="completed">Completed</button>
          </div>
          <span class="ml-auto text-sm text-ink-400">${active} active task${active !== 1 ? "s" : ""}</span>
        </div>
        <div class="min-h-0 flex-1 space-y-2 overflow-auto">
          ${
            filtered.length
              ? filtered.map((t) => todoRow(t)).join("")
              : emptyState("No tasks yet. Tasks are managed on mobile.")
          }
        </div>
        <div>${backToDashboard()}</div>
      ${panelClose}`,
    );
    // Highlight the active filter button
    content.querySelectorAll<HTMLElement>("[data-filter]").forEach((b) => {
      b.classList.toggle("btn-active", b.dataset.filter === s.filter);
    });
    refreshIcons(content);
  };

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;

    if (target.closest<HTMLElement>("[data-action='task-back']")) {
      closeTaskDetail();
      return;
    }

    const row = target.closest<HTMLElement>("[data-task-id]");
    if (row?.dataset.taskId) {
      void openTaskDetail(row.dataset.taskId);
      return;
    }

    const button = target.closest<HTMLElement>("[data-filter]");
    if (!button?.dataset.filter) return;
    todosAtom.set({ ...todosAtom.get(), filter: button.dataset.filter as "all" | "active" | "completed" });
  });

  render();

  api
    .listTasks()
    .then((res) => {
      if (res.success && res.data) todosAtom.set({ ...todosAtom.get(), items: res.data, status: "idle", message: "" });
      else todosAtom.set({ ...todosAtom.get(), status: "error", message: res.error ?? "Failed to load tasks" });
    })
    .catch((e) => todosAtom.set({ ...todosAtom.get(), status: "error", message: String(e) }));

  const unsub = todosAtom.subscribe(render);
  const unsubDetail = taskDetailAtom.subscribe(render);
  return () => {
    unsub();
    unsubDetail();
  };
}
