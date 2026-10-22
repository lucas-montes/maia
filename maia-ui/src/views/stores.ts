import { api } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { storesAtom } from "../lib/stores";
import { isEnvelopeFailure, unwrapList } from "../lib/api";
import { panelClose, panelOpen, statusBanner } from "./shared";

/** Stores — simple CRUD: create by name, list with created dates. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Stores", "Where you shop")}
      <div data-slot="status"></div>
      <div class="flex gap-2">
        <input id="store-name" placeholder="New store name" class="input input-bordered flex-1" />
        <button class="btn btn-primary btn-sm" data-action="store-create">Create</button>
      </div>
      <div class="list-divided" data-slot="list"></div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;

  const render = (): void => {
    const s = storesAtom.get();
    setHtml(statusEl, statusBanner(s));
    setHtml(
      listEl,
      s.items
        .map(
          (st) => `
          <div class="flex justify-between">
            <span class="text-sm text-ink-100">${escapeHtml(st.name)}</span>
            <span class="text-xs text-ink-400">${new Date(st.createdAt).toLocaleDateString()}</span>
          </div>`,
        )
        .join("") || `<div class="p-3 text-xs text-ink-400">No stores</div>`,
    );
    refreshIcons(listEl);
  };

  const load = async (): Promise<void> => {
    storesAtom.set({ ...storesAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.listStores();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        storesAtom.set({ ...storesAtom.get(), status: "error", message: failure });
        return;
      }
      storesAtom.set({ ...storesAtom.get(), items: unwrapList(res), status: "idle", message: "" });
    } catch (e) {
      storesAtom.set({ ...storesAtom.get(), status: "error", message: String(e) });
    }
  };

  root.addEventListener("click", (event) => {
    if (!(event.target as HTMLElement).closest("[data-action='store-create']")) return;
    const input = root.querySelector<HTMLInputElement>("#store-name");
    const name = input?.value.trim();
    if (name) {
      api.createStore({ name }).then(() => {
        if (input) input.value = "";
        void load();
      });
    }
  });

  void load();
  const unsub = storesAtom.subscribe(render);
  return unsub;
}
