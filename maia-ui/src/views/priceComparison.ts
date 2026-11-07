import { api } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { isEnvelopeFailure, unwrapList } from "../lib/api";
import { priceComparisonAtom } from "../lib/stores";
import { panelClose, panelOpen, statusBanner } from "./shared";

const NUTRIENTS: { value: string; label: string }[] = [
  { value: "protein", label: "Protein" },
  { value: "carbs", label: "Carbs" },
  { value: "fat", label: "Fat" },
  { value: "sodium", label: "Sodium" },
  { value: "fiber", label: "Fiber" },
  { value: "sugar", label: "Sugar" },
];

const PERIODS: { value: "week" | "month" | "year"; label: string }[] = [
  { value: "week", label: "This week" },
  { value: "month", label: "This month" },
  { value: "year", label: "This year" },
];

/**
 * Price Comparison (T05) — cheapest-per-nutrient-gram ranking across every
 * ingredient with price data for the selected period, derived from T04's
 * period price-per-100g. An ingredient with no priced observations in the
 * period (or no figure for the chosen nutrient) is simply absent from the
 * list rather than shown with a misleading zero.
 */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Price Comparison", "Cheapest ingredients per gram of a nutrient, this period", "panel-fit")}
      <div data-slot="status"></div>
      <div class="flex flex-wrap gap-2">
        <select id="pc-period" class="select select-bordered select-sm">
          ${PERIODS.map((p) => `<option value="${p.value}">${p.label}</option>`).join("")}
        </select>
        <select id="pc-nutrient" class="select select-bordered select-sm">
          ${NUTRIENTS.map((n) => `<option value="${n.value}">${n.label}</option>`).join("")}
        </select>
      </div>
      <div class="list-divided min-h-0 flex-1 overflow-auto" data-slot="list"></div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;
  const periodSel = root.querySelector("#pc-period") as HTMLSelectElement;
  const nutrientSel = root.querySelector("#pc-nutrient") as HTMLSelectElement;

  const render = (): void => {
    const s = priceComparisonAtom.get();
    setHtml(statusEl, statusBanner(s));
    periodSel.value = s.period;
    nutrientSel.value = s.nutrient;
    const nutrientLabel = NUTRIENTS.find((n) => n.value === s.nutrient)?.label ?? s.nutrient;
    const rows = s.items
      .map(
        (it, i) => `
        <div class="flex items-center justify-between gap-3 py-2">
          <div class="min-w-0">
            <div class="text-xs text-ink-500">#${i + 1}</div>
            <div class="truncate text-sm font-medium text-ink-100">${escapeHtml(it.name)}</div>
            <div class="text-xs text-ink-500">${it.pricePer100g.toFixed(3)} ${escapeHtml(it.currencyCode)} / 100g</div>
          </div>
          <div class="shrink-0 text-right">
            <div class="font-mono text-sm font-semibold text-accent-400">${it.pricePerNutrientGram.toFixed(4)}</div>
            <div class="text-xs text-ink-500">${escapeHtml(it.currencyCode)} / g ${escapeHtml(nutrientLabel.toLowerCase())}</div>
          </div>
        </div>`,
      )
      .join("");
    setHtml(
      listEl,
      rows ||
        `<div class="p-3 text-xs text-ink-400">No ingredient has both a price and ${escapeHtml(nutrientLabel.toLowerCase())} data for this period yet.</div>`,
    );
    refreshIcons(listEl);
  };

  const load = async (): Promise<void> => {
    const s = priceComparisonAtom.get();
    priceComparisonAtom.set({ ...s, status: "loading", message: "" });
    try {
      const res = await api.nutrientPriceComparison({ period: s.period, nutrient: s.nutrient, atMs: Date.now() });
      const failure = isEnvelopeFailure(res);
      if (failure) {
        priceComparisonAtom.set({ ...priceComparisonAtom.get(), status: "error", message: failure });
        return;
      }
      priceComparisonAtom.set({ ...priceComparisonAtom.get(), items: unwrapList(res), status: "idle", message: "" });
    } catch (e) {
      priceComparisonAtom.set({ ...priceComparisonAtom.get(), status: "error", message: String(e) });
    }
  };

  periodSel.addEventListener("change", () => {
    priceComparisonAtom.set({ ...priceComparisonAtom.get(), period: periodSel.value as "week" | "month" | "year" });
    void load();
  });
  nutrientSel.addEventListener("change", () => {
    priceComparisonAtom.set({ ...priceComparisonAtom.get(), nutrient: nutrientSel.value });
    void load();
  });

  void load();
  const unsub = priceComparisonAtom.subscribe(render);
  return unsub;
}
