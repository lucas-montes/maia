import { describe, expect, test } from "vitest";
import type { FinanceTransaction } from "./api";
import { canTake, rankCandidates, type ReceiptFacts } from "./receiptCandidates";

const DAY = 86_400_000;
const receipt: ReceiptFacts = { id: "r1", total: 42.75, currency: "EUR", date: 100 * DAY };

function txn(id: string, over: Partial<FinanceTransaction> = {}): FinanceTransaction {
  return {
    id,
    type: "expense",
    amount: 10,
    currencyCode: "EUR",
    amountBase: 10,
    rateUsed: 1,
    category: null,
    date: 100 * DAY,
    note: id,
    receiptId: null,
    isDraft: false,
    accountId: "a",
    toAccountId: null,
    accountName: "Checking",
    toAccountName: null,
    trade: null,
    ...over,
  };
}

const text = (t: FinanceTransaction): string => `${t.note ?? ""} ${t.accountName ?? ""}`;

describe("canTake", () => {
  test("only an expense or an income without another receipt", () => {
    expect(canTake(txn("a"), "r1")).toBe(true);
    expect(canTake(txn("a", { type: "income" }), "r1")).toBe(true);
    expect(canTake(txn("a", { type: "transfer" }), "r1")).toBe(false);
    expect(canTake(txn("a", { trade: { side: "buy", symbol: "VOO", quantity: 1, price: 1 } }), "r1")).toBe(false);
    expect(canTake(txn("a", { receiptId: "r2" }), "r1")).toBe(false);
    expect(canTake(txn("a", { receiptId: "r1" }), "r1")).toBe(true);
  });
});

describe("rankCandidates", () => {
  test("the same amount comes first, then the closer amount, then the closer day", () => {
    const list = [
      txn("far-amount", { amount: 5 }),
      txn("close-amount", { amount: 40 }),
      txn("exact-later", { amount: 42.75, date: 103 * DAY }),
      txn("exact", { amount: 42.75, date: 101 * DAY }),
    ];
    expect(rankCandidates(list, receipt, "", text).map((t) => t.id)).toEqual([
      "exact",
      "exact-later",
      "close-amount",
      "far-amount",
    ]);
  });

  test("the same amount in another currency is not the same amount", () => {
    const list = [txn("usd", { amount: 42.75, currencyCode: "USD" }), txn("eur", { amount: 42.0 })];
    expect(rankCandidates(list, receipt, "", text)[0].id).toBe("eur");
  });

  test("an unsearched list stays near the receipt's day, a search reaches all", () => {
    const list = [txn("near"), txn("old", { date: 50 * DAY })];
    expect(rankCandidates(list, receipt, "", text).map((t) => t.id)).toEqual(["near"]);
    expect(rankCandidates(list, receipt, "old", text).map((t) => t.id)).toEqual(["old"]);
  });
});
