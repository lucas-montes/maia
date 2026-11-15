import type { MatchingPage, ReceiptEffect, StoreRecommendation } from "./api";

/** Where "Open" goes for an effect: a view, and the row to show there. */
export interface OpenTarget {
  view: "stores" | "transactions" | "ingredients";
  id: string;
}

/** One line saying what an effect did, from the receipt's point of view. */
export function effectSentence(e: ReceiptEffect): string {
  const label = e.targetLabel ?? e.targetId;
  const str = (k: string): string => {
    const v = e.after[k] ?? e.before[k];
    return typeof v === "string" ? v : "";
  };
  switch (e.kind) {
    case "store_resolved":
      return `Put on store ${label}`;
    case "store_created":
      return `Created store ${label}`;
    case "store_renamed":
      return `Renamed store “${String(e.before.name ?? "")}” → “${String(e.after.name ?? "")}”`;
    case "store_filled": {
      const fields = Object.keys(e.after).map((k) => k.replace(/_/g, " "));
      return `Filled ${label}: ${fields.join(", ")}`;
    }
    case "alias_learned":
      return `Learned that “${str("merchant") || e.targetId}” is this store`;
    case "transaction_linked":
      return `Linked to ${label}`;
    case "price_recorded":
      return `Recorded price ${label}${str("printed") ? ` from “${str("printed")}”` : ""}`;
    case "product_alias_learned":
      return `Learned that “${str("printed")}” is this ingredient at this store`;
    default:
      return `${e.kind} ${label}`;
  }
}

/** Where an effect's target lives in the app, if it can be opened. */
export function openTarget(e: ReceiptEffect): OpenTarget | null {
  if (!e.targetLive) return null;
  switch (e.targetTable) {
    case "stores":
      return { view: "stores", id: e.targetId };
    case "transactions":
      return { view: "transactions", id: e.targetId };
    case "ingredient_prices": {
      const ingredient = e.after.ingredientId;
      return typeof ingredient === "string" ? { view: "ingredients", id: ingredient } : null;
    }
    case "store_aliases": {
      const store = e.after.storeId;
      return typeof store === "string" ? { view: "stores", id: store } : null;
    }
    default:
      return null;
  }
}

/** Effects that are still in force, oldest first, and the undone ones apart. */
export function splitEffects(effects: ReceiptEffect[]): { live: ReceiptEffect[]; undone: ReceiptEffect[] } {
  return {
    live: effects.filter((e) => e.undoneAt === null),
    undone: effects.filter((e) => e.undoneAt !== null),
  };
}

/** What still needs a decision on this receipt, as short phrases. */
export function openItems(page: MatchingPage): string[] {
  if (!page.read) return ["not read yet"];
  const out: string[] = [];
  if (!page.store) out.push("no store");
  if (page.recommendations.some((r) => r.kind === "rename")) out.push("store could be renamed");
  if (!page.transactionId) out.push("no transaction");
  const unmatched = page.lines.filter((l) => l.kind === "product" && !l.matched).length;
  if (unmatched) out.push(`${unmatched} line${unmatched === 1 ? "" : "s"} unmatched`);
  return out;
}

/** A recommendation as the sentence its button stands next to. */
export function recommendationSentence(r: StoreRecommendation): string {
  switch (r.kind) {
    case "rename":
      return `“${r.current}” is named only by its brand; this receipt says it is “${r.proposed}”.`;
    case "newStore":
      return r.conflictsWith
        ? `This shop is not ${r.conflictsWith.name} (${[r.country, r.taxId].filter(Boolean).join(", ") || "another shop"}). Create “${r.proposed}”?`
        : `No store fits. Create “${r.proposed}”?`;
    case "sameStore":
      return `Looks like ${r.store.name}${r.store.country ? ` (${r.store.country})` : ""}.`;
  }
}

/** The badge text for who decided. */
export function sourceBadge(source: string | null): string {
  switch (source) {
    case "auto":
      return "auto";
    case "ai":
      return "AI";
    case "manual":
      return "you";
    default:
      return "";
  }
}
