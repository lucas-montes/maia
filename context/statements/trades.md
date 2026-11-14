# Trades and positions

A broker's statement holds two kinds of money: income (dividends, interest)
and trades. A trade is not income or an expense. The money stays the user's;
it only changes form, from cash to shares or back. Maia books a trade as a
**movement between the broker's two accounts** and derives everything else
(how many shares, what they cost, what a sale gained) from the trades.

Decided in `context/plans/finance-model.md` ("a trade books its cash leg;
positions are derived").

## What a parser keeps

Every trade line carries:

- **`amount`**: the cash the trade moved, signed by its side. A buy is
  negative and a sale positive, whatever sign the export wrote.
- **`trade`**: a `TradeDetails { side, symbol, quantity, price }`. The
  quantity is always positive, because the side says which way the shares
  went. The store writes it into the line's `metadata_json` under `"trade"`
  for every source, and `store::stored_trade` reads it back.

| Source | Cash | Fallback when the row has none |
|---|---|---|
| Firstrade CSV | `Amount` | quantity × price |
| Trading 212 CSV | `Total` (fees included, account currency) | none, because the price is in the instrument's currency |
| Trading 212 API | the fill's `walletImpact.netValue` | none |

**A trade's identity ignores its amount.** Trades used to be staged at 0, and
the fingerprint still takes 0 for them, so a trade staged before keeps its
line id and an older export of it still reads as a duplicate.

## Staging

A trade with its details and some cash is `pending`, like any entry. One with
nothing to book is `informational` (`StatementLineDraft::is_informational`).
That is either a trade with no stored details or one that moved no cash.

A statement holding a trade **read without its details** is read again on the
same bytes. Normally the same bytes are a no-op, but the details can only come
from the bytes, so this is how a file staged by an older build recovers them.
Only `dump/` is watched, so such a file has to be dropped there again.

`match_pending` never looks at trades. A trade's other side is the broker's own
securities account, not a line of any statement, and a buy must not pair with a
deposit that happens to share its amount.

## Booking

`store::import_trade` books one transaction:

| Side | `account_id` | `to_account_id` |
|---|---|---|
| buy | the cash leaf | the securities leaf |
| sell | the securities leaf | the cash leaf |

The transaction is `type = 'transfer'` at `|amount|`, and a `trades` row
(migration v14) records its side, symbol, quantity and price. A transaction
with a `trades` row is a trade. One without is a movement. There is no `kind`
column on `transactions`, because that table syncs with the phone, which would
see the trade as a plain transfer and could push the column back.

- The **cash leaf** is the statement's own account, found the way any import
  finds it (see [account-tree.md](account-tree.md)).
- The **securities leaf** is the root's `type = 'securities'` leaf, created on
  the first trade as `"{institution} Securities"`, with the root's source and no
  external id. A deleted one is an error, like any deleted account an import
  reaches.
- **The user doesn't pick the type.** A request asking for an expense still
  books the trade. A trade with no stored details is refused per entry, never
  booked as an expense or an income.

The review offers a bookable trade like any entry, labelled `Bought 7 VOO @
500.00` with which way the cash goes. The preview names the securities account
it would create.

## Positions

`trades::positions` derives every holding of every live securities account at
**average cost**. Nothing is stored.

- A buy adds its cash to the cost.
- A sale takes the average cost of the shares it sold off the cost. The rest
  of its proceeds is the **realised gain**, which is shown and never booked as
  income.
- Fees are inside the cash, so they count in a buy's cost and come out of a
  sale's proceeds. On the same day a buy is counted before a sale.
- A sale of more shares than are accounted for, or a trade with no share
  count, makes the cost and the gain **unknown** (`None`). The desktop shows
  "unknown" in that case, never a number made up from nothing.

### Opening positions

An export rarely reaches back to the first purchase. An **opening position**
(migration v15, `opening_positions`, one per account and symbol) states what an
account held, and what that cost in total, at the **end of a UTC day**. The
position starts from it, and trades on or before that day are taken to be in it.

It is written through the desktop-only
`PUT`/`DELETE /accounts/:id/opening-positions/:symbol`. Like the roots, it is
server-only state.

The desktop's **Positions** card on a securities account shows the shares,
average price, cost, realised gain and "held at cost" per currency. It has
the form to state an opening position.

**Known gap:** the securities account's headline balance still sums its
transfers. Showing the cost there instead needs the cost converted to the base
currency (see the plan's T18 deviation).

## Where the code lives

| Concern | File |
|---|---|
| `TradeSide`, `TradeDetails`, what is informational | `src/statements/draft.rs` |
| the parsers | `src/statements/{firstrade,t212_csv,t212}.rs` |
| staging, re-read, matching, `import_trade` | `src/statements/store.rs` |
| the securities account in the preview | `src/statements/preview.rs` |
| `trades` / `opening_positions` tables | `src/db.rs` (v14, v15) |
| recording trades, positions, opening positions | `src/trades.rs` |
| the opening-position endpoints | `src/handlers/positions.rs` |
| desktop commands (`list_transactions` join, `list_positions`, opening writes) | `maia-ui/src-tauri/src/lib.rs` |
| review labels | `maia-ui/src/lib/statementReview.ts`, `maia-ui/src/views/statements.ts` |
| ledger labels | `maia-ui/src/lib/transactionLabel.ts`, `maia-ui/src/views/transactions.ts` |
| positions card | `maia-ui/src/lib/positions.ts`, `maia-ui/src/views/accounts.ts` |

See also: [account-tree.md](account-tree.md),
[entry-lifecycle.md](entry-lifecycle.md), [overview.md](../overview.md),
[context-map.md](../context-map.md)
