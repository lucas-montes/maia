import Chart from "chart.js/auto";
import { api } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { toast } from "../lib/overlay";
import { ingredientsAtom, storesAtom } from "../lib/stores";
import { isEnvelopeFailure, unwrapList } from "../lib/api";
import { panelClose, panelOpen, statusBanner } from "./shared";

const numOrNull = (v: string): number | null => {
  if (v === "") return null;
  const n = parseFloat(v);
  return isNaN(n) ? null : n;
};

/** One ingredient's price observation, as `list_ingredients` embeds it
 *  (see T03: the Tauri command now selects `packageGrams`/`sourceUrl`). */
interface PriceRow {
  id: string;
  storeId: string;
  price: number;
  currencyCode: string;
  packageGrams: number | null;
  recordedAt: number;
  sourceUrl: string | null;
}

/** Cost per 100g in the observation's own currency; null without a package
 *  weight (same rule as mobile's `IngredientPrice.costPer100gInBase`, minus
 *  the base-currency conversion the desktop view does not have rates for
 *  yet -- rows are shown in their native currency instead of mixed). */
function costPer100g(p: PriceRow): number | null {
  if (!p.packageGrams || p.packageGrams <= 0) return null;
  return (p.price / p.packageGrams) * 100;
}

function storeName(stores: any[], id: string): string {
  return stores.find((s) => s.id === id)?.name ?? "Unknown store";
}

