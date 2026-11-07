import Chart from "chart.js/auto";
import { api } from "../lib/api";
import type { CategoryOverride, FinanceScenario, ScenarioAssumptions } from "../lib/api";
import { isEnvelopeFailure } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { confirmDialog, toast } from "../lib/overlay";
import { BASELINE_PSEUDO_ID, simulationsAtom } from "../lib/stores";
import { panelClose, panelOpen, statusBanner } from "./shared";

/**
 * Simulations — named "what if" scenarios projected forward from the real
 * transaction history, overlaid on one chart plus a summary table.
 *
 * All writes go through the server (see `handlers::finance_scenarios`); this
 * view only ever reads via `api.*` and never touches SQLite directly.
 */

/** Fixed categorical hue order (dark-surface steps), never cycled — see the
    dataviz skill's reference palette. Slot order is the CVD-safety
    mechanism, so scenarios keep the same color as selection order changes
    only insofar as they keep the same position in `selectedIds`. */
const SERIES_COLORS = [
  "#3987e5", // blue
  "#d95926", // orange
  "#199e70", // aqua
  "#c98500", // yellow
  "#d55181", // magenta
  "#008300", // green
  "#9085e9", // violet
  "#e66767", // red
];

function fmtMoney(v: number): string {
  return v.toLocaleString(undefined, { maximumFractionDigits: 0 });
}

