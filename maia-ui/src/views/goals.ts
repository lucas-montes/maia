import { api, type Goal } from "../lib/api";
import { escapeHtml, formatDate, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { goalsAtom, todosAtom } from "../lib/stores";
import { backToDashboard, emptyState, panelClose, panelOpen, statusBanner } from "./shared";

function goalBadge(status: string): string {
  switch (status) {
    case "active":
      return `<span class="badge badge-sm badge-info">${status}</span>`;
    case "on-hold":
      return `<span class="badge badge-sm badge-warn">${status}</span>`;
    case "done":
      return `<span class="badge badge-sm badge-ok">${status}</span>`;
    default:
      return `<span class="badge badge-sm badge-muted">${escapeHtml(status)}</span>`;
  }
}

function goalCard(g: Goal, taskCount: number): string {
  const progress = Math.min(100, Math.max(0, g.progress));
  return `
    <div class="rounded-maia-card border border-surface-800 bg-base-100 p-4">
      <div class="flex items-start justify-between gap-3">
        <div class="min-w-0 flex-1">
          <h4 class="font-medium text-ink-100">${escapeHtml(g.title)}</h4>
          ${g.description ? `<p class="mt-1 text-sm text-ink-400">${escapeHtml(g.description)}</p>` : ""}
        </div>
        ${goalBadge(g.status || "active")}
      </div>
      <div class="mt-3 flex items-center gap-3 text-sm text-ink-400">
        ${g.deadline ? `<span>Due: ${escapeHtml(formatDate(g.deadline))}</span>` : ""}
        <span>${taskCount} task${taskCount !== 1 ? "s" : ""}</span>
      </div>
      <div class="mt-3 flex items-center gap-3">
        <progress class="progress flex-1 ${g.progress >= 100 ? "progress-success" : "progress-primary"}" value="${progress}" max="100"></progress>
        <span class="w-10 text-right font-mono text-xs text-ink-400">${g.progress}%</span>
      </div>
    </div>`;
}

/** Goals — read-only cards with task counts and a progress bar. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Goals", "Track your objectives and progress — read-only, managed on mobile")}
      <div data-slot="status"></div>
      <div class="flex items-center justify-between">
        <h3 class="font-medium text-ink-200">Goals</h3>
        <span class="text-sm text-ink-400" data-slot="count"></span>
      </div>
      <div class="space-y-3" data-slot="list"></div>
      <div>${backToDashboard()}</div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const countEl = root.querySelector("[data-slot='count']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;

  const render = (): void => {
    const s = goalsAtom.get();
    const tasks = todosAtom.get().items;
    const taskCounts = new Map<number, number>();
    for (const t of tasks) {
      if (t.goal_id !== null) taskCounts.set(t.goal_id, (taskCounts.get(t.goal_id) ?? 0) + 1);
    }
    setHtml(statusEl, statusBanner(s));
    countEl.textContent = `${s.items.length} goal${s.items.length !== 1 ? "s" : ""}`;
    setHtml(
      listEl,
      s.items.length
        ? s.items.map((g) => goalCard(g, taskCounts.get(g.id) ?? 0)).join("")
        : emptyState("No goals yet. Goals are managed on mobile."),
    );
    refreshIcons(listEl);
  };

  api
    .listGoals()
    .then((res) => {
      if (res.success && res.data) goalsAtom.set({ items: res.data, status: "idle", message: "" });
      else goalsAtom.set({ items: [], status: "error", message: res.error ?? "Failed to load goals" });
    })
    .catch((e) => goalsAtom.set({ items: [], status: "error", message: String(e) }));
  if (todosAtom.get().items.length === 0) {
    api
      .listTasks()
      .then((res) => {
        if (res.success && res.data) todosAtom.set({ ...todosAtom.get(), items: res.data, status: "idle", message: "" });
      })
      .catch(() => {});
  }

  const unsubGoals = goalsAtom.subscribe(render);
  const unsubTodos = todosAtom.subscribe(render);
  return () => {
    unsubGoals();
    unsubTodos();
  };
}
