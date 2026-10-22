import { api, type Todo } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { todosAtom } from "../lib/stores";
import { backToDashboard, emptyState, panelClose, panelOpen, statusBanner } from "./shared";

function formatDateMs(ms: number): string {
  const d = new Date(ms);
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function todoRow(t: Todo): string {
  const done = t.done === 1 || t.taskStatus === 1;
  return `
    <div class="list-row ${done ? "opacity-60" : ""}">
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
    </div>`;
}

/** Tasks — read-only list with filters (tasks are managed on mobile). */
export function mount(root: HTMLElement): () => void {
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
    const done = (t: Todo): boolean => t.done === 1 || t.taskStatus === 1;
    countEl.textContent = `${s.items.filter((t) => !done(t)).length} active task${s.items.filter((t) => !done(t)).length !== 1 ? "s" : ""}`;

    let filtered = s.items;
    if (s.filter === "active") filtered = s.items.filter((t) => !done(t));
    else if (s.filter === "completed") filtered = s.items.filter(done);

    setHtml(
      listEl,
      filtered.length ? filtered.map((t) => todoRow(t)).join("") : emptyState("No tasks yet. Tasks are managed on mobile."),
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
