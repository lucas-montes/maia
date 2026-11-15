import type { FinanceTransaction } from "./api";

/** What the picker knows about a receipt: its parsed total, currency and day. */
export interface ReceiptFacts {
  id: string;
  total: number | null;
  currency: string | null;
  /** Epoch milliseconds, the receipt's own date. */
  date: number;
}

const DAY_MS = 86_400_000;

/** How far from the receipt's day an unsearched list reaches. A card payment
 *  can post a few days after the purchase. */
export const NEARBY_DAYS = 10;

/** A transaction can take this receipt when it is an expense or an income,
 *  not a trade, and holds no other receipt. The server enforces the same rule
 *  (`maia::receipt_links`); this only keeps the picker from offering a refusal. */
export function canTake(t: FinanceTransaction, receiptId: string): boolean {
  if (t.trade) return false;
  if (t.type !== "expense" && t.type !== "income") return false;
  return t.receiptId === null || t.receiptId === receiptId;
}

/** True when the transaction is for the receipt's amount, in its currency. */
export function sameAmount(t: FinanceTransaction, r: ReceiptFacts): boolean {
  if (r.total === null) return false;
  if (r.currency && r.currency.toUpperCase() !== t.currencyCode.toUpperCase()) return false;
  return Math.abs(Math.abs(t.amount) - r.total) < 0.005;
}

/** The transactions the picker offers, best first.
 *
 *  With no search, these are the ones within `NEARBY_DAYS` of the receipt's
 *  day: those for the same amount first, then by how close the amount is, then
 *  by how close the day is. A search reaches every transaction that can take
 *  the receipt, matched on its label, note, category and account. */
export function rankCandidates(
  transactions: FinanceTransaction[],
  r: ReceiptFacts,
  query: string,
  text: (t: FinanceTransaction) => string,
): FinanceTransaction[] {
  const q = query.trim().toLowerCase();
  const days = (t: FinanceTransaction): number => Math.abs(t.date - r.date) / DAY_MS;
  // Amounts in two currencies are not comparable: one in another currency
  // ranks after every one in the receipt's.
  const gap = (t: FinanceTransaction): number => {
    if (r.total === null) return 0;
    if (r.currency && r.currency.toUpperCase() !== t.currencyCode.toUpperCase()) return Number.MAX_VALUE;
    return Math.abs(Math.abs(t.amount) - r.total);
  };
  return transactions
    .filter((t) => canTake(t, r.id))
    .filter((t) => (q ? text(t).toLowerCase().includes(q) : days(t) <= NEARBY_DAYS))
    .sort(
      (a, b) =>
        Number(sameAmount(b, r)) - Number(sameAmount(a, r)) || gap(a) - gap(b) || days(a) - days(b),
    );
}
