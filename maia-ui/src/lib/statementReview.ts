import type { StatementDetail, TransferMatch } from "./api";

/** Which entries of a staged statement the review screen can actually act on.
 *
 *  The review renders a line in one of three places: an ordinary entry row, a
 *  transfer row that stands for a matched pair, or the table of trades with
 *  nothing to book. Only some of those carry a checkbox, so "every pending line" is not
 *  the same set as "every line the page offers to import" -- a pending line
 *  whose transfer counterpart was already imported, for instance, is shown as
 *  a transfer row with no checkbox at all.
 *
 *  Select-all, the per-account checkbox and the rendered checkboxes all read
 *  the set computed here, so one click selects exactly what the page shows as
 *  checkable and the "N selected" count can never outrun the ticks on screen.
 */

/** Every live (non-dismissed) transfer match of a review, deduplicated.
 *
 *  The detail endpoint returns a match when *either* of its lines belongs to
 *  the statement, so the two statements of a cross-account pair both report
 *  it; a file review that concatenates its statements would otherwise render
 *  the same transfer twice. */
export function liveMatches(details: StatementDetail[]): TransferMatch[] {
  const seen = new Set<string>();
  const matches: TransferMatch[] = [];
  for (const detail of details) {
    for (const match of detail.matches) {
      if (match.status === "dismissed" || seen.has(match.id)) continue;
      seen.add(match.id);
      matches.push(match);
    }
  }
  return matches;
}

/** Line ids a live match covers: they render as transfer rows, not as
 *  ordinary entries. */
export function matchedLineIds(matches: TransferMatch[]): Set<string> {
  const ids = new Set<string>();
  for (const match of matches) {
    ids.add(match.debitLineId);
    ids.add(match.creditLineId);
  }
  return ids;
}

/** The side of a matched pair that is staged in this review, if either is.
 *
 *  A counterpart may live in another file entirely, in which case only the
 *  local side can be selected -- the server expands the pair itself. */
export function localMatchSide(match: TransferMatch, local: Set<string>): string | null {
  if (local.has(match.debitLineId)) return match.debitLineId;
  if (local.has(match.creditLineId)) return match.creditLineId;
  return null;
}

/** The entries the review can act on.
 *
 *  `pending` is work the user has not done yet. `error` is work that failed --
 *  a missing FX rate, say -- and can be retried, so it is offered a checkbox
 *  too: an entry the import could not book is exactly the one the user wants
 *  to try again. Everything else is settled (`imported`, `skipped`) or was
 *  never importable (`duplicate`, `informational`).
 */
export function isImportable(line: { status: string }): boolean {
  return line.status === "pending" || line.status === "error";
}

/** The parser's own record of what it read, kept and dropped.
 *
 *  A parser that drops rows says how many: the review shows the entries it
 *  kept, and this says what went missing. Empty when the parser recorded no
 *  counts, so a statement from a parser that keeps no such record simply has
 *  nothing to show.
 */
export function parsedRowsSummary(parsed: unknown): string {
  if (!parsed || typeof parsed !== "object") return "";
  const record = parsed as Record<string, unknown>;
  const read = typeof record.rowsRead === "number" ? record.rowsRead : null;
  const kept = typeof record.rowsKept === "number" ? record.rowsKept : null;
  const skipped = typeof record.rowsSkipped === "number" ? record.rowsSkipped : null;
  if (read === null || kept === null || skipped === null) return "";
  return `${read} row${read !== 1 ? "s" : ""} read · ${kept} kept · ${skipped} skipped`;
}

/** What a review can still import, and what stands in the way. */
export interface ReviewEmptiness {
  /** Entries the review can still import. */
  importable: number;
  /** Entries that duplicate another file, and so cannot be imported. */
  duplicates: number;
}

/** What the review can still import, and what stands in the way.
 *
 *  An entry that duplicates another file is neither importable nor settled:
 *  the claim is released when the other file is discarded, so re-dropping this
 *  one re-reads it. A review holding nothing but those has to say so, or it
 *  reads as broken -- a table of entries with no checkboxes and nothing saying
 *  why there is nothing to tick.
 */
export function reviewEmptiness(details: StatementDetail[]): ReviewEmptiness {
  const lines = details.flatMap((detail) => detail.lines);
  return {
    importable: selectableLineIds(details).size,
    duplicates: lines.filter((line) => line.status === "duplicate").length,
  };
}

/** The line ids the review offers a checkbox for, across every statement given
 *  (one for the statement detail page, all of a file's for the file review). */
export function selectableLineIds(details: StatementDetail[]): Set<string> {
  const matches = liveMatches(details);
  const matched = matchedLineIds(matches);
  const local = new Set<string>();
  for (const detail of details) {
    for (const line of detail.lines) local.add(line.id);
  }

  const ids = new Set<string>();
  for (const detail of details) {
    for (const line of detail.lines) {
      // A trade the server can book is staged pending and offered like any
      // other entry; one with nothing to book is informational, so
      // `isImportable` leaves it out. A matched line is
      // offered through its transfer row instead of here.
      if (matched.has(line.id)) continue;
      if (isImportable(line)) ids.add(line.id);
    }
  }
  for (const match of matches) {
    // A pair is one transfer, so it is selectable only while both sides can
    // still be booked -- importing or skipping one side settles the pair. A
    // side that failed keeps its place, so the pair can be retried as one
    // transfer.
    if (!isImportable(match.debit) || !isImportable(match.credit)) continue;
    const id = localMatchSide(match, local);
    if (id) ids.add(id);
  }
  return ids;
}

/** What a trade line bought or sold, as the server stored it. */
export interface TradeDetails {
  side: "buy" | "sell";
  symbol: string;
  quantity: number | null;
  price: number | null;
}

/** The trade a line describes, read from its metadata; `null` for a line
 *  that is not a trade, or a trade read before trades kept their details. */
export function tradeOf(line: { lineKind: string; metadata: unknown }): TradeDetails | null {
  if (line.lineKind !== "trade" || !line.metadata || typeof line.metadata !== "object") return null;
  const trade = (line.metadata as Record<string, unknown>).trade;
  if (!trade || typeof trade !== "object") return null;
  const record = trade as Record<string, unknown>;
  if (record.side !== "buy" && record.side !== "sell") return null;
  return {
    side: record.side,
    symbol: typeof record.symbol === "string" ? record.symbol : "",
    quantity: typeof record.quantity === "number" ? record.quantity : null,
    price: typeof record.price === "number" ? record.price : null,
  };
}

/** A trade the review shows but cannot import: the server had no details to
 *  book it from, or it moved no cash. It sits in the trades table rather
 *  than among the entries. */
export function isUnbookableTrade(line: { lineKind: string; status: string }): boolean {
  return line.lineKind === "trade" && line.status === "informational";
}

/** `Bought 7 VOO @ 500.00`, from a trade's details; empty when it has none. */
export function tradeLabel(trade: TradeDetails | null): string {
  if (!trade) return "";
  const verb = trade.side === "buy" ? "Bought" : "Sold";
  const quantity = trade.quantity === null ? "" : ` ${formatQuantity(trade.quantity)}`;
  const symbol = trade.symbol ? ` ${trade.symbol}` : "";
  const price = trade.price === null ? "" : ` @ ${trade.price.toFixed(2)}`;
  return `${verb}${quantity}${symbol}${price}`;
}

/** Whole shares as integers, fractional ones without trailing zeros. */
function formatQuantity(quantity: number): string {
  return Number.isInteger(quantity) ? String(quantity) : String(Number(quantity.toFixed(6)));
}
