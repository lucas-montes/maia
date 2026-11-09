import Chart from "chart.js/auto";
import {
  api,
  isEnvelopeFailure,
  unwrapList,
  type FinanceAccount,
  type FinanceTransaction,
  type StatementSummary,
} from "../lib/api";
import { escapeHtml, formatEpoch, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { accountsAtom } from "../lib/stores";
import { panelClose, panelOpen, statusBanner } from "./shared";

type Period = "day" | "week" | "month";

const SOURCE_LABELS: Record<string, string> = {
  credit_agricole: "Crédit Agricole",
  trading212: "Trading 212",
  firstrade: "Firstrade",
};

/** Account types, matching what the budget accounts push accepts and the
 *  phone's `AccountType` understands; the list keeps the account's current
 *  type visible even when it is outside this set. */
const ACCOUNT_TYPES = ["bank", "savings", "investment", "cash", "credit", "other"];

/** Amount with its currency code, signed for the account the row belongs to.
 *  Income and inbound transfers show `+`; expenses and outbound transfers
 *  show `−`; rows already in base currency use their base amount, foreign
 *  currency keeps its own. */
function signedAmount(
  tx: FinanceTransaction,
  accountId: string,
  baseCurrency: string,
): { value: number; currency: string } {
  let inbound: boolean;
  switch (tx.type) {
    case "income":
      inbound = tx.accountId === accountId;
      break;
    case "transfer":
      inbound = tx.toAccountId === accountId;
      break;
    default:
      inbound = false;
  }
  // Rows already in base currency fold into amount_base, which is what
  // balances are kept in; foreign rows show their original amount.
  const magnitude = tx.currencyCode === baseCurrency ? tx.amountBase : tx.amount;
  return { value: inbound ? magnitude : -Math.abs(magnitude), currency: tx.currencyCode };
}

type SeriesPoint = { bucket: string; balance: number };

/** Accounts — the real list: computed balance per account, click-through to
 *  everything that matters for it (transactions, the statements that filled
 *  it, its balance history). */
export function mount(root: HTMLElement): () => void {
  /** Open account detail; `null` shows the list. Local to the view (like the
   *  statements view's state) so re-renders keep the selection alive. */
  let selectedId: string | null = null;
  /** The account whose edit panel is open, when one is. */
  let editingAccountId: string | null = null;
  let baseCurrency = "EUR";
  let statementItems: StatementSummary[] = [];
  let transactionItems: FinanceTransaction[] = [];
  let chart: Chart | null = null;
  let series: SeriesPoint[] = [];
  let seriesPeriod: Period = "month";

  root.innerHTML = `
    ${panelOpen("Accounts", "Balances and history across your accounts")}
      <div data-slot="status"></div>
      <div data-slot="content"></div>
    ${panelClose}
  `;
  const statusEl = root.querySelector("[data-slot='status']") as HTMLElement;
  const contentEl = root.querySelector("[data-slot='content']") as HTMLElement;

  function accountById(id: string): FinanceAccount | undefined {
    return accountsAtom.get().items.find((a) => a.id === id);
  }

  function statementCount(id: string): number {
    return statementItems.filter((s) => s.accountId === id).length;
  }

  function transactionsOf(id: string): FinanceTransaction[] {
    return transactionItems
      .filter((t) => !t.isDraft && (t.accountId === id || t.toAccountId === id))
      .sort((a, b) => b.date - a.date)
      .slice(0, 25);
  }

  function statementsOf(id: string): StatementSummary[] {
    return statementItems.filter((s) => s.accountId === id);
  }

  function headlineBalance(value: number): string {
    if (!Number.isFinite(value)) return "—";
    return value.toLocaleString(undefined, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    });
  }

  function listHtml(items: FinanceAccount[]): string {
    if (items.length === 0) {
      return `
        <div class="rounded border border-surface-800 bg-base-100 p-4 text-sm text-ink-400">
          No accounts yet. Import a statement on the
          <a class="link" href="#/statements">Statements</a> page and its accounts appear here with their real balance.
        </div>`;
    }
    return `
      <div class="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto">
        ${items
          .map((ac) => {
            const source = ac.source ? SOURCE_LABELS[ac.source] ?? ac.source : null;
            const statements = statementCount(ac.id);
            const negative = ac.balance < 0;
            return `
            <button class="list-row w-full text-left" type="button" data-account-id="${escapeHtml(ac.id)}">
              <div class="flex flex-wrap items-center gap-2">
                <span class="truncate text-sm font-medium text-ink-100">${escapeHtml(ac.name)}</span>
                <span class="badge badge-ghost badge-xs">${escapeHtml(ac.type)}</span>
                ${source ? `<span class="badge badge-xs badge-outline">${escapeHtml(source)}</span>` : ""}
                ${statements > 0 ? `<span class="badge badge-xs badge-muted">${statements} statement${statements !== 1 ? "s" : ""}</span>` : ""}
              </div>
              <div class="text-sm font-mono ${negative ? "text-err-400" : "text-ok-400"}">
                ${ac.balance < 0 ? "−" : ""}${headlineBalance(Math.abs(ac.balance))} ${escapeHtml(baseCurrency)}
              </div>
            </button>`;
          })
          .join("")}
      </div>`;
  }

  function transactionEntries(id: string): string {
    const rows = transactionsOf(id).map((t) => {
      const { value, currency } = signedAmount(t, id, baseCurrency);
      const counterpart =
        t.type === "transfer"
          ? (t.accountId === id ? t.toAccountName : t.accountName) ?? "?"
          : null;
      const label = t.note || t.category || t.type;
      return `
        <tr>
          <td class="whitespace-nowrap text-xs text-ink-400">${escapeHtml(formatEpoch(t.date))}</td>
          <td>
            <div class="text-sm text-ink-200">${escapeHtml(label)}</div>
            ${counterpart ? `<div class="text-xs text-ink-500">↔ ${escapeHtml(counterpart)}</div>` : ""}
          </td>
          <td class="whitespace-nowrap text-right font-mono text-sm ${value >= 0 ? "text-ok-400" : "text-err-400"}">
            ${value >= 0 ? "+" : "−"}${headlineBalance(Math.abs(value))} ${escapeHtml(currency)}
          </td>
        </tr>`;
    });
    return `
      <div class="rounded border border-surface-800 bg-base-100 p-4">
        <h3 class="mb-2 text-sm font-medium text-ink-200">Recent transactions</h3>
        <table class="table table-sm w-full">
          <thead><tr><th>Date</th><th>Description</th><th class="text-right">Effect</th></tr></thead>
          <tbody>
            ${rows.join("") || `<tr><td colspan="3" class="text-center text-sm text-ink-500">No transactions on this account.</td></tr>`}
          </tbody>
        </table>
      </div>`;
  }

  function statementEntries(id: string): string {
    const rows = statementsOf(id).map((s) => {
      const period = s.periodStart && s.periodEnd ? `${s.periodStart} → ${s.periodEnd}` : "";
      return `
        <tr>
          <td class="text-sm text-ink-200">${escapeHtml(`${s.institution} · ${s.accountName}`)}</td>
          <td class="text-xs text-ink-500">${escapeHtml(period)}</td>
          <td class="text-xs">${s.pendingCount} pending · ${s.importedCount} imported</td>
          <td><span class="badge badge-ghost badge-xs">${escapeHtml(s.status)}</span></td>
        </tr>`;
    });
    return `
      <div class="rounded border border-surface-800 bg-base-100 p-4">
        <div class="mb-2 flex items-center justify-between">
          <h3 class="text-sm font-medium text-ink-200">Statements</h3>
          ${statementsOf(id).length > 0 ? `<a class="btn btn-ghost btn-xs" href="#/statements">Open Statements</a>` : ""}
        </div>
        <table class="table table-sm w-full">
          <thead><tr><th>Statement</th><th>Period</th><th>Entries</th><th>Status</th></tr></thead>
          <tbody>
            ${rows.join("") || `<tr><td colspan="4" class="text-center text-sm text-ink-500">No statement accounts under this account.</td></tr>`}
          </tbody>
        </table>
      </div>`;
  }

  function editPanelHtml(ac: FinanceAccount): string {
    const types = [...new Set([...ACCOUNT_TYPES, ac.type])];
    return `
      <div class="mt-3 space-y-3 border-t border-surface-800 pt-3">
        <div class="grid grid-cols-2 gap-3 text-sm">
          <label class="space-y-1">
            <span class="text-xs text-ink-500">Name</span>
            <input class="input input-bordered input-sm w-full" data-edit-field="name" value="${escapeHtml(ac.name)}" />
          </label>
          <label class="space-y-1">
            <span class="text-xs text-ink-500">Type</span>
            <select class="select select-bordered select-sm w-full" data-edit-field="type">
              ${types
                .map(
                  (t) =>
                    `<option value="${escapeHtml(t)}" ${t === ac.type ? "selected" : ""}>${escapeHtml(t)}</option>`,
                )
                .join("")}
            </select>
          </label>
          <label class="space-y-1">
            <span class="text-xs text-ink-500">Opening balance (${escapeHtml(baseCurrency)})</span>
            <input class="input input-bordered input-sm w-full font-mono" type="number" step="0.01" data-edit-field="opening" value="${ac.openingBalance}" />
          </label>
          <label class="space-y-1">
            <span class="text-xs text-ink-500">Note</span>
            <input class="input input-bordered input-sm w-full" data-edit-field="note" value="${escapeHtml(ac.note ?? "")}" />
          </label>
        </div>
        <p class="text-xs text-ink-500">Every balance on this account shifts by the same amount: the ledger is untouched, the opening balance is what changes.</p>
        <div class="flex justify-end gap-2">
          <button class="btn btn-ghost btn-sm" type="button" data-action="account-edit-cancel">Cancel</button>
          <button class="btn btn-primary btn-sm" type="button" data-action="account-edit-save" data-account-id="${escapeHtml(ac.id)}">Save</button>
        </div>
      </div>`;
  }

  function detailHtml(ac: FinanceAccount): string {
    const source = ac.source ? SOURCE_LABELS[ac.source] ?? ac.source : null;
    const negative = ac.balance < 0;
    const editing = editingAccountId === ac.id;
    return `
      <div class="min-h-0 flex-1 space-y-4 overflow-auto">
        <button class="btn btn-ghost btn-sm" type="button" data-action="accounts-back">← Back to accounts</button>

        <div class="rounded border border-surface-800 bg-base-100 p-4">
          <div class="mb-3 flex flex-wrap items-center gap-2">
            <h3 class="text-base font-medium text-ink-100">${escapeHtml(ac.name)}</h3>
            <span class="badge badge-ghost badge-xs">${escapeHtml(ac.type)}</span>
            ${source ? `<span class="badge badge-xs badge-outline">${escapeHtml(source)}</span>` : ""}
            ${!editing ? `<button class="btn btn-ghost btn-xs ml-auto" type="button" data-action="account-edit" data-account-id="${escapeHtml(ac.id)}">
              <i data-lucide="pencil" class="h-3.5 w-3.5"></i> Edit
            </button>` : ""}
          </div>
          ${editing ? editPanelHtml(ac) : ""}
          <div class="grid grid-cols-2 gap-3 text-sm">
            <div><span class="text-ink-500">Current balance:</span> <span class="font-mono ${negative ? "text-err-400" : "text-ok-400"}">${ac.balance < 0 ? "−" : ""}${headlineBalance(Math.abs(ac.balance))} ${escapeHtml(baseCurrency)}</span></div>
            <div><span class="text-ink-500">Opening balance:</span> <span class="font-mono text-ink-300">${headlineBalance(ac.openingBalance)} ${escapeHtml(baseCurrency)}</span></div>
            ${ac.externalId ? `<div><span class="text-ink-500">Account number:</span> <span class="font-mono text-ink-300">${escapeHtml(ac.externalId)}</span></div>` : ""}
            <div><span class="text-ink-500">Created:</span> <span class="text-ink-300">${escapeHtml(formatEpoch(ac.createdAt))}</span></div>
          </div>
        </div>

        <div class="rounded border border-surface-800 bg-base-100 p-4">
          <div class="mb-2 flex items-center justify-between">
            <h3 class="text-sm font-medium text-ink-200">Balance over time</h3>
            <div class="join">
              ${(["day", "week", "month"] as Period[])
                .map(
                  (p) => `
                <button class="btn btn-xs join-item ${seriesPeriod === p ? "btn-primary" : ""}" type="button" data-action="balance-period" data-period="${p}">${p}</button>`,
                )
                .join("")}
            </div>
          </div>
          <div class="h-48"><canvas data-slot="balance-chart"></canvas></div>
        </div>

        ${transactionEntries(ac.id)}
        ${statementEntries(ac.id)}
      </div>`;
  }

  function render(): void {
    const a = accountsAtom.get();
    setHtml(statusEl, statusBanner(a));
    const items = a.items as FinanceAccount[];
    if (selectedId) {
      const ac = accountById(selectedId);
      if (!ac) {
        selectedId = null;
      } else {
        setHtml(contentEl, detailHtml(ac));
        refreshIcons(contentEl);
        renderChart();
        void loadSeries();
        return;
      }
    }
    setHtml(contentEl, listHtml(items));
    refreshIcons(contentEl);
  }

  // ---- Balance history chart ----

  function renderChart(): void {
    const canvas = contentEl.querySelector<HTMLCanvasElement>("[data-slot='balance-chart']");
    if (!canvas) return;
    // A re-render rebuilds the page; the previous instance's canvas is
    // detached then, and updating it would draw into nothing.
    if (chart && !chart.canvas.isConnected) {
      chart.destroy();
      chart = null;
    }
    const data = series.map((p) => p.balance);
    const labels = series.map((p) => p.bucket);
    if (chart) {
      chart.data.labels = labels as never;
      chart.data.datasets[0].data = data as never;
      chart.update();
      return;
    }
    chart = new Chart(canvas, {
      type: "line",
      data: {
        labels: labels as never,
        datasets: [
          {
            data: data as never,
            borderColor: "#3987e5",
            backgroundColor: "rgba(57, 135, 229, 0.15)",
            borderWidth: 2,
            pointRadius: 0,
            tension: 0.2,
            fill: true,
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
  }

  const loadSeries = async (): Promise<void> => {
    const id = selectedId;
    if (!id) return;
    try {
      const res = await api.financeAccountBalance(id, seriesPeriod);
      if (selectedId !== id) return;
      series = (unwrapList(res) as SeriesPoint[]) ?? [];
      renderChart();
    } catch {
      // The chart already shows the last good series; a failure must not
      // blank the detail page.
    }
  };

  // ---- Data ----

  /** The transaction and statement lists drive badges on the list page and
   *  the detail headings; a failure must not blank anything, so they are
   *  loaded best-effort in one pass. */
  const loadTransactionsAndStatements = async (): Promise<void> => {
    const [trRes, stRes] = await Promise.allSettled([
      api.listTransactions(),
      api.listStatements({}),
    ] as const);
    if (trRes.status === "fulfilled" && !isEnvelopeFailure(trRes.value)) {
      transactionItems = unwrapList(trRes.value) as FinanceTransaction[];
    }
    if (stRes.status === "fulfilled") {
      const res = stRes.value as { data?: { items?: StatementSummary[] } };
      statementItems = res.data?.items ?? [];
    }
    if (selectedId === null) render();
  };

  const load = async (): Promise<void> => {
    accountsAtom.set({ ...accountsAtom.get(), status: "loading", message: "" });
    try {
      const res = await api.listAccounts();
      const failure = isEnvelopeFailure(res);
      if (failure) {
        accountsAtom.set({ ...accountsAtom.get(), status: "error", message: failure });
        render();
        return;
      }
      const data = (res as { data?: unknown })?.data ?? res;
      accountsAtom.set({ ...accountsAtom.get(), items: Array.isArray(data) ? data : [], status: "idle", message: "" });
    } catch (e) {
      accountsAtom.set({ ...accountsAtom.get(), status: "error", message: String(e) });
    }
    render();
  };

  const unsub = accountsAtom.subscribe(render);

  const loadBaseCurrency = async (): Promise<void> => {
    try {
      const res = await api.getBaseCurrency();
      if (res.success && res.data) {
        baseCurrency = res.data;
        if (selectedId === null) render();
      }
    } catch {
      // The command's own default (EUR) is what the list already shows.
    }
  };

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    const accountRow = target.closest<HTMLElement>("[data-account-id]");
    // A row carrying its own action (the detail page's Edit/Save buttons also
    // carry data-account-id) is a control, not a navigation.
    if (accountRow?.dataset.accountId && !accountRow.closest("[data-action]")) {
      selectedId = accountRow.dataset.accountId;
      render();
      return;
    }
    if (target.matches("[data-action='accounts-back']")) {
      selectedId = null;
      render();
      return;
    }
    const periodButton = target.closest<HTMLElement>("[data-action='balance-period']");
    if (periodButton?.dataset.period) {
      seriesPeriod = periodButton.dataset.period as Period;
      render();
      return;
    }
    if (target.closest("[data-action='account-edit']")) {
      editingAccountId = target.closest<HTMLElement>("[data-account-id]")?.dataset.accountId ?? null;
      render();
      return;
    }
    if (target.closest("[data-action='account-edit-cancel']")) {
      editingAccountId = null;
      render();
      return;
    }
    const saveButton = target.closest<HTMLElement>("[data-action='account-edit-save']");
    if (saveButton?.dataset.accountId) {
      void saveAccountEdits(saveButton.dataset.accountId);
    }
  });

  /** Reads the open edit panel, validates it client-side, and pushes the
   *  change server-ward. Reuses `load()` so the new balance/name arrive the
   *  same way the page first got its data. */
  const saveAccountEdits = async (id: string): Promise<void> => {
    const nameEl = root.querySelector<HTMLInputElement>("[data-edit-field='name']");
    const openingEl = root.querySelector<HTMLInputElement>("[data-edit-field='opening']");
    if (!nameEl || !openingEl) return;
    const name = nameEl.value.trim();
    if (!name) {
      accountsAtom.set({ ...accountsAtom.get(), status: "error", message: "Account name cannot be empty" });
      return;
    }
    const opening = Number(openingEl.value.trim());
    if (!Number.isFinite(opening)) {
      accountsAtom.set({ ...accountsAtom.get(), status: "error", message: "Opening balance must be a number" });
      return;
    }
    const typeEl = root.querySelector<HTMLSelectElement>("[data-edit-field='type']");
    const noteEl = root.querySelector<HTMLInputElement>("[data-edit-field='note']");
    accountsAtom.set({ ...accountsAtom.get(), status: "saving", message: "" });
    try {
      const res = await api.saveAccount({
        id,
        name,
        accountType: typeEl?.value ?? "",
        openingBalance: opening,
        note: noteEl?.value ?? "",
      });
      if (!res.success) {
        accountsAtom.set({ ...accountsAtom.get(), status: "error", message: res.error ?? "Could not save the account" });
        return;
      }
      editingAccountId = null;
      // `load()` re-renders with its own status; the confirmation must land
      // after it, and the atom subscription carries it to the banner.
      await load();
      accountsAtom.set({ ...accountsAtom.get(), status: "saved", message: "Account saved" });
    } catch (e) {
      accountsAtom.set({ ...accountsAtom.get(), status: "error", message: String(e) });
    }
  };

  void load();
  void loadTransactionsAndStatements();
  void loadBaseCurrency();
  render();
  return () => {
    if (chart) chart.destroy();
    unsub();
  };
}
