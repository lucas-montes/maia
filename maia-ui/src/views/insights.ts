import Chart from "chart.js/auto";
import { api } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { isEnvelopeFailure, unwrapList } from "../lib/api";
import { panelClose, panelOpen, statusBanner } from "./shared";

/**
 * Insights — charts across training, nutrition and finance.
 *
 * Every chart here follows the `bodyMetrics.ts` pattern: a `<canvas>`, a
 * `Chart` instance held in a closure variable, and a `renderX()` that updates
 * `chart.data` in place rather than destroying/recreating the chart.
 *
 * Each section keeps its own period (and exercise/account selection) as a
 * closure-local variable and its own `Chart` instance, fetched and rendered
 * independently. Nothing here is shared across sections — switching one
 * chart's day/week/month toggle only re-fetches and re-renders that one
 * chart, never another, per the plan's explicit constraint.
 */

type Period = "day" | "week" | "month";

const AXIS_TICK = "#94a3b8";
const AXIS_GRID = "#1e293b";
const BLUE = "#3987e5";
const BLUE_FILL = "rgba(57, 135, 229, 0.15)";

// Fixed-order categorical hues (dark-surface step), picked for CVD-safe
// adjacent pairs — never cycled, never reassigned by rank. A category beyond
// this count folds into "Other" rather than generating a ninth hue.
const SERIES_COLORS = ["#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#008300", "#9085e9", "#e66767"];
const OTHER_COLOR = "#52525b";
const MAX_CATEGORY_SERIES = 7;

/** Formats a bucket key ("YYYY-MM-DD" for day/week-start, "YYYY-MM-01" for
 *  month, per the Rust `bucket_key` convention) into an axis label. */
