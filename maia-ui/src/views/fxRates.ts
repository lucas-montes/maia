import { api } from "../lib/api";
import { escapeHtml, todayStr } from "../lib/dom";
import { setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { fxRatesAtom } from "../lib/stores";
import { isEnvelopeFailure, unwrapList } from "../lib/api";
import { panelClose, panelOpen, statusBanner } from "./shared";

/** FxRates — add a manual rate and refresh the list for a base/date. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("FxRates", "Add new • refresh • no edit")}
      <div data-slot="status"></div>
      <div class="flex flex-wrap gap-2">
        <input id="fx-base" value="${escapeHtml(fxRatesAtom.get().base)}" placeholder="Base" class="input input-bordered w-20" />
        <input id="fx-date" type="date" value="${escapeHtml(fxRatesAtom.get().date)}" class="input input-bordered" />
        <button class="btn btn-secondary btn-sm" data-action="fx-refresh">Refresh</button>
      </div>
      <div class="flex gap-2">
        <input id="fx-code" placeholder="Code EUR" class="input input-bordered" />
        <input id="fx-rate" placeholder="Rate" type="number" step="0.0001" class="input input-bordered" />
        <button class="btn btn-primary btn-sm" data-action="fx-add">Add</button>
      </div>
      <div class="list-divided min-h-0 flex-1 overflow-auto" data-slot="list"></div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;

  const render = (): void => {
    const f = fxRatesAtom.get();
    setHtml(statusEl, statusBanner(f));
    setHtml(
      listEl,
      f.items
        .map(
          (x) => `
          <div class="flex justify-between text-xs">
            <span>${escapeHtml(x.code)}→${escapeHtml(x.baseCode || f.base)} ${x.rateToBase} • ${escapeHtml(x.rateDate || x.date || "")}</span>
            <span class="badge badge-muted">${x.manual ? "manual" : "auto"}</span>
          </div>`,
        )
        .join("") || `<div class="p-2 text-xs text-ink-400">No rates</div>`,
    );
    refreshIcons(listEl);
  };

  const load = async (): Promise<void> => {
    const f = fxRatesAtom.get();
    fxRatesAtom.set({ ...f, status: "loading", message: "" });
    try {
      const res = await api.listFxRates({ base: f.base, date: f.date });
      const failure = isEnvelopeFailure(res);
      if (failure) {
        fxRatesAtom.set({ ...fxRatesAtom.get(), status: "error", message: failure });
        return;
      }
      fxRatesAtom.set({ ...fxRatesAtom.get(), items: unwrapList(res), status: "idle", message: "" });
    } catch (e) {
      fxRatesAtom.set({ ...fxRatesAtom.get(), status: "error", message: String(e) });
    }
  };

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    if (target.closest("[data-action='fx-add']")) {
      const code = (root.querySelector<HTMLInputElement>("#fx-code")?.value ?? "").trim().toUpperCase();
      const rateInput = root.querySelector<HTMLInputElement>("#fx-rate")?.value ?? "";
      const rate = parseFloat(rateInput);
      const base = (root.querySelector<HTMLInputElement>("#fx-base")?.value ?? "").trim().toUpperCase() || "USD";
      const date = root.querySelector<HTMLInputElement>("#fx-date")?.value || todayStr();
      if (code && rate) {
        api.createFxRate({ code, baseCode: base, rateDate: date, rateToBase: rate }).then(() => {
          fxRatesAtom.set({ ...fxRatesAtom.get(), base, date });
          void load();
        });
      }
      return;
    }
    if (target.closest("[data-action='fx-refresh']")) {
      const b = root.querySelector<HTMLInputElement>("#fx-base")?.value || "USD";
      const d = root.querySelector<HTMLInputElement>("#fx-date")?.value || fxRatesAtom.get().date;
      fxRatesAtom.set({ ...fxRatesAtom.get(), base: b, date: d });
      void load();
    }
  });

  void load();
  const unsub = fxRatesAtom.subscribe(render);
  return unsub;
}
