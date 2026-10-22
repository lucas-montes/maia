import { api } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { transactionsAtom } from "../lib/stores";
import { isEnvelopeFailure, unwrapList } from "../lib/api";
import { panelClose, panelOpen, statusBanner } from "./shared";

/** Transactions — read-only list with a live search filter. Only the list
    re-renders on keystrokes; the input keeps focus. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `
    ${panelOpen("Transactions", "Expenses and income")}
      <div data-slot="status"></div>
      <div class="flex gap-2">
        <input id="tr-search" placeholder="Search transactions" class="input input-bordered flex-1" />
        <button class="btn btn-secondary btn-sm" data-action="tr-search">Search</button>
      </div>
      <div class="list-divided min-h-0 flex-1 overflow-auto" data-slot="list"></div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;
  const searchInput = root.querySelector<HTMLInputElement>("#tr-search") as HTMLInputElement;

  const render = (): void => {
    const tr = transactionsAtom.get();
    setHtml(statusEl, statusBanner(tr));
    const q = tr.search.toLowerCase();
    const filtered = q
      ? tr.items.filter((x) => `${x.category || ""} ${x.note || ""} ${x.type}`.toLowerCase().includes(q))
      : tr.items;
    setHtml(
      listEl,
      filtered
        .slice(0, 50)
        .map(
          (x) => `
          <div class="flex justify-between">
            <span class="text-sm ${x.type === "expense" ? "text-err-400" : x.type === "income" ? "text-ok-400" : "text-ink-100"}">
              ${escapeHtml(x.type)} ${x.amount} ${escapeHtml(x.currencyCode)} → ${x.amountBase} base (rate ${x.rateUsed})
            </span>
            <span class="text-xs text-ink-400">${new Date(x.date).toLocaleDateString()} • ${escapeHtml(x.category || "")}</span>
          </div>`,
        )
        .join("") || `<div class="p-3 text-xs text-ink-400">No transactions</div>`,
    );
    refreshIcons(listEl);
  };

  // Live filter on keystrokes — updates the list container only.
  root.addEventListener("input", (event) => {
    if ((event.target as HTMLElement) === searchInput) {
      transactionsAtom.set({ ...transactionsAtom.get(), search: searchInput.value });
    }
  });
  root.addEventListener("click", (event) => {
    if (!(event.target as HTMLElement).closest("[data-action='tr-search']")) return;
    transactionsAtom.set({ ...transactionsAtom.get(), search: searchInput.value });
  });

  const load = async (): Promise<void> => {
    transactionsAtom.set({ ...transactionsAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.listTransactions();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        transactionsAtom.set({ ...transactionsAtom.get(), status: "error", message: failure });
        return;
      }
      transactionsAtom.set({ ...transactionsAtom.get(), items: unwrapList(res), status: "idle", message: "" });
    } catch (e) {
      transactionsAtom.set({ ...transactionsAtom.get(), status: "error", message: String(e) });
    }
  };

  void load();
  const unsub = transactionsAtom.subscribe(render);
  return unsub;
}
