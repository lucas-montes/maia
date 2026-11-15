import { api, isEnvelopeFailure, unwrapList, type ReceiptEffect, type StoreDetails } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { takeFocus } from "../lib/focus";
import { refreshIcons } from "../lib/icons";
import { toast } from "../lib/overlay";
import { receiptsAtom, storesAtom } from "../lib/stores";
import { panelClose, panelOpen, statusBanner } from "./shared";

type StoreRow = StoreDetails & { createdAt: number; receiptCount: number; priceCount: number };

/** The identifier fields, in the order the form shows them. */
const FIELDS: [keyof StoreDetails, string][] = [
  ["brand", "Brand"],
  ["legalName", "Legal name"],
  ["country", "Country (ISO, e.g. HR)"],
  ["taxId", "Tax number (OIB, SIRET…)"],
  ["taxIdKind", "Tax number kind"],
  ["vatNumber", "VAT number"],
  ["address", "Address"],
  ["postcode", "Postcode"],
  ["city", "City"],
  ["website", "Website"],
];

/** Stores: what identifies each one (so two shops of a chain in two countries
 *  stay two stores), editable; how many receipts and prices it has; and the
 *  receipts that made or changed it. */
export function mount(root: HTMLElement): () => void {
  let selected: string | null = takeFocus("stores");
  let origins: ReceiptEffect[] = [];

  root.innerHTML = `
    ${panelOpen("Stores", "Where you shop")}
      <div data-slot="status"></div>
      <div class="flex gap-2">
        <input id="store-name" placeholder="New store name" class="input input-bordered flex-1" />
        <button class="btn btn-primary btn-sm" data-action="store-create">Create</button>
      </div>
      <div class="grid min-h-0 flex-1 gap-4 md:grid-cols-2">
        <div class="list-divided min-h-0 overflow-auto" data-slot="list"></div>
        <div class="min-h-0 overflow-auto" data-slot="detail"></div>
      </div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;
  const detailEl = root.querySelector("[data-slot='detail']") as HTMLElement;

  const items = (): StoreRow[] => storesAtom.get().items as StoreRow[];

  const detailHtml = (s: StoreRow): string => {
    const receipts = [...new Set(origins.map((e) => e.receiptId))];
    return `
      <div class="space-y-3 rounded border border-surface-800 bg-base-100 p-4">
        <div>
          <label class="field-label text-xs">Name</label>
          <input class="input input-bordered input-sm w-full" data-field="name" value="${escapeHtml(s.name)}" />
        </div>
        <div class="grid grid-cols-2 gap-2">
          ${FIELDS.map(
            ([key, label]) => `
            <div>
              <label class="field-label text-xs">${escapeHtml(label)}</label>
              <input class="input input-bordered input-sm w-full" data-field="${key}" value="${escapeHtml(String(s[key] ?? ""))}" />
            </div>`,
          ).join("")}
        </div>
        <div class="flex items-center justify-between">
          <span class="text-xs text-ink-500">${s.receiptCount} receipt${s.receiptCount === 1 ? "" : "s"} · ${s.priceCount} price${s.priceCount === 1 ? "" : "s"}${s.detailsSource ? ` · details by ${escapeHtml(s.detailsSource)}` : ""}</span>
          <button class="btn btn-primary btn-sm" data-action="store-save">Save</button>
        </div>
        ${
          receipts.length
            ? `<div class="text-xs text-ink-400">From receipt${receipts.length === 1 ? "" : "s"}: ${receipts
                .map((id) => `<button class="link link-accent" data-action="open-receipt" data-receipt="${escapeHtml(id)}">${escapeHtml(id.slice(0, 8))}</button>`)
                .join(" ")}</div>`
            : ""
        }
      </div>`;
  };

  const render = (): void => {
    const s = storesAtom.get();
    setHtml(statusEl, statusBanner(s));
    setHtml(
      listEl,
      items()
        .map(
          (st) => `
          <button type="button" class="list-row w-full text-left ${st.id === selected ? "active" : ""}" data-store="${escapeHtml(st.id)}">
            <div class="min-w-0 flex-1">
              <div class="truncate text-sm text-ink-100">${escapeHtml(st.name)}</div>
              <div class="text-xs text-ink-500">${escapeHtml([st.country, st.city, st.taxId].filter(Boolean).join(" · ") || "no details yet")}</div>
            </div>
            <span class="text-xs text-ink-400">${st.receiptCount} · ${st.priceCount}</span>
          </button>`,
        )
        .join("") || `<div class="p-3 text-xs text-ink-400">No stores</div>`,
    );
    const current = items().find((x) => x.id === selected);
    setHtml(detailEl, current ? detailHtml(current) : `<div class="p-3 text-xs text-ink-500">Pick a store to see and edit what identifies it.</div>`);
    refreshIcons(root);
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

  const select = async (id: string): Promise<void> => {
    selected = id;
    origins = [];
    render();
    const res = await api
      .matchingRequest<{ effects: ReceiptEffect[] }>("GET", `/receipt-effects?table=stores&id=${encodeURIComponent(id)}`)
      .catch(() => null);
    if (res?.success && res.data && selected === id) {
      origins = res.data.effects;
      render();
    }
  };

  const save = async (): Promise<void> => {
    const s = items().find((x) => x.id === selected);
    if (!s) return;
    const value = (k: string): string => root.querySelector<HTMLInputElement>(`[data-field='${k}']`)?.value.trim() ?? "";
    const name = value("name");
    if (!name) {
      toast("A store needs a name", "err");
      return;
    }
    const details: Record<string, string> = {};
    for (const [key] of FIELDS) {
      const v = value(key);
      if (v !== String(s[key] ?? "")) details[key] = v;
    }
    const id = encodeURIComponent(s.id);
    if (name !== s.name) {
      const res = await api.matchingRequest("PUT", `/stores/${id}/name`, { name });
      if (!res.success) return void toast(res.error ?? "Could not rename the store", "err");
    }
    if (Object.keys(details).length) {
      const res = await api.matchingRequest("PUT", `/stores/${id}/details`, details);
      if (!res.success) return void toast(res.error ?? "Could not save the store", "err");
    }
    toast("Store saved", "ok");
    await load();
  };

  root.addEventListener("click", (event) => {
    const t = event.target as HTMLElement;
    const row = t.closest<HTMLElement>("[data-store]");
    if (row?.dataset.store) {
      void select(row.dataset.store);
      return;
    }
    if (t.closest("[data-action='store-save']")) {
      void save();
      return;
    }
    const open = t.closest<HTMLElement>("[data-action='open-receipt']");
    if (open?.dataset.receipt) {
      receiptsAtom.set({ ...receiptsAtom.get(), selectedId: open.dataset.receipt });
      window.location.hash = "#/receipts";
      return;
    }
    if (!t.closest("[data-action='store-create']")) return;
    const input = root.querySelector<HTMLInputElement>("#store-name");
    const name = input?.value.trim();
    if (name) {
      api.createStore({ name }).then(() => {
        if (input) input.value = "";
        void load();
      });
    }
  });

  void load().then(() => {
    if (selected) void select(selected);
  });
  const unsub = storesAtom.subscribe(render);
  return unsub;
}
