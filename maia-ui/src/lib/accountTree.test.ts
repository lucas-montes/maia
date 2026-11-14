import { describe, expect, test } from "vitest";
import type { FinanceAccount, FinanceAccountRoot } from "./api";
import { buildAccountTree, rootsForLeaf, totalBalance } from "./accountTree";

function account(id: string, over: Partial<FinanceAccount> = {}): FinanceAccount {
  return {
    id,
    name: id,
    type: "cash",
    openingBalance: 0,
    note: null,
    createdAt: 0,
    source: null,
    externalId: null,
    rootId: null,
    balance: 0,
    ...over,
  };
}

function root(id: string, over: Partial<FinanceAccountRoot> = {}): FinanceAccountRoot {
  return { id, name: id, kind: "bank", source: null, ...over };
}

describe("buildAccountTree", () => {
  test("puts a leaf under its root and totals the leaves", () => {
    const tree = buildAccountTree(
      [root("r1", { name: "Crédit Agricole", source: "credit_agricole" })],
      [
        account("a1", { rootId: "r1", balance: 100 }),
        account("a2", { rootId: "r1", balance: 50.5 }),
      ],
    );
    expect(tree.roots).toHaveLength(1);
    expect(tree.roots[0].root.name).toBe("Crédit Agricole");
    expect(tree.roots[0].leaves.map((l) => l.id)).toEqual(["a1", "a2"]);
    expect(tree.roots[0].total).toBe(150.5);
  });

  test("keeps an account with no root aside rather than dropping it", () => {
    const tree = buildAccountTree([], [account("a1", { balance: 10 })]);
    expect(tree.roots).toEqual([]);
    expect(tree.loose.map((a) => a.id)).toEqual(["a1"]);
  });

  test("a root with no leaves still appears, worth nothing", () => {
    const tree = buildAccountTree([root("r1")], []);
    expect(tree.roots).toHaveLength(1);
    expect(tree.roots[0].leaves).toEqual([]);
    expect(tree.roots[0].total).toBe(0);
  });

  test("a leaf of a root that is not there is kept aside", () => {
    // The root was deleted while the leaf still points at it.
    const tree = buildAccountTree([], [account("a1", { rootId: "gone" })]);
    expect(tree.loose.map((a) => a.id)).toEqual(["a1"]);
  });
});

describe("totalBalance", () => {
  test("counts what it can and ignores a balance it cannot", () => {
    expect(totalBalance([account("a1", { balance: 10 })])).toBe(10);
    expect(totalBalance([account("a1", { balance: Number.NaN })])).toBe(0);
    expect(totalBalance([account("a1", { balance: 10 }), account("a2", { balance: Number.NaN })])).toBe(10);
  });
});

describe("rootsForLeaf", () => {
  test("offers a leaf with a provider number only the roots of its source", () => {
    const roots = [
      root("r-ca", { source: "credit_agricole" }),
      root("r-fr", { source: "firstrade" }),
      root("r-mine", { source: null }),
    ];
    const leaf = account("a1", { source: "credit_agricole" });
    expect(rootsForLeaf(roots, leaf).map((r) => r.id)).toEqual(["r-ca"]);
  });

  test("offers a leaf with no provider number the roots with no source", () => {
    const roots = [
      root("r-ca", { source: "credit_agricole" }),
      root("r-mine", { source: null }),
    ];
    const leaf = account("a1", { source: null });
    expect(rootsForLeaf(roots, leaf).map((r) => r.id)).toEqual(["r-mine"]);
  });
});
