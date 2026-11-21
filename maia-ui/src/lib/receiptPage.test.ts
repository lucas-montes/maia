import { describe, expect, test } from "vitest";
import type { MatchingPage, ReceiptEffect } from "./api";
import { effectSentence, openItems, openTarget, recommendationSentence, splitEffects } from "./receiptPage";

function effect(over: Partial<ReceiptEffect>): ReceiptEffect {
  return {
    id: "e1",
    receiptId: "r1",
    kind: "store_resolved",
    targetTable: "stores",
    targetId: "lidl",
    line: null,
    before: {},
    after: {},
    source: "auto",
    model: null,
    createdAt: 1,
    undoneAt: null,
    targetLabel: "lidl",
    targetLive: true,
    ...over,
  };
}

describe("effects", () => {
  test("each kind reads as what it did", () => {
    expect(effectSentence(effect({}))).toBe("Put on store lidl");
    expect(effectSentence(effect({ kind: "store_renamed", before: { name: "lidl" }, after: { name: "Lidl Hrvatska" } }))).toBe(
      "Renamed store “lidl” → “Lidl Hrvatska”",
    );
    expect(effectSentence(effect({ kind: "store_filled", after: { tax_id: "1", country: "HR" } }))).toBe(
      "Filled lidl: tax id, country",
    );
    expect(
      effectSentence(
        effect({ kind: "price_recorded", targetTable: "ingredient_prices", targetLabel: "Banana · 1.59 EUR", after: { printed: "BANANE" } }),
      ),
    ).toBe("Recorded price Banana · 1.59 EUR from “BANANE”");
  });

  test("open goes to the store, the transaction, or the ingredient of a price", () => {
    expect(openTarget(effect({}))).toEqual({ view: "stores", id: "lidl" });
    expect(openTarget(effect({ targetTable: "transactions", targetId: "t1" }))).toEqual({ view: "transactions", id: "t1" });
    expect(
      openTarget(effect({ targetTable: "ingredient_prices", targetId: "p1", after: { ingredientId: "banana" } })),
    ).toEqual({ view: "ingredients", id: "banana" });
    expect(openTarget(effect({ targetLive: false }))).toBeNull();
  });

  test("undone effects are kept apart", () => {
    const { live, undone } = splitEffects([effect({}), effect({ id: "e2", undoneAt: 5 })]);
    expect(live.map((e) => e.id)).toEqual(["e1"]);
    expect(undone.map((e) => e.id)).toEqual(["e2"]);
  });
});

describe("open items", () => {
  const page: MatchingPage = {
    receiptId: "r1",
    read: true,
    readBy: null,
    store: null,
    storeSource: null,
    recommendations: [],
    transactionId: null,
    linkSource: null,
    transactionCandidates: [],
    lines: [
      { line: 0, name: "BANANE", kind: "product", quantity: null, unit: null, price: 1, currency: "EUR", matched: null, candidates: [] },
      { line: 1, name: "BOCA", kind: "deposit", quantity: null, unit: null, price: 1, currency: "EUR", matched: null, candidates: [] },
    ],
    effects: [],
  };

  test("lists what still needs a decision, ignoring deposits", () => {
    expect(openItems(page)).toEqual(["no store", "no transaction", "1 line unmatched"]);
    expect(openItems({ ...page, read: false })).toEqual(["not read yet"]);
  });

  test("recommendations read as sentences", () => {
    expect(recommendationSentence({ kind: "rename", storeId: "lidl", current: "lidl", proposed: "Lidl Hrvatska" })).toContain(
      "“Lidl Hrvatska”",
    );
    expect(
      recommendationSentence({
        kind: "newStore",
        proposed: "Lidl FR",
        brand: "Lidl",
        country: "FR",
        taxId: "343",
        conflictsWith: { id: "lidl", name: "Lidl Hrvatska" } as never,
      }),
    ).toBe("This shop is not Lidl Hrvatska (FR, 343). Create “Lidl FR”?");
  });
});
