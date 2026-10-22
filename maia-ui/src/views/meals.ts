import { api } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { mealsAtom } from "../lib/stores";
import { isEnvelopeFailure, unwrapList } from "../lib/api";
import { panelClose, panelOpen, statusBanner } from "./shared";

/**
 * Display name for a meal.
 *
 * A blank name is valid — most meals are not named — so fall back to a generic
 * label built from when it was eaten, mirroring the mobile list.
 */
function mealTitle(meal: any): string {
  const name = (meal.name ?? "").trim();
  if (name) return name;
  return `Meal · ${new Date(meal.eatenAt).toLocaleString()}`;
}

/** Meals — read-only list with a macro summary per meal. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Meals", "Auto-calc all macros (cal/prot/carb/fat + sodium/fiber/sugar)")}
      <div data-slot="status"></div>
      <div class="min-h-0 flex-1 space-y-2 overflow-auto" data-slot="list"></div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;

  const render = (): void => {
    const m = mealsAtom.get();
    setHtml(statusEl, statusBanner(m));
    setHtml(
      listEl,
      m.items
        .slice(0, 50)
        .map(
          (meal) => `
          <div class="rounded border border-surface-800 bg-base-100 p-3">
            <div class="text-sm text-ink-100">${escapeHtml(mealTitle(meal))}</div>
            <div class="text-xs text-ink-400">${(meal.foods || []).map((mf: any) => `${escapeHtml(mf.foodName || mf.foodId)} ${mf.amount}g · ${Number(mf.calories).toFixed(0)} kcal`).join(" • ") || "No foods"}</div>
          </div>`,
        )
        .join("") || `<div class="text-xs text-ink-400">No meals</div>`,
    );
    refreshIcons(listEl);
  };

  const load = async (): Promise<void> => {
    mealsAtom.set({ ...mealsAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.listMeals();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        mealsAtom.set({ ...mealsAtom.get(), status: "error", message: failure });
        return;
      }
      mealsAtom.set({ ...mealsAtom.get(), items: unwrapList(res), status: "idle", message: "" });
    } catch (e) {
      mealsAtom.set({ ...mealsAtom.get(), status: "error", message: String(e) });
    }
  };

  void load();
  const unsub = mealsAtom.subscribe(render);
  return unsub;
}
