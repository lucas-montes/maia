import { api } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { workoutsAtom } from "../lib/stores";
import { panelClose, panelOpen, statusBanner } from "./shared";
import { isEnvelopeFailure, unwrapList } from "../lib/api";

/** Workouts — read-only list from the synced database. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Workouts", "From maia.db workouts • per-tab sync")}
      <div data-slot="status"></div>
      <div class="mb-2 text-xs text-ink-500" data-slot="count"></div>
      <div class="min-h-0 flex-1 space-y-2 overflow-auto" data-slot="list"></div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const countEl = root.querySelector("[data-slot='count']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;

  const render = (): void => {
    const s = workoutsAtom.get();
    const w = s.items;
    setHtml(statusEl, statusBanner(s));
    countEl.textContent = `maia.db • ${w.length} workouts`;
    setHtml(
      listEl,
      w.slice(0, 50)
        .map(
          (x) => `
          <div class="rounded border border-surface-800 bg-base-100 p-3">
            <div class="text-sm text-ink-100">${escapeHtml(x.name)} — ${new Date(x.date).toLocaleDateString()}</div>
            <div class="text-xs text-ink-400">${escapeHtml(x.notes || "")}</div>
          </div>`,
        )
        .join("") || `<div class="text-xs text-ink-400">No workouts</div>`,
    );
    refreshIcons(listEl);
  };

  const load = async (): Promise<void> => {
    try {
      const res = await api.listWorkouts();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        workoutsAtom.set({ items: [], status: "error", message: failure });
        return;
      }
      workoutsAtom.set({ items: unwrapList(res), status: "idle", message: "" });
    } catch (e) {
      workoutsAtom.set({ items: [], status: "error", message: String(e) });
    }
  };

  void load();
  const unsub = workoutsAtom.subscribe(render);
  return unsub;
}
