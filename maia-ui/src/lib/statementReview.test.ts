import { describe, expect, test } from "vitest";
import type { StatementDetail, StatementLine, TransferMatch } from "./api";
import {
  liveMatches,
  matchedLineIds,
  parsedRowsSummary,
  reviewEmptiness,
  selectableLineIds,
} from "./statementReview";

function line(id: string, over: Partial<StatementLine> = {}): StatementLine {
  return {
    id,
    sourceRef: null,
    postedAt: 1_770_000_000_000,
    description: id,
    amount: -10,
    currencyCode: "EUR",
    balanceAfter: null,
    categoryHint: null,
    lineKind: "debit",
    suggestedType: null,
    suggestedCategory: null,
    metadata: null,
    status: "pending",
    transactionId: null,
    importError: null,
    duplicateOf: null,
    ...over,
  };
}

/** A match as the detail endpoint returns it: both sides inlined, so the
 *  counterpart is known even when it belongs to another statement. */
function match(
  id: string,
  debit: StatementLine,
  credit: StatementLine,
  over: Partial<TransferMatch> = {},
): TransferMatch {
  const side = (l: StatementLine) => ({
    id: l.id,
    description: l.description,
    amount: l.amount,
    currencyCode: l.currencyCode,
    postedAt: l.postedAt,
    status: l.status,
  });
  return {
    id,
    debitLineId: debit.id,
    creditLineId: credit.id,
    transactionId: null,
    confidence: 0.98,
    reason: null,
    status: "auto",
    debit: side(debit),
    credit: side(credit),
    ...over,
  };
}

function detail(id: string, lines: StatementLine[], matches: TransferMatch[] = []): StatementDetail {
  return {
    statement: {
      id,
      source: "credit_agricole",
      institution: "Crédit Agricole",
      accountName: id,
      accountNumber: null,
      accountId: null,
      currencyCode: "EUR",
      periodStart: null,
      periodEnd: null,
      openingBalance: null,
      closingBalance: null,
      status: "staged",
      resolvedAccountName: null,
      parseError: null,
      warnings: [],
      createdAt: 0,
      updatedAt: 0,
      sourcePath: "/dump/ca/ca.csv",
      storedPath: null,
      parsed: null,
    },
    lines,
    matches,
  };
}

describe("selectableLineIds", () => {
  test("offers pending entries and skips settled ones", () => {
    const d = detail("s1", [
      line("a"),
      line("b", { status: "imported" }),
      line("c", { status: "duplicate" }),
      line("d", { status: "skipped" }),
    ]);
    expect([...selectableLineIds([d])]).toEqual(["a"]);
  });

  test("never offers an informational trade, pending or not", () => {
    const d = detail("s1", [
      line("trade", { lineKind: "trade", status: "pending" }),
      line("cash"),
    ]);
    expect([...selectableLineIds([d])]).toEqual(["cash"]);
  });

  test("a matched pair is offered once, through the side staged here", () => {
    const out = line("out", { amount: -50 });
    const inn = line("in", { amount: 50, lineKind: "credit" });
    const d = detail("s1", [out, inn], [match("m1", out, inn)]);
    // Both sides are local, so the transfer row selects the debit side and the
    // credit side is not offered separately.
    expect([...selectableLineIds([d])]).toEqual(["out"]);
  });

  test("a pair whose counterpart is already settled is offered by neither path", () => {
    // The transfer row renders without a checkbox (one side is no longer
    // pending) and the entry table leaves the line out, so select-all must not
    // pick it either -- otherwise "Import selected (N)" counts a line the user
    // cannot see ticked.
    const out = line("out");
    const settled = line("foreign", { status: "imported" });
    const d = detail("s1", [out], [match("m1", out, settled)]);
    expect([...selectableLineIds([d])]).toEqual([]);
  });

  test("a counterpart in another file still offers the local side", () => {
    const out = line("out");
    const foreign = line("foreign"); // pending, but not among this review's lines
    const d = detail("s1", [out], [match("m1", out, foreign)]);
    expect([...selectableLineIds([d])]).toEqual(["out"]);
  });

  test("a dismissed match falls back to two ordinary entries", () => {
    const out = line("out");
    const inn = line("in");
    const d = detail("s1", [out, inn], [match("m1", out, inn, { status: "dismissed" })]);
    expect([...selectableLineIds([d])].sort()).toEqual(["in", "out"]);
  });

  test("a file review spans its statements and counts a shared pair once", () => {
    const out = line("out");
    const inn = line("in");
    const m = match("m1", out, inn);
    // Both statements of the file report the pair (the endpoint selects a
    // match when either side belongs to the statement).
    const a = detail("s1", [out, line("fee")], [m]);
    const b = detail("s2", [inn, line("rent")], [m]);
    expect([...selectableLineIds([a, b])].sort()).toEqual(["fee", "out", "rent"]);
  });

  test("offers a failed entry, so the import can be retried", () => {
    // An entry whose import failed is exactly the one the user wants to try
    // again: a missing FX rate, say. It has to be offered a checkbox, or the
    // failure can never be retried.
    const d = detail("s1", [
      line("a"),
      line("b", { status: "error", importError: "no USD/EUR rate" }),
      line("c", { status: "duplicate" }),
    ]);
    expect([...selectableLineIds([d])].sort()).toEqual(["a", "b"]);
  });

  test("a pair whose import failed is offered, so the transfer can be retried", () => {
    const out = line("out", { status: "error" });
    const inn = line("in", { status: "error" });
    const d = detail("s1", [out, inn], [match("m1", out, inn)]);
    expect([...selectableLineIds([d])]).toEqual(["out"]);
  });

  test("a pair whose other side is settled is offered by neither path", () => {
    const out = line("out", { status: "error" });
    const settled = line("foreign", { status: "imported" });
    const d = detail("s1", [out], [match("m1", out, settled)]);
    expect([...selectableLineIds([d])]).toEqual([]);
  });
});

