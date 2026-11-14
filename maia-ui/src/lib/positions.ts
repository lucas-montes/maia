import type { Position } from "./api";

/** What a positions table shows for one holding. Money the trades cannot
 *  account for reads "unknown" rather than a number made up from nothing. */
export interface PositionCells {
  shares: string;
  averagePrice: string;
  cost: string;
  realisedGain: string;
  /** Whether the cost is unknown, which an opening position would fix. */
  needsOpening: boolean;
}

function amount(value: number | null, currency: string): string {
  if (value === null) return "unknown";
  const text = value.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
  return currency ? `${text} ${currency}` : text;
}

/** Whole shares as integers, fractional ones without trailing zeros. */
function shares(quantity: number): string {
  return Number.isInteger(quantity) ? String(quantity) : String(Number(quantity.toFixed(6)));
}

export function positionCells(position: Position): PositionCells {
  const currency = position.currencyCode ?? "";
  return {
    shares: shares(position.quantity),
    averagePrice:
      position.quantity === 0 ? "—" : amount(position.averagePrice, currency),
    cost: position.quantity === 0 && position.costBasis !== null ? "—" : amount(position.costBasis, currency),
    realisedGain: amount(position.realisedGain, currency),
    needsOpening: position.costBasis === null,
  };
}

/** What the open holdings cost in all, per currency; `null` for a currency
 *  where some holding's cost is unknown. */
export function heldAtCost(positions: Position[]): Map<string, number | null> {
  const totals = new Map<string, number | null>();
  for (const position of positions) {
    if (position.quantity === 0) continue;
    const currency = position.currencyCode ?? "";
    const sum = totals.has(currency) ? totals.get(currency) ?? null : 0;
    totals.set(
      currency,
      sum === null || position.costBasis === null ? null : sum + position.costBasis,
    );
  }
  return totals;
}
