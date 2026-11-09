import { api, isEnvelopeFailure, unwrapList, type FinanceAccount, type FinanceTransaction } from "../lib/api";
import { escapeHtml, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { transactionsAtom } from "../lib/stores";
import { backToDashboard, panelClose, panelOpen, statusBanner } from "./shared";

function money(value: number): string {
  return value.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

const TYPE_CLASS: Record<string, string> = {
  expense: "text-err-400",
  income: "text-ok-400",
  transfer: "text-ink-100",
};

const TYPE_BADGE: Record<string, string> = {
  expense: "badge-err",
  income: "badge-ok",
  transfer: "badge-ghost",
};

/** Transactions — a real ledger view: type and account filters, transfers
 *  named with both sides, and amounts that stop announcing `→ X base
 *  (rate 1)` for entries already in the base currency. */
export function mount(root: HTMLElement): () => void {
  let accounts: FinanceAccount[] = [];
  let baseCurrency = "EUR";
  let typeFilter = "";
  let accountFilter = "";

  root.innerHTML = `
    ${panelOpen("Transactions", "Expenses, income and transfers")}
      <div data-slot="status"></div>
      <div class="flex flex-wrap gap-2">
        <input placeholder="Search description / category" class="input input-bordered w-64" data-filter="search" />
        <select class="select select-sm select-bordered" data-filter="type">
          <option value="">All types</option>
          <option value="expense">Expenses</option>
          <option value="income">Income</option>
          <option value="transfer">Transfers</option>
        </select>
        <select class="select select-sm select-bordered" data-filter="account">
          <option value="">All accounts</option>
        </select>
      </div>
      <div data-slot="counts" class="text-xs text-ink-500"></div>
      <div class="list-divided min-h-0 flex-1 overflow-auto" data-slot="list"></div>
      <div>${backToDashboard()}</div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const listEl = root.querySelector("[data-slot='list']") as HTMLElement;
  const countsEl = root.querySelector("[data-slot='counts']") as HTMLElement;
  const searchInput = root.querySelector<HTMLInputElement>("[data-filter='search']") as HTMLInputElement;
  const accountSelect = root.querySelector<HTMLSelectElement>("[data-filter='account']") as HTMLSelectElement;

  /** The conversion line only earns its place when the entry is not already
   *  in base currency (and the rate is not a pointless 1). */
  function conversionLine(t: FinanceTransaction): string {
    if (t.currencyCode === baseCurrency || t.rateUsed === 0) return "";
    return `<div class="text-xs text-ink-500">≈ ${money(t.amountBase)} ${escapeHtml(baseCurrency)} @ ${t.rateUsed}</div>`;
  }

  function accountLeg(t: FinanceTransaction): string {
    const own = bothNames(t.accountName, t.accountId);
    const other = bothNames(t.toAccountName, t.toAccountId);
    if (t.type === "transfer") {
      return `${own} <i data-lucide="arrow-right" class="inline h-3 w-3 opacity-60"></i> ${other}`;
    }
    return own;
  }

  function bothNames(name: string | null, id: string | null): string {
    if (name) return name;
    if (id) return "unknown account";
    return "no account";
  }

  function rowHtml(t: FinanceTransaction): string {
    const label = t.note || t.category || "";
    const colored = TYPE_CLASS[t.type] ?? "text-ink-100";
    return `
      <div class="flex items-start justify-between gap-3 py-1.5">
        <div class="min-w-0 flex-1">
          <div class="flex flex-wrap items-center gap-2">
            <span class="badge badge-xs ${TYPE_BADGE[t.type] ?? "badge-ghost"}">${escapeHtml(t.type)}</span>
            ${t.isDraft ? `<span class="badge badge-xs badge-warn">draft</span>` : ""}
            ${label ? `<span class="truncate text-sm text-ink-200">${escapeHtml(label)}</span>` : ""}
          </div>
          <div class="mt-0.5 text-xs text-ink-500">${accountLeg(t)}${t.category ? ` • ${escapeHtml(t.category)}` : ""}</div>
        </div>
        <div class="shrink-0 text-right">
          <div class="font-mono text-sm ${colored}">
            ${t.type === "expense" ? "−" : t.type === "income" ? "+" : ""}${money(t.amount)} ${escapeHtml(t.currencyCode)}
          </div>
          <div class="text-xs text-ink-500">${new Date(t.date).toLocaleDateString()}</div>
          ${conversionLine(t)}
        </div>
      </div>`;
  }

  const render = (): void => {
    const tr = transactionsAtom.get();
    setHtml(statusEl, statusBanner(tr));
    const q = tr.search.toLowerCase();
    let filtered = tr.items as FinanceTransaction[];
    if (q) {
      filtered = filtered.filter((t) =>
        `${t.note || ""} ${t.category || ""} ${t.type} ${t.accountName || ""} ${t.toAccountName || ""}`
          .toLowerCase()
          .includes(q),
      );
    }
    if (typeFilter) filtered = filtered.filter((t) => t.type === typeFilter);
    if (accountFilter) {
      filtered = filtered.filter(
        (t) => t.accountId === accountFilter || t.toAccountId === accountFilter,
      );
    }
    const shown = filtered.slice(0, 200);
    setHtml(
      countsEl,
      `${filtered.length} transaction${filtered.length !== 1 ? "s" : ""}${filtered.length > shown.length ? ` (showing ${shown.length})` : ""}`,
    );
    setHtml(
      listEl,
      shown
        .map(rowHtml)
        .join("") || `<div class="p-3 text-sm text-ink-500">No transactions match.</div>`,
    );
    refreshIcons(listEl);
  };

  const rebuildAccountOptions = (): void => {
    const current = accountFilter;
    accountSelect.innerHTML = `
      <option value="">All accounts</option>
      ${accounts
        .map(
          (a) =>
            `<option value="${escapeHtml(a.id)}" ${a.id === current ? "selected" : ""}>${escapeHtml(a.name)}</option>`,
        )
        .join("")}`;
  };

  // Live filter on keystrokes — updates the list container only.
  root.addEventListener("input", (event) => {
    if ((event.target as HTMLElement) === searchInput) {
      transactionsAtom.set({ ...transactionsAtom.get(), search: searchInput.value });
    }
  });
  root.addEventListener("change", (event) => {
    const target = event.target as HTMLSelectElement;
    if (target.dataset.filter === "type") {
      typeFilter = target.value;
      render();
      return;
    }
    if (target.dataset.filter === "account") {
      accountFilter = target.value;
      render();
    }
  });

  const load = async (): Promise<void> => {
    transactionsAtom.set({ ...transactionsAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.listTransactions();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        transactionsAtom.set({ ...transactionsAtom.get(), status: "error", message: failure });
        render();
        return;
      }
      transactionsAtom.set({ ...transactionsAtom.get(), items: unwrapList(res), status: "idle", message: "" });
    } catch (e) {
      transactionsAtom.set({ ...transactionsAtom.get(), status: "error", message: String(e) });
    }
    render();
  };

  const loadAccounts = async (): Promise<void> => {
    try {
      const res = await api.listAccounts();
      if (isEnvelopeFailure(res)) return;
      accounts = ((res as { data?: unknown }).data ?? []) as FinanceAccount[];
      rebuildAccountOptions();
    } catch {
      // Filters degrade to type-only when accounts cannot load.
    }
  };

  const loadBaseCurrency = async (): Promise<void> => {
    try {
      const res = await api.getBaseCurrency();
      if (res.success && res.data) {
        baseCurrency = res.data;
        if (transactionsAtom.get().status === "idle") render();
      }
    } catch {
      // The command's own default (EUR) is what rows already assume.
    }
  };

  void load();
  void loadAccounts();
  void loadBaseCurrency();
  const unsub = transactionsAtom.subscribe(render);
  return unsub;
}
