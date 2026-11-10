import {
  api,
  isEnvelopeFailure,
  type StatementDetail,
  type StatementFeedStatus,
  type StatementImportLine,
  type StatementLine,
  type StatementSummary,
  type TransferMatch,
} from "../lib/api";
import { escapeHtml, formatEpoch, setHtml } from "../lib/dom";
import { refreshIcons } from "../lib/icons";
import {
  liveMatches,
  localMatchSide,
  matchedLineIds,
  selectableLineIds,
} from "../lib/statementReview";
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

/** The final path segment of a dropped file's path, for display. */
function basename(path: string): string {
  return path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path;
}

const AWAITING_STATUSES = new Set(["staged", "partial", "error"]);

/** Groups statements by the file they were staged from.
 *
 *  A CA export contains one account section per account, which the parser
 *  stages as one statement each; grouping them by `source_path` is what makes
 *  "the file" visible in the UI. Statements without a source path (feed
 *  syncs) stand alone under a pseudo-group. */
function groupStatements(items: StatementSummary[]): { key: string; label: string; statements: StatementSummary[] }[] {
  const groups = new Map<string, StatementSummary[]>();
  for (const s of items) {
    const key = s.sourcePath ?? `~${s.id}`;
    const list = groups.get(key) ?? [];
    list.push(s);
    groups.set(key, list);
  }
  return [...groups.entries()].map(([key, statements]) => ({
    key,
    label: key.startsWith("~") ? statementTitle(statements[0]) : basename(key),
    statements,
  }));
}

function groupPending(statements: StatementSummary[]): number {
  return statements.reduce((sum, s) => sum + s.pendingCount, 0);
}

function bulkBar(label: string, actions: { import?: boolean; skip?: boolean; "delete"?: boolean }): string {
  return `
    <div class="sticky bottom-0 z-10 flex flex-wrap items-center gap-2 rounded border border-surface-700 bg-base-200/95 p-2 backdrop-blur">
      <span class="text-sm text-ink-300">${escapeHtml(label)}</span>
      <span class="flex-1"></span>
      ${actions.import ? `<button class="btn btn-primary btn-xs" type="button" data-action="bulk-import">Import selected</button>` : ""}
      ${actions.skip ? `<button class="btn btn-ghost btn-xs" type="button" data-action="bulk-skip">Skip selected</button>` : ""}
      ${actions.delete ? `<button class="btn btn-ghost btn-xs text-err-400" type="button" data-action="bulk-delete">Delete…</button>` : ""}
      <button class="btn btn-ghost btn-xs" type="button" data-action="bulk-clear">Clear</button>
    </div>`;
}

function listHtml(
  items: StatementSummary[],
  selectedId: string | null,
  selectedStatements: string[],
): string {
  if (items.length === 0) {
    return `<div class="p-4 text-center text-sm text-ink-500">No statements in this view. Use Add statement above to drop CSV exports in, upload from the phone, or sync Trading 212.</div>`;
  }
  const picked = new Set(selectedStatements);
  const groups = groupStatements(items).map(({ key, label, statements }) => {
    const allPicked = statements.every((s) => picked.has(s.id));
    const somePicked = statements.some((s) => picked.has(s.id));
    const rows = statements
      .map((s) => {
        const period = s.periodStart && s.periodEnd ? `${s.periodStart} → ${s.periodEnd}` : "";
        // The checkbox sits outside the row button (an input cannot nest in
        // one), so it is offset by hand: mt-3 matches .list-row's own p-3,
        // which is what puts the row's first line where it is.
        return `
        <div class="flex items-start gap-2">
          <input type="checkbox" class="checkbox checkbox-sm mt-3" data-statement-check="${escapeHtml(s.id)}" ${picked.has(s.id) ? "checked" : ""} title="Select for bulk actions" />
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
              </div>
            </div>
          </button>
        </div>`;
      })
      .join("");
    return `
      <div class="rounded-lg border border-base-300 bg-base-100">
        <div class="flex items-center gap-2 px-3 py-2">
          <input type="checkbox" class="checkbox checkbox-sm" data-file-check="${escapeHtml(key)}" ${allPicked ? "checked" : ""} ${somePicked && !allPicked ? "data-indeterminate" : ""} title="Select all accounts of this file" />
          <button class="min-w-0 flex-1 text-left" type="button" data-action="open-file" data-file-path="${escapeHtml(key.startsWith("~") ? "" : key)}">
            <span class="text-sm font-medium text-ink-200 underline decoration-base-300 underline-offset-2">${escapeHtml(label)}</span>
            <span class="ml-2 text-xs text-ink-500">${statements.length} account${statements.length !== 1 ? "s" : ""} · ${groupPending(statements)} entr${groupPending(statements) !== 1 ? "ies" : "y"} pending</span>
          </button>
          ${key.startsWith("~") ? "" : `<button class="btn btn-ghost btn-xs" type="button" data-action="open-file" data-file-path="${escapeHtml(key)}">Review file</button>`}
        </div>
        <div class="space-y-0.5 border-t border-base-300 p-1">${rows}</div>
      </div>`;
  });
  return `
    ${picked.size > 0 ? bulkBar(`${picked.size} statement${picked.size !== 1 ? "s" : ""} selected`, { import: true, delete: true }) : ""}
    <div class="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto">${groups.join("")}</div>`;
}