export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Simulations", "Project net worth forward from real transaction history, under a scenario's assumptions", "panel-fit")}
      <div data-slot="status"></div>
      <div class="rounded-lg border border-surface-800 bg-base-100 p-3 text-xs text-ink-400" data-slot="baseline-info"></div>
      <div class="flex items-center justify-between gap-2">
        <div class="text-xs font-semibold uppercase tracking-widest text-ink-400">Scenarios</div>
        <button class="btn btn-success btn-sm" data-action="scenario-new">+ New Scenario</button>
      </div>
      <div class="space-y-2" data-slot="scenario-list"></div>
      <div class="rounded border border-surface-800 bg-base-100 p-3">
        <canvas data-slot="chart" height="140"></canvas>
      </div>
      <div class="overflow-auto" data-slot="summary-table"></div>
    ${panelClose}
  `;

  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const baselineInfoEl = root.querySelector("[data-slot='baseline-info']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='scenario-list']") as HTMLElement;
  const canvas = root.querySelector("[data-slot='chart']") as HTMLCanvasElement;
  const tableEl = root.querySelector("[data-slot='summary-table']") as HTMLElement;
  let chart: Chart | null = null;

  /** Color for a selected id, by its position in the selection order — stable
      as long as the selection itself does not change. */
  function colorFor(id: string): string {
    const idx = simulationsAtom.get().selectedIds.indexOf(id);
    return SERIES_COLORS[idx % SERIES_COLORS.length] ?? SERIES_COLORS[0];
  }

  function scenarioById(id: string): FinanceScenario | undefined {
    return simulationsAtom.get().items.find((s) => s.id === id);
  }

  function labelFor(id: string): string {
    if (id === BASELINE_PSEUDO_ID) return "Baseline (no changes)";
    return scenarioById(id)?.name ?? "(deleted scenario)";
  }

  function assumptionsSummary(a: ScenarioAssumptions): string {
    const bits = [`${a.horizonMonths}mo horizon`];
    if (a.incomeDeltaMonthly) bits.push(`income ${a.incomeDeltaMonthly > 0 ? "+" : ""}${fmtMoney(a.incomeDeltaMonthly)}/mo`);
    if (a.categoryOverrides.length) bits.push(`${a.categoryOverrides.length} category override${a.categoryOverrides.length > 1 ? "s" : ""}`);
    if (a.inflationAnnualPct) bits.push(`inflation ${a.inflationAnnualPct}%/yr`);
    if (a.investmentReturnAnnualPct) bits.push(`return ${a.investmentReturnAnnualPct}%/yr`);
    return bits.join(" · ");
  }

  // ---- Baseline info strip ----

  function renderBaselineInfo(): void {
    const s = simulationsAtom.get();
    if (!s.baseline) {
      setHtml(baselineInfoEl, `Loading baseline…`);
      return;
    }
    const b = s.baseline;
    setHtml(
      baselineInfoEl,
      `
      <div class="flex flex-wrap items-center gap-3">
        <span>Baseline window:
          <input type="number" min="1" max="60" value="${s.windowMonths}" data-action="window-months" class="input input-bordered input-xs w-16 text-center" /> months
        </span>
        <span class="text-ink-500">·</span>
        <span>avg income <b class="text-ink-200">€${fmtMoney(b.monthlyIncome)}</b>/mo</span>
        <span class="text-ink-500">·</span>
        <span>avg expense <b class="text-ink-200">€${fmtMoney(b.totalMonthlyExpense)}</b>/mo</span>
        <span class="text-ink-500">·</span>
        <span>current net worth <b class="text-ink-200">€${fmtMoney(b.currentNetWorth)}</b></span>
      </div>
      <p class="mt-1 text-ink-500">A short window can skew the average — widen it if your history is thin or seasonal.</p>
      `,
    );
  }

  // ---- Scenario list ----

  function renderList(): void {
    const s = simulationsAtom.get();
    setHtml(statusEl, statusBanner(s));
    const baselineRow = `
      <label class="flex items-center gap-3 rounded-lg border border-surface-800 px-3 py-2">
        <input type="checkbox" class="checkbox checkbox-sm" data-action="toggle-select" data-id="${BASELINE_PSEUDO_ID}" ${s.selectedIds.includes(BASELINE_PSEUDO_ID) ? "checked" : ""} />
        <span class="h-2.5 w-2.5 shrink-0 rounded-full" style="background:${s.selectedIds.includes(BASELINE_PSEUDO_ID) ? colorFor(BASELINE_PSEUDO_ID) : "transparent"}; border:1px dashed #898781;"></span>
        <div class="min-w-0 flex-1">
          <div class="text-sm font-medium text-ink-100">Baseline (no changes)</div>
          <div class="text-xs text-ink-500">Projects today's averages forward unmodified — the reference line.</div>
        </div>
      </label>`;
    const rows = s.items
      .map((it) => {
        const selected = s.selectedIds.includes(it.id);
        return `
        <div class="flex items-center gap-3 rounded-lg border border-surface-800 px-3 py-2">
          <input type="checkbox" class="checkbox checkbox-sm" data-action="toggle-select" data-id="${escapeHtml(it.id)}" ${selected ? "checked" : ""} />
          <span class="h-2.5 w-2.5 shrink-0 rounded-full" style="background:${selected ? colorFor(it.id) : "transparent"}; border:1px solid #898781;"></span>
          <div class="min-w-0 flex-1">
            <div class="truncate text-sm font-medium text-ink-100">${escapeHtml(it.name)}</div>
            <div class="truncate text-xs text-ink-500">${escapeHtml(assumptionsSummary(it.assumptions))}</div>
          </div>
          <div class="flex shrink-0 gap-2">
            <button class="btn btn-ghost btn-xs" data-action="scenario-edit" data-id="${escapeHtml(it.id)}">Edit</button>
            <button class="btn btn-ghost btn-xs text-err-400" data-action="scenario-delete" data-id="${escapeHtml(it.id)}">Delete</button>
          </div>
        </div>`;
      })
      .join("");
    setHtml(listEl, baselineRow + rows || baselineRow);
    refreshIcons(listEl);
    renderEditor();
  }

  // ---- Chart ----

  function renderChart(): void {
    const s = simulationsAtom.get();
    const ids = s.selectedIds.filter((id) => id === BASELINE_PSEUDO_ID || scenarioById(id));
    const datasets = ids
      .map((id) => {
        const p = s.projections[id];
        if (!p) return null;
        return {
          label: labelFor(id),
          data: p.series.map((pt) => pt.netWorth),
          borderColor: colorFor(id),
          backgroundColor: colorFor(id),
          borderWidth: 2,
          borderDash: id === BASELINE_PSEUDO_ID ? [6, 4] : undefined,
          pointRadius: 0,
          pointHoverRadius: 4,
          tension: 0.2,
          fill: false,
        };
      })
      .filter((d): d is NonNullable<typeof d> => d !== null);
    const maxLen = Math.max(0, ...datasets.map((d) => d.data.length));
    const labels = Array.from({ length: maxLen }, (_, i) => String(i));

    if (chart) {
      chart.data.labels = labels;
      chart.data.datasets = datasets as any;
      chart.update();
      return;
    }
    chart = new Chart(canvas, {
      type: "line",
      data: { labels, datasets: datasets as any },
      options: {
        responsive: true,
        maintainAspectRatio: false,
        interaction: { mode: "index", intersect: false },
        plugins: {
          legend: { display: true, position: "bottom", labels: { color: "#94a3b8" } },
          tooltip: {
            callbacks: {
              label: (ctx) => ` ${ctx.dataset.label}: €${fmtMoney(ctx.parsed.y ?? 0)}`,
            },
          },
        },
        scales: {
          x: {
            title: { display: true, text: "Month", color: "#94a3b8" },
            ticks: { color: "#94a3b8" },
            grid: { color: "#1e293b" },
          },
          y: {
            title: { display: true, text: "Net worth", color: "#94a3b8" },
            ticks: { color: "#94a3b8" },
            grid: { color: "#1e293b" },
          },
        },
      },
    });
  }

  // ---- Summary table ----

  function renderTable(): void {
    const s = simulationsAtom.get();
    const ids = s.selectedIds.filter((id) => id === BASELINE_PSEUDO_ID || scenarioById(id));
    const rows = ids
      .map((id) => {
        const p = s.projections[id];
        if (!p) return "";
        const sum = p.summary;
        return `
        <tr>
          <td class="flex items-center gap-2">
            <span class="h-2.5 w-2.5 shrink-0 rounded-full" style="background:${colorFor(id)}"></span>
            ${escapeHtml(labelFor(id))}
          </td>
          <td class="text-right">€${fmtMoney(sum.endingNetWorth)}</td>
          <td class="text-right ${sum.totalNetSaved >= 0 ? "text-ok-400" : "text-err-400"}">${sum.totalNetSaved >= 0 ? "+" : ""}€${fmtMoney(sum.totalNetSaved)}</td>
          <td class="text-right">€${fmtMoney(sum.finalMonthlyExpenseTotal)}/mo</td>
        </tr>`;
      })
      .join("");
    setHtml(
      tableEl,
      `
      <table class="table table-sm">
        <thead>
          <tr>
            <th>Scenario</th>
            <th class="text-right">Ending net worth</th>
            <th class="text-right">Total net saved</th>
            <th class="text-right">Final monthly expense</th>
          </tr>
        </thead>
        <tbody>${rows || `<tr><td colspan="4" class="text-xs text-ink-400">Select a scenario above to see its numbers.</td></tr>`}</tbody>
      </table>`,
    );
  }

  function renderAll(): void {
    renderBaselineInfo();
    renderList();
    renderChart();
    renderTable();
  }

  // ---- Editor modal ----

  let editorKey = "";
  /** Only rebuild the override rows when their count changes (add/remove) —
      rebuilding on every keystroke of an amount/category input would destroy
      the input the user is actively typing into, the same focus trap
      `foods.ts` avoids for its ingredient rows. */
  let lastOverridesCount = -1;

  function draftRowsHtml(overrides: CategoryOverride[]): string {
    return (
      overrides
        .map(
          (o, idx) => `
        <div class="flex items-center gap-2" data-row="${idx}">
          <input type="text" placeholder="Category (exact match)" value="${escapeHtml(o.category)}" data-action="override-category" data-idx="${idx}" class="input input-bordered input-sm flex-1" />
          <input type="number" step="any" value="${o.monthlyAmount}" data-action="override-amount" data-idx="${idx}" class="input input-bordered input-sm w-28" />
          <span class="text-xs text-ink-500">/mo</span>
          <button class="btn btn-ghost btn-xs text-err-400" data-action="override-remove" data-idx="${idx}">Remove</button>
        </div>`,
        )
        .join("") || `<div class="text-xs text-ink-400">No category overrides yet.</div>`
    );
  }

  function baselineCategoryChips(): string {
    const categories = simulationsAtom.get().baseline?.categoryMonthly ?? [];
    if (!categories.length) return "";
    return `<datalist id="sim-baseline-categories">${categories
      .map((c) => `<option value="${escapeHtml(c.category)}"></option>`)
      .join("")}</datalist>`;
  }

  function buildEditorSkeleton(modalRoot: HTMLElement, editing: FinanceScenario | null): void {
    const s = simulationsAtom.get();
    const categories = s.baseline?.categoryMonthly ?? [];
    lastOverridesCount = -1;
    modalRoot.innerHTML = `
      <dialog class="modal modal-open" id="sim-editor-modal">
        <div class="modal-box flex max-h-[90vh] max-w-2xl flex-col overflow-hidden">
          <div class="flex items-center justify-between border-b border-surface-800 pb-4">
            <h3 class="font-semibold text-ink-100">${editing ? "Edit" : "New"} Scenario</h3>
            <button class="btn btn-ghost btn-sm" data-action="sim-close" aria-label="Close">✕</button>
          </div>
          <div class="min-h-0 flex-1 space-y-5 overflow-auto py-4">
            <div>
              <label class="field-label">Name</label>
              <input id="sim-name" placeholder="e.g. Move to Lisbon" value="${escapeHtml(s.editingName)}" class="input input-bordered w-full" />
            </div>
            <div class="grid grid-cols-2 gap-4">
              <div>
                <label class="field-label">Horizon (months)</label>
                <input id="sim-horizon" type="number" min="1" max="600" value="${escapeHtml(s.editingHorizonMonths)}" class="input input-bordered w-full" />
              </div>
              <div>
                <label class="field-label">Income change (monthly)</label>
                <input id="sim-income-delta" type="number" step="any" value="${escapeHtml(s.editingIncomeDeltaMonthly)}" class="input input-bordered w-full" />
              </div>
              <div>
                <label class="field-label">Inflation (%/year)</label>
                <input id="sim-inflation" type="number" step="any" value="${escapeHtml(s.editingInflationAnnualPct)}" class="input input-bordered w-full" />
                <p class="mt-1 text-xs text-ink-500">Compounds against categories not overridden below.</p>
              </div>
              <div>
                <label class="field-label">Investment return (%/year)</label>
                <input id="sim-return" type="number" step="any" value="${escapeHtml(s.editingInvestmentReturnAnnualPct)}" class="input input-bordered w-full" />
                <p class="mt-1 text-xs text-ink-500">Compounds against the running net worth balance.</p>
              </div>
            </div>
            <div>
              <div class="mb-2 flex items-center justify-between">
                <div class="text-xs font-semibold uppercase tracking-widest text-ink-400">Category overrides</div>
                <button class="btn btn-ghost btn-xs" data-action="override-add">+ Add</button>
              </div>
              <p class="mb-2 text-xs text-ink-500">Replaces that category's baseline monthly amount outright (e.g. "rent" becomes €1200). Category names must match exactly.</p>
              ${categories.length ? `<p class="mb-2 text-xs text-ink-500">Current baseline: ${categories.map((c) => `<span class="chip">${escapeHtml(c.category)} €${fmtMoney(c.monthlyAmount)}/mo</span>`).join(" ")}</p>` : ""}
              <div class="space-y-2" data-slot="override-rows"></div>
              ${baselineCategoryChips()}
            </div>
          </div>
          <div class="modal-action border-t border-surface-800 pt-4">
            <button class="btn btn-success btn-sm" data-action="sim-save">Save Scenario</button>
          </div>
        </div>
        <form method="dialog" class="modal-backdrop"><button>close</button></form>
      </dialog>`;
    updateEditorSections(modalRoot);
    refreshIcons(modalRoot);
    (modalRoot.querySelector("#sim-name") as HTMLInputElement | null)?.focus();
  }

  function updateEditorSections(modalRoot: HTMLElement): void {
    const s = simulationsAtom.get();
    const count = s.editingCategoryOverrides.length;
    if (count === lastOverridesCount) return;
    lastOverridesCount = count;
    const rowsEl = modalRoot.querySelector("[data-slot='override-rows']");
    if (!rowsEl) return;
    setHtml(rowsEl as HTMLElement, draftRowsHtml(s.editingCategoryOverrides));
    rowsEl.querySelectorAll("input[data-action='override-category']").forEach((input) => {
      input.setAttribute("list", "sim-baseline-categories");
    });
  }

  function renderEditor(): void {
    const s = simulationsAtom.get();
    const modalRoot = document.getElementById("modal-root");
    if (!s.editorOpen || !modalRoot) {
      if (modalRoot && modalRoot.firstElementChild?.id === "sim-editor-modal") {
        modalRoot.replaceChildren();
        editorKey = "";
      }
      return;
    }
    const editing = s.editingId ? scenarioById(s.editingId) ?? null : null;
    const key = s.editingId ?? "new";
    if (key !== editorKey) {
      editorKey = key;
      buildEditorSkeleton(modalRoot, editing);
    } else {
      updateEditorSections(modalRoot);
    }
  }

  function closeEditor(): void {
    simulationsAtom.set({
      ...simulationsAtom.get(),
      editorOpen: false,
      editingId: null,
      editingName: "",
      editingHorizonMonths: "24",
      editingIncomeDeltaMonthly: "0",
      editingInflationAnnualPct: "0",
      editingInvestmentReturnAnnualPct: "0",
      editingCategoryOverrides: [],
    });
  }

  function openEditorFor(scenario: FinanceScenario | null): void {
    const a = scenario?.assumptions;
    simulationsAtom.set({
      ...simulationsAtom.get(),
      editorOpen: true,
      editingId: scenario?.id ?? null,
      editingName: scenario?.name ?? "",
      editingHorizonMonths: String(a?.horizonMonths ?? 24),
      editingIncomeDeltaMonthly: String(a?.incomeDeltaMonthly ?? 0),
      editingInflationAnnualPct: String(a?.inflationAnnualPct ?? 0),
      editingInvestmentReturnAnnualPct: String(a?.investmentReturnAnnualPct ?? 0),
      editingCategoryOverrides: a ? a.categoryOverrides.map((o) => ({ ...o })) : [],
    });
  }

  // ---- Data loading ----

  async function loadBaseline(): Promise<void> {
    const windowMonths = simulationsAtom.get().windowMonths;
    try {
      const res = await api.getFinanceBaseline(windowMonths);
      if (isEnvelopeFailure(res)) return;
      const data = (res as { data?: unknown }).data;
      simulationsAtom.set({ ...simulationsAtom.get(), baseline: data as any });
    } catch {
      // The projections below will surface the error status; the info strip
      // just stays on its "Loading…" placeholder.
    }
  }

  async function loadScenarios(): Promise<void> {
    simulationsAtom.set({ ...simulationsAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.listFinanceScenarios();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        simulationsAtom.set({ ...simulationsAtom.get(), status: "error", message: failure });
        return;
      }
      const data = (res as { data?: { items?: FinanceScenario[] } }).data;
      simulationsAtom.set({ ...simulationsAtom.get(), items: data?.items ?? [], status: "idle", message: "" });
    } catch (e) {
      simulationsAtom.set({ ...simulationsAtom.get(), status: "error", message: String(e) });
    }
  }

  async function loadProjections(): Promise<void> {
    const s = simulationsAtom.get();
    const windowMonths = s.windowMonths;
    const next: typeof s.projections = {};
    await Promise.all(
      s.selectedIds.map(async (id) => {
        const scenario = id === BASELINE_PSEUDO_ID ? null : scenarioById(id);
        if (id !== BASELINE_PSEUDO_ID && !scenario) return;
        try {
          const res =
            id === BASELINE_PSEUDO_ID
              ? await api.projectFinanceScenario({
                  assumptions: {
                    horizonMonths: longestSelectedHorizon(),
                    incomeDeltaMonthly: 0,
                    categoryOverrides: [],
                    inflationAnnualPct: 0,
                    investmentReturnAnnualPct: 0,
                  },
                  windowMonths,
                })
              : await api.projectFinanceScenario({ scenarioId: id, windowMonths });
          if (isEnvelopeFailure(res)) return;
          const data = (res as { data?: { projection?: typeof next[string] } }).data;
          if (data?.projection) next[id] = data.projection;
        } catch {
          // Leave this id's projection missing; it just won't draw a line.
        }
      }),
    );
    simulationsAtom.set({ ...simulationsAtom.get(), projections: next });
    renderAll();
  }

  function longestSelectedHorizon(): number {
    const s = simulationsAtom.get();
    const horizons = s.selectedIds
      .map((id) => scenarioById(id)?.assumptions.horizonMonths)
      .filter((h): h is number => typeof h === "number");
    return Math.max(24, ...horizons);
  }

  function save(): void {
    const s = simulationsAtom.get();
    const name = s.editingName.trim();
    if (!name) {
      toast("Please enter a name", "err");
      return;
    }
    const horizonMonths = Number.parseInt(s.editingHorizonMonths, 10);
    if (!Number.isFinite(horizonMonths) || horizonMonths <= 0) {
      toast("Horizon must be a positive number of months", "err");
      return;
    }
    const incomeDeltaMonthly = Number.parseFloat(s.editingIncomeDeltaMonthly) || 0;
    const inflationAnnualPct = Number.parseFloat(s.editingInflationAnnualPct) || 0;
    const investmentReturnAnnualPct = Number.parseFloat(s.editingInvestmentReturnAnnualPct) || 0;
    const categoryOverrides = s.editingCategoryOverrides.filter((o) => o.category.trim());
    const assumptions: ScenarioAssumptions = {
      horizonMonths,
      incomeDeltaMonthly,
      categoryOverrides,
      inflationAnnualPct,
      investmentReturnAnnualPct,
    };
    const call = s.editingId
      ? api.updateFinanceScenario({ id: s.editingId, name, assumptions })
      : api.createFinanceScenario({ name, assumptions });
    call
      .then((res) => {
        if (isEnvelopeFailure(res)) {
          toast(`Could not save: ${(res as { error?: string }).error || "unknown error"}`, "err");
          return;
        }
        closeEditor();
        void loadScenarios().then(() => void loadProjections());
      })
      .catch((e) => toast(`Could not save: ${e}`, "err"));
  }

  // ---- Events ----

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;

    if (target.closest("[data-action='scenario-new']")) {
      openEditorFor(null);
      return;
    }

    const toggle = target.closest<HTMLElement>("[data-action='toggle-select']");
    if (toggle?.dataset.id) {
      const id = toggle.dataset.id;
      const s = simulationsAtom.get();
      const selectedIds = s.selectedIds.includes(id)
        ? s.selectedIds.filter((x) => x !== id)
        : [...s.selectedIds, id];
      simulationsAtom.set({ ...s, selectedIds });
      void loadProjections();
      return;
    }

    const edit = target.closest<HTMLElement>("[data-action='scenario-edit']");
    if (edit?.dataset.id) {
      openEditorFor(scenarioById(edit.dataset.id) ?? null);
      return;
    }

    const del = target.closest<HTMLElement>("[data-action='scenario-delete']");
    if (del?.dataset.id) {
      const id = del.dataset.id;
      const scenario = scenarioById(id);
      void confirmDialog("Delete scenario?", `Delete "${scenario?.name ?? id}"? This can't be undone.`).then((ok) => {
        if (!ok) return;
        api
          .deleteFinanceScenario(id)
          .then(() => {
            const s = simulationsAtom.get();
            simulationsAtom.set({ ...s, selectedIds: s.selectedIds.filter((x) => x !== id) });
            void loadScenarios().then(() => void loadProjections());
          })
          .catch((e) => toast(`Could not delete: ${e}`, "err"));
      });
      return;
    }
  });

  // The editor modal lives in the global overlay root.
  document.getElementById("modal-root")?.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    const editor = document.getElementById("sim-editor-modal");
    if (!editor) return;

    if (target.closest("[data-action='sim-close']") || target.closest(".modal-backdrop")) {
      closeEditor();
      return;
    }
    if (target.closest("[data-action='override-add']")) {
      const s = simulationsAtom.get();
      simulationsAtom.set({ ...s, editingCategoryOverrides: [...s.editingCategoryOverrides, { category: "", monthlyAmount: 0 }] });
      return;
    }
    const remove = target.closest<HTMLElement>("[data-action='override-remove']");
    if (remove?.dataset.idx) {
      const idx = Number.parseInt(remove.dataset.idx, 10);
      const s = simulationsAtom.get();
      simulationsAtom.set({ ...s, editingCategoryOverrides: s.editingCategoryOverrides.filter((_, i) => i !== idx) });
      return;
    }
    if (target.closest("[data-action='sim-save']")) save();
  });

  document.addEventListener("input", (event) => {
    const target = event.target as HTMLElement;
    if (!target.isConnected) return;
    const s = simulationsAtom.get();
    if (target.id === "sim-name") {
      simulationsAtom.set({ ...s, editingName: (target as HTMLInputElement).value });
      return;
    }
    if (target.id === "sim-horizon") {
      simulationsAtom.set({ ...s, editingHorizonMonths: (target as HTMLInputElement).value });
      return;
    }
    if (target.id === "sim-income-delta") {
      simulationsAtom.set({ ...s, editingIncomeDeltaMonthly: (target as HTMLInputElement).value });
      return;
    }
    if (target.id === "sim-inflation") {
      simulationsAtom.set({ ...s, editingInflationAnnualPct: (target as HTMLInputElement).value });
      return;
    }
    if (target.id === "sim-return") {
      simulationsAtom.set({ ...s, editingInvestmentReturnAnnualPct: (target as HTMLInputElement).value });
      return;
    }
    const cat = target.closest<HTMLElement>("[data-action='override-category']");
    if (cat?.dataset.idx) {
      const idx = Number.parseInt(cat.dataset.idx, 10);
      const value = (target as HTMLInputElement).value;
      simulationsAtom.set({
        ...s,
        editingCategoryOverrides: s.editingCategoryOverrides.map((o, i) => (i === idx ? { ...o, category: value } : o)),
      });
      return;
    }
    const amt = target.closest<HTMLElement>("[data-action='override-amount']");
    if (amt?.dataset.idx) {
      const idx = Number.parseInt(amt.dataset.idx, 10);
      const parsed = Number.parseFloat((target as HTMLInputElement).value);
      simulationsAtom.set({
        ...s,
        editingCategoryOverrides: s.editingCategoryOverrides.map((o, i) => (i === idx ? { ...o, monthlyAmount: Number.isNaN(parsed) ? 0 : parsed } : o)),
      });
    }
  });

  // `change` rather than `input`: the baseline info strip re-renders on
  // every state update, which would wipe this field's focus mid-keystroke if
  // it reacted to every character typed instead of to the committed value.
  document.addEventListener("change", (event) => {
    const target = event.target as HTMLElement;
    if (!target.isConnected || target.dataset.action !== "window-months") return;
    const months = Number.parseInt((target as HTMLInputElement).value, 10);
    if (Number.isFinite(months) && months > 0) {
      simulationsAtom.set({ ...simulationsAtom.get(), windowMonths: months });
      void loadBaseline().then(() => void loadProjections());
    }
  });

  const onKey = (e: KeyboardEvent): void => {
    if (e.key !== "Escape") return;
    if (simulationsAtom.get().editorOpen) closeEditor();
  };
  document.addEventListener("keydown", onKey);

  void loadBaseline().then(() => void loadScenarios().then(() => void loadProjections()));
  const unsub = simulationsAtom.subscribe(renderAll);
  return () => {
    document.removeEventListener("keydown", onKey);
    chart?.destroy();
    chart = null;
    unsub();
  };
}
