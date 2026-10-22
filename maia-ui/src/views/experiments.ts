import { api } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { experimentsAtom } from "../lib/stores";
import { isEnvelopeFailure, unwrapList } from "../lib/api";
import { panelClose, panelOpen, statusBanner } from "./shared";

/** Fake activity heatmap, shaded like the mobile app's. */
function heatmap(): string {
  const classes = ["bg-surface-800", "bg-ok-900", "bg-ok-700", "bg-ok-500", "bg-ok-300"];
  const cells = Array.from({ length: 30 })
    .map((_, i) => `<div class="h-4 w-4 rounded ${classes[Math.floor(Math.random() * 5)]}" title="day ${i}"></div>`)
    .join("");
  return `<div class="mt-2 grid grid-cols-7 gap-1">${cells}</div>`;
}

/** Experiments — read-only list with status, purpose, and a heatmap. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Experiments", "Linked tasks/goals/notes/tags + heatmap")}
      <div data-slot="status"></div>
      <div class="grid min-h-0 flex-1 gap-2 overflow-auto" data-slot="list"></div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;

  const render = (): void => {
    const e = experimentsAtom.get();
    setHtml(statusEl, statusBanner(e));
    setHtml(
      listEl,
      e.items
        .map(
          (ex) => `
          <div class="rounded border border-surface-800 bg-base-100 p-3">
            <div class="text-sm text-ink-100">${escapeHtml(ex.name)} — ${escapeHtml(ex.status)}</div>
            <div class="text-xs text-ink-400">${escapeHtml(ex.purpose || "")}</div>
            <div class="text-xs text-ink-500">${new Date(ex.startDate).toLocaleDateString()} → ${ex.endDate ? new Date(ex.endDate).toLocaleDateString() : "open"} • ${escapeHtml(ex.categories || "")}</div>
            ${heatmap()}
          </div>`,
        )
        .join("") || `<div class="text-xs text-ink-400">No experiments</div>`,
    );
    refreshIcons(listEl);
  };

  const load = async (): Promise<void> => {
    experimentsAtom.set({ ...experimentsAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.listExperiments();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        experimentsAtom.set({ ...experimentsAtom.get(), status: "error", message: failure });
        return;
      }
      experimentsAtom.set({ ...experimentsAtom.get(), items: unwrapList(res), status: "idle", message: "" });
    } catch (err) {
      experimentsAtom.set({ ...experimentsAtom.get(), status: "error", message: String(err) });
    }
  };

  void load();
  const unsub = experimentsAtom.subscribe(render);
  return unsub;
}