function bucketLabel(bucket: string, period: Period): string {
  const d = new Date(`${bucket}T00:00:00Z`);
  if (Number.isNaN(d.getTime())) return bucket;
  if (period === "month") return d.toLocaleDateString(undefined, { year: "numeric", month: "short" });
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

/** Dataset label suffix for the nutrition charts: week/month buckets hold
 *  mean daily values (`compute_nutrition_averages` on the Rust side), so the
 *  axis meaning has to travel with the label. */
function nutritionSeriesLabel(series: string, period: Period): string {
  return period === "day" ? series : `${series} · avg/day`;
}

/** A `join` group of period buttons, scoped to one section by `data-section`
 *  so the click handler below only ever touches that section's state. */
function periodButtons(sectionId: string, active: Period, options: Period[]): string {
  return `<div class="join" data-period-group="${sectionId}">
    ${options
      .map(
        (p) =>
          `<button type="button" class="join-item btn btn-xs ${p === active ? "btn-active" : ""}" data-section="${sectionId}" data-period="${p}">${p[0].toUpperCase()}${p.slice(1)}</button>`,
      )
      .join("")}
  </div>`;
}

function colorFor(category: string, index: number): string {
  return category === "Other" ? OTHER_COLOR : SERIES_COLORS[index % SERIES_COLORS.length];
}

/** Pivots flat `{bucket, category, total}` rows into chart-ready series: one
 *  dataset per category (aligned to a shared bucket axis, 0 where a category
 *  had no spending in a bucket), capped at `MAX_CATEGORY_SERIES` named series
 *  with the rest folded into "Other" by total magnitude. */
function pivotCategoryTotals(rows: { bucket: string; category: string; total: number }[]): {
  buckets: string[];
  series: { category: string; data: number[] }[];
} {
  const buckets: string[] = [];
  const seenBuckets = new Set<string>();
  for (const r of rows) {
    if (!seenBuckets.has(r.bucket)) {
      seenBuckets.add(r.bucket);
      buckets.push(r.bucket);
    }
  }

  const totalByCategory = new Map<string, number>();
  for (const r of rows) totalByCategory.set(r.category, (totalByCategory.get(r.category) ?? 0) + r.total);
  const ranked = [...totalByCategory.entries()].sort((a, b) => b[1] - a[1]);
  const kept = new Set(ranked.slice(0, MAX_CATEGORY_SERIES).map(([c]) => c));
  const categories = [...kept, ...(ranked.length > MAX_CATEGORY_SERIES ? ["Other"] : [])];

  const byBucket = new Map<string, Map<string, number>>();
  for (const r of rows) {
    const cat = kept.has(r.category) ? r.category : "Other";
    const m = byBucket.get(r.bucket) ?? new Map<string, number>();
    m.set(cat, (m.get(cat) ?? 0) + r.total);
    byBucket.set(r.bucket, m);
  }

  const series = categories.map((category) => ({
    category,
    data: buckets.map((b) => byBucket.get(b)?.get(category) ?? 0),
  }));
  return { buckets, series };
}

export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Insights", "Charts across training, nutrition, and finance", "panel-fit")}
      <div class="min-h-0 flex-1 overflow-auto space-y-6" data-slot="sections">
        <section class="card bg-base-200 border border-base-300">
          <div class="card-body gap-3">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <h3 class="card-title text-base"><i data-lucide="dumbbell" class="h-4 w-4"></i>Exercise progression</h3>
              <div>
                <!-- Native combobox, per 'simulations.ts': a searchable text
                     box whose datalist offers the full exercise catalog. -->
                <input id="insight-exercise" list="insight-exercise-options" autocomplete="off"
                       placeholder="Search exercise…" class="input input-bordered input-sm min-w-[220px]" />
                <datalist id="insight-exercise-options"></datalist>
              </div>
            </div>
            <div data-slot="progression-status"></div>
            <div class="h-56"><canvas data-slot="progression-chart"></canvas></div>
          </div>
        </section>

        <section class="card bg-base-200 border border-base-300">
          <div class="card-body gap-3">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <h3 class="card-title text-base"><i data-lucide="dumbbell" class="h-4 w-4"></i>Training volume</h3>
              ${periodButtons("volume", "week", ["week", "month"])}
            </div>
            <div data-slot="volume-status"></div>
            <div class="h-56"><canvas data-slot="volume-chart"></canvas></div>
          </div>
        </section>

        <section class="card bg-base-200 border border-base-300">
          <div class="card-body gap-3">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <h3 class="card-title text-base"><i data-lucide="utensils" class="h-4 w-4"></i>Calories</h3>
              ${periodButtons("calories", "week", ["day", "week", "month"])}
            </div>
            <div data-slot="calories-status"></div>
            <div class="h-56"><canvas data-slot="calories-chart"></canvas></div>
          </div>
        </section>

        <section class="card bg-base-200 border border-base-300">
          <div class="card-body gap-3">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <h3 class="card-title text-base"><i data-lucide="utensils" class="h-4 w-4"></i>Macros</h3>
              ${periodButtons("macros", "week", ["day", "week", "month"])}
            </div>
            <div data-slot="macros-status"></div>
            <div class="h-56"><canvas data-slot="macros-chart"></canvas></div>
          </div>
        </section>

        <section class="card bg-base-200 border border-base-300">
          <div class="card-body gap-3">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <h3 class="card-title text-base"><i data-lucide="wallet" class="h-4 w-4"></i>Spending by category</h3>
              ${periodButtons("category", "week", ["day", "week", "month"])}
            </div>
            <div data-slot="category-status"></div>
            <div class="h-64"><canvas data-slot="category-chart"></canvas></div>
          </div>
        </section>

        <section class="card bg-base-200 border border-base-300">
          <div class="card-body gap-3">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <h3 class="card-title text-base"><i data-lucide="wallet" class="h-4 w-4"></i>Account balance</h3>
              <div class="flex flex-wrap items-center gap-2">
                <select id="insight-account" class="select select-bordered select-sm"></select>
                ${periodButtons("balance", "week", ["day", "week", "month"])}
              </div>
            </div>
            <div data-slot="balance-status"></div>
            <div class="h-56"><canvas data-slot="balance-chart"></canvas></div>
          </div>
        </section>
      </div>
    ${panelClose}
  `;

  const exerciseInput = root.querySelector<HTMLInputElement>("#insight-exercise")!;
  const exerciseDatalist = root.querySelector<HTMLDataListElement>("#insight-exercise-options")!;
  // The catalog behind the datalist; the input carries names, ids live here.
  let exercises: { id: string; name: string }[] = [];
  const accountSelect = root.querySelector<HTMLSelectElement>("#insight-account")!;

  const progressionStatus = root.querySelector("[data-slot='progression-status']") as HTMLElement;
  const progressionCanvas = root.querySelector("[data-slot='progression-chart']") as HTMLCanvasElement;
  let progressionChart: Chart | null = null;
  // Parallel to the chart's data points; read from the tooltip callback so
  // reps show up on hover without a second y-axis (two scales on one chart
  // reads as two different charts pretending to be one).
  let progressionReps: (number | null)[] = [];

  const volumeStatus = root.querySelector("[data-slot='volume-status']") as HTMLElement;
  const volumeCanvas = root.querySelector("[data-slot='volume-chart']") as HTMLCanvasElement;
  let volumeChart: Chart | null = null;
  let volumePeriod: Period = "week";

  const caloriesStatus = root.querySelector("[data-slot='calories-status']") as HTMLElement;
  const caloriesCanvas = root.querySelector("[data-slot='calories-chart']") as HTMLCanvasElement;
  let caloriesChart: Chart | null = null;
  let caloriesPeriod: Period = "week";

  const macrosStatus = root.querySelector("[data-slot='macros-status']") as HTMLElement;
  const macrosCanvas = root.querySelector("[data-slot='macros-chart']") as HTMLCanvasElement;
  let macrosChart: Chart | null = null;
  let macrosPeriod: Period = "week";

  const categoryStatus = root.querySelector("[data-slot='category-status']") as HTMLElement;
  const categoryCanvas = root.querySelector("[data-slot='category-chart']") as HTMLCanvasElement;
  let categoryChart: Chart | null = null;
  let categoryPeriod: Period = "week";

  const balanceStatus = root.querySelector("[data-slot='balance-status']") as HTMLElement;
  const balanceCanvas = root.querySelector("[data-slot='balance-chart']") as HTMLCanvasElement;
  let balanceChart: Chart | null = null;
  let balancePeriod: Period = "week";

  const axisScales = {
    x: { ticks: { color: AXIS_TICK }, grid: { color: AXIS_GRID } },
    y: { ticks: { color: AXIS_TICK }, grid: { color: AXIS_GRID } },
  };

  const loadProgression = async (exerciseId: string): Promise<void> => {
    if (!exerciseId) return;
    setHtml(progressionStatus, statusBanner({ status: "loading", message: "" }));
    try {
      const res = await api.exerciseProgression(exerciseId);
      const failure = isEnvelopeFailure(res);
      if (failure) {
        setHtml(progressionStatus, statusBanner({ status: "error", message: failure }));
        return;
      }
      const rows = unwrapList(res) as { date: number; weightKg: number; reps: number | null }[];
      setHtml(progressionStatus, "");
      const labels = rows.map((r) => new Date(r.date).toLocaleDateString());
      const weights = rows.map((r) => r.weightKg);
      progressionReps = rows.map((r) => r.reps);
      if (progressionChart) {
        progressionChart.data.labels = labels;
        progressionChart.data.datasets[0].data = weights;
        progressionChart.update();
        return;
      }
      progressionChart = new Chart(progressionCanvas, {
        type: "line",
        data: {
          labels,
          datasets: [
            {
              label: "Weight (kg)",
              data: weights,
              borderColor: BLUE,
              backgroundColor: BLUE_FILL,
              fill: true,
              tension: 0.3,
              spanGaps: true,
            },
          ],
        },
        options: {
          responsive: true,
          maintainAspectRatio: false,
          plugins: {
            legend: { display: false },
            tooltip: {
              callbacks: {
                label: (ctx) => {
                  const reps = progressionReps[ctx.dataIndex];
                  const base = `Weight: ${ctx.parsed.y} kg`;
                  return reps != null ? `${base} × ${reps} reps` : base;
                },
              },
            },
          },
          scales: axisScales,
        },
      });
    } catch (e) {
      setHtml(progressionStatus, statusBanner({ status: "error", message: String(e) }));
    }
  };

  // Resolves the free-text exercise search into a specific exercise and
  // redraws the chart. The datalist does live filtering natively; this only
  // reacts once typing settles, and only when a real name has been entered.
  let exerciseResolveTimer: number | null = null;
  const resolveExerciseInput = (): void => {
    if (exerciseResolveTimer !== null) window.clearTimeout(exerciseResolveTimer);
    exerciseResolveTimer = window.setTimeout(() => {
      exerciseResolveTimer = null;
      const typed = exerciseInput.value.trim();
      if (!typed) return;
      const match = exercises.find((e) => e.name.trim().toLowerCase() === typed.toLowerCase());
      if (match) {
        void loadProgression(match.id);
      } else if (exercises.length) {
        setHtml(
          progressionStatus,
          statusBanner({ status: "error", message: `No exercise matching "${typed}"` }),
        );
      }
    }, 250);
  };

  const loadExerciseOptions = async (): Promise<void> => {
    try {
      const res = await api.listExercises();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        setHtml(progressionStatus, statusBanner({ status: "error", message: failure }));
        return;
      }
      const items = unwrapList(res) as { id: string; name: string }[];
      exercises = items;
      exerciseDatalist.innerHTML =
        items.map((e) => `<option value="${escapeHtml(e.name)}"></option>`).join("");
      if (items.length) {
        exerciseInput.value = items[0].name;
        void loadProgression(items[0].id);
      }
    } catch (e) {
      setHtml(progressionStatus, statusBanner({ status: "error", message: String(e) }));
    }
  };

  const loadVolume = async (): Promise<void> => {
    setHtml(volumeStatus, statusBanner({ status: "loading", message: "" }));
    try {
      const res = await api.exerciseVolume(volumePeriod);
      const failure = isEnvelopeFailure(res);
      if (failure) {
        setHtml(volumeStatus, statusBanner({ status: "error", message: failure }));
        return;
      }
      const rows = unwrapList(res) as { bucket: string; volumeKg: number }[];
      setHtml(volumeStatus, "");
      const labels = rows.map((r) => bucketLabel(r.bucket, volumePeriod));
      const data = rows.map((r) => r.volumeKg);
      if (volumeChart) {
        volumeChart.data.labels = labels;
        volumeChart.data.datasets[0].data = data;
        volumeChart.update();
        return;
      }
      volumeChart = new Chart(volumeCanvas, {
        type: "bar",
        data: { labels, datasets: [{ label: "Volume (kg)", data, backgroundColor: BLUE }] },
        options: {
          responsive: true,
          maintainAspectRatio: false,
          plugins: { legend: { display: false } },
          scales: axisScales,
        },
      });
    } catch (e) {
      setHtml(volumeStatus, statusBanner({ status: "error", message: String(e) }));
    }
  };

  const loadCalories = async (): Promise<void> => {
    setHtml(caloriesStatus, statusBanner({ status: "loading", message: "" }));
    try {
      const res = await api.nutritionCalories(caloriesPeriod);
      const failure = isEnvelopeFailure(res);
      if (failure) {
        setHtml(caloriesStatus, statusBanner({ status: "error", message: failure }));
        return;
      }
      const rows = unwrapList(res) as { bucket: string; calories: number }[];
      setHtml(caloriesStatus, "");
      const labels = rows.map((r) => bucketLabel(r.bucket, caloriesPeriod));
      const data = rows.map((r) => r.calories);
      const label = nutritionSeriesLabel("Calories", caloriesPeriod);
      if (caloriesChart) {
        caloriesChart.data.labels = labels;
        caloriesChart.data.datasets[0].label = label;
        caloriesChart.data.datasets[0].data = data;
        caloriesChart.update();
        return;
      }
      caloriesChart = new Chart(caloriesCanvas, {
        type: "line",
        data: {
          labels,
          datasets: [
            {
              label,
              data,
              borderColor: BLUE,
              backgroundColor: BLUE_FILL,
              fill: true,
              tension: 0.3,
              spanGaps: true,
            },
          ],
        },
        options: {
          responsive: true,
          maintainAspectRatio: false,
          plugins: { legend: { display: false } },
          scales: axisScales,
        },
      });
    } catch (e) {
      setHtml(caloriesStatus, statusBanner({ status: "error", message: String(e) }));
    }
  };

  const loadMacros = async (): Promise<void> => {
    setHtml(macrosStatus, statusBanner({ status: "loading", message: "" }));
    try {
      const res = await api.nutritionMacros(macrosPeriod);
      const failure = isEnvelopeFailure(res);
      if (failure) {
        setHtml(macrosStatus, statusBanner({ status: "error", message: failure }));
        return;
      }
      const rows = unwrapList(res) as { bucket: string; protein: number; carbs: number; fat: number }[];
      setHtml(macrosStatus, "");
      const labels = rows.map((r) => bucketLabel(r.bucket, macrosPeriod));
      const protein = rows.map((r) => r.protein);
      const carbs = rows.map((r) => r.carbs);
      const fat = rows.map((r) => r.fat);
      if (macrosChart) {
        macrosChart.data.labels = labels;
        macrosChart.data.datasets[0].label = nutritionSeriesLabel("Protein (g)", macrosPeriod);
        macrosChart.data.datasets[0].data = protein;
        macrosChart.data.datasets[1].label = nutritionSeriesLabel("Carbs (g)", macrosPeriod);
        macrosChart.data.datasets[1].data = carbs;
        macrosChart.data.datasets[2].label = nutritionSeriesLabel("Fat (g)", macrosPeriod);
        macrosChart.data.datasets[2].data = fat;
        macrosChart.update();
        return;
      }
      macrosChart = new Chart(macrosCanvas, {
        type: "line",
        data: {
          labels,
          datasets: [
            { label: nutritionSeriesLabel("Protein (g)", macrosPeriod), data: protein, borderColor: SERIES_COLORS[0], backgroundColor: SERIES_COLORS[0], tension: 0.3, spanGaps: true },
            { label: nutritionSeriesLabel("Carbs (g)", macrosPeriod), data: carbs, borderColor: SERIES_COLORS[1], backgroundColor: SERIES_COLORS[1], tension: 0.3, spanGaps: true },
            { label: nutritionSeriesLabel("Fat (g)", macrosPeriod), data: fat, borderColor: SERIES_COLORS[2], backgroundColor: SERIES_COLORS[2], tension: 0.3, spanGaps: true },
          ],
        },
        options: {
          responsive: true,
          maintainAspectRatio: false,
          plugins: { legend: { display: true, labels: { color: AXIS_TICK } } },
          scales: axisScales,
        },
      });
    } catch (e) {
      setHtml(macrosStatus, statusBanner({ status: "error", message: String(e) }));
    }
  };

  const loadCategory = async (): Promise<void> => {
    setHtml(categoryStatus, statusBanner({ status: "loading", message: "" }));
    try {
      const res = await api.financeCategoryTotals(categoryPeriod);
      const failure = isEnvelopeFailure(res);
      if (failure) {
        setHtml(categoryStatus, statusBanner({ status: "error", message: failure }));
        return;
      }
      const rows = unwrapList(res) as { bucket: string; category: string; total: number }[];
      setHtml(categoryStatus, "");
      const { buckets, series } = pivotCategoryTotals(rows);
      const labels = buckets.map((b) => bucketLabel(b, categoryPeriod));
      const datasets = series.map((s, i) => ({ label: s.category, data: s.data, backgroundColor: colorFor(s.category, i) }));
      if (categoryChart) {
        // The number of categories can change between periods, unlike every
        // other chart's fixed series count, so the datasets array itself is
        // replaced here rather than only its `.data` -- still an in-place
        // mutation of the existing Chart instance, just at one level up.
        categoryChart.data.labels = labels;
        categoryChart.data.datasets = datasets;
        categoryChart.update();
        return;
      }
      categoryChart = new Chart(categoryCanvas, {
        type: "bar",
        data: { labels, datasets },
        options: {
          responsive: true,
          maintainAspectRatio: false,
          plugins: { legend: { display: true, labels: { color: AXIS_TICK } } },
          scales: axisScales,
        },
      });
    } catch (e) {
      setHtml(categoryStatus, statusBanner({ status: "error", message: String(e) }));
    }
  };

  const loadBalance = async (): Promise<void> => {
    const accountId = accountSelect.value;
    if (!accountId) return;
    setHtml(balanceStatus, statusBanner({ status: "loading", message: "" }));
    try {
      const res = await api.financeAccountBalance(accountId, balancePeriod);
      const failure = isEnvelopeFailure(res);
      if (failure) {
        setHtml(balanceStatus, statusBanner({ status: "error", message: failure }));
        return;
      }
      const rows = unwrapList(res) as { bucket: string; balance: number }[];
      setHtml(balanceStatus, "");
      const labels = rows.map((r) => bucketLabel(r.bucket, balancePeriod));
      const data = rows.map((r) => r.balance);
      if (balanceChart) {
        balanceChart.data.labels = labels;
        balanceChart.data.datasets[0].data = data;
        balanceChart.update();
        return;
      }
      balanceChart = new Chart(balanceCanvas, {
        type: "line",
        data: {
          labels,
          datasets: [
            {
              label: "Balance",
              data,
              borderColor: BLUE,
              backgroundColor: BLUE_FILL,
              fill: true,
              tension: 0.3,
              spanGaps: true,
            },
          ],
        },
        options: {
          responsive: true,
          maintainAspectRatio: false,
          plugins: { legend: { display: false } },
          scales: axisScales,
        },
      });
    } catch (e) {
      setHtml(balanceStatus, statusBanner({ status: "error", message: String(e) }));
    }
  };

  const loadAccountOptions = async (): Promise<void> => {
    try {
      const res = await api.listAccounts();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        setHtml(balanceStatus, statusBanner({ status: "error", message: failure }));
        return;
      }
      const items = unwrapList(res) as { id: string; name: string }[];
      accountSelect.innerHTML =
        items.map((a) => `<option value="${escapeHtml(a.id)}">${escapeHtml(a.name)}</option>`).join("") ||
        `<option value="">No accounts</option>`;
      if (items.length) {
        accountSelect.value = items[0].id;
        void loadBalance();
      }
    } catch (e) {
      setHtml(balanceStatus, statusBanner({ status: "error", message: String(e) }));
    }
  };

  const setActivePeriodButtons = (sectionId: string, period: Period): void => {
    const group = root.querySelector(`[data-period-group="${sectionId}"]`);
    group?.querySelectorAll<HTMLButtonElement>("button").forEach((btn) => {
      btn.classList.toggle("btn-active", btn.dataset.period === period);
    });
  };

  // One listener for every period toggle; `data-section` scopes each click to
  // exactly one chart's reload, so no toggle ever affects another section.
  root.addEventListener("click", (event) => {
    const btn = (event.target as HTMLElement).closest<HTMLButtonElement>("[data-period]");
    if (!btn) return;
    const section = btn.dataset.section;
    const period = btn.dataset.period as Period | undefined;
    if (!section || !period) return;
    setActivePeriodButtons(section, period);
    if (section === "volume") {
      volumePeriod = period;
      void loadVolume();
    } else if (section === "calories") {
      caloriesPeriod = period;
      void loadCalories();
    } else if (section === "macros") {
      macrosPeriod = period;
      void loadMacros();
    } else if (section === "category") {
      categoryPeriod = period;
      void loadCategory();
    } else if (section === "balance") {
      balancePeriod = period;
      void loadBalance();
    }
  });

  exerciseInput.addEventListener("input", () => void resolveExerciseInput());
  exerciseInput.addEventListener("change", () => void resolveExerciseInput());
  accountSelect.addEventListener("change", () => void loadBalance());

  // Independent fetches -- none awaits another, so one slow chart never
  // blocks the rest of the page from rendering.
  void loadExerciseOptions();
  void loadVolume();
  void loadCalories();
  void loadMacros();
  void loadCategory();
  void loadAccountOptions();

  refreshIcons(root);

  return () => {
    if (exerciseResolveTimer !== null) window.clearTimeout(exerciseResolveTimer);
    progressionChart?.destroy();
    volumeChart?.destroy();
    caloriesChart?.destroy();
    macrosChart?.destroy();
    categoryChart?.destroy();
    balanceChart?.destroy();
  };
}
