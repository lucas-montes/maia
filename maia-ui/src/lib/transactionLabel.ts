import type { FinanceTransaction } from "./api";
import { tradeLabel } from "./statementReview";

/** What a transaction is, as the ledger shows it. A trade is booked as a
 *  transfer between a broker's cash and securities accounts, but it reads as
 *  a trade: the money did not move between two places the user keeps cash. */
export function transactionKind(t: Pick<FinanceTransaction, "type" | "trade">): string {
  return t.trade ? "trade" : t.type;
}

/** The line a ledger row leads with.
 *
 *  `Bought 7 VOO @ 500.00` for a trade, `Transfer → Livret A` for a transfer,
 *  `Dividend` and `Interest` for the income a broker pays (the statement's
 *  own description is long and says less), and otherwise what the user
 *  wrote: the note, then the category, then the type itself. */
export function transactionLabel(
  t: Pick<FinanceTransaction, "type" | "trade" | "category" | "note" | "toAccountName">,
): string {
  if (t.trade) return tradeLabel(t.trade);
  if (t.type === "transfer") return `Transfer → ${t.toAccountName ?? "another account"}`;
  if (t.type === "income") {
    const category = (t.category ?? "").toLowerCase();
    if (category === "dividends" || category === "dividend") return "Dividend";
    if (category === "interest") return "Interest";
  }
  return t.note || t.category || t.type;
}