/** Ingredients — searchable list, add modal, inline rename, and (T03) a
 *  per-ingredient price history + cost-per-100g chart, parity with mobile's
 *  `_PricesSection`. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Ingredients", "Manage foods • search, edit, and build meals", "panel-fit")}
      <div data-slot="status"></div>
      <div data-slot="body"></div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const bodyEl = root.querySelector("[data-slot='body']") as HTMLElement;

  let chart: Chart | null = null;
  // Ephemeral "add price" form state, local to whichever ingredient is open.
  // Not atom-backed: it never needs to survive a view switch.
  let priceFormOpen = false;
  let priceFormStoreId = "";
  let priceFormNewStore = "";

  const destroyChart = (): void => {
    chart?.destroy();
    chart = null;
  };

  // --- List -----------------------------------------------------------------

  const listHtml = (): string => {
    const s = ingredientsAtom.get();
    const q = s.search.toLowerCase();
    const rows = s.items
      .filter((it) => !q || `${it.name} ${it.brand || ""} ${it.barcode || ""}`.toLowerCase().includes(q))
      .slice(0, 50)
      .map(
        (it) => `
        <div class="group rounded-xl border border-surface-800 bg-base-100 p-3 transition-colors hover:border-surface-700 hover:bg-base-100">
          <div class="flex justify-between gap-3">
            <button type="button" class="min-w-0 text-left" data-action="ing-open" data-id="${escapeHtml(it.id)}">
              <div class="truncate text-sm font-medium text-ink-100">
                ${escapeHtml(it.name)} ${it.is_archived ? `<span class="badge badge-warn ml-2">archived</span>` : ""}
              </div>
              <div class="mt-1 flex flex-wrap gap-1.5 text-xs text-ink-400">
                <span class="chip">${it.calories_per100g} kcal</span>
                <span class="chip">P ${it.protein_per100g}</span>
                <span class="chip">C ${it.carbs_per100g}</span>
                <span class="chip">F ${it.fat_per100g}</span>
                ${it.brand ? `<span class="text-ink-500">• ${escapeHtml(it.brand)}</span>` : ""}
                ${it.barcode ? `<span class="font-mono text-[11px] text-ink-500">• ${escapeHtml(it.barcode)}</span>` : ""}
                ${(it.prices || []).length ? `<span class="chip">${it.prices.length} price${it.prices.length === 1 ? "" : "s"}</span>` : ""}
              </div>
            </button>
            <button class="btn btn-ghost btn-xs self-start opacity-60 group-hover:opacity-100" data-action="ing-edit" data-id="${escapeHtml(it.id)}">Edit</button>
          </div>
        </div>`,
      )
      .join("");
    return `
      <div class="flex items-start justify-between gap-4">
        <div></div>
        <button class="btn btn-success btn-sm shrink-0" data-action="ing-open-modal">+ Add Ingredient</button>
      </div>
      <div class="flex gap-2">
        <input id="ing-search" placeholder="Search by name, brand or barcode..." class="input input-bordered flex-1" value="${escapeHtml(s.search)}" />
        <button class="btn btn-secondary btn-sm" data-action="ing-search">Search</button>
      </div>
      <div class="grid min-h-0 flex-1 gap-2 overflow-auto pr-1">${rows || `<div class="p-3 text-xs text-ink-400">No ingredients</div>`}</div>`;
  };

  // --- Detail / price history -------------------------------------------------

  const priceHistoryRows = (prices: PriceRow[], stores: any[]): string => {
    const sorted = [...prices].sort((a, b) => b.recordedAt - a.recordedAt);
    return sorted
      .map((p) => {
        const per100g = costPer100g(p);
        return `
        <div class="flex items-center justify-between gap-3 border-b border-surface-800 py-2 last:border-0">
          <div class="min-w-0">
            <div class="truncate text-sm text-ink-100">${escapeHtml(storeName(stores, p.storeId))}</div>
            <div class="text-xs text-ink-500">
              ${new Date(p.recordedAt).toLocaleDateString()}
              ${p.packageGrams ? ` • ${p.packageGrams % 1 === 0 ? p.packageGrams.toFixed(0) : p.packageGrams.toFixed(1)} g` : ""}
              ${p.sourceUrl ? ` • <a class="text-accent-400 underline" href="${escapeHtml(p.sourceUrl)}" target="_blank" rel="noreferrer">source</a>` : ""}
            </div>
          </div>
          <div class="shrink-0 text-right">
            <div class="font-mono text-sm font-semibold text-ink-100">${p.price.toFixed(2)} ${escapeHtml(p.currencyCode)}</div>
            ${per100g != null ? `<div class="text-xs text-ink-500">${per100g.toFixed(3)} ${escapeHtml(p.currencyCode)}/100g</div>` : `<div class="text-xs text-ink-500">no package size</div>`}
          </div>
        </div>`;
      })
      .join("");
  };

  const priceFormHtml = (stores: any[]): string => {
    if (!priceFormOpen) {
      return `<button class="btn btn-secondary btn-sm mt-3" data-action="price-form-open"><i data-lucide="shopping-basket" class="h-4 w-4"></i> Add price</button>`;
    }
    const options = stores
      .map((s) => `<option value="${escapeHtml(s.id)}" ${s.id === priceFormStoreId ? "selected" : ""}>${escapeHtml(s.name)}</option>`)
      .join("");
    const today = new Date().toISOString().slice(0, 10);
    return `
      <div class="mt-3 space-y-2 rounded border border-surface-800 bg-base-100 p-3">
        <div class="text-xs font-semibold uppercase tracking-widest text-ink-400">Add price</div>
        <div class="flex gap-2">
          <select id="price-store" class="select select-bordered select-sm flex-1">
            <option value="">Select a store…</option>
            ${options}
          </select>
        </div>
        <div class="flex gap-2">
          <input id="price-new-store" placeholder="…or add a new store" class="input input-bordered input-sm flex-1" value="${escapeHtml(priceFormNewStore)}" />
        </div>
        <div class="grid grid-cols-2 gap-2">
          <input id="price-amount" type="number" step="0.01" placeholder="Price" class="input input-bordered input-sm" />
          <input id="price-currency" placeholder="EUR" maxlength="3" class="input input-bordered input-sm" />
        </div>
        <div class="grid grid-cols-2 gap-2">
          <input id="price-grams" type="number" step="0.1" placeholder="Package size (g)" class="input input-bordered input-sm" />
          <input id="price-date" type="date" value="${today}" class="input input-bordered input-sm" />
        </div>
        <input id="price-source-url" placeholder="Source URL (optional)" class="input input-bordered input-sm w-full" />
        <div class="flex justify-end gap-2">
          <button class="btn btn-ghost btn-sm" data-action="price-form-cancel">Cancel</button>
          <button class="btn btn-primary btn-sm" data-action="price-form-save">Save</button>
        </div>
      </div>`;
  };

  const detailHtml = (it: any, stores: any[]): string => {
    const prices: PriceRow[] = it.prices || [];
    return `
      <button class="btn btn-ghost btn-sm" data-action="ing-back">← Back to list</button>
      <div class="rounded border border-surface-800 bg-base-100 p-4">
        <div class="flex items-center justify-between">
          <h3 class="text-sm font-medium text-ink-200">${escapeHtml(it.name)}</h3>
          <button class="btn btn-ghost btn-xs" data-action="ing-edit" data-id="${escapeHtml(it.id)}">Edit</button>
        </div>
        <div class="mt-2 flex flex-wrap gap-1.5 text-xs text-ink-400">
          <span class="chip">${it.calories_per100g} kcal</span>
          <span class="chip">P ${it.protein_per100g}</span>
          <span class="chip">C ${it.carbs_per100g}</span>
          <span class="chip">F ${it.fat_per100g}</span>
        </div>
      </div>
      <div class="rounded border border-surface-800 bg-base-100 p-4">
        <h3 class="mb-2 text-sm font-medium text-ink-200">Cost per 100g over time</h3>
        ${prices.filter((p) => costPer100g(p) != null).length >= 2 ? `<canvas data-slot="chart" height="140"></canvas>` : `<div class="p-3 text-xs text-ink-400">Needs at least two priced observations with a package size to plot a trend.</div>`}
      </div>
      <div class="rounded border border-surface-800 bg-base-100 p-4">
        <h3 class="mb-2 text-sm font-medium text-ink-200">Price history</h3>
        ${prices.length ? priceHistoryRows(prices, stores) : `<div class="p-2 text-xs text-ink-400">No prices recorded yet.</div>`}
        ${priceFormHtml(stores)}
      </div>`;
  };

  const renderChart = (it: any): void => {
    const canvas = bodyEl.querySelector<HTMLCanvasElement>("[data-slot='chart']");
    if (!canvas) {
      destroyChart();
      return;
    }
    const prices: PriceRow[] = it.prices || [];
    const points = prices
      .map((p) => ({ x: p.recordedAt, y: costPer100g(p), currency: p.currencyCode }))
      .filter((p) => p.y != null)
      .sort((a, b) => a.x - b.x);
    const labels = points.map((p) => new Date(p.x).toLocaleDateString());
    const data = points.map((p) => p.y as number);
    if (chart) {
      chart.data.labels = labels;
      chart.data.datasets[0].data = data;
      chart.update();
      return;
    }
    chart = new Chart(canvas, {
      type: "line",
      data: {
        labels,
        datasets: [
          {
            label: "Cost per 100g",
            data,
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

  // --- Combined render --------------------------------------------------------

  const render = (): void => {
    const s = ingredientsAtom.get();
    setHtml(statusEl, statusBanner(s));
    destroyChart();
    if (s.selectedId) {
      const it = s.items.find((x) => x.id === s.selectedId);
      if (!it) {
        setHtml(bodyEl, `<div class="p-4 text-sm text-ink-500">Ingredient not found.</div>`);
      } else {
        setHtml(bodyEl, detailHtml(it, storesAtom.get().items));
        renderChart(it);
      }
    } else {
      setHtml(bodyEl, listHtml());
    }
    refreshIcons(bodyEl);
  };

  const load = async (): Promise<void> => {
    ingredientsAtom.set({ ...ingredientsAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.listIngredients();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        ingredientsAtom.set({ ...ingredientsAtom.get(), status: "error", message: failure });
        return;
      }
      ingredientsAtom.set({ ...ingredientsAtom.get(), items: unwrapList(res), status: "idle", message: "" });
    } catch (e) {
      ingredientsAtom.set({ ...ingredientsAtom.get(), status: "error", message: String(e) });
    }
  };

  const loadStores = async (): Promise<void> => {
    try {
      const res = await api.listStores();
      if (!isEnvelopeFailure(res)) {
        storesAtom.set({ ...storesAtom.get(), items: unwrapList(res) });
      }
    } catch {
      // Store names are a display nicety here; a failed load just falls back
      // to "Unknown store" rather than blocking the ingredient view.
    }
  };

  const create = async (button: HTMLButtonElement): Promise<void> => {
    const read = (id: string): string => (document.getElementById(id) as HTMLInputElement | null)?.value ?? "";
    const name = read("ing-new-name").trim();
    const cal = numOrNull(read("ing-new-cal"));
    const prot = numOrNull(read("ing-new-prot"));
    const carb = numOrNull(read("ing-new-carb"));
    const fat = numOrNull(read("ing-new-fat"));
    const fiber = numOrNull(read("ing-new-fiber"));
    const sugar = numOrNull(read("ing-new-sugar"));
    const sodium = numOrNull(read("ing-new-sodium"));
    const brand = read("ing-new-brand").trim() || null;
    const barcode = read("ing-new-barcode").trim() || null;
    if (!name) {
      toast("Please enter a name", "err");
      return;
    }
    if (cal === null) {
      toast("Please enter calories", "err");
      return;
    }
    button.disabled = true;
    button.textContent = "Adding...";
    try {
      await api.createIngredient({
        name,
        caloriesPer100g: cal,
        proteinPer100g: prot ?? 0,
        carbsPer100g: carb ?? 0,
        fatPer100g: fat ?? 0,
        fiberPer100g: fiber,
        sugarPer100g: sugar,
        sodiumPer100g: sodium,
        brand,
        barcode,
      });
      closeAddModal();
      await load();
    } catch {
      button.disabled = false;
      button.textContent = "Add Ingredient →";
    }
  };

  const openAddModal = (): void => {
    const modalRoot = document.getElementById("modal-root");
    if (!modalRoot) return;
    modalRoot.innerHTML = `
      <dialog class="modal modal-open" id="ing-add-modal">
        <div class="modal-box flex max-h-[90vh] max-w-2xl flex-col overflow-hidden">
          <div class="flex items-center justify-between border-b border-surface-800 pb-4">
            <div>
              <h3 class="font-semibold text-ink-100">Add New Ingredient</h3>
              <p class="mt-1 text-xs text-ink-500">Add a food item with nutrition per 100g — you can add pictures &amp; prices afterwards</p>
            </div>
            <button class="btn btn-ghost btn-sm" data-action="ing-close-modal" aria-label="Close">✕</button>
          </div>
          <div class="min-h-0 flex-1 space-y-6 overflow-auto py-4">
            <div>
              <h4 class="mb-3 text-xs font-semibold uppercase tracking-widest text-ink-400">Basic Information</h4>
              <div class="space-y-4">
                <div>
                  <label class="field-label">Name <span class="text-err-400">*</span></label>
                  <input id="ing-new-name" placeholder="e.g. Oats, Chicken Breast, Olive Oil" class="input input-bordered w-full" />
                </div>
                <div class="grid grid-cols-1 gap-4 sm:grid-cols-2">
                  <div>
                    <label class="field-label">Brand (optional)</label>
                    <input id="ing-new-brand" placeholder="e.g. Quaker" class="input input-bordered w-full" />
                  </div>
                  <div>
                    <label class="field-label">Barcode (optional)</label>
                    <input id="ing-new-barcode" placeholder="e.g. 0123456789" class="input input-bordered w-full font-mono" />
                  </div>
                </div>
              </div>
            </div>
            <div>
              <h4 class="mb-3 flex items-center gap-2 text-xs font-semibold uppercase tracking-widest text-ok-400">Nutrition per 100g</h4>
              <div class="grid grid-cols-2 gap-3 sm:grid-cols-4">
                <div class="sm:col-span-2">
                  <label class="field-label text-xs">Calories <span class="text-err-400">*</span></label>
                  <input id="ing-new-cal" type="number" step="0.1" placeholder="368" class="input input-bordered w-full" />
                </div>
                <div>
                  <label class="field-label text-xs">Protein</label>
                  <input id="ing-new-prot" type="number" step="0.1" placeholder="16" class="input input-bordered w-full" />
                </div>
                <div>
                  <label class="field-label text-xs">Carbs</label>
                  <input id="ing-new-carb" type="number" step="0.1" placeholder="60" class="input input-bordered w-full" />
                </div>
                <div>
                  <label class="field-label text-xs">Fat</label>
                  <input id="ing-new-fat" type="number" step="0.1" placeholder="7" class="input input-bordered w-full" />
                </div>
                <div>
                  <label class="field-label text-xs">Fiber</label>
                  <input id="ing-new-fiber" type="number" step="0.1" placeholder="10" class="input input-bordered w-full" />
                </div>
                <div>
                  <label class="field-label text-xs">Sugar</label>
                  <input id="ing-new-sugar" type="number" step="0.1" placeholder="1" class="input input-bordered w-full" />
                </div>
                <div>
                  <label class="field-label text-xs">Sodium</label>
                  <input id="ing-new-sodium" type="number" step="0.1" placeholder="0.1" class="input input-bordered w-full" />
                </div>
              </div>
            </div>
          </div>
          <div class="modal-action border-t border-surface-800 pt-4">
            <button class="btn btn-ghost btn-sm" data-action="ing-close-modal">Cancel</button>
            <button class="btn btn-success btn-sm px-6" data-action="ing-create">Add Ingredient →</button>
          </div>
        </div>
        <form method="dialog" class="modal-backdrop"><button>close</button></form>
      </dialog>`;
    const modal = modalRoot.querySelector("#ing-add-modal") as HTMLElement;
    if (!modal) return;
    modal.addEventListener("click", (event) => {
      const target = event.target as HTMLElement;
      if (target.closest("[data-action='ing-close-modal']") || target.closest(".modal-backdrop")) {
        closeAddModal();
        return;
      }
      const createBtn = target.closest<HTMLButtonElement>("[data-action='ing-create']");
      if (createBtn) void create(createBtn);
    });
    refreshIcons(modal);
  };

  const closeAddModal = (): void => {
    ingredientsAtom.set({ ...ingredientsAtom.get(), showAddModal: false });
    document.getElementById("modal-root")?.replaceChildren();
  };

  const savePrice = async (): Promise<void> => {
    const s = ingredientsAtom.get();
    const ingredientId = s.selectedId;
    if (!ingredientId) return;
    const read = (id: string): string => (bodyEl.querySelector(`#${id}`) as HTMLInputElement | null)?.value ?? "";
    const price = numOrNull(read("price-amount"));
    const currency = (read("price-currency").trim() || "EUR").toUpperCase();
    const grams = numOrNull(read("price-grams"));
    const sourceUrl = read("price-source-url").trim() || null;
    const dateStr = read("price-date");
    const recordedAt = dateStr ? new Date(`${dateStr}T00:00:00`).getTime() : Date.now();
    const newStoreName = read("price-new-store").trim();
    let storeId = (bodyEl.querySelector("#price-store") as HTMLSelectElement | null)?.value || "";

    if (!price || price <= 0) {
      toast("Enter a positive price", "err");
      return;
    }
    if (!storeId && !newStoreName) {
      toast("Select or add a store", "err");
      return;
    }
    try {
      if (!storeId && newStoreName) {
        const created = (await api.createStore({ name: newStoreName })) as any;
        storeId = created?.data?.id;
        if (!storeId) throw new Error("could not create store");
        await loadStores();
      }
      const saved = (await api.addIngredientPrice({
        ingredientId,
        storeId,
        price,
        currencyCode: currency,
        packageGrams: grams,
        recordedAt,
        sourceUrl,
      })) as any;
      if (saved?.success === false) {
        toast(saved.error || "Could not save price", "err");
        return;
      }
      priceFormOpen = false;
      priceFormStoreId = "";
      priceFormNewStore = "";
      await load();
    } catch (e) {
      toast(`Could not save price: ${e}`, "err");
    }
  };

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    if (target.closest("[data-action='ing-open-modal']")) {
      ingredientsAtom.set({ ...ingredientsAtom.get(), showAddModal: true });
      openAddModal();
      document.getElementById("ing-new-name")?.focus();
      return;
    }
    if (target.closest("[data-action='ing-close-modal']")) {
      closeAddModal();
      return;
    }
    const createBtn = target.closest<HTMLButtonElement>("[data-action='ing-create']");
    if (createBtn) {
      void create(createBtn);
      return;
    }
    if (target.closest("[data-action='ing-search']")) {
      ingredientsAtom.set({ ...ingredientsAtom.get(), search: (document.getElementById("ing-search") as HTMLInputElement | null)?.value ?? "" });
      return;
    }
    const openRow = target.closest<HTMLElement>("[data-action='ing-open']");
    if (openRow?.dataset.id) {
      priceFormOpen = false;
      ingredientsAtom.set({ ...ingredientsAtom.get(), selectedId: openRow.dataset.id });
      void loadStores();
      return;
    }
    if (target.closest("[data-action='ing-back']")) {
      ingredientsAtom.set({ ...ingredientsAtom.get(), selectedId: null });
      return;
    }
    if (target.closest("[data-action='price-form-open']")) {
      priceFormOpen = true;
      render();
      return;
    }
    if (target.closest("[data-action='price-form-cancel']")) {
      priceFormOpen = false;
      render();
      return;
    }
    if (target.closest("[data-action='price-form-save']")) {
      void savePrice();
      return;
    }
    const edit = target.closest<HTMLElement>("[data-action='ing-edit']");
    if (edit?.dataset.id) {
      const id = edit.dataset.id;
      const it = ingredientsAtom.get().items.find((e) => e.id === id);
      if (it) {
        const name = prompt("New name", it.name);
        if (name) {
          api.updateIngredient({ id, name, caloriesPer100g: it.calories_per100g, proteinPer100g: it.protein_per100g, carbsPer100g: it.carbs_per100g, fatPer100g: it.fat_per100g }).then(() => void load());
        }
      }
    }
  });

  // Keep the store select's current pick across re-renders triggered by
  // something other than the select itself (e.g. the price list refreshing).
  root.addEventListener("change", (event) => {
    const target = event.target as HTMLElement;
    if (target.id === "price-store") {
      priceFormStoreId = (target as HTMLSelectElement).value;
    }
    if (target.id === "price-new-store") {
      priceFormNewStore = (target as HTMLInputElement).value;
    }
  });

  // Esc closes the add modal, matching the old behaviour.
  const onKey = (e: KeyboardEvent): void => {
    if (e.key === "Escape" && ingredientsAtom.get().showAddModal) {
      closeAddModal();
    }
  };
  document.addEventListener("keydown", onKey);

  void load();
  void loadStores();
  const unsub = ingredientsAtom.subscribe(render);
  const unsubStores = storesAtom.subscribe(render);
  return () => {
    document.removeEventListener("keydown", onKey);
    destroyChart();
    unsub();
    unsubStores();
  };
}
