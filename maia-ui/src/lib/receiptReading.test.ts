import { describe, expect, test } from "vitest";
import { addressLine, isOldReading, quantityLabel, readReading } from "./receiptReading";

const v1 = JSON.stringify({
  merchant: "Carrefour",
  date: "2026-08-15 14:30:00",
  total: 42.75,
  currency: "EUR",
  items: [{ name: "Milk", price: 1.2, currency: "EUR" }],
  discounts: [{ description: "loyalty", amount: 2 }],
});

const v2 = JSON.stringify({
  merchant: "LIDL HRVATSKA d.o.o. k.d.",
  brand: "Lidl",
  tax_id: "66089976432",
  tax_id_kind: "oib",
  address: "Ulica grada Vukovara 1",
  city: "Zagreb",
  postcode: "10000",
  country: "HR",
  date: "2026-08-01T18:02:00",
  payment_method: "card",
  card_last4: "6510",
  total: 23.69,
  currency: "EUR",
  vat: [{ rate: 25, base: 18.95, amount: 4.74 }],
  items: [
    { name: "BANANE", quantity: 1.234, unit: "kg", unit_price: 1.29, price: 1.59, kind: "product", currency: "EUR" },
    { name: "POVRATNA NAKNADA", quantity: 2, unit: "pcs", unit_price: 0.07, price: 0.14, kind: "deposit" },
    { name: "Mystery", price: 1, kind: "spaceship" },
  ],
});

describe("readReading", () => {
  test("a v1 reading reads, and is old", () => {
    const r = readReading(v1)!;
    expect(r.merchant).toBe("Carrefour");
    expect(r.items[0].kind).toBe("product");
    expect(r.discounts).toEqual([{ description: "loyalty", amount: 2 }]);
    expect(isOldReading(r)).toBe(true);
  });

  test("a v2 reading keeps every field", () => {
    const r = readReading(v2)!;
    expect(r.taxId).toBe("66089976432");
    expect(r.cardLast4).toBe("6510");
    expect(r.vat).toHaveLength(1);
    expect(r.items[1].kind).toBe("deposit");
    // An unknown kind reads as a product, and a missing currency falls back to the receipt's.
    expect(r.items[2].kind).toBe("product");
    expect(r.items[2].currency).toBe("EUR");
    expect(isOldReading(r)).toBe(false);
    expect(addressLine(r)).toBe("Ulica grada Vukovara 1, 10000 Zagreb, HR");
  });

  test("nothing, or not an object, is no reading", () => {
    expect(readReading(null)).toBeNull();
    expect(readReading("not json")).toBeNull();
    expect(readReading("[1]")).toBeNull();
  });
});

describe("quantityLabel", () => {
  test("weighed, counted, and absent", () => {
    const r = readReading(v2)!;
    expect(quantityLabel(r.items[0])).toBe("1.234 kg × 1.29");
    expect(quantityLabel(r.items[1])).toBe("2 pcs × 0.07");
    expect(quantityLabel(r.items[2])).toBe("");
  });
});
