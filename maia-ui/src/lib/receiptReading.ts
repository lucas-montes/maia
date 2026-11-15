/** A receipt's reading, as the server's `receipt_ocr::ParsedReceipt` (schema
 *  v2) writes it into `receipts.parsed_json`. Keys are snake_case. Only the v1
 *  core (merchant, date, total, currency, items with name/price/currency) is
 *  guaranteed: everything else is null when the receipt did not print it. */

export interface ReadingItem {
  name: string;
  quantity: number | null;
  unit: string | null;
  unitPrice: number | null;
  price: number;
  discount: number | null;
  vatRate: number | null;
  code: string | null;
  /** product | deposit | bag | fee | other; an absent one reads as product. */
  kind: string;
  currency: string;
}

export interface Reading {
  merchant: string | null;
  brand: string | null;
  taxId: string | null;
  taxIdKind: string | null;
  vatNumber: string | null;
  address: string | null;
  city: string | null;
  postcode: string | null;
  country: string | null;
  branch: string | null;
  website: string | null;
  date: string | null;
  receiptNumber: string | null;
  paymentMethod: string | null;
  cardLast4: string | null;
  subtotal: number | null;
  total: number | null;
  currency: string | null;
  vat: { rate: number; base: number | null; amount: number }[];
  discounts: { description: string; amount: number }[];
  loyalty: string | null;
  items: ReadingItem[];
}

const ITEM_KINDS = ["product", "deposit", "bag", "fee", "other"];

function str(v: unknown): string | null {
  if (typeof v === "string") return v.trim() || null;
  if (typeof v === "number") return String(v);
  return null;
}

function num(v: unknown): number | null {
  if (typeof v === "number" && Number.isFinite(v)) return v;
  if (typeof v === "string" && v.trim() !== "" && Number.isFinite(Number(v))) return Number(v);
  return null;
}

function list(v: unknown): Record<string, unknown>[] {
  return Array.isArray(v) ? v.filter((x): x is Record<string, unknown> => !!x && typeof x === "object") : [];
}

/** Reads the stored JSON text; null when there is none or it is not an object. */
export function readReading(parsedJson: string | null): Reading | null {
  if (!parsedJson) return null;
  let raw: unknown;
  try {
    raw = JSON.parse(parsedJson);
  } catch {
    return null;
  }
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const o = raw as Record<string, unknown>;
  return {
    merchant: str(o.merchant),
    brand: str(o.brand),
    taxId: str(o.tax_id),
    taxIdKind: str(o.tax_id_kind),
    vatNumber: str(o.vat_number),
    address: str(o.address),
    city: str(o.city),
    postcode: str(o.postcode),
    country: str(o.country),
    branch: str(o.branch),
    website: str(o.website),
    date: str(o.date),
    receiptNumber: str(o.receipt_number),
    paymentMethod: str(o.payment_method),
    cardLast4: str(o.card_last4),
    subtotal: num(o.subtotal),
    total: num(o.total),
    currency: str(o.currency),
    vat: list(o.vat)
      .map((v) => ({ rate: num(v.rate), base: num(v.base), amount: num(v.amount) }))
      .filter((v): v is { rate: number; base: number | null; amount: number } => v.rate !== null && v.amount !== null),
    discounts: list(o.discounts)
      .map((d) => ({ description: str(d.description) ?? "", amount: num(d.amount) }))
      .filter((d): d is { description: string; amount: number } => d.amount !== null),
    loyalty: str(o.loyalty),
    items: list(o.items)
      .filter((it) => typeof it.name === "string" && num(it.price) !== null)
      .map((it) => {
        const kind = str(it.kind);
        return {
          name: String(it.name),
          quantity: num(it.quantity),
          unit: str(it.unit),
          unitPrice: num(it.unit_price),
          price: num(it.price) as number,
          discount: num(it.discount),
          vatRate: num(it.vat_rate),
          code: str(it.code),
          kind: kind && ITEM_KINDS.includes(kind) ? kind : "product",
          currency: str(it.currency) ?? str(o.currency) ?? "",
        };
      }),
  };
}

/** True when the reading predates schema v2: none of the fields v2 added. A
 *  receipt like that is worth reading again. */
export function isOldReading(r: Reading): boolean {
  return (
    r.brand === null &&
    r.taxId === null &&
    r.paymentMethod === null &&
    r.country === null &&
    r.items.every((it) => it.quantity === null && it.unit === null)
  );
}

/** `1.234 kg × 1.29`, `2 pcs × 0.07`, or "" when the line printed no quantity. */
export function quantityLabel(it: ReadingItem): string {
  if (it.quantity === null) return "";
  const q = Number.isInteger(it.quantity) ? String(it.quantity) : it.quantity.toFixed(3).replace(/0+$/, "");
  const unit = it.unit ?? "";
  const each = it.unitPrice !== null ? ` × ${it.unitPrice.toFixed(2)}` : "";
  return `${q}${unit ? ` ${unit}` : ""}${each}`;
}

/** The shop's address on one line: street, postcode city, country. */
export function addressLine(r: Reading): string {
  const town = [r.postcode, r.city].filter(Boolean).join(" ");
  return [r.address, town, r.country].filter(Boolean).join(", ");
}
