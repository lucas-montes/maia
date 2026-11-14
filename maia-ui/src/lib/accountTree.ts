import type { FinanceAccount, FinanceAccountRoot } from "./api";

/** A root and the accounts under it. */
export interface AccountRootNode {
  root: FinanceAccountRoot;
  /** The accounts that hang under this root. */
  leaves: FinanceAccount[];
  /** The sum of the leaves' balances. */
  total: number;
}

/** The accounts arranged as the tree: roots with their leaves, and the
 *  accounts that belong to no root. */
export interface AccountTree {
  roots: AccountRootNode[];
  /** Accounts with no root: the user's own, not filed under an institution. */
  loose: FinanceAccount[];
}

/** The sum of a set of balances.
 *
 *  A balance that is not a number — the desktop computes one per account, and
 *  an account it cannot price has none — counts as nothing rather than
 *  poisoning the total.
 */
export function totalBalance(accounts: FinanceAccount[]): number {
  return accounts.reduce(
    (sum, account) => (Number.isFinite(account.balance) ? sum + account.balance : sum),
    0,
  );
}

/** Arranges the accounts under their roots.
 *
 *  A root's total is the sum of its leaves' balances: the tree is a view of
 *  the same money, not a second set of numbers. An account with no root is
 *  kept aside rather than dropped — it is the user's own, not filed under an
 *  institution.
 */
export function buildAccountTree(
  roots: FinanceAccountRoot[],
  accounts: FinanceAccount[],
): AccountTree {
  const known = new Set(roots.map((root) => root.id));
  const byRoot = new Map<string, FinanceAccount[]>();
  const loose: FinanceAccount[] = [];
  for (const account of accounts) {
    // A leaf whose root is not among the roots — it was deleted, or the two
    // lists came from different moments — is kept aside rather than dropped.
    // An account that vanishes from the view is worse than one that looks
    // unfiled.
    if (!account.rootId || !known.has(account.rootId)) {
      loose.push(account);
      continue;
    }
    const leaves = byRoot.get(account.rootId) ?? [];
    leaves.push(account);
    byRoot.set(account.rootId, leaves);
  }

  return {
    roots: roots.map((root) => {
      const leaves = byRoot.get(root.id) ?? [];
      return { root, leaves, total: totalBalance(leaves) };
    }),
    loose,
  };
}

/** The roots a leaf may move under: the ones of its own source, and the ones
 *  with no source of their own.
 *
 *  A leaf with a provider number belongs to one institution, so it can only
 *  go under that institution's root; a leaf without one is the user's own and
 *  can go under a root of their own. The server enforces the same rule — this
 *  is what keeps the picker from offering a move it would refuse.
 */
export function rootsForLeaf(
  roots: FinanceAccountRoot[],
  leaf: FinanceAccount,
): FinanceAccountRoot[] {
  return roots.filter((root) => (root.source ?? null) === (leaf.source ?? null));
}
