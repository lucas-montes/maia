import { api, type Todo } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { parseTags } from "../lib/dom";
import { todosAtom } from "../lib/stores";
import { backToDashboard, emptyState, panelClose, panelOpen, statusBanner } from "./shared";

function priorityChip(p: string): string {
  switch (p) {
    case "high":
      return `<span class="badge badge-sm badge-err">${p}</span>`;
    case "medium":
      return `<span class="badge badge-sm badge-warn">${p}</span>`;
    case "low":
      return `<span class="badge badge-sm badge-ok">${p}</span>`;
    default:
      return `<span class="badge badge-sm badge-muted">${escapeHtml(p)}</span>`;
  }
}

function todoRow(t: Todo, goalTitle: string | undefined): string {
  const done = t.completed_at !== null;
  return `
    <div class="list-row ${done ? "opacity-60" : ""}">
      <span class="mt-1 flex h-4 w-4 items-center justify-center text-sm ${done ? "text-ok-400" : "text-ink-600"}">
        ${done ? "✓" : "○"}
      </span>
      <div class="min-w-0 flex-1">
        <span class="block font-medium ${done ? "text-ink-500 line-through" : "text-ink-100"}">${escapeHtml(t.title)}</span>
        <div class="mt-1 flex flex-wrap items-center gap-2 text-sm text-ink-400">
          ${t.priority ? priorityChip(t.priority) : ""}
          ${t.due_date ? `<span>Due: ${escapeHtml(t.due_date)}</span>` : ""}
          ${parseTags(t.tags).length ? `<span class="text-xs text-accent-400">${escapeHtml(parseTags(t.tags).join(", "))}</span>` : ""}
          ${goalTitle ? `<span class="text-accent-400">${escapeHtml(goalTitle)}</span>` : ""}
        </div>
      </div>
    </div>`;
}

/** Tasks — read-only list with filters (tasks are managed on mobile). */
export function mount(root: HTMLElement): () => void {
  // Tasks and goals load together: the list shows each task's goal title.
  let goalTitles = new Map<number, string>();

  root.innerHTML = `
    ${panelOpen("Tasks", "Manage your tasks and priorities")}
      <div data-slot="status"></div>
      <div class="flex items-center gap-3">
        <div class="join">
          <button class="btn btn-sm join-item" type="button" data-filter="all">All</button>
          <button class="btn btn-sm join-item" type="button" data-filter="active">Active</button>
          <button class="btn btn-sm join-item" type="button" data-filter="completed">Completed</button>
        </div>
        <span class="ml-auto text-sm text-ink-400" data-slot="count"></span>
      </div>
      <div class="space-y-2" data-slot="list"></div>
      <div>${backToDashboard()}</div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const countEl = root.querySelector("[data-slot='count']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;

  const render = (): void => {
    const s = todosAtom.get();
    setHtml(statusEl, statusBanner(s));
    countEl.textContent = `${s.items.length} task${s.items.length !== 1 ? "s" : ""}`;

    let filtered = s.items;
    if (s.filter === "active") filtered = s.items.filter((t) => t.completed_at === null);
    else if (s.filter === "completed") filtered = s.items.filter((t) => t.completed_at !== null);

    const goalTitlesMap = goalTitles;
    setHtml(
      listEl,
      filtered.length
        ? filtered.map((t) => todoRow(t, goalTitlesMap.get(t.goal_id ?? -1))).join("")
        : emptyState("No tasks yet. Tasks are managed on mobile."),
    );
    // Highlight the active filter button
    root.querySelectorAll<HTMLElement>("[data-filter]").forEach((b) => {
      b.classList.toggle("btn-active", b.dataset.filter === s.filter);
    });
    refreshIcons(listEl);
  };

  root.addEventListener("click", (event) => {
    const button = (event.target as HTMLElement).closest<HTMLElement>("[data-filter]");
    if (!button?.dataset.filter) return;
    todosAtom.set({ ...todosAtom.get(), filter: button.dataset.filter as "all" | "active" | "completed" });
  });

  render();

  api
    .listGoals()
    .then((res) => {
      if (res.success && res.data) goalTitles = new Map(res.data.map((g) => [g.id, g.title]));
      render();
    })
    .catch(() => {});
  api
    .listTasks()
    .then((res) => {
      if (res.success && res.data) todosAtom.set({ ...todosAtom.get(), items: res.data, status: "idle", message: "" });
      else todosAtom.set({ ...todosAtom.get(), status: "error", message: res.error ?? "Failed to load tasks" });
    })
    .catch((e) => todosAtom.set({ ...todosAtom.get(), status: "error", message: String(e) }));

  const unsub = todosAtom.subscribe(render);
  return () => {
    unsub();
  };
}
