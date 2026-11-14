import { describe, expect, test } from "vitest";
import type { Position } from "./api";
import { heldAtCost, positionCells } from "./positions";

function position(over: Partial<Position> = {}): Position {
  return {
    accountId: "sec",
    accountName: "Firstrade Securities",
    symbol: "VOO",
    currencyCode: "USD",
    quantity: 2,
    costBasis: 220,
    averagePrice: 110,
    realisedGain: 40,
    trades: 3,
    opening: null,
    ...over,
  };
}

describe("positionCells", () => {
  test("a known holding shows its numbers", () => {
    expect(positionCells(position())).toEqual({
      shares: "2",
      averagePrice: "110.00 USD",
      cost: "220.00 USD",
      realisedGain: "40.00 USD",
      needsOpening: false,
    });
  });

  test("a holding whose cost nothing explains says unknown, and asks for an opening", () => {
    const cells = positionCells(
      position({ symbol: "INTC", quantity: -35, costBasis: null, averagePrice: null, realisedGain: null }),
    );
    expect(cells.cost).toBe("unknown");
    expect(cells.realisedGain).toBe("unknown");
    expect(cells.needsOpening).toBe(true);
  });

  test("a sold-out holding shows no cost or price, only its gain", () => {
    const cells = positionCells(position({ quantity: 0, costBasis: 0, averagePrice: null, realisedGain: 4.52 }));
    expect(cells).toMatchObject({ shares: "0", averagePrice: "—", cost: "—", realisedGain: "4.52 USD" });
  });
});

describe("heldAtCost", () => {
  test("sums the open holdings per currency, unknown when one is", () => {
    const totals = heldAtCost([
      position(),
      position({ symbol: "NVDA", quantity: 0, costBasis: 0 }),
      position({ symbol: "VWCE", currencyCode: "EUR", costBasis: 100 }),
      position({ symbol: "INTC", currencyCode: "EUR", costBasis: null }),
    ]);
    expect(totals.get("USD")).toBe(220);
    expect(totals.get("EUR")).toBeNull();
  });
});
