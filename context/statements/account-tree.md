# The account tree

Accounts are two levels: a **root** is the institution a set of accounts
belongs to, and its **leaves** are the accounts themselves. A root's balance
is the sum of its leaves.

Roots are **server-only**. The phone never sees them and never pushes them:
the sync surface (`/budget-accounts`) carries leaves and nothing else.

## The tables

`account_roots(id, name, kind, source, created_at, updated_at, deleted_at)`

| Column | Meaning |
|---|---|
| `id` | minted (`Uuid::now_v7()`), never derived from the source |
| `name` | the institution's name — "Crédit Agricole", "Firstrade" |
| `kind` | what the institution *is*: `bank`, `broker` or `neobank` |
| `source` | the provider the root's leaves come from; `NULL` for a root the user made |

`source` has a **partial unique index** (`idx_account_roots_source`), like
`idx_accounts_external`: a root the user made has no source, and two of those
must not collide with each other.

`accounts.root_id TEXT REFERENCES account_roots(id) ON DELETE CASCADE` —
nullable, because a hand-made account has no root until the user gives it one.

## The types

`accounts.type` says what an account **holds**, not what kind of institution
it is at:

| Type | Holds |
|---|---|
| `cash` | money you can spend |
| `savings` | money you put away (a Livret A, an LDD) |
| `securities` | the broker's positions — a sibling leaf of the cash one |

The old vocabulary (`bank`, `savings`, `investment`, `other`) mixed the two
axes: `bank` and `investment` named institutions, `savings` named a product.
Migration v13 remaps it — `bank` → `cash`, `investment` → `cash`, `other` →
`cash`, `savings` unchanged — and `updated_at` moves with it, because the pull
sends `type` and the phone has to learn the new value.

`credit` is left alone: it is a card, not a product, and the new vocabulary
has no word for it.

## The rules

- **A leaf's source must equal its root's.** A leaf with a source can only
  live under the root for that source; a leaf without one can live under any
  root without a source.
- **The identity stays on the leaf**: `(source, external_id)` and
  `idx_accounts_external` are unchanged. The root has no identity of its own.
- **The securities leaf has no external id.** It is found as "the securities
  leaf of this root".
- **A trade is a movement, not income or expense.** Its cash leg moves between
  the cash leaf and the securities leaf.

## How a root is made

An import that meets a new account resolves the root for its source first,
and creates it if there is none:

```rust
fn resolve_root(tx, source, now) -> anyhow::Result<String> {
    if let Some(id) = lookup_root(tx, source)? { return Ok(id); }
    let (name, kind) = root_for_source(source);
    // minted, never derived — the source is what identifies the root
    ...
}
```

So there is **one root per institution**, and the accounts are its leaves: a
Crédit Agricole import finds the root the migration made, a Firstrade import
makes a *broker* root with a *cash* leaf. The securities leaf is made later,
when a trade books.

The leaf's type follows the root's **kind**: a bank has savings products — a
Livret A, an LDD, an épargne — and everything else it holds is cash. A broker's
account is cash, and so is a bank without branches.

## Where the code lives

| Concern | File |
|---|---|
| the schema and the migration | `src/db.rs` (v13) |
| the source → root mapping | `src/statements/mod.rs` (`SourceId::kind`, `root_for_source`) |
| the type a new account gets | `src/statements/store.rs` (`default_account_type`) |
| resolving and creating roots on import | `src/statements/store.rs` |
| the identity endpoint | `src/handlers/accounts.rs` |
| the phone's read and write | `src/handlers/pull.rs`, `src/handlers/push.rs` |

See also: [entry-lifecycle.md](entry-lifecycle.md), [overview.md](../overview.md),
[context-map.md](../context-map.md)
