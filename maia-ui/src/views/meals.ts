import Chart from "chart.js/auto";
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

interface CostEstimate {
  totalBase: number;
  complete: boolean;
  currencyCode: string;
}

/** T06: per-meal cost chip. `undefined` means "not loaded yet", `null` means
 *  loaded but nothing in the meal has ever been priced -- shown as nothing
 *  rather than a misleading 0. */
function costChip(estimate: CostEstimate | null | undefined): string {
  if (estimate === undefined) return "";
  if (estimate === null) return "";
  const amount = estimate.totalBase.toFixed(2);
  const label = estimate.complete ? `~${amount} ${estimate.currencyCode}` : `~${amount} ${estimate.currencyCode} (partial)`;
  return `<span class="chip ${estimate.complete ? "" : "text-warn-400"}">${escapeHtml(label)}</span>`;
}

const ROLLUP_BUCKETS: { value: "day" | "week" | "month"; label: string; windowMs: number }[] = [
  { value: "day", label: "Last 30 days", windowMs: 30 * 24 * 3600 * 1000 },
  { value: "week", label: "Last 12 weeks", windowMs: 12 * 7 * 24 * 3600 * 1000 },
  { value: "month", label: "Last 12 months", windowMs: 365 * 24 * 3600 * 1000 },
];

/** Meals — read-only list with a macro summary, an estimated cost per meal
 *  (T06), and a day/week/month "estimated food spend" rollup chart. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Meals", "Auto-calc all macros (cal/prot/carb/fat + sodium/fiber/sugar)", "panel-fit")}
      <div data-slot="status"></div>
      <div class="rounded border border-surface-800 bg-base-100 p-3">
        <div class="mb-2 flex items-center justify-between">
          <h3 class="text-sm font-medium text-ink-200">Estimated food spend</h3>
          <select id="rollup-bucket" class="select select-bordered select-sm">
            ${ROLLUP_BUCKETS.map((b) => `<option value="${b.value}">${b.label}</option>`).join("")}
          </select>
        </div>
        <!-- Fixed-height wrapper: with "maintainAspectRatio: false", Chart.js
             sizes the canvas to its containing block, so an auto-height parent
             produces a can-grow-forever feedback loop. Same pattern as
             every chart panel in insights.ts. -->
        <div class="relative h-48"><canvas data-slot="rollup-chart"></canvas></div>
        <div class="mt-1 text-xs text-ink-500" data-slot="rollup-note"></div>
      </div>
      <div class="min-h-0 flex-1 space-y-2 overflow-auto" data-slot="list"></div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;
  const rollupCanvas = root.querySelector("[data-slot='rollup-chart']") as HTMLCanvasElement;
  const rollupNoteEl = root.querySelector("[data-slot='rollup-note']") as HTMLElement;
  const bucketSel = root.querySelector("#rollup-bucket") as HTMLSelectElement;

  let rollupChart: Chart | null = null;
  const costCache = new Map<string, CostEstimate | null>();

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
            <div class="flex items-center justify-between gap-2">
              <div class="text-sm text-ink-100">${escapeHtml(mealTitle(meal))}</div>
              ${costChip(costCache.get(meal.id))}
            </div>
            <div class="text-xs text-ink-400">${(meal.foods || []).map((mf: any) => `${escapeHtml(mf.foodName || mf.foodId)} ${mf.amount}g · ${Number(mf.calories).toFixed(0)} kcal`).join(" • ") || "No foods"}</div>
          </div>`,
        )
        .join("") || `<div class="text-xs text-ink-400">No meals</div>`,
    );
    refreshIcons(listEl);
  };

  /** Fetches cost estimates for whatever meals are visible and not already
   *  cached, then re-renders once. One command per meal -- fine at the list's
   *  own 50-row cap, and avoided entirely for meals already looked up. */
  const loadCosts = async (): Promise<void> => {
    const visible = mealsAtom.get().items.slice(0, 50);
    const missing = visible.filter((meal) => !costCache.has(meal.id));
    if (!missing.length) return;
    await Promise.all(
      missing.map(async (meal) => {
        try {
          const res = (await api.mealCostEstimate(meal.id)) as any;
          costCache.set(meal.id, res?.success === false ? null : (res?.data ?? null));
        } catch {
          costCache.set(meal.id, null);
        }
      }),
    );
    render();
  };

  const loadRollup = async (): Promise<void> => {
    const bucket = ROLLUP_BUCKETS.find((b) => b.value === bucketSel.value) ?? ROLLUP_BUCKETS[1];
    const untilMs = Date.now();
    const sinceMs = untilMs - bucket.windowMs;
    try {
      const res = (await api.foodSpendRollup({ bucket: bucket.value, sinceMs, untilMs })) as any;
      if (res?.success === false) {
        setHtml(rollupNoteEl, escapeHtml(res.error || "Could not load spend rollup"));
        return;
      }
      const buckets: { bucketStartMs: number; totalBase: number; complete: boolean }[] = res?.data ?? [];
      const labels = buckets.map((b) => new Date(b.bucketStartMs).toLocaleDateString());
      const totals = buckets.map((b) => b.totalBase);
      const incomplete = buckets.some((b) => !b.complete);
      setHtml(
        rollupNoteEl,
        buckets.length
          ? incomplete
            ? "Some buckets include meals with at least one unpriced ingredient — totals there are a partial, best-effort sum."
            : ""
          : "No priced meals in this window yet.",
      );
      if (rollupChart) {
        rollupChart.data.labels = labels;
        rollupChart.data.datasets[0].data = totals;
        rollupChart.update();
      } else {
        rollupChart = new Chart(rollupCanvas, {
          type: "bar",
          data: {
            labels,
            datasets: [
              {
                label: "Estimated spend",
                data: totals,
                backgroundColor: "rgba(56, 189, 248, 0.5)",
                borderColor: "#38bdf8",
              },
            ],
          },
          options: {
            responsive: true,
            maintainAspectRatio: false,
            plugins: { legend: { display: false } },
            scales: {
              x: { ticks: { color: "#94a3b8" }, grid: { color: "#1e293b" } },
              y: { ticks: { color: "#94a3b8" }, grid: { color: "#1e293b" }, beginAtZero: true },
            },
          },
        });
      }
    } catch (e) {
      setHtml(rollupNoteEl, `Could not load spend rollup: ${e}`);
    }
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
      void loadCosts();
    } catch (e) {
      mealsAtom.set({ ...mealsAtom.get(), status: "error", message: String(e) });
    }
  };

  bucketSel.addEventListener("change", () => void loadRollup());

  void load();
  void loadRollup();
  const unsub = mealsAtom.subscribe(render);
  return () => {
    rollupChart?.destroy();
    rollupChart = null;
    unsub();
  };
}
