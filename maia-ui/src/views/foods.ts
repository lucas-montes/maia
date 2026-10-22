import { api } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { toast } from "../lib/overlay";
import { foodsAtom, ingredientsAtom } from "../lib/stores";
import { isEnvelopeFailure, unwrapList } from "../lib/api";
import { panelClose, panelOpen, statusBanner } from "./shared";

/**
 * Foods (recipes): named recipes with their live per-100g profile.
 *
 * `nutrition` is null when a part's ingredient row is missing — that is shown as
 * such rather than as zeros, because zeros would read as a real value.
 */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Recipes", "Foods built from ingredients · nutrition is calculated from the composition")}
      <div class="flex items-start justify-between gap-4">
        <div></div>
        <button class="btn btn-success btn-sm shrink-0" data-action="food-new">+ New Recipe</button>
      </div>
      <div data-slot="status"></div>
      <div class="grid min-h-0 flex-1 gap-2 overflow-auto pr-1" data-slot="list"></div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;

  const render = (): void => {
    const f = foodsAtom.get();
    setHtml(statusEl, statusBanner(f));
    const rows = f.items
      .map((it) => {
        const n = it.nutrition;
        const macros = n
          ? `<span class="chip">${Number(n.caloriesPer100g).toFixed(0)} kcal</span><span class="chip">P ${Number(n.proteinPer100g).toFixed(1)}</span><span class="chip">C ${Number(n.carbsPer100g).toFixed(1)}</span><span class="chip">F ${Number(n.fatPer100g).toFixed(1)}</span>`
          : `<span class="badge badge-warn">ingredients missing — cannot calculate</span>`;
        const parts = (it.components || [])
          .map((cp: any) => `<span class="rounded-full border border-surface-700 bg-surface-800 px-2 py-1">${escapeHtml(cp.name)} <b class="text-ink-200">${cp.amount}g</b></span>`)
          .join("");
        const action = it.archivedAt
          ? `<button class="btn btn-ghost btn-xs" data-action="food-restore" data-id="${escapeHtml(it.id)}">Restore</button>`
          : `<button class="btn btn-ghost btn-xs text-err-400" data-action="food-archive" data-id="${escapeHtml(it.id)}">Archive</button>`;
        return `<div class="group rounded-xl border border-surface-800 bg-base-100 p-3 transition-colors hover:border-surface-700 hover:bg-base-100">
          <div class="flex justify-between gap-3">
            <div class="min-w-0">
              <div class="truncate text-sm font-medium text-ink-100">${escapeHtml(it.name)}${it.archivedAt ? ` <span class="badge badge-warn ml-2">archived</span>` : ""}</div>
              <div class="mt-1 flex flex-wrap gap-1.5 text-xs text-ink-400">${macros}</div>
            </div>
            <div class="flex shrink-0 gap-2 self-start">${action}<button class="btn btn-ghost btn-xs" data-action="food-edit" data-id="${escapeHtml(it.id)}">Edit</button></div>
          </div>
          <div class="mt-2 flex flex-wrap items-center gap-2 text-xs text-ink-400">${parts || `<span class="text-ink-500">No ingredients yet</span>`}</div>
        </div>`;
      })
      .join("");
    setHtml(listEl, rows || `<div class="p-3 text-xs text-ink-400">No recipes yet. Combine two or more ingredients into a food you can log in one go.</div>`);
    refreshIcons(listEl);
    renderEditor();
  };

  /**
   * Editor for a food's composition: which ingredients go in, and how much.
   *
   * Amounts are absolute amounts in the batch, not percentages — their sum is the
   * weight the per-100g figures are divided by. The draft is the composition and
   * nothing else; nutrition is derived server-side on save.
   */
  let editorKey = "";

  function nameOf(id: string): string {
    const e = ingredientsAtom.get().items.find((x: any) => x.id === id);
    return e ? e.name : "(unknown ingredient)";
  }

  function draftRowsHtml(f: ReturnType<typeof foodsAtom.get>): string {
    return (
      f.draft
        .map(
          (d) => `
          <div class="flex items-center gap-2 rounded-lg border border-surface-800 px-3 py-2">
            <div class="flex-1 truncate text-sm text-ink-100">${escapeHtml(nameOf(d.ingredientId))}</div>
            <input data-action="food-amount" data-id="${escapeHtml(d.ingredientId)}" type="number" step="any" min="0" value="${d.amount}" class="input input-bordered input-sm w-20" />
            <button class="btn btn-ghost btn-xs text-err-400" data-action="food-rm-part" data-id="${escapeHtml(d.ingredientId)}">Remove</button>
          </div>`,
        )
        .join("") || `<div class="text-xs text-ink-400">Nothing added yet.</div>`
    );
  }

  function candidatesHtml(f: ReturnType<typeof foodsAtom.get>): string {
    const q = f.search.trim().toLowerCase();
    const used = new Set(f.draft.map((d) => d.ingredientId));
    const candidates = ingredientsAtom.get().items.filter(
      (e: any) => !e.is_archived && !used.has(e.id) && (!q || (e.name || "").toLowerCase().includes(q)),
    );
    return (
      candidates
        .slice(0, 25)
        .map(
          (e: any) => `
          <div class="flex items-center gap-2 rounded-lg border border-surface-800 px-3 py-2">
            <div class="flex-1 truncate text-sm text-ink-200">${escapeHtml(e.name)}</div>
            <div class="text-xs text-ink-500">${e.calories_per100g} kcal/100g</div>
            <button class="btn btn-ghost btn-xs" data-action="food-add-part" data-id="${escapeHtml(e.id)}">Add</button>
          </div>`,
        )
        .join("") ||
      `<div class="text-xs text-ink-400">${q ? "No ingredient matches your search." : "No other ingredients available yet."}</div>`
    );
  }

  /** Build the full dialog. Runs only when the editor opens or switches
      recipes — keystrokes inside it must not rebuild it. */
  function buildEditorSkeleton(
    modalRoot: HTMLElement,
    f: ReturnType<typeof foodsAtom.get>,
    editing: any,
  ): void {
    modalRoot.innerHTML = `
      <dialog class="modal modal-open" id="food-editor-modal">
        <div class="modal-box flex max-h-[90vh] max-w-2xl flex-col overflow-hidden">
          <div class="flex items-center justify-between border-b border-surface-800 pb-4">
            <div>
              <h3 class="font-semibold text-ink-100">${editing ? "Edit" : "New"} Recipe</h3>
              <p class="mt-1 text-xs text-ink-500">
                Nutrition per 100g is worked out from the amounts below · batch weighs <span data-slot="batch-total">${f.draft.reduce((sum, d) => sum + Number(d.amount || 0), 0).toFixed(0)}</span> g
              </p>
            </div>
            <button class="btn btn-ghost btn-sm" data-action="food-close" aria-label="Close">✕</button>
          </div>
          <div class="min-h-0 flex-1 space-y-5 overflow-auto py-4">
            <div>
              <label class="field-label">Name</label>
              <input id="food-name" placeholder="e.g. Granola" value="${escapeHtml(f.editingName)}" class="input input-bordered w-full" />
            </div>
            <div>
              <div class="mb-3 text-xs font-semibold uppercase tracking-widest text-ink-400">Ingredients (<span data-slot="draft-count">${f.draft.length}</span>)</div>
              <div class="space-y-2" data-slot="draft-rows"></div>
            </div>
            <div>
              <div class="mb-3 text-xs font-semibold uppercase tracking-widest text-ink-400">Add an ingredient</div>
              <input id="food-search" placeholder="Search ingredients..." value="${escapeHtml(f.search)}" class="input input-bordered mb-3 w-full" />
              <div class="max-h-64 space-y-2 overflow-auto" data-slot="candidates"></div>
            </div>
          </div>
          <div class="modal-action border-t border-surface-800 pt-4">
            <span class="mr-auto hidden text-xs text-ink-500 sm:inline">Press Esc to close</span>
            <button class="btn btn-success btn-sm" data-action="food-save">Save Recipe</button>
          </div>
        </div>
        <form method="dialog" class="modal-backdrop"><button>close</button></form>
      </dialog>`;
    updateEditorSections(modalRoot, f, editing);
    refreshIcons(modalRoot);
    (modalRoot.querySelector("#food-name") as HTMLInputElement | null)?.focus();
  }

  /** Refresh only the dynamic sections. The name, search, and amount inputs
      keep their DOM identity, so focus and caret survive every update: draft
      rows are rebuilt only when the row set itself changes, amounts are left
      alone (the inputs hold the typed values), and totals/counters refresh
      in place. */
  let lastDraftSig = "";
  function updateEditorSections(
    modalRoot: HTMLElement,
    f: ReturnType<typeof foodsAtom.get>,
    editing: any,
  ): void {
    const total = f.draft.reduce((sum, d) => sum + Number(d.amount || 0), 0);
    const totalEl = modalRoot.querySelector("[data-slot='batch-total']");
    if (totalEl) totalEl.textContent = total.toFixed(0);
    const countEl = modalRoot.querySelector("[data-slot='draft-count']");
    if (countEl) countEl.textContent = String(f.draft.length);
    if (editing) (modalRoot.querySelector("h3") as HTMLElement).textContent = "Edit Recipe";
    const sig = f.draft.map((d) => d.ingredientId).join("|");
    if (sig !== lastDraftSig) {
      lastDraftSig = sig;
      const rowsEl = modalRoot.querySelector("[data-slot='draft-rows']");
      if (rowsEl) setHtml(rowsEl as HTMLElement, draftRowsHtml(f));
    }
    const candidatesEl = modalRoot.querySelector("[data-slot='candidates']");
    if (candidatesEl) setHtml(candidatesEl as HTMLElement, candidatesHtml(f));
  }

  function renderEditor(): void {
    const f = foodsAtom.get();
    const editing = f.editorOpen ? (f.editingId ? f.items.find((x) => x.id === f.editingId) : null) : null;
    const modalRoot = document.getElementById("modal-root");
    if (!f.editorOpen || !modalRoot) {
      if (modalRoot && modalRoot.firstElementChild?.id === "food-editor-modal") {
        modalRoot.replaceChildren();
        editorKey = "";
      }
      return;
    }
    const key = `${f.editingId ?? ""}`;
    if (key !== editorKey) {
      editorKey = key;
      buildEditorSkeleton(modalRoot, f, editing);
    } else {
      updateEditorSections(modalRoot, f, editing);
    }
  }

  const load = async (): Promise<void> => {
    foodsAtom.set({ ...foodsAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.listFoods();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        foodsAtom.set({ ...foodsAtom.get(), status: "error", message: failure });
        return;
      }
      foodsAtom.set({ ...foodsAtom.get(), items: unwrapList(res), status: "idle", message: "" });
    } catch (e) {
      foodsAtom.set({ ...foodsAtom.get(), status: "error", message: String(e) });
    }
  };

  const loadIngredients = async (): Promise<void> => {
    try {
      const res = await api.listIngredients();
      const data = (res as { data?: unknown })?.data ?? res;
      ingredientsAtom.set({
        ...ingredientsAtom.get(),
        items: Array.isArray(data) ? data : ((data as { items?: unknown })?.items as any[]) ?? [],
        status: "idle",
        message: "",
      });
    } catch {
      // candidates list just stays empty
    }
  };

  const save = (): void => {
    const f = foodsAtom.get();
    const name = f.editingName.trim();
    if (!name) {
      toast("Please enter a name", "err");
      return;
    }
    if (!f.draft.length) {
      toast("Add at least one ingredient", "err");
      return;
    }
    if (f.draft.some((d) => !(d.amount > 0))) {
      toast("Every amount must be greater than 0", "err");
      return;
    }
    const components = f.draft.map((d) => ({ ingredientId: d.ingredientId, amount: d.amount }));
    const id = f.editingId;
    const call = id ? api.updateFood({ id, name, components }) : api.createFood({ name, components });
    call
      .then((res) => {
        if (isEnvelopeFailure(res)) {
          toast(`Could not save: ${(res as { error?: string }).error || "unknown error"}`, "err");
          return;
        }
        foodsAtom.set({ ...foodsAtom.get(), editorOpen: false, editingId: null, editingName: "", draft: [] });
        void load();
      })
      .catch((e) => toast(`Could not save: ${e}`, "err"));
  };

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    if (target.closest("[data-action='food-new']")) {
      foodsAtom.set({ ...foodsAtom.get(), editorOpen: true, editingId: null, editingName: "", draft: [], search: "" });
      return;
    }
    if (target.closest("[data-action='food-close']") || target.closest("[data-food-editor-backdrop]")) {
      foodsAtom.set({ ...foodsAtom.get(), editorOpen: false, editingId: null, editingName: "", draft: [] });
      return;
    }
    const edit = target.closest<HTMLElement>("[data-action='food-edit']");
    if (edit?.dataset.id) {
      const f = foodsAtom.get().items.find((x) => x.id === edit.dataset.id);
      if (f) {
        foodsAtom.set({
          ...foodsAtom.get(),
          editorOpen: true,
          editingId: f.id,
          editingName: f.name,
          draft: (f.components || []).map((cp: any) => ({ ingredientId: cp.ingredientId, amount: Number(cp.amount) })),
          search: "",
        });
      }
      return;
    }
    const archive = target.closest<HTMLElement>("[data-action='food-archive']");
    if (archive?.dataset.id) {
      api.archiveFood(archive.dataset.id).then(() => void load()).catch((e) => toast(`Could not archive: ${e}`, "err"));
      return;
    }
    const restore = target.closest<HTMLElement>("[data-action='food-restore']");
    if (restore?.dataset.id) {
      api.restoreFood(restore.dataset.id).then(() => void load()).catch((e) => toast(`Could not restore: ${e}`, "err"));
      return;
    }
    const addPart = target.closest<HTMLElement>("[data-action='food-add-part']");
    if (addPart?.dataset.id) {
      const id = addPart.dataset.id;
      const f = foodsAtom.get();
      if (f.draft.some((d) => d.ingredientId === id)) return;
      foodsAtom.set({ ...f, draft: [...f.draft, { ingredientId: id, amount: 100 }] });
      return;
    }
    const rmPart = target.closest<HTMLElement>("[data-action='food-rm-part']");
    if (rmPart?.dataset.id) {
      const f = foodsAtom.get();
      foodsAtom.set({ ...f, draft: f.draft.filter((d) => d.ingredientId !== rmPart.dataset.id) });
      return;
    }
    if (target.closest("[data-action='food-save']")) {
      save();
    }
  });

  // The editor modal lives in the global overlay root, so it carries its own
  // listeners — this keeps Save/Remove/Add/Close working while it is open.
  document.getElementById("modal-root")?.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    const editor = document.getElementById("food-editor-modal");
    if (!editor) return;

    if (target.closest("[data-action='food-close']") || target.closest(".modal-backdrop")) {
      foodsAtom.set({ ...foodsAtom.get(), editorOpen: false, editingId: null, editingName: "", draft: [] });
      return;
    }
    const addPart = target.closest<HTMLElement>("[data-action='food-add-part']");
    if (addPart?.dataset.id) {
      const id = addPart.dataset.id;
      const f = foodsAtom.get();
      if (f.draft.some((d) => d.ingredientId === id)) return;
      foodsAtom.set({ ...f, draft: [...f.draft, { ingredientId: id, amount: 100 }] });
      return;
    }
    const rmPart = target.closest<HTMLElement>("[data-action='food-rm-part']");
    if (rmPart?.dataset.id) {
      const f = foodsAtom.get();
      foodsAtom.set({ ...f, draft: f.draft.filter((d) => d.ingredientId !== rmPart.dataset.id) });
      return;
    }
    if (target.closest("[data-action='food-save']")) save();
  });

  // Typing updates the store without re-rendering the input — the old focus
  // hack (focus + setSelectionRange after every keystroke) is gone.
  document.addEventListener("input", (event) => {
    const target = event.target as HTMLElement;
    if (!target.isConnected) return;
    if (target.id === "food-search") {
      foodsAtom.set({ ...foodsAtom.get(), search: (target as HTMLInputElement).value });
      return;
    }
    if (target.id === "food-name") {
      foodsAtom.set({ ...foodsAtom.get(), editingName: (target as HTMLInputElement).value });
      return;
    }
    const amount = target.closest<HTMLElement>("[data-action='food-amount']");
    if (amount?.dataset.id) {
      const parsed = Number.parseFloat((target as HTMLInputElement).value);
      const f = foodsAtom.get();
      foodsAtom.set({
        ...f,
        draft: f.draft.map((d) => (d.ingredientId === amount.dataset.id ? { ...d, amount: Number.isNaN(parsed) ? 0 : parsed } : d)),
      });
    }
  });

  // Esc closes the recipe editor.
  const onKey = (e: KeyboardEvent): void => {
    if (e.key !== "Escape") return;
    if (foodsAtom.get().editorOpen) {
      foodsAtom.set({ ...foodsAtom.get(), editorOpen: false, editingId: null, editingName: "", draft: [] });
    }
  };
  document.addEventListener("keydown", onKey);

  void load();
  void loadIngredients();
  const unsub = foodsAtom.subscribe(render);
  return () => {
    document.removeEventListener("keydown", onKey);
    unsub();
  };
}
