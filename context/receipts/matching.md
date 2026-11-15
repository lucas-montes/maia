# Receipts: reading and matching

A receipt is **not an expense**. It is its own thing: the picture, what the
reader read off it, and what the server matched it to. Transactions come from
statements only. A receipt is at most linked to one, and a receipt paid in
cash (or whose statement isn't imported yet) simply stays unlinked.

Planned in `context/plans/receipt-matching.md`.

## Who does what

- **The phone** photographs a receipt and sends it
  (`POST /receipt-pictures/upload`). Once the server accepts it (`202`), the
  phone hard-deletes its row and image, with no tombstone and no log. It never
  reads a parse and creates nothing.
  The sync screen's "Receipts" push sends every receipt still on the phone
  the same way. Its "Ingredients" push sends the stores, then each live
  ingredient with its pictures and prices (`POST /ingredients`). The server
  never revives an ingredient or price it deleted (an undone match stays
  undone), never overwrites a price a receipt made, and skips a price that
  clashes with one it already has at that store and moment.
- **The server** reads the receipt, then matches its store, its transaction
  and its lines (`src/matching`). It never books anything.
- **The desktop** shows the receipt's page and sends the user's choices
  back. Its only matching logic is the order of its offline picker
  (`lib/receiptCandidates.ts`).

## The reading (schema v2)

`receipt_ocr::ParsedReceipt` is everything the receipt prints. The schema is
hand-written for Gemini's `responseSchema`, because it needs `nullable` and
`enum`. Keys are snake_case. Only `merchant`, `date`, `total`, `currency` and
each item's `name`, `price` and `currency` are required, so a v1 reading
still reads.

| Group | Fields |
|---|---|
| issuer | `merchant` (legal name), `brand`, `tax_id`, `tax_id_kind` (oib, siret, siren, vat, other), `vat_number`, `address`, `city`, `postcode`, `country` (ISO 2), `branch`, `website` |
| sale | `date`, `receipt_number`, `payment_method` (card, cash, other), `card_last4`, `subtotal`, `total`, `currency`, `vat[]`, `discounts[]` (receipt-wide), `loyalty` |
| item | `name`, `quantity`, `unit` (pcs, kg, g, l, ml, m), `unit_price`, `price` (line total after its own discount), `discount`, `vat_rate`, `code`, `kind` (product, deposit, bag, fee, other) |

`POST /receipt-pictures/:id/parse` reads a stored receipt again, for the
desktop's "Read again". A failed re-read keeps the reading it had, and a
failed parse always leaves the `parsing` status.

## Rules every matcher follows

1. **It acts only with one candidate.** Otherwise it suggests.
2. **Every decision records who made it.** That is `auto` (a rule), `ai` (an
   accepted AI proposal) or `manual`. A rule never overrides `ai` or `manual`.
3. **Every change is a receipt effect.** It is a row in `receipt_effects`,
   written in the same database transaction as the change, with `before` and
   `after`. The receipt's page lists them, and each can be opened, edited and
   undone.
4. **An undo is remembered.** It writes a row to `receipt_rejections`, so the
   rule doesn't redo it. Undoing a price also forgets the learned name.
5. **One normaliser** (`matching/text.rs`) for merchant names, descriptions
   and printed names. It lower-cases, folds diacritics, and drops legal forms
   (`d.o.o.`, `SAS`), card boilerplate (`Paiement par carte X6510`) and bare
   numbers.

Matching runs after a parse (`run_ocr`), after a statement import (both
import handlers run `matching::run_all`), on `POST /receipt-pictures/:id/match`,
and on the backfill `POST /receipt-pictures/match`. It is idempotent.

## Stores

A store is a chain in one country, so Lidl Hrvatska and Lidl France are two
stores. What identifies a store lives on `stores` itself (migration v16):
`brand`, `legal_name`, `country`, `tax_id` (unique among live stores),
`tax_id_kind`, `vat_number`, `address`, `city`, `postcode`, `website`, and
`details_source`.

**Resolution**, strongest first:

1. The tax or VAT number. A VAT number's digits match the national number.
2. A learned name (`store_aliases`, the normalised merchant), unless the
   store's own tax number or country contradicts the receipt.
3. Exactly one store of the brand that nothing contradicts.

Resolving fills the store's **empty** identifier columns from the reading. A
filled one is never overwritten, because a different value means a
different shop.

**Recommendations** are computed on read and accepted from the receipt's page:

| Kind | When | Accepting it |
|---|---|---|
| `rename` | the store is named only by its brand (`lidl` → `Lidl Hrvatska`), and all its receipts say one country | renames the store |
| `newStore` | no store fits; `conflictsWith` names a store of the same brand that the receipt's country or tax number rules out | creates a store from the reading |
| `sameStore` | a likely store the rules could not settle | puts the receipt on that store |

**Sync:** `push_stores` only adds stores. It names a placeholder (a store
named after its own id), but it never renames or revives a store the server
owns. The pull still sends only `id`/`name`.

## Transactions

**Hard tests:**
- a live expense that is not a trade, and is free of any other live receipt;
- exactly the receipt's total, in the receipt's currency (another currency is
  a suggestion only);
- the purchase day within ±1 day of the receipt's. That is the `dd/mm` in a
  card description, with the year rolled back across New Year. Without one,
  the posting date must fall from the day before the receipt to 5 days after;
- not a rejected pair.

**Automatic link:** exactly one transaction passes, and:
- its shop name or card digits match, or it is the only expense of that
  amount within a week;
- a `cash` receipt never lands on a card payment;
- the pairing is mutual: no other waiting receipt passes for that
  transaction.

Linking goes through `receipt_links::link_in` with its source. Moving off a
link the rules or the AI made rejects that pair. A receipt on its own old
draft moves to the real transaction, and the draft is deleted.

## Lines

What is trusted is what the user already matched at the same store.
`product_aliases(store_id, printed, ingredient_id, package_grams)` is learned
from every manual or AI match, under the normalised printed name, and also
under `code:<code>` when the line prints a code. Later receipts from the store
price those lines automatically. A name that merely looks alike is only
suggested, and so is a matching barcode.

**Price:** the line total after its own discount.
- A weighed line (`kg`, `g`, `l`, `ml`) records its weight as
  `package_grams`.
- A counted line records the unit price, with the package from the alias,
  the match, or the ingredient's last known one.
- It is recorded at noon on the receipt's day, one observation per
  ingredient, store and day.

`receipt_line_matches` records which line made which price, and a re-parse
that changes a line's printed name makes its match stale. Deposits, bags and
fees are never matched.

## Improve with AI

`POST /receipt-pictures/:id/assist` sends the model the reading, the stores,
the candidate transactions, the ingredients and the store's learned names. It
gets back proposals for the store, the transaction and each line, each with a
confidence and a reason. Proposals naming an unknown id are dropped. Nothing
is applied: the desktop shows them inline, and accepting one uses the same
endpoint as a manual choice with `source: "ai"`. Proposing a new ingredient
opens a small form, then the match.

## Endpoints

| Route | Does |
|---|---|
| `GET /receipt-pictures/:id/matching` | the page: reading, store, `storeSource`, recommendations, `transactionId`, `linkSource`, candidates, lines, effects with target labels |
| `POST /receipt-pictures/:id/match`, `POST /receipt-pictures/match` | run the rules on one receipt, or on all |
| `PUT /receipt-pictures/:id/store {storeId, source?}` / `POST … {name?, source?}` | choose a store, or create one from the reading |
| `PUT`/`DELETE /receipt-pictures/:id/transaction` | link (`{transactionId, source?}`) or unlink |
| `PUT`/`DELETE /receipt-pictures/:id/lines/:line` | match a line (`{ingredientId, packageGrams?, sourceUrl?, source?}`), or unmatch it and forget the name |
| `POST /receipt-pictures/:id/effects/:effect/undo` | undo one effect; `409` when its target changed since |
| `POST /receipt-pictures/:id/assist` | AI proposals |
| `PUT /stores/:id/name {name, receiptId?}`, `PUT /stores/:id/details` | rename a store (an effect when `receiptId` is given), or edit its identifiers |
| `GET /receipt-effects?table=&id=` | the receipts that changed a row (back links) |

## The desktop

- **Receipt detail:** the reading with Read again, then the matching page
  (`views/receiptPage.ts`): what is still open, the store, the transaction,
  the lines, and "What this receipt did" with Open, Edit and Undo, plus
  Improve with AI.
- **Open** goes through `lib/focus.ts` (or the ingredients atom) to the
  store, the transaction or the ingredient.
- **Back links:** the Stores view edits identifiers and links to the
  receipts behind a store. Transactions and ingredient prices link back to
  their receipt.

## Where the code lives

| Concern | File |
|---|---|
| reading schema | `src/receipt_ocr.rs` |
| parse, re-parse, after-parse matching | `src/handlers/receipt_ocr.rs` |
| normaliser, purchase day | `src/matching/text.rs` |
| stores, recommendations | `src/matching/stores.rs` |
| transactions | `src/matching/transactions.rs`, `src/receipt_links.rs` |
| lines and prices | `src/matching/products.rs` |
| effects, undo, rejections | `src/matching/effects.rs` |
| AI proposals | `src/matching/assist.rs`, `ai/src/model.rs` (`generate_json`) |
| endpoints | `src/handlers/matching.rs`, `src/handlers/receipts.rs` |
| tables | `src/db.rs` v16 (store identifiers, `store_aliases`, `receipt_rejections`, `receipt_effects`, receipt columns) and v17 (`product_aliases`, `receipt_line_matches`) |
| desktop | `maia-ui/src/views/receiptPage.ts`, `receipts.ts`, `stores.ts`; `lib/receiptPage.ts`, `lib/receiptReading.ts`, `lib/focus.ts` |
| phone | mobile `lib/src/budget/services/budget_sync.dart` (`send`) |

See also: [account-tree.md](../statements/account-tree.md),
[overview.md](../overview.md), [context-map.md](../context-map.md)
