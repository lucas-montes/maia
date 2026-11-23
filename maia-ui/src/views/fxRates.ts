import { api, isEnvelopeFailure, type FxRate, type FxRateCode } from "../lib/api";
import { escapeHtml, setHtml, todayStr } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { fxRatesAtom } from "../lib/stores";
import { panelClose, panelOpen, statusBanner } from "./shared";

const PAGE_SIZE = 100;

type Filters = ReturnType<typeof fxRatesAtom.get>["filters"];

/** `YYYY-MM-DD` for `days` before today, in local time like `todayStr`. */
function daysAgo(days: number): string {
  const d = new Date();
  d.setDate(d.getDate() - days);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

function formatRate(n: number): string {
  if (!Number.isFinite(n) || n === 0) return "–";
  return n.toLocaleString(undefined, { maximumSignificantDigits: 6 });
}

/**
 * FX rates: a flat history against the deployment's base currency, filtered
 * by currency, date range and source, loaded a page at a time.
 */
export function mount(root: HTMLElement): () => void {
  const f0 = fxRatesAtom.get().filters;
  root.innerHTML = `
    ${panelOpen("FX rates", "Daily rates against your base currency", "panel-fit")}
      <div class="flex flex-wrap items-end gap-2">
        <label class="flex flex-col gap-1 text-xs">Currency
          <select id="fx-code" class="select select-bordered select-sm min-w-[9rem]">
            <option value="">All currencies</option>
            ${f0.code ? `<option value="${escapeHtml(f0.code)}" selected>${escapeHtml(f0.code)}</option>` : ""}
          </select>
        </label>
        <label class="flex flex-col gap-1 text-xs">From
          <input id="fx-from" type="date" value="${escapeHtml(f0.from)}" class="input input-bordered input-sm" />
        </label>
        <label class="flex flex-col gap-1 text-xs">To
          <input id="fx-to" type="date" value="${escapeHtml(f0.to)}" class="input input-bordered input-sm" />
        </label>
        <label class="flex flex-col gap-1 text-xs">Source
          <select id="fx-source" class="select select-bordered select-sm">
            <option value="">All</option>
            <option value="auto">Fetched</option>
            <option value="manual">Manual</option>
          </select>
        </label>
        <div class="join">
          <button type="button" class="join-item btn btn-sm" data-preset="30">30 days</button>
          <button type="button" class="join-item btn btn-sm" data-preset="365">1 year</button>
          <button type="button" class="join-item btn btn-sm" data-preset="all">All time</button>
        </div>
        <button type="button" class="btn btn-ghost btn-sm" data-action="fx-clear">Clear</button>
      </div>

      <div data-slot="status"></div>
      <div class="min-h-0 flex-1 overflow-auto rounded-box border border-base-300" data-slot="scroll">
        <table class="table table-sm table-pin-rows w-full">
          <thead><tr>
            <th>Date</th>
            <th>Currency</th>
            <th class="text-right" data-slot="rate-head">Rate</th>
            <th class="text-right" data-slot="inverse-head">Inverse</th>
            <th>Source</th>
          </tr></thead>
          <tbody data-slot="rows"></tbody>
        </table>
        <div class="p-2 text-center" data-slot="more"></div>
      </div>

      <details class="collapse collapse-arrow bg-base-200 border border-base-300">
        <summary class="collapse-title text-sm font-medium">Add a manual rate</summary>
        <div class="collapse-content">
          <form class="flex flex-wrap items-end gap-2" data-slot="add-form">
            <label class="flex flex-col gap-1 text-xs">Currency
              <input id="fx-add-code" maxlength="3" placeholder="USD" autocomplete="off"
                     class="input input-bordered input-sm w-20 uppercase" list="fx-code-options" />
              <datalist id="fx-code-options"></datalist>
            </label>
            <label class="flex flex-col gap-1 text-xs"><span>Rate (1 unit in <span data-slot="add-base">base</span>)</span>
              <input id="fx-add-rate" type="number" step="any" min="0" placeholder="0.92" class="input input-bordered input-sm w-32" />
            </label>
            <label class="flex flex-col gap-1 text-xs">Date
              <input id="fx-add-date" type="date" value="${todayStr()}" class="input input-bordered input-sm" />
            </label>
            <button type="submit" class="btn btn-primary btn-sm">Add rate</button>
            <div class="w-full" data-slot="add-status"></div>
          </form>
        </div>
      </details>
    ${panelClose}
  `;

  // The router reuses `root` across views, so listeners go on this page's own
  // element and disappear with it.
  const page = root.firstElementChild as HTMLElement;
  const $ = <T extends HTMLElement>(sel: string) => page.querySelector(sel) as T;
  const codeSel = $<HTMLSelectElement>("#fx-code");
  const fromIn = $<HTMLInputElement>("#fx-from");
  const toIn = $<HTMLInputElement>("#fx-to");
  const sourceSel = $<HTMLSelectElement>("#fx-source");
  const statusEl = $("[data-slot='status']");
  const rowsEl = $("[data-slot='rows']");
  const moreEl = $("[data-slot='more']");
  const scrollEl = $("[data-slot='scroll']");
  const addStatus = $("[data-slot='add-status']");
  sourceSel.value = f0.source;

  let base = "";
  let items: FxRate[] = [];
  let hasMore = false;
  // Bumped on every new query so a slow response for old filters is dropped.
  let generation = 0;
  let debounce: number | null = null;

  const filters = (): Filters => ({
    code: codeSel.value,
    from: fromIn.value,
    to: toIn.value,
    source: sourceSel.value as Filters["source"],
  });

  const renderRows = (): void => {
    setHtml(
      rowsEl,
      items
        .map(
          (x) => `<tr>
            <td class="tabular-nums">${escapeHtml(x.rateDate)}</td>
            <td class="font-medium">${escapeHtml(x.code)}</td>
            <td class="text-right tabular-nums">${formatRate(x.rateToBase)}</td>
            <td class="text-right tabular-nums text-base-content/60">${formatRate(1 / x.rateToBase)}</td>
            <td><span class="badge badge-sm ${x.manual ? "badge-warning" : "badge-ghost"}">${x.manual ? "manual" : "fetched"}</span></td>
          </tr>`,
        )
        .join("") ||
        `<tr><td colspan="5" class="p-4 text-center text-base-content/60">No rates match these filters</td></tr>`,
    );
    setHtml(
      moreEl,
      hasMore ? `<button type="button" class="btn btn-sm" data-action="fx-more">Load ${PAGE_SIZE} more</button>` : "",
    );
    if (base) {
      setHtml($("[data-slot='rate-head']"), `1 unit in ${escapeHtml(base)}`);
      setHtml($("[data-slot='inverse-head']"), `1 ${escapeHtml(base)} in unit`);
    }
  };

  const load = async (append: boolean): Promise<void> => {
    const gen = ++generation;
    const f = filters();
    if (!append) fxRatesAtom.set({ filters: f });
    setHtml(statusEl, statusBanner({ status: "loading", message: "" }));
    try {
      const res = await api.listFxRates({ ...f, limit: PAGE_SIZE, offset: append ? items.length : 0 });
      if (gen !== generation) return;
      const failure = isEnvelopeFailure(res);
      if (failure || !res.data) {
        setHtml(statusEl, statusBanner({ status: "error", message: failure ?? "No data" }));
        return;
      }
      setHtml(statusEl, "");
      base = res.data.base;
      items = append ? [...items, ...res.data.items] : res.data.items;
      hasMore = res.data.hasMore;
      if (!append) scrollEl.scrollTop = 0;
      renderRows();
    } catch (e) {
      if (gen === generation) setHtml(statusEl, statusBanner({ status: "error", message: String(e) }));
    }
  };

  const reload = (): void => {
    if (debounce !== null) window.clearTimeout(debounce);
    debounce = window.setTimeout(() => {
      debounce = null;
      void load(false);
    }, 200);
  };

  const loadCodes = async (): Promise<void> => {
    try {
      const res = await api.listFxRateCodes();
      if (isEnvelopeFailure(res) || !res.data) return;
      const codes: FxRateCode[] = res.data.codes;
      base = res.data.base;
      setHtml($("[data-slot='add-base']"), escapeHtml(base));
      const selected = codeSel.value || fxRatesAtom.get().filters.code;
      codeSel.innerHTML =
        `<option value="">All currencies</option>` +
        codes
          .map((c) => `<option value="${escapeHtml(c.code)}">${escapeHtml(c.code)} (${c.count})</option>`)
          .join("");
      // Keep a remembered filter even if that currency has no rows any more,
      // so the select never silently shows "All" while the list is filtered.
      if (selected && !codes.some((c) => c.code === selected)) {
        codeSel.insertAdjacentHTML("beforeend", `<option value="${escapeHtml(selected)}">${escapeHtml(selected)} (0)</option>`);
      }
      codeSel.value = selected;
      $("#fx-code-options").innerHTML = codes.map((c) => `<option value="${escapeHtml(c.code)}"></option>`).join("");
    } catch {
      // The list still works unfiltered; the currency dropdown just stays at "All".
    }
  };

  const onChange = (event: Event): void => {
    const t = event.target as HTMLElement;
    if (t === codeSel || t === sourceSel || t === fromIn || t === toIn) reload();
  };
  page.addEventListener("change", onChange);
  // Typing a date fires `input` before `change`; only a complete date reloads.
  page.addEventListener("input", (event) => {
    const t = event.target as HTMLInputElement;
    if ((t === fromIn || t === toIn) && (t.value === "" || /^\d{4}-\d{2}-\d{2}$/.test(t.value))) reload();
  });

  page.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    const preset = target.closest<HTMLElement>("[data-preset]")?.dataset.preset;
    if (preset) {
      fromIn.value = preset === "all" ? "" : daysAgo(Number(preset));
      toIn.value = "";
      reload();
      return;
    }
    if (target.closest("[data-action='fx-clear']")) {
      codeSel.value = "";
      sourceSel.value = "";
      fromIn.value = "";
      toIn.value = "";
      reload();
      return;
    }
    if (target.closest("[data-action='fx-more']")) {
      void load(true);
    }
  });

  $<HTMLFormElement>("[data-slot='add-form']").addEventListener("submit", (event) => {
    event.preventDefault();
    const codeIn = $<HTMLInputElement>("#fx-add-code");
    const rateIn = $<HTMLInputElement>("#fx-add-rate");
    const dateIn = $<HTMLInputElement>("#fx-add-date");
    const code = codeIn.value.trim().toUpperCase();
    const rate = parseFloat(rateIn.value);
    const date = dateIn.value || todayStr();
    const fail = (message: string) => setHtml(addStatus, statusBanner({ status: "error", message }));
    if (!/^[A-Z]{3}$/.test(code)) return fail("Currency must be a 3-letter code, e.g. USD");
    if (code === base) return fail(`${code} is the base currency`);
    if (!(rate > 0)) return fail("Rate must be a number above 0");
    setHtml(addStatus, statusBanner({ status: "saving", message: "" }));
    api
      .createFxRate({ code, baseCode: base, rateDate: date, rateToBase: rate })
      .then((res) => {
        const failure = isEnvelopeFailure(res);
        if (failure) return fail(failure);
        setHtml(addStatus, statusBanner({ status: "saved", message: `Added ${code} for ${date}` }));
        codeIn.value = "";
        rateIn.value = "";
        void loadCodes();
        void load(false);
      })
      .catch((e) => fail(String(e)));
  });

  void loadCodes();
  void load(false);
  refreshIcons(page);

  return () => {
    generation++;
    if (debounce !== null) window.clearTimeout(debounce);
  };
}
