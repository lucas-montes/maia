import Chart from "chart.js/auto";
import { api } from "../lib/api";
import { setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { bodyMetricsAtom } from "../lib/stores";
import { isEnvelopeFailure, unwrapList } from "../lib/api";
import { panelClose, panelOpen, statusBanner } from "./shared";

/** Body Metrics — weight/height chart plus the raw list. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Body Metrics", "Weight and height over time")}
      <div data-slot="status"></div>
      <div class="rounded border border-surface-800 bg-base-100 p-3">
        <canvas data-slot="chart" height="120"></canvas>
      </div>
      <div class="list-divided min-h-0 flex-1 overflow-auto" data-slot="list"></div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;
  const canvas = root.querySelector("[data-slot='chart']") as HTMLCanvasElement;
  let chart: Chart | null = null;

  const renderList = (): void => {
    const b = bodyMetricsAtom.get();
    setHtml(statusEl, statusBanner(b));
    setHtml(
      listEl,
      b.items
        .map(
          (x) => `
          <div class="flex justify-between text-xs">
            <span>${new Date(x.date).toLocaleDateString()}</span>
            <span>${x.weightKg ?? ""} kg • ${x.heightCm ?? ""} cm</span>
          </div>`,
        )
        .join("") || `<div class="p-2 text-xs text-ink-400">No data</div>`,
    );
    refreshIcons(listEl);
  };

  const renderChart = (): void => {
    const items = [...bodyMetricsAtom.get().items].sort((a, b) => new Date(a.date).getTime() - new Date(b.date).getTime());
    const labels = items.map((x) => new Date(x.date).toLocaleDateString());
    const weights = items.map((x) => x.weightKg ?? null);
    if (chart) {
      chart.data.labels = labels;
      const ds = chart.data.datasets[0];
      ds.data = weights;
      chart.update();
      return;
    }
    chart = new Chart(canvas, {
      type: "line",
      data: {
        labels,
        datasets: [
          {
            label: "Weight (kg)",
            data: weights,
            borderColor: "#38bdf8",
            backgroundColor: "rgba(56, 189, 248, 0.15)",
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
        scales: {
          x: { ticks: { color: "#94a3b8" }, grid: { color: "#1e293b" } },
          y: { ticks: { color: "#94a3b8" }, grid: { color: "#1e293b" } },
        },
      },
    });
  };

  const load = async (): Promise<void> => {
    bodyMetricsAtom.set({ ...bodyMetricsAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.listBodyMetrics();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        bodyMetricsAtom.set({ ...bodyMetricsAtom.get(), status: "error", message: failure });
        return;
      }
      bodyMetricsAtom.set({ ...bodyMetricsAtom.get(), items: unwrapList(res), status: "idle", message: "" });
    } catch (e) {
      bodyMetricsAtom.set({ ...bodyMetricsAtom.get(), status: "error", message: String(e) });
    }
  };

  void load();
  const unsub = bodyMetricsAtom.subscribe(() => {
    renderList();
    renderChart();
  });
  return () => {
    chart?.destroy();
    chart = null;
    unsub();
  };
}