describe("parsedRowsSummary", () => {
  test("reports what the parser read, kept and dropped", () => {
    expect(parsedRowsSummary({ rowsRead: 67, rowsKept: 66, rowsSkipped: 1 })).toBe(
      "67 rows read · 66 kept · 1 skipped",
    );
  });

  test("says nothing when the parser kept no such record", () => {
    expect(parsedRowsSummary({ lines: 12 })).toBe("");
    expect(parsedRowsSummary(null)).toBe("");
    expect(parsedRowsSummary(undefined)).toBe("");
    expect(parsedRowsSummary("not an object")).toBe("");
  });

  test("a missing count is not invented", () => {
    expect(parsedRowsSummary({ rowsRead: 67, rowsSkipped: 1 })).toBe("");
  });
});

describe("reviewEmptiness", () => {
  test("a review with nothing importable says how many entries are duplicates", () => {
    // Every entry duplicates another file: there is nothing to tick, and the
    // review has to say why rather than looking broken.
    const d = detail("s1", [
      line("a", { status: "duplicate" }),
      line("b", { status: "duplicate" }),
    ]);
    expect(reviewEmptiness([d])).toEqual({ importable: 0, duplicates: 2 });
  });

  test("a review with something to import is not empty", () => {
    const d = detail("s1", [line("a"), line("b", { status: "duplicate" })]);
    expect(reviewEmptiness([d])).toEqual({ importable: 1, duplicates: 1 });
  });

  test("a settled entry is neither importable nor a duplicate", () => {
    const d = detail("s1", [line("a", { status: "imported" })]);
    expect(reviewEmptiness([d])).toEqual({ importable: 0, duplicates: 0 });
  });

  test("a file review counts across its statements", () => {
    const a = detail("s1", [line("a", { status: "duplicate" })]);
    const b = detail("s2", [line("b", { status: "duplicate" }), line("c")]);
    expect(reviewEmptiness([a, b])).toEqual({ importable: 1, duplicates: 2 });
  });
});

describe("liveMatches / matchedLineIds", () => {  test("deduplicates a pair reported by both of its statements", () => {
    const out = line("out");
    const inn = line("in");
    const m = match("m1", out, inn);
    expect(liveMatches([detail("s1", [out], [m]), detail("s2", [inn], [m])])).toHaveLength(1);
  });

  test("drops dismissed matches, so their lines stay ordinary entries", () => {
    const out = line("out");
    const inn = line("in");
    const d = detail("s1", [out, inn], [match("m1", out, inn, { status: "dismissed" })]);
    expect(liveMatches([d])).toEqual([]);
    expect(matchedLineIds(liveMatches([d])).size).toBe(0);
  });
});
