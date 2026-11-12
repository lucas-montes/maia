import type { StatementDetail, TransferMatch } from "./api";

/** Which entries of a staged statement the review screen can actually act on.
 *
 *  The review renders a line in one of three places: an ordinary entry row, a
 *  transfer row that stands for a matched pair, or the informational trade
 *  table. Only some of those carry a checkbox, so "every pending line" is not
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
      // Trades are informational and never imported; a matched line is
      // offered through its transfer row instead of here.
      if (line.lineKind === "trade" || matched.has(line.id)) continue;
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
