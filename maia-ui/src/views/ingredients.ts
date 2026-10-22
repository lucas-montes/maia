import { api } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { toast } from "../lib/overlay";
import { ingredientsAtom } from "../lib/stores";
import { isEnvelopeFailure, unwrapList } from "../lib/api";
import { panelClose, panelOpen, statusBanner } from "./shared";

const numOrNull = (v: string): number | null => {
  if (v === "") return null;
  const n = parseFloat(v);
  return isNaN(n) ? null : n;
};

/** Ingredients — searchable list, add modal, and inline rename. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Ingredients", "Manage foods • search, edit, and build meals")}
      <div class="flex items-start justify-between gap-4">
        <div></div>
        <button class="btn btn-success btn-sm shrink-0" data-action="ing-open-modal">+ Add Ingredient</button>
      </div>
      <div data-slot="status"></div>
      <div class="flex gap-2">
        <input id="ing-search" placeholder="Search by name, brand or barcode..." class="input input-bordered flex-1" />
        <button class="btn btn-secondary btn-sm" data-action="ing-search">Search</button>
      </div>
      <div class="grid min-h-0 flex-1 gap-2 overflow-auto pr-1" data-slot="list"></div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;

  const render = (): void => {
    const s = ingredientsAtom.get();
    setHtml(statusEl, statusBanner(s));
    const q = s.search.toLowerCase();
    const rows = s.items
      .filter((it) => !q || `${it.name} ${it.brand || ""} ${it.barcode || ""}`.toLowerCase().includes(q))
      .slice(0, 50)
      .map(
        (it) => `
        <div class="group rounded-xl border border-surface-800 bg-base-100 p-3 transition-colors hover:border-surface-700 hover:bg-base-100">
          <div class="flex justify-between gap-3">
            <div class="min-w-0">
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
              </div>
            </div>
            <button class="btn btn-ghost btn-xs self-start opacity-60 group-hover:opacity-100" data-action="ing-edit" data-id="${escapeHtml(it.id)}">Edit</button>
          </div>
        </div>`,
      )
      .join("");
    setHtml(listEl, rows || `<div class="p-3 text-xs text-ink-400">No ingredients</div>`);
    refreshIcons(listEl);
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

  const create = async (button: HTMLButtonElement): Promise<void> => {
    const read = (id: string): string => (document.getElementById(id) as HTMLInputElement | null)?.value ?? "";
    const readNum = (id: string): string => read(id);
    const name = read("ing-new-name").trim();
    const cal = numOrNull(readNum("ing-new-cal"));
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
                  <input id="ing-new-sodium" type="number" step="1" placeholder="5" class="input input-bordered w-full" />
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

  // Esc closes the add modal, matching the old behaviour.
  const onKey = (e: KeyboardEvent): void => {
    if (e.key === "Escape" && ingredientsAtom.get().showAddModal) {
      closeAddModal();
    }
  };
  document.addEventListener("keydown", onKey);

  void load();
  const unsub = ingredientsAtom.subscribe(render);
  return () => {
    document.removeEventListener("keydown", onKey);
    unsub();
  };
}
