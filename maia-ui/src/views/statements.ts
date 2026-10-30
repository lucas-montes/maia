import {
  api,
  type StatementDetail,
  type StatementFeedStatus,
  type StatementImportLine,
  type StatementLine,
  type StatementSummary,
  type TransferMatch,
} from "../lib/api";
import { escapeHtml, formatEpoch, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import { statementsAtom } from "../lib/stores";
import { backToDashboard, panelClose, panelOpen, statusBanner } from "./shared";

const STATUS_LABELS: Record<string, string> = {
  staged: "staged",
  partial: "partially imported",
  imported: "imported",
  ignored: "ignored",
  error: "parse error",
};

function statusBadge(status: string): string {
  switch (status) {
    case "imported":
      return "badge-ok";
    case "partial":
      return "badge-warn";
    case "error":
      return "badge-err";
    case "ignored":
      return "badge-muted";
    default:
      return "badge-muted";
  }
}

function money(amount: number, currency: string): string {
  const value = Math.abs(amount).toFixed(2);
  return `${amount < 0 ? "−" : ""}${value} ${currency}`;
}

function lineStatusBadge(line: StatementLine): string {
  switch (line.status) {
    case "imported":
      return `<span class="badge badge-ok">imported</span>`;
    case "duplicate":
      return `<span class="badge badge-warn" title="Already staged from another statement">duplicate</span>`;
    case "skipped":
      return `<span class="badge badge-muted">skipped</span>`;
    case "informational":
      return `<span class="badge badge-muted">info</span>`;
    case "error":
      return `<span class="badge badge-err" title="${escapeHtml(line.importError ?? "")}">error</span>`;
    default:
      return "";
  }
}

function statementTitle(s: Pick<StatementSummary, "institution" | "accountName">): string {
  return `${s.institution} · ${s.accountName}`;
}

function listHtml(items: StatementSummary[], selectedId: string | null): string {
  if (items.length === 0) {
    return `<div class="p-4 text-center text-sm text-ink-500">No statements staged yet. Drop a file in a watched directory, upload from the phone, or sync Trading 212.</div>`;
  }
  const rows = items
    .map((s) => {
      const period = s.periodStart && s.periodEnd ? `${s.periodStart} → ${s.periodEnd}` : "";
      return `
        <button class="list-row w-full text-left ${selectedId === s.id ? "active" : ""}" type="button" data-statement-id="${escapeHtml(s.id)}">
          <div class="min-w-0 flex-1">
            <div class="flex items-center gap-2">
              <span class="truncate text-sm font-medium text-ink-100">${escapeHtml(statementTitle(s))}</span>
              <span class="badge ${statusBadge(s.status)}">${escapeHtml(STATUS_LABELS[s.status] ?? s.status)}</span>
            </div>
            <div class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-ink-500">
              ${s.accountNumber ? `<span class="font-mono">${escapeHtml(s.accountNumber)}</span>` : ""}
              ${period ? `<span>${escapeHtml(period)}</span>` : ""}
              <span>${s.pendingCount} pending</span>
              <span>${s.importedCount} imported</span>
              ${s.closingBalance !== null ? `<span class="font-mono text-accent-400">${escapeHtml(money(s.closingBalance, s.currencyCode))}</span>` : ""}
            </div>
            ${s.parseError ? `<div class="mt-1 truncate text-xs text-err-400">${escapeHtml(s.parseError)}</div>` : ""}
          </div>
        </button>`;
    })
    .join("");
  return `<div class="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto">${rows}</div>`;
}

function lineRow(line: StatementLine, checked: boolean): string {
  const selectable = line.status === "pending";
  const type = line.suggestedType ?? (line.amount < 0 ? "expense" : "income");
  return `
    <tr data-line-row="${escapeHtml(line.id)}" class="${checked ? "bg-surface-800/40" : ""}">
      <td class="w-8 align-top">
        ${selectable ? `<input type="checkbox" class="checkbox checkbox-sm" data-line-id="${escapeHtml(line.id)}" ${checked ? "checked" : ""} />` : ""}
      </td>
      <td class="whitespace-nowrap align-top text-xs text-ink-400">${escapeHtml(formatEpoch(line.postedAt))}</td>
      <td class="align-top">
        <div class="text-sm text-ink-200">${escapeHtml(line.description)}</div>
        ${line.importError ? `<div class="mt-1 text-xs text-err-400">${escapeHtml(line.importError)}</div>` : ""}
      </td>
      <td class="whitespace-nowrap text-right align-top font-mono text-sm ${line.amount < 0 ? "text-err-400" : "text-ok-400"}">
        ${escapeHtml(money(line.amount, line.currencyCode))}
      </td>
      <td class="w-32 align-top">
        ${selectable ? `<select class="select select-xs select-bordered w-full" data-type-for="${escapeHtml(line.id)}">
            <option value="expense" ${type === "expense" ? "selected" : ""}>expense</option>
            <option value="income" ${type === "income" ? "selected" : ""}>income</option>
            <option value="transfer" ${type === "transfer" ? "selected" : ""}>transfer</option>
          </select>` : ""}
      </td>
      <td class="w-36 align-top">
        ${selectable ? `<input class="input input-xs input-bordered w-full" placeholder="category" value="${escapeHtml(line.suggestedCategory ?? "")}" data-category-for="${escapeHtml(line.id)}" />` : ""}
      </td>
      <td class="whitespace-nowrap align-top">${lineStatusBadge(line)}</td>
    </tr>`;
}

function transferGroupsHtml(
  detail: StatementDetail,
  selected: Set<string>,
): string {
  const localIds = new Set(detail.lines.map((line) => line.id));
  const matches = detail.matches.filter((m) => m.status !== "dismissed");
  if (matches.length === 0) return "";
  const cards = matches
    .map((match: TransferMatch) => {
      const debit = match.debit;
      const credit = match.credit;
      // One side may belong to another statement (a CA file stages one
      // statement per account). Only a line in this statement can be selected
      // here; the server expands the pair when either side is imported.
      const localId = localIds.has(debit.id)
        ? debit.id
        : localIds.has(credit.id)
          ? credit.id
          : null;
      const checked = localId !== null && selected.has(localId);
      const selectable =
        localId !== null && debit.status === "pending" && credit.status === "pending";
      const confidence = Math.round(match.confidence * 100);
      const badge =
        match.status === "auto"
          ? `<span class="badge badge-ok">auto · ${confidence}%</span>`
          : match.status === "linked"
            ? `<span class="badge badge-ok">linked</span>`
            : `<span class="badge badge-warn">suggested · ${confidence}%</span>`;
      return `
        <div class="rounded border border-surface-800 bg-base-100 p-3" data-transfer-match="${escapeHtml(match.id)}">
          <div class="mb-2 flex items-center justify-between gap-2">
            <div class="flex items-center gap-2">
              ${selectable ? `<input type="checkbox" class="checkbox checkbox-sm" data-line-id="${escapeHtml(localId)}" ${checked ? "checked" : ""} />` : ""}
              <span class="text-sm font-medium text-ink-200">Transfer</span>
              ${badge}
            </div>
            ${match.reason ? `<span class="text-xs text-ink-500">${escapeHtml(match.reason)}</span>` : ""}
          </div>
          <div class="grid grid-cols-2 gap-3 text-xs">
            <div>
              <div class="text-ink-500">Out${localIds.has(debit.id) ? "" : " · other statement"}</div>
              <div class="text-ink-200">${escapeHtml(debit.description)}</div>
              <div class="mt-1 font-mono text-err-400">${escapeHtml(money(debit.amount, debit.currencyCode))}</div>
            </div>
            <div>
              <div class="text-ink-500">In${localIds.has(credit.id) ? "" : " · other statement"}</div>
              <div class="text-ink-200">${escapeHtml(credit.description)}</div>
              <div class="mt-1 font-mono text-ok-400">${escapeHtml(money(credit.amount, credit.currencyCode))}</div>
            </div>
          </div>
          ${match.status === "auto" ? `<p class="mt-2 text-xs text-ink-500">Both sides import as one transfer transaction.</p>` : `<p class="mt-2 text-xs text-ink-500">Suggested only: importing one side keeps the other as a separate line.</p>`}
        </div>`;
    })
    .join("");
  return `<div class="space-y-2">
      <h3 class="text-sm font-medium text-ink-200">Transfers</h3>
      ${cards}
    </div>`;
}

function detailHtml(detail: StatementDetail, selected: Set<string>): string {
  const s = detail.statement;
  const matched = new Set<string>();
  for (const match of detail.matches) {
    if (match.status !== "dismissed") {
      matched.add(match.debitLineId);
      matched.add(match.creditLineId);
    }
  }
  const trades = detail.lines.filter((line) => line.lineKind === "trade");
  const ordinary = detail.lines.filter(
    (line) => line.lineKind !== "trade" && !matched.has(line.id),
  );
  const pending = detail.lines.filter((line) => line.status === "pending").length;

  const tradeRows = trades
    .map(
      (line) => `
        <tr>
          <td class="whitespace-nowrap text-xs text-ink-400">${escapeHtml(formatEpoch(line.postedAt))}</td>
          <td class="text-sm text-ink-300">${escapeHtml(line.description)}</td>
          <td class="text-right text-xs text-ink-500">informational</td>
        </tr>`,
    )
    .join("");

  return `
    <div class="min-h-0 flex-1 space-y-4 overflow-auto">
      <div class="rounded border border-surface-800 bg-base-100 p-4">
        <div class="mb-3 flex flex-wrap items-center gap-2">
          <h3 class="text-sm font-medium text-ink-200">${escapeHtml(statementTitle(s))}</h3>
          <span class="badge ${statusBadge(s.status)}">${escapeHtml(STATUS_LABELS[s.status] ?? s.status)}</span>
          ${s.accountNumber ? `<span class="font-mono text-xs text-ink-500">${escapeHtml(s.accountNumber)}</span>` : ""}
        </div>
        <div class="grid grid-cols-2 gap-3 text-sm">
          <div><span class="text-ink-500">Source:</span> <span class="text-ink-300">${escapeHtml(s.source)}</span></div>
          <div><span class="text-ink-500">Currency:</span> <span class="text-ink-300">${escapeHtml(s.currencyCode)}</span></div>
          <div><span class="text-ink-500">Period:</span> <span class="text-ink-300">${escapeHtml(s.periodStart ?? "?")} → ${escapeHtml(s.periodEnd ?? "?")}</span></div>
          <div><span class="text-ink-500">Budget account:</span> <span class="text-ink-300">${escapeHtml(s.resolvedAccountName ?? "created on first import")}</span></div>
          <div><span class="text-ink-500">Opening:</span> <span class="font-mono text-ink-300">${s.openingBalance === null ? "?" : escapeHtml(money(s.openingBalance, s.currencyCode))}</span></div>
          <div><span class="text-ink-500">Closing:</span> <span class="font-mono text-ink-300">${s.closingBalance === null ? "?" : escapeHtml(money(s.closingBalance, s.currencyCode))}</span></div>
          <div><span class="text-ink-500">Lines:</span> <span class="text-ink-300">${detail.lines.length} (${pending} pending)</span></div>
          <div><span class="text-ink-500">Updated:</span> <span class="text-ink-300">${escapeHtml(formatEpoch(s.updatedAt))}</span></div>
        </div>
        ${s.parseError ? `<div class="mt-3 rounded border border-surface-800 bg-base-100 p-2 text-xs text-err-400">Parse failed: ${escapeHtml(s.parseError)}</div>` : ""}
        ${s.warnings.length > 0 ? `<div class="mt-3 rounded border border-surface-800 bg-base-100 p-2 text-xs text-warn-400">${s.warnings.map((w) => escapeHtml(w)).join("<br />")}</div>` : ""}
      </div>

      ${transferGroupsHtml(detail, selected)}

      ${trades.length > 0 ? `<div class="rounded border border-surface-800 bg-base-100 p-4">
        <h3 class="mb-2 text-sm font-medium text-ink-200">Trades <span class="font-normal text-ink-500">(informational, never imported)</span></h3>
        <table class="table table-xs w-full">
          <thead><tr><th>Date</th><th>Description</th><th class="text-right">Status</th></tr></thead>
          <tbody>${tradeRows}</tbody>
        </table>
      </div>` : ""}

      <div class="rounded border border-surface-800 bg-base-100 p-4">
        <div class="mb-2 flex items-center justify-between">
          <h3 class="text-sm font-medium text-ink-200">Lines</h3>
          <div class="flex items-center gap-2">
            <button class="btn btn-ghost btn-xs" type="button" data-action="select-all">Select pending</button>
            <button class="btn btn-ghost btn-xs" type="button" data-action="select-none">Clear</button>
          </div>
        </div>
        <table class="table table-sm w-full">
          <thead>
            <tr>
              <th></th><th>Date</th><th>Description</th><th class="text-right">Amount</th>
              <th>Type</th><th>Category</th><th>Status</th>
            </tr>
          </thead>
          <tbody>
            ${ordinary.map((line) => lineRow(line, selected.has(line.id))).join("") || `<tr><td colspan="7" class="text-center text-sm text-ink-500">No importable lines.</td></tr>`}
          </tbody>
        </table>
      </div>

      <div class="flex flex-wrap items-center gap-3">
        <button class="btn btn-ghost btn-sm" type="button" data-action="statements-back">← Back to list</button>
        <button class="btn btn-primary btn-sm" type="button" data-action="import-selected" ${selected.size === 0 ? "disabled" : ""}>Import selected (${selected.size})</button>
        <button class="btn btn-secondary btn-sm" type="button" data-action="import-all" ${pending === 0 ? "disabled" : ""}>Import all pending</button>
        <button class="btn btn-ghost btn-sm" type="button" data-action="skip-selected" ${selected.size === 0 ? "disabled" : ""}>Skip selected</button>
        <button class="btn btn-ghost btn-sm" type="button" data-action="ignore-statement" ${s.status === "imported" ? "disabled" : ""}>Ignore statement</button>
        <button class="btn btn-ghost btn-sm text-err-400" type="button" data-action="delete-statement">Delete</button>
      </div>
    </div>`;
}

function feedCard(feeds: StatementFeedStatus[]): string {
  if (feeds.length === 0) return "";
  const t212 = feeds.find((feed) => feed.provider === "trading212");
  if (!t212) return "";
  const state = !t212.enabled
    ? `<span class="badge badge-muted">disabled</span>`
    : !t212.credentialsSet
      ? `<span class="badge badge-warn">credentials missing</span>`
      : t212.status === "error"
        ? `<span class="badge badge-err">error</span>`
        : `<span class="badge badge-ok">${escapeHtml(t212.status)}</span>`;
  return `
    <div class="rounded border border-surface-800 bg-base-100 p-3">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="flex items-center gap-2">
          <span class="text-sm font-medium text-ink-200">Trading 212</span>
          ${state}
          <span class="text-xs text-ink-500">${escapeHtml(t212.environment)} · ${t212.lookbackDays}d lookback</span>
        </div>
        <button class="btn btn-secondary btn-xs" type="button" data-action="sync-feed" ${!t212.enabled || !t212.credentialsSet ? "disabled" : ""}>Sync now</button>
      </div>
      ${t212.lastSyncedAt ? `<div class="mt-1 text-xs text-ink-500">Last sync ${escapeHtml(formatEpoch(t212.lastSyncedAt))}</div>` : ""}
      ${t212.lastError ? `<div class="mt-1 text-xs text-err-400">${escapeHtml(t212.lastError)}</div>` : ""}
    </div>`;
}

/** Statements — staged bank/broker files, review, import, feeds. */
export function mount(root: HTMLElement): () => void {
  root.innerHTML = `<div data-slot="content"></div>`;
  const content = root.querySelector("[data-slot='content']") as HTMLElement;
  let feeds: StatementFeedStatus[] = [];

  const selectedSet = (): Set<string> => new Set(statementsAtom.get().selectedLines);

  const render = (): void => {
    const state = statementsAtom.get();
    if (state.selectedId && state.detail) {
      setHtml(
        content,
        `${panelOpen("Statement Review", state.detail.statement.institution)}
          ${statusBanner(state)}
          ${detailHtml(state.detail as StatementDetail, selectedSet())}
        ${panelClose}`,
      );
    } else {
      setHtml(
        content,
        `${panelOpen("Statements", "Review staged bank and broker statements before importing")}
          ${statusBanner(state)}
          ${feedCard(feeds)}
          <div class="flex flex-wrap items-center gap-2">
            <select class="select select-sm select-bordered" data-filter="status">
              <option value="">All statuses</option>
              <option value="staged" ${state.filter === "staged" ? "selected" : ""}>Staged</option>
              <option value="partial" ${state.filter === "partial" ? "selected" : ""}>Partially imported</option>
              <option value="imported" ${state.filter === "imported" ? "selected" : ""}>Imported</option>
              <option value="ignored" ${state.filter === "ignored" ? "selected" : ""}>Ignored</option>
              <option value="error" ${state.filter === "error" ? "selected" : ""}>Parse errors</option>
            </select>
            <select class="select select-sm select-bordered" data-filter="source">
              <option value="">All sources</option>
              <option value="credit_agricole" ${state.source === "credit_agricole" ? "selected" : ""}>Crédit Agricole</option>
              <option value="trading212" ${state.source === "trading212" ? "selected" : ""}>Trading 212</option>
              <option value="firstrade" ${state.source === "firstrade" ? "selected" : ""}>Firstrade</option>
            </select>
            <button class="btn btn-ghost btn-sm" type="button" data-action="scan">Scan watch folders</button>
            <button class="btn btn-ghost btn-sm" type="button" data-action="refresh">Refresh</button>
          </div>
          <div class="text-sm text-ink-400">${state.items.length} statement${state.items.length !== 1 ? "s" : ""}</div>
          ${listHtml(state.items as StatementSummary[], state.selectedId)}
          <div>${backToDashboard()}</div>
        ${panelClose}`,
      );
    }
    refreshIcons(content);
  };

  const loadList = async (): Promise<void> => {
    const state = statementsAtom.get();
    statementsAtom.set({ ...state, status: "loading", message: "" });
    try {
      const res = await api.listStatements({
        status: state.filter || undefined,
        source: state.source || undefined,
      });
      if (!res.success) throw new Error(res.error ?? "Could not list statements");
      statementsAtom.set({
        ...statementsAtom.get(),
        items: res.data?.items ?? [],
        status: "idle",
        message: "",
      });
    } catch (e) {
      statementsAtom.set({
        ...statementsAtom.get(),
        status: "error",
        message: `Failed to load statements: ${e}`,
      });
    }
    render();
  };

  const loadFeeds = async (): Promise<void> => {
    try {
      const res = await api.listStatementFeeds();
      if (res.success && res.data) {
        feeds = res.data.items ?? [];
        render();
      }
    } catch {
      // The feed card is supplementary; a failure here must not hide the list.
    }
  };

  const loadDetail = async (id: string): Promise<void> => {
    statementsAtom.set({
      ...statementsAtom.get(),
      selectedId: id,
      detail: null,
      status: "loading",
      message: "",
    });
    render();
    try {
      const res = await api.getStatement(id);
      if (!res.success || !res.data) throw new Error(res.error ?? "Could not load statement");
      const detail = res.data;
      const pending = detail.lines
        .filter((line: StatementLine) => line.status === "pending")
        .map((line: StatementLine) => line.id);
      statementsAtom.set({
        ...statementsAtom.get(),
        detail,
        selectedLines: pending,
        status: "idle",
        message: "",
      });
    } catch (e) {
      statementsAtom.set({
        ...statementsAtom.get(),
        status: "error",
        message: `Failed to load statement: ${e}`,
      });
    }
    render();
  };

  /** Reads the per-line type/category controls, so an edit at import time wins
   *  over the parser's suggestion. */
  const importLines = (action: "import" | "skip"): StatementImportLine[] => {
    const selected = selectedSet();
    return [...selected].map((id) => {
      const type = content.querySelector<HTMLSelectElement>(`[data-type-for="${CSS.escape(id)}"]`)?.value;
      const category = content.querySelector<HTMLInputElement>(`[data-category-for="${CSS.escape(id)}"]`)?.value.trim();
      return {
        id,
        action,
        ...(action === "import" && type ? { type: type as StatementImportLine["type"] } : {}),
        ...(action === "import" && category ? { category } : {}),
      };
    });
  };

  const runImport = async (lines: StatementImportLine[]): Promise<void> => {
    const state = statementsAtom.get();
    if (!state.selectedId) return;
    statementsAtom.set({ ...state, status: "saving", message: "" });
    render();
    try {
      const res = await api.importStatement({
        id: state.selectedId,
        lines,
        autoLink: true,
      });
      if (!res.success) throw new Error(res.error ?? "Import failed");
      statementsAtom.set({
        ...statementsAtom.get(),
        status: "saved",
        message: "Statement imported into draft transactions.",
      });
      await loadDetail(state.selectedId);
      statementsAtom.set({ ...statementsAtom.get(), status: "saved", message: "Statement imported into draft transactions." });
      render();
    } catch (e) {
      statementsAtom.set({
        ...statementsAtom.get(),
        status: "error",
        message: `Import failed: ${e}`,
      });
      render();
    }
  };

  const scan = async (): Promise<void> => {
    statementsAtom.set({ ...statementsAtom.get(), status: "saving", message: "" });
    render();
    try {
      const res = await api.scanStatements();
      if (!res.success) throw new Error(res.error ?? "Scan failed");
      statementsAtom.set({
        ...statementsAtom.get(),
        status: "saved",
        message: "Watch folders scanned.",
      });
      await loadList();
      await loadFeeds();
    } catch (e) {
      statementsAtom.set({
        ...statementsAtom.get(),
        status: "error",
        message: `Scan failed: ${e}`,
      });
      render();
    }
  };

  const syncFeed = async (): Promise<void> => {
    statementsAtom.set({ ...statementsAtom.get(), status: "saving", message: "" });
    render();
    try {
      const res = await api.syncStatementFeed("trading212");
      if (!res.success) throw new Error(res.error ?? "Sync failed");
      statementsAtom.set({
        ...statementsAtom.get(),
        status: "saved",
        message: "Trading 212 synced.",
      });
      await loadList();
      await loadFeeds();
    } catch (e) {
      statementsAtom.set({
        ...statementsAtom.get(),
        status: "error",
        message: `Trading 212 sync failed: ${e}`,
      });
      render();
    }
  };

  const mutateStatement = async (kind: "ignore" | "delete"): Promise<void> => {
    const state = statementsAtom.get();
    if (!state.selectedId) return;
    if (kind === "delete" && !window.confirm("Delete this staged statement? Imported transactions are kept.")) {
      return;
    }
    statementsAtom.set({ ...state, status: "saving", message: "" });
    render();
    try {
      const res =
        kind === "ignore"
          ? await api.ignoreStatement(state.selectedId)
          : await api.deleteStatement(state.selectedId);
      if (!res.success) throw new Error(res.error ?? "Request failed");
      statementsAtom.set({
        ...statementsAtom.get(),
        selectedId: null,
        detail: null,
        status: "saved",
        message: kind === "ignore" ? "Statement ignored." : "Statement deleted.",
      });
      await loadList();
    } catch (e) {
      statementsAtom.set({
        ...statementsAtom.get(),
        status: "error",
        message: `${kind === "ignore" ? "Ignore" : "Delete"} failed: ${e}`,
      });
      render();
    }
  };

  root.addEventListener("change", (event) => {
    const target = event.target as HTMLElement;
    if (target.matches("[data-filter='status']")) {
      statementsAtom.set({
        ...statementsAtom.get(),
        filter: (target as HTMLSelectElement).value,
      });
      void loadList();
      return;
    }
    if (target.matches("[data-filter='source']")) {
      statementsAtom.set({
        ...statementsAtom.get(),
        source: (target as HTMLSelectElement).value,
      });
      void loadList();
      return;
    }
    if (target.matches("[data-line-id]")) {
      const checkbox = target as HTMLInputElement;
      const id = checkbox.dataset.lineId as string;
      const state = statementsAtom.get();
      const next = new Set(state.selectedLines);
      if (checkbox.checked) next.add(id);
      else next.delete(id);

      // A matched pair is one transfer. When both sides are staged in this
      // statement, selecting one selects the other so the review and the
      // server agree. A counterpart in another statement is left out: the
      // server expands the pair itself, and sending a foreign line id here
      // would be rejected as "not in this statement".
      const detail = state.detail as StatementDetail | null;
      const match = detail?.matches.find(
        (m) => m.status !== "dismissed" && (m.debitLineId === id || m.creditLineId === id),
      );
      if (match) {
        const other = match.debitLineId === id ? match.creditLineId : match.debitLineId;
        if (detail?.lines.some((line) => line.id === other)) {
          if (checkbox.checked) next.add(other);
          else next.delete(other);
        }
      }

      statementsAtom.set({ ...state, selectedLines: [...next] });
      render();
    }
  });

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    const statementButton = target.closest<HTMLElement>("[data-statement-id]");
    if (statementButton?.dataset.statementId) {
      void loadDetail(statementButton.dataset.statementId);
      return;
    }
    const action = target.closest<HTMLElement>("[data-action]")?.dataset.action;
    switch (action) {
      case "statements-back": {
        statementsAtom.set({
          ...statementsAtom.get(),
          selectedId: null,
          detail: null,
          selectedLines: [],
          status: "idle",
          message: "",
        });
        render();
        void loadList();
        return;
      }
      case "refresh":
        void loadList();
        return;
      case "scan":
        void scan();
        return;
      case "sync-feed":
        void syncFeed();
        return;
      case "select-all": {
        const detail = statementsAtom.get().detail as StatementDetail | null;
        if (!detail) return;
        statementsAtom.set({
          ...statementsAtom.get(),
          selectedLines: detail.lines
            .filter((line) => line.status === "pending")
            .map((line) => line.id),
        });
        render();
        return;
      }
      case "select-none":
        statementsAtom.set({ ...statementsAtom.get(), selectedLines: [] });
        render();
        return;
      case "import-selected":
        void runImport(importLines("import"));
        return;
      case "import-all":
        void runImport([]);
        return;
      case "skip-selected":
        void runImport(importLines("skip"));
        return;
      case "ignore-statement":
        void mutateStatement("ignore");
        return;
      case "delete-statement":
        void mutateStatement("delete");
        return;
      default:
        return;
    }
  });

  render();
  void loadList();
  void loadFeeds();
  const unsubscribe = statementsAtom.subscribe(render);
  return () => unsubscribe();
}