/** One entry row. [selectable] comes from `selectableLineIds` rather than from
 *  the line's own status, so the checkbox a row shows and the set select-all
 *  picks can never disagree. */
function lineRow(line: StatementLine, checked: boolean, selectable: boolean): string {
  const type = line.suggestedType ?? (line.amount < 0 ? "expense" : "income");
  return `
    <tr data-line-row="${escapeHtml(line.id)}" class="${checked ? "bg-surface-800/40" : ""}">
      <td class="w-8 align-top">
        ${selectable ? `<input type="checkbox" class="checkbox checkbox-sm align-top" data-line-id="${escapeHtml(line.id)}" ${checked ? "checked" : ""} />` : ""}
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

/** Transfers as one row each: money moving between two of your own accounts.
 *
 *  A match may have one side in another statement of the same file (a CA file
 *  stages one statement per account); the row names both accounts, and only a
 *  line present in [localIds] can be selected here. When neither side is yet
 *  tied to a real account, a per-row counterpart picker chooses where the
 *  money goes — or nothing at all, which imports the line as a plain
 *  expense/income instead of an invented account. */
function transferRowsHtml(
  matches: TransferMatch[],
  localIds: Set<string>,
  selected: Set<string>,
  accounts: { id: string; name: string; type: string }[],
  selectableIds: Set<string>,
): string {
  const live = matches.filter((m) => m.status !== "dismissed");
  if (live.length === 0) return "";
  const rows = live
    .map((match: TransferMatch) => {
      const debit = match.debit;
      const credit = match.credit;
      const localId = localMatchSide(match, localIds);
      const checked = localId !== null && selected.has(localId);
      const selectable = localId !== null && selectableIds.has(localId);
      const confidence = Math.round(match.confidence * 100);
      const badge =
        match.status === "auto"
          ? `<span class="badge badge-ok">auto · ${confidence}%</span>`
          : match.status === "linked"
            ? `<span class="badge badge-ok">linked</span>`
            : `<span class="badge badge-warn">suggested · ${confidence}%</span>`;
      const outAccount = match.debitAccountName ? escapeHtml(match.debitAccountName) : "Out";
      const inAccount = match.creditAccountName ? escapeHtml(match.creditAccountName) : "In";
      return `
        <tr data-transfer-match="${escapeHtml(match.id)}">
          <td class="w-8 align-top">
            ${selectable ? `<input type="checkbox" class="checkbox checkbox-sm align-top" data-line-id="${escapeHtml(localId)}" ${checked ? "checked" : ""} />` : ""}
          </td>
          <td class="whitespace-nowrap align-top text-xs text-ink-400">${escapeHtml(formatEpoch(debit.postedAt))}</td>
          <td class="align-top">
            <div class="text-xs text-ink-500">${outAccount} <span class="text-ink-400">→</span> ${inAccount}</div>
            <div class="text-sm text-ink-200">${escapeHtml(debit.description)}</div>
            ${match.reason ? `<div class="mt-1 text-xs text-ink-500">${escapeHtml(match.reason)}</div>` : ""}
          </td>
          <td class="whitespace-nowrap text-right align-top font-mono text-sm">
            <span class="text-err-400">${escapeHtml(money(debit.amount, debit.currencyCode))}</span>
            <span class="text-ink-500"> → </span>
            <span class="text-ok-400">${escapeHtml(money(credit.amount, credit.currencyCode))}</span>
          </td>
          <td class="align-top">${badge}</td>
          <td class="align-top">
            ${selectable
              ? `<select class="select select-xs select-bordered w-40" data-counterpart-for="${escapeHtml(localId)}" title="Pick where the money moves — or leave empty to book as expense/income">
                  <option value="">— import as expense/income —</option>
                  ${accounts
                    .map(
                      (a) =>
                        `<option value="${escapeHtml(a.id)}">${escapeHtml(a.name)} (${escapeHtml(a.type)})</option>`,
                    )
                    .join("")}
                </select>`
              : `<span class="text-xs text-ink-500">${match.status === "auto" ? "both sides import as one transfer" : "suggested: importing one side leaves the other separate"}</span>`}
          </td>
        </tr>`;
    })
    .join("");
  return `
    <div class="rounded border border-surface-800 bg-base-100 p-4">
      <h3 class="text-sm font-medium text-ink-200">Transfers <span class="font-normal text-ink-500">(money between your own accounts)</span></h3>
      <p class="mt-1 mb-2 text-xs text-ink-500">An automatic pair imports as one transfer; pick the other account when the review does not know it, or leave the choice empty to book the entry as a plain expense or income.</p>
      <table class="table table-sm w-full">
        <thead><tr><th></th><th>Date</th><th>Movement</th><th class="text-right">Amount</th><th>Match</th><th>Counterpart</th></tr></thead>
        <tbody>${rows}</tbody>
      </table>
    </div>`;
}

function detailHtml(
  detail: StatementDetail,
  selected: Set<string>,
  accounts: { id: string; name: string; type: string }[],
): string {
  const s = detail.statement;
  const matched = matchedLineIds(liveMatches([detail]));
  const trades = detail.lines.filter((line) => line.lineKind === "trade");
  const ordinary = detail.lines.filter(
    (line) => line.lineKind !== "trade" && !matched.has(line.id),
  );
  const selectable = selectableLineIds([detail]);
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
          <div><span class="text-ink-500">Account:</span> <span class="text-ink-300">${escapeHtml(s.resolvedAccountName ?? `will be created on import (${s.accountName})`)}</span></div>
          <div><span class="text-ink-500">Opening:</span> <span class="font-mono text-ink-300">${s.openingBalance === null ? "?" : escapeHtml(money(s.openingBalance, s.currencyCode))}</span></div>
          <div><span class="text-ink-500">Closing:</span> <span class="font-mono text-ink-300">${s.closingBalance === null ? "?" : escapeHtml(money(s.closingBalance, s.currencyCode))}</span></div>
          <div><span class="text-ink-500">Entries:</span> <span class="text-ink-300">${detail.lines.length} (${pending} pending)</span></div>
          <div><span class="text-ink-500">Updated:</span> <span class="text-ink-300">${escapeHtml(formatEpoch(s.updatedAt))}</span></div>
        </div>
        ${s.parseError ? `<div class="mt-3 rounded border border-surface-800 bg-base-100 p-2 text-xs text-err-400">Parse failed: ${escapeHtml(s.parseError)}</div>` : ""}
        ${s.warnings.length > 0 ? `<div class="mt-3 rounded border border-surface-800 bg-base-100 p-2 text-xs text-warn-400">${s.warnings.map((w) => escapeHtml(w)).join("<br />")}</div>` : ""}
      </div>

      ${transferRowsHtml(detail.matches, new Set(detail.lines.map((line) => line.id)), selected, accounts, selectable)}

      ${trades.length > 0 ? `<div class="rounded border border-surface-800 bg-base-100 p-4">
        <h3 class="mb-2 text-sm font-medium text-ink-200">Trades <span class="font-normal text-ink-500">(informational, never imported)</span></h3>
        <table class="table table-xs w-full">
          <thead><tr><th>Date</th><th>Description</th><th class="text-right">Status</th></tr></thead>
          <tbody>${tradeRows}</tbody>
        </table>
      </div>` : ""}

      <div class="rounded border border-surface-800 bg-base-100 p-4">
        <div class="mb-2 flex items-center justify-between">
          <h3 class="text-sm font-medium text-ink-200">Entries to add</h3>
          <div class="flex items-center gap-2">
            <button class="btn btn-ghost btn-xs" type="button" data-action="select-all" ${selectable.size === 0 ? "disabled" : ""}>Select pending</button>
            <button class="btn btn-ghost btn-xs" type="button" data-action="select-none" ${selected.size === 0 ? "disabled" : ""}>Clear</button>
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
            ${ordinary.map((line) => lineRow(line, selected.has(line.id), selectable.has(line.id))).join("") || `<tr><td colspan="7" class="text-center text-sm text-ink-500">No importable lines.</td></tr>`}
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

/** The file review page: every account and entry one dropped file produced.
 *
 *  This is the screen that answers "what will this import create?" -- the
 *  accounts first, then the entries grouped by account, then transfers and
 *  informational trades. Selection is shared across all accounts, so one
 *  import call can cover the whole file. */
function fileHtml(
  statements: StatementDetail[],
  selected: Set<string>,
  accounts: { id: string; name: string; type: string }[],
): string {
  const localIds = new Set<string>();
  const trades: StatementLine[] = [];
  for (const detail of statements) {
    for (const line of detail.lines) localIds.add(line.id);
    trades.push(...detail.lines.filter((line) => line.lineKind === "trade"));
  }
  const matches: TransferMatch[] = liveMatches(statements);
  const selectable = selectableLineIds(statements);

  const pending = statements.reduce(
    (sum, detail) => sum + detail.lines.filter((line) => line.status === "pending").length,
    0,
  );

  // Every cell of an account row is top-aligned: the account cell is two lines
  // tall whenever a number is known, and a cell left on the table's default
  // `vertical-align: middle` centres its checkbox against both of them -- which
  // is what left the checkbox column sitting a line below its account name.
  const accountRows = statements
    .map((detail) => {
      const s = detail.statement;
      const accountSelectable = detail.lines.filter((line) => selectable.has(line.id));
      const accountPending = detail.lines.filter((line) => line.status === "pending");
      const allPicked =
        accountSelectable.length > 0 && accountSelectable.every((line) => selected.has(line.id));
      const somePicked = accountSelectable.some((line) => selected.has(line.id));
      return `
        <tr>
          <td class="w-8 align-top">
            <input type="checkbox" class="checkbox checkbox-sm align-top" data-account-check="${escapeHtml(s.id)}"
              ${allPicked ? "checked" : ""} ${somePicked && !allPicked ? "data-indeterminate" : ""}
              title="Select every pending entry of this account" ${accountSelectable.length === 0 ? "disabled" : ""} />
          </td>
          <td class="align-top">
            <button class="text-left text-sm font-medium text-ink-200 hover:underline" type="button" data-statement-id="${escapeHtml(s.id)}">${escapeHtml(statementTitle(s))}</button>
            ${s.accountNumber ? `<div class="font-mono text-xs text-ink-500">${escapeHtml(s.accountNumber)}</div>` : ""}
          </td>
          <td class="align-top text-xs text-ink-400">${escapeHtml(s.currencyCode)}</td>
          <td class="align-top text-xs">
            ${s.resolvedAccountName
              ? `<span class="text-ink-400">uses existing account</span> <span class="text-ink-200">${escapeHtml(s.resolvedAccountName)}</span>`
              : `<span class="badge badge-warn">will be created on import</span> <span class="text-ink-200">${escapeHtml(s.accountName)}</span>`}
          </td>
          <td class="text-right align-top text-xs text-ink-400">${accountPending.length}</td>
          <td class="text-right align-top">
            <button class="btn btn-ghost btn-xs text-err-400" type="button" data-action="delete-account" data-statement-id="${escapeHtml(s.id)}">Delete</button>
          </td>
        </tr>`;
    })
    .join("");

  const matched = matchedLineIds(matches);
  const entrySections = statements
    .map((detail) => {
      const s = detail.statement;
      const ordinary = detail.lines.filter(
        (line) => line.lineKind !== "trade" && !matched.has(line.id),
      );
      if (ordinary.length === 0) return "";
      const accountPending = ordinary.filter((line) => line.status === "pending").length;
      return `
        <div class="rounded border border-surface-800 bg-base-100 p-4">
          <div class="mb-2 flex flex-wrap items-center justify-between gap-2">
            <h3 class="text-sm font-medium text-ink-200">${escapeHtml(statementTitle(s))} <span class="font-normal text-ink-500">· ${accountPending} entr${accountPending !== 1 ? "ies" : "y"} to add</span></h3>
          </div>
          <table class="table table-sm w-full">
            <thead>
              <tr>
                <th></th><th>Date</th><th>Description</th><th class="text-right">Amount</th>
                <th>Type</th><th>Category</th><th>Status</th>
              </tr>
            </thead>
            <tbody>
              ${ordinary.map((line) => lineRow(line, selected.has(line.id), selectable.has(line.id))).join("")}
            </tbody>
          </table>
        </div>`;
    })
    .join("");

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
        <div class="mb-2 flex flex-wrap items-center justify-between gap-2">
          <h3 class="text-sm font-medium text-ink-200">Accounts</h3>
          <div class="flex items-center gap-2">
            <span class="text-xs text-ink-500">${statements.length} account${statements.length !== 1 ? "s" : ""} · ${pending} entr${pending !== 1 ? "ies" : "y"} pending</span>
            <button class="btn btn-ghost btn-xs" type="button" data-action="select-all" ${selectable.size === 0 ? "disabled" : ""}>Select pending</button>
            <button class="btn btn-ghost btn-xs" type="button" data-action="select-none" ${selected.size === 0 ? "disabled" : ""}>Clear</button>
          </div>
        </div>
        <table class="table table-sm w-full">
          <thead><tr><th></th><th>Account</th><th>Currency</th><th>Status</th><th class="text-right">Pending</th><th></th></tr></thead>
          <tbody>${accountRows}</tbody>
        </table>
      </div>

      ${entrySections || `<div class="rounded border border-surface-800 bg-base-100 p-4 text-sm text-ink-500">No importable entries in this file.</div>`}

      ${transferRowsHtml(matches, localIds, selected, accounts, selectable)}

      ${trades.length > 0 ? `<div class="rounded border border-surface-800 bg-base-100 p-4">
        <h3 class="mb-2 text-sm font-medium text-ink-200">Trades <span class="font-normal text-ink-500">(informational, never imported)</span></h3>
        <table class="table table-xs w-full">
          <thead><tr><th>Date</th><th>Description</th><th class="text-right">Status</th></tr></thead>
          <tbody>${tradeRows}</tbody>
        </table>
      </div>` : ""}

      <div class="flex flex-wrap items-center gap-3">
        <button class="btn btn-ghost btn-sm" type="button" data-action="file-back">← Back to list</button>
        <button class="btn btn-primary btn-sm" type="button" data-action="file-import" ${selected.size === 0 ? "disabled" : ""}>Import selected (${selected.size})</button>
        <button class="btn btn-ghost btn-sm" type="button" data-action="file-skip" ${selected.size === 0 ? "disabled" : ""}>Skip selected</button>
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
  /** Existing accounts for the transfer counterpart picker; `listAccounts`
   *  is the desktop's local read, so it is cheap and always current. */
  let accounts: { id: string; name: string; type: string }[] = [];

  const loadAccounts = async (): Promise<void> => {
    try {
      const res = await api.listAccounts();
      if (isEnvelopeFailure(res)) return;
      accounts = ((res as { data?: unknown }).data ?? []) as { id: string; name: string; type: string }[];
      // A late list must not silently keep showing a stale or empty picker.
      render();
    } catch {
      // The picker degrades to "import as expense/income".
    }
  };

  const selectedSet = (): Set<string> => new Set(statementsAtom.get().selectedLines);

  /** Files picked in the drop form, kept here rather than in the atom: they
   *  are transient UI state and re-renders must not lose them. */
  let pickedFiles: File[] = [];

  /** Reads one picked file as base64, stripping the data-URL prefix. */
  const readAsBase64 = (file: File): Promise<string> =>
    new Promise((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => {
        const result = String(reader.result ?? "");
        const comma = result.indexOf(",");
        resolve(comma >= 0 ? result.slice(comma + 1) : result);
      };
      reader.onerror = () => reject(new Error(`could not read ${file.name}`));
      reader.readAsDataURL(file);
    });

  /** Re-applies the drop form's transient state after a re-render, since
   *  `setHtml` rebuilds it from markup that knows nothing about the picks. */
  const syncDropForm = (): void => {
    const details = content.querySelector<HTMLDetailsElement>("details[data-slot='drop-form']");
    const label = content.querySelector<HTMLElement>("[data-drop-selected]");
    const add = content.querySelector<HTMLButtonElement>("[data-action='drop-statement-files']");
    if (!label || !add) return;
    if (pickedFiles.length === 0) {
      label.textContent = "No files chosen";
      add.disabled = true;
      return;
    }
    label.textContent =
      pickedFiles.length === 1 ? pickedFiles[0].name : `${pickedFiles.length} files chosen`;
    add.disabled = false;
    if (details) details.open = true;
  };

  const dropStatementFiles = async (): Promise<void> => {
    const source = content.querySelector<HTMLSelectElement>("[data-drop-source]")?.value ?? "";
    const files = pickedFiles;
    if (files.length === 0 || !source) return;
    statementsAtom.set({ ...statementsAtom.get(), status: "saving", message: "" });
    rerender();
    try {
      const drops = await Promise.all(
        files.map(async (file) => ({ name: file.name, data: await readAsBase64(file) })),
      );
      const res = await api.addStatementFiles(source, drops);
      if (!res.success) throw new Error(res.error ?? "Could not place the files");
      const placed = res.data ?? [];
      // The dropdown's live value wins over the atom: the render that opened
      // the form may be older than a change the user just made.
      localStorage.setItem("maia.statements.lastDropSource", source);
      const dir = placed[0]?.replace(/[/\\][^/\\]*$/, "") ?? "the dump folder";
      pickedFiles = [];
      statementsAtom.set({
        ...statementsAtom.get(),
        lastDropSource: source,
        status: "saved",
        message: `${placed.length} file(s) placed in ${dir}. The watcher stages them automatically.`,
      });
      rerender();
    } catch (e) {
      statementsAtom.set({
        ...statementsAtom.get(),
        status: "error",
        message: `Adding statements failed: ${e}`,
      });
      rerender();
    }
  };

  const render = (): void => {
    const state = statementsAtom.get();
    const accountOptions = accounts.map((a) => ({ id: a.id, name: a.name, type: a.type }));
    if (state.selectedId && state.detail) {
      setHtml(
        content,
        `${panelOpen("Statement Review", state.detail.statement.institution, "panel-fit")}
          ${statusBanner(state)}
          ${detailHtml(state.detail as StatementDetail, selectedSet(), accountOptions)}
        ${panelClose}`,
      );
    } else if (state.fileReview) {
      setHtml(
        content,
        `${panelOpen("File review", basename(state.fileReview.path), "panel-fit")}
          ${statusBanner(state)}
          ${fileHtml(state.fileReview.statements as StatementDetail[], selectedSet(), accountOptions)}
        ${panelClose}`,
      );
    } else {
      setHtml(
        content,
        `${panelOpen("Statements", "Review staged bank and broker statements before importing", "panel-fit")}
          ${statusBanner(state)}
          ${feedCard(feeds)}
          <div class="flex flex-wrap items-center gap-2">
            <select class="select select-sm select-bordered" data-filter="status">
              <option value="awaiting" ${state.filter === "awaiting" ? "selected" : ""}>Awaiting approval</option>
              <option value="" ${state.filter === "" ? "selected" : ""}>All statuses</option>
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
            <button class="btn btn-ghost btn-sm" type="button" data-action="scan">Scan dump folders</button>
            <button class="btn btn-ghost btn-sm" type="button" data-action="refresh">Refresh</button>
          </div>
          <details class="rounded-lg border border-base-300 bg-base-200/40" data-slot="drop-form">
            <summary class="cursor-pointer px-3 py-2 text-sm font-medium text-ink-200 select-none">Add statement…</summary>
            <div class="flex flex-wrap items-center gap-2 border-t border-base-300 px-3 py-2">
              <select class="select select-sm select-bordered" data-drop-source>
                <option value="credit_agricole"${state.lastDropSource === "credit_agricole" ? " selected" : ""}>Crédit Agricole</option>
                <option value="trading212"${state.lastDropSource === "trading212" ? " selected" : ""}>Trading 212</option>
                <option value="firstrade"${state.lastDropSource === "firstrade" ? " selected" : ""}>Firstrade</option>
              </select>
              <input type="file" class="hidden" data-drop-input accept=".csv,.txt" multiple />
              <button class="btn btn-ghost btn-sm" type="button" data-action="choose-statement-files">Choose files…</button>
              <span class="text-sm text-ink-500" data-drop-selected>No files chosen</span>
              <button class="btn btn-primary btn-sm" type="button" data-action="drop-statement-files" disabled>Add</button>
            </div>
          </details>
          <div class="flex flex-wrap items-center justify-between gap-2">
            <div class="text-sm text-ink-400">${state.items.length} statement${state.items.length !== 1 ? "s" : ""}</div>
            <button class="btn btn-ghost btn-xs" type="button" data-action="select-all-statements" ${(state.items as StatementSummary[]).some((s) => s.pendingCount > 0) ? "" : "disabled"}>Select all pending</button>
          </div>
          ${listHtml(state.items as StatementSummary[], state.selectedId, state.selectedStatements)}
          <div>${backToDashboard()}</div>
        ${panelClose}`,
      );
    }
    content.querySelectorAll<HTMLInputElement>("[data-indeterminate]").forEach((el) => {
      el.indeterminate = true;
    });
    refreshIcons(content);
  };

  const loadList = async (): Promise<void> => {
    const state = statementsAtom.get();
    statementsAtom.set({ ...state, status: "loading", message: "" });
    try {
      // "Awaiting approval" is a view over several server statuses at once
      // (the list endpoint filters by one), so it is filtered here.
      const status = state.filter === "awaiting" || state.filter === "" ? undefined : state.filter;
      const res = await api.listStatements({
        status,
        source: state.source || undefined,
      });
      if (!res.success) throw new Error(res.error ?? "Could not list statements");
      let items = (res.data?.items ?? []) as StatementSummary[];
      if (state.filter === "awaiting") {
        items = items.filter((item) => AWAITING_STATUSES.has(item.status));
      }
      statementsAtom.set({
        ...statementsAtom.get(),
        items,
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
    syncDropForm();
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
      // Opening a statement pre-selects what it offers a checkbox for, for the
      // same reason select-all does: a selection the page cannot show as
      // ticked is one the user cannot undo.
      const pending = [...selectableLineIds([detail])];
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
  /** Opens the file review page for one dropped file. */
  const openFileReview = async (path: string): Promise<void> => {
    if (!path) return;
    statementsAtom.set({
      ...statementsAtom.get(),
      selectedId: null,
      detail: null,
      selectedLines: [],
      status: "loading",
      message: "",
      fileReview: { path, statements: [] },
    });
    rerender();
    try {
      const res = await api.getStatementFile(path);
      if (!res.success || !res.data) throw new Error(res.error ?? "Could not load the file");
      statementsAtom.set({
        ...statementsAtom.get(),
        status: "idle",
        message: "",
        fileReview: { path, statements: res.data.statements },
      });
    } catch (e) {
      statementsAtom.set({
        ...statementsAtom.get(),
        fileReview: null,
        status: "error",
        message: `Failed to load the file: ${e}`,
      });
    }
    rerender();
  };

  /** Imports or skips the selected entries across every account of a file. */
  const runFileImport = async (action: "import" | "skip"): Promise<void> => {
    const review = statementsAtom.get().fileReview;
    if (!review || selectedSet().size === 0) return;
    const lines = importLines(action);
    const byStatement = new Map<string, StatementImportLine[]>();
    for (const line of lines) {
      const owner = (review.statements as StatementDetail[]).find((detail) =>
        detail.lines.some((candidate) => candidate.id === line.id),
      );
      if (!owner) continue;
      const list = byStatement.get(owner.statement.id) ?? [];
      list.push(line);
      byStatement.set(owner.statement.id, list);
    }
    if (byStatement.size === 0) return;
    const count = lines.length;
    const message =
      action === "import"
        ? `${count} entr${count !== 1 ? "ies" : "y"} imported.`
        : `${count} entr${count !== 1 ? "ies" : "y"} skipped.`;
    statementsAtom.set({ ...statementsAtom.get(), status: "saving", message: "" });
    rerender();
    try {
      const res = await api.importStatements(
        [...byStatement.entries()].map(([id, lines]) => ({ id, lines, autoLink: true })),
      );
      if (!res.success) throw new Error(res.error ?? "Import failed");
      statementsAtom.set({ ...statementsAtom.get(), selectedLines: [] });
      await openFileReview(review.path);
      statementsAtom.set({ ...statementsAtom.get(), status: "saved", message });
      rerender();
    } catch (e) {
      statementsAtom.set({
        ...statementsAtom.get(),
        status: "error",
        message: `Import failed: ${e}`,
      });
      rerender();
    }
  };

  /** Soft-deletes statements, from the list or from a file review. */
  const deleteAccounts = async (ids: string[], label: string): Promise<void> => {
    if (ids.length === 0) return;
    if (!window.confirm(`Delete ${label}? Imported transactions are kept.`)) return;
    const state = statementsAtom.get();
    statementsAtom.set({ ...state, status: "saving", message: "" });
    rerender();
    try {
      const res = await api.deleteStatements(ids);
      if (!res.success) throw new Error(res.error ?? "Delete failed");
      const deleted = res.data?.deleted ?? [];
      const message = `${deleted.length} statement${deleted.length !== 1 ? "s" : ""} deleted.`;
      if (state.fileReview) {
        const remaining = (state.fileReview.statements as StatementDetail[]).filter(
          (detail) => !deleted.includes(detail.statement.id),
        );
        if (remaining.length === 0) {
          statementsAtom.set({
            ...statementsAtom.get(),
            fileReview: null,
            selectedLines: [],
          });
          await loadList();
        } else {
          await openFileReview(state.fileReview.path);
        }
      } else {
        await loadList();
      }
      statementsAtom.set({
        ...statementsAtom.get(),
        selectedStatements: [],
        status: "saved",
        message,
      });
      rerender();
    } catch (e) {
      statementsAtom.set({
        ...statementsAtom.get(),
        status: "error",
        message: `Delete failed: ${e}`,
      });
      rerender();
    }
  };

  /** Imports every pending entry of the statements picked on the list page. */
  const bulkImportSelected = async (): Promise<void> => {
    const state = statementsAtom.get();
    if (state.selectedStatements.length === 0) return;
    statementsAtom.set({ ...state, status: "saving", message: "" });
    rerender();
    try {
      const details = await Promise.all(
        state.selectedStatements.map((id) => api.getStatement(id)),
      );
      const statements = details
        .filter((res) => res.success && res.data)
        .map((res) => res.data as StatementDetail)
        .map((detail) => ({
          id: detail.statement.id,
          lines: detail.lines
            .filter((line) => line.status === "pending")
            .map((line) => ({ id: line.id, action: "import" as const })),
          autoLink: true,
        }))
        .filter((entry) => entry.lines.length > 0);
      if (statements.length === 0) throw new Error("nothing is pending in the selected statements");
      const res = await api.importStatements(statements);
      if (!res.success) throw new Error(res.error ?? "Import failed");
      await loadList();
      statementsAtom.set({
        ...statementsAtom.get(),
        selectedStatements: [],
        status: "saved",
        message: `Imported pending entries from ${statements.length} statement${statements.length !== 1 ? "s" : ""}.`,
      });
      rerender();
    } catch (e) {
      statementsAtom.set({
        ...statementsAtom.get(),
        status: "error",
        message: `Import failed: ${e}`,
      });
      rerender();
    }
  };

  const importLines = (action: "import" | "skip"): StatementImportLine[] => {
    const selected = selectedSet();
    return [...selected].map((id) => {
      const type = content.querySelector<HTMLSelectElement>(`[data-type-for="${CSS.escape(id)}"]`)?.value;
      const category = content.querySelector<HTMLInputElement>(`[data-category-for="${CSS.escape(id)}"]`)?.value.trim();
      const counterpart = content.querySelector<HTMLSelectElement>(
        `[data-counterpart-for="${CSS.escape(id)}"]`,
      )?.value;
      return {
        id,
        action,
        ...(action === "import" && type ? { type: type as StatementImportLine["type"] } : {}),
        ...(action === "import" && category ? { category } : {}),
        // An explicit counterpart makes the row a real transfer; without one
        // the server books it as an expense/income rather than inventing an
        // account.
        ...(action === "import" && counterpart ? { counterpartAccountId: counterpart } : {}),
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
        // A statement deleted from its own detail page may have been opened
        // from a file review; that snapshot is stale now, so return to the
        // list rather than showing a ghost account.
        fileReview: null,
        selectedLines: [],
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
    if (target.matches("[data-drop-input]")) {
      const input = target as HTMLInputElement;
      pickedFiles = [...(input.files ?? [])];
      syncDropForm();
      return;
    }
    if (target.matches("[data-statement-check]")) {
      const checkbox = target as HTMLInputElement;
      const id = checkbox.dataset.statementCheck as string;
      const state = statementsAtom.get();
      const next = new Set(state.selectedStatements);
      if (checkbox.checked) next.add(id);
      else next.delete(id);
      statementsAtom.set({ ...state, selectedStatements: [...next] });
      render();
      return;
    }
    if (target.matches("[data-file-check]")) {
      const checkbox = target as HTMLInputElement;
      const key = checkbox.dataset.fileCheck as string;
      const state = statementsAtom.get();
      const group = groupStatements(state.items as StatementSummary[]).find((g) => g.key === key);
      if (!group) return;
      const next = new Set(state.selectedStatements);
      for (const statement of group.statements) {
        if (checkbox.checked) next.add(statement.id);
        else next.delete(statement.id);
      }
      statementsAtom.set({ ...state, selectedStatements: [...next] });
      render();
      return;
    }
    if (target.matches("[data-account-check]")) {
      const checkbox = target as HTMLInputElement;
      const statementId = checkbox.dataset.accountCheck as string;
      const state = statementsAtom.get();
      const review = (state.fileReview?.statements as StatementDetail[] | undefined) ?? [];
      const detail = review.find((candidate) => candidate.statement.id === statementId);
      if (!detail) return;
      // Selectability is a property of the whole file (a transfer pair can
      // straddle two of its accounts), so the set is computed over every
      // statement and then narrowed to this one's lines.
      const selectable = selectableLineIds(review);
      const pending = detail.lines.filter((line) => selectable.has(line.id)).map((line) => line.id);
      const next = new Set(state.selectedLines);
      for (const id of pending) {
        if (checkbox.checked) next.add(id);
        else next.delete(id);
      }
      statementsAtom.set({ ...state, selectedLines: [...next] });
      render();
      return;
    }
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
    // A row carrying its own action (the delete button) is not a navigation.
    if (statementButton?.dataset.statementId && !statementButton.closest("[data-action]")) {
      void loadDetail(statementButton.dataset.statementId);
      return;
    }
    const actionEl = target.closest<HTMLElement>("[data-action]");
    const action = actionEl?.dataset.action;
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
      case "file-back":
        statementsAtom.set({
          ...statementsAtom.get(),
          fileReview: null,
          selectedId: null,
          detail: null,
          selectedLines: [],
          status: "idle",
          message: "",
        });
        render();
        void loadList();
        return;
      case "open-file":
        void openFileReview(actionEl?.dataset.filePath ?? "");
        return;
      case "file-import":
        void runFileImport("import");
        return;
      case "file-skip":
        void runFileImport("skip");
        return;
      case "delete-account":
        void deleteAccounts(
          [actionEl?.dataset.statementId ?? ""].filter(Boolean),
          "this statement",
        );
        return;
      case "bulk-import":
        void bulkImportSelected();
        return;
      case "bulk-delete": {
        const count = statementsAtom.get().selectedStatements.length;
        void deleteAccounts(
          statementsAtom.get().selectedStatements,
          `${count} statement${count !== 1 ? "s" : ""}`,
        );
        return;
      }
      case "bulk-clear":
        statementsAtom.set({ ...statementsAtom.get(), selectedStatements: [] });
        render();
        return;
      case "refresh":
        void loadList();
        return;
      case "scan":
        void scan();
        return;
      case "choose-statement-files":
        content.querySelector<HTMLInputElement>("[data-drop-input]")?.click();
        return;
      case "drop-statement-files":
        void dropStatementFiles();
        return;
      case "sync-feed":
        void syncFeed();
        return;
      case "select-all-statements": {
        // Top-level equivalent of "select-all" within a file: picks every
        // statement in the current (filtered) view that still has pending
        // entries, so the bulk import/delete bar below can act on all of
        // them in one shot, across every file group.
        const state = statementsAtom.get();
        const ids = (state.items as StatementSummary[])
          .filter((s) => s.pendingCount > 0)
          .map((s) => s.id);
        statementsAtom.set({ ...state, selectedStatements: ids });
        render();
        return;
      }
      case "select-all": {
        // Selects every entry the page offers a checkbox for -- not every
        // pending line. The two differ: a trade is informational, and a
        // transfer whose counterpart is already settled has no checkbox, so
        // selecting those would inflate "Import selected (N)" past the ticks
        // actually on screen.
        const state = statementsAtom.get();
        const review = state.fileReview
          ? (state.fileReview.statements as StatementDetail[])
          : state.detail
            ? [state.detail as StatementDetail]
            : null;
        if (!review) return;
        statementsAtom.set({ ...state, selectedLines: [...selectableLineIds(review)] });
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

  const rerender = (): void => {
    render();
    syncDropForm();
  };

  render();
  void loadList();
  void loadFeeds();
  void loadAccounts();
  const unsubscribe = statementsAtom.subscribe(rerender);
  return () => unsubscribe();
}
