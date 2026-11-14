import { describe, expect, test } from "vitest";
import { transactionKind, transactionLabel } from "./transactionLabel";

const base = {
  type: "expense",
  trade: null,
  category: null,
  note: null,
  toAccountName: null,
} as const;

describe("transactionLabel", () => {
  test("a trade says what it bought or sold", () => {
    const t = {
      ...base,
      type: "transfer",
      toAccountName: "Firstrade Securities",
      trade: { side: "buy", symbol: "VOO", quantity: 7, price: 500 },
    } as const;
    expect(transactionLabel(t)).toBe("Bought 7 VOO @ 500.00");
    expect(transactionKind(t)).toBe("trade");
  });

  test("a transfer names where the money went", () => {
    const t = { ...base, type: "transfer", toAccountName: "Livret A", note: "VIR LIVRET A" };
    expect(transactionLabel(t)).toBe("Transfer → Livret A");
    expect(transactionKind(t)).toBe("transfer");
  });

  test("a broker's income reads as a dividend or interest", () => {
    const note = "COASTAL S&P 500 ETF CASH DIV ON 15 SHS REC 09/28/26";
    expect(transactionLabel({ ...base, type: "income", category: "Dividends", note })).toBe("Dividend");
    expect(transactionLabel({ ...base, type: "income", category: "Interest", note })).toBe("Interest");
  });

  test("anything else is the note, then the category, then the type", () => {
    expect(transactionLabel({ ...base, note: "Groceries at Lidl", category: "Food" })).toBe(
      "Groceries at Lidl",
    );
    expect(transactionLabel({ ...base, category: "Food" })).toBe("Food");
    expect(transactionLabel(base)).toBe("expense");
  });
});
