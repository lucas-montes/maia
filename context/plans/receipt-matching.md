# Receipt matching: receipts ↔ transactions, receipts ↔ stores, lines ↔ products

> Server + desktop. The phone only sends receipts (see `context/overview.md`, "The phone only
> reads finance"). Builds on the manual link (`src/receipt_links.rs`, `PUT`/`DELETE
> /receipt-pictures/:id/transaction`, server commit abeb509) and the manual line match
> (`POST /ingredient-prices/match`). **Supersedes** `ingredient-price-tracking.md`'s rule "matching
> is always manual — no auto-matching" for the cases below.

## What there is to match

| Receipt side (OCR, `receipt_ocr::ParsedReceipt`) | Other side | Today |
|---|---|---|
| `total`, `currency`, `date`, `merchant` | a `transactions` row (expense) | manual picker only |
| `merchant` | a `stores` row | `match_receipt_item` finds a store by **exact** `lower(name)`, else creates one |
| `items[]` (`name`, `price`, `currency`) | an `ingredients` row → an `ingredient_prices` row | manual per line, asks package grams |

What the live data looks like (2026-11-15, read-only):
- **Card payments.** Crédit Agricole writes `Paiement par carte X6510 SPAR ZAGREB 30/07 - Spar`. The
  `30/07` is the **purchase day**, and the entry is dated when it **posted**, 2–4 days later.
  Trading 212 writes the bare merchant (`Lidl`, `Konzum`, `SPAR`), dated on the purchase day.
- **Stores.** There are 3: `konzum`, `lidl`, `spar`. A receipt prints the legal name
  (`LIDL HRVATSKA d.o.o. k.d.`), so an exact-name lookup would make a second Lidl.
- **Products.** There are 9 ingredients, with English names and brands, 6 of them with barcodes. The
  receipts are Croatian and abbreviated, and the OCR reads no barcode. Name similarity across the
  two languages is weak.
- **Counts.** There are 0 receipts and 0 prices so far. Nothing has to be migrated, only designed.

## Rules (all three matchers)

1. **Linking automatically needs exactly one candidate.** The matcher links on its own only when
   exactly one candidate passes every hard test. With two or more, it suggests them and links
   nothing.
2. **Every automatic link says so and can be undone.** It is recorded as `source = 'auto'`, and the
   desktop shows it as "matched automatically" with Undo. A manual link is `'manual'`. The matcher
   never replaces a manual link.
3. **An undo is remembered.** Undoing an automatic link records the rejected pair, and the matcher
   never proposes that pair again. Without this, the next import would redo the link the user
   removed.
4. **Matching is idempotent and re-runnable.** It runs on events and can be re-run over everything
   (backfill). A second run changes nothing.
5. **The server does it; the desktop shows it.** The logic lives in the `maia` crate as pure
   scoring functions over rows, plus one writer per link. The desktop gets no matching logic beyond
   the picker's ordering. The phone is not involved.
6. **One normaliser.** One function normalises merchant text, statement descriptions and printed
   item names: lower case, strip diacritics, drop punctuation and digits-only tokens, drop legal
   suffixes (`d.d.`, `d.o.o.`, `k.d.`, `sa`, `sas`, `sarl`, `gmbh`, `ltd`, `inc`), drop card
   boilerplate (`paiement par carte`, `x6510`-style card refs, `prlvt`, `sepa`), and collapse
   whitespace. It goes in `src/matching/text.rs`, with tests built from the live strings above.

## A. Receipt ↔ store

The merchant text is resolved to a store through **aliases**. The `stores` table syncs with the
phone, so it gets no new column.

- **Migration v16** adds `store_aliases(alias TEXT PRIMARY KEY, store_id → stores ON DELETE
  CASCADE, source 'auto'|'manual', created_at)`. The key is the *normalised* merchant. It is
  server-only state, off the sync surface, like `account_roots`.
- **`receipts.store_id`** (nullable, server-only, kept out of every push upsert, as
  `transaction_id` already is) records which store the receipt is from.
- **Resolution, in order:**
  1. An alias hit gives that store.
  2. Otherwise, a live store whose normalised name is a **whole token** of the normalised merchant
     (`lidl` in `lidl hrvatska`). Exactly one such store gives it, and an `auto` alias is recorded.
  3. Otherwise there is no store. The desktop offers "this is store X" or "new store". Either way a
     `manual` alias is recorded.
- **When it runs:** right after a parse succeeds (`run_ocr`, `handlers/receipt_ocr.rs:226`), and in
  the backfill.
- `match_receipt_item` uses the receipt's resolved store, falling back to the same resolution,
  **instead of the exact-name upsert**. This fixes the second-Lidl problem at the source. It takes
  a `receiptId` so it can find the store.
- **Desktop:** the receipt detail shows the store, with "auto" and Change. A change is a manual
  alias. Merging two stores is out of scope; it is named below as a later task.

## B. Receipt ↔ transaction

**Candidates** are live `expense` transactions that are not trades, and whose `receipt_id` is
empty or points at this receipt's own draft. An `income` is never auto-matched; refunds stay
manual.

**Hard tests.** All must pass.
- **Amount.** `|t.amount| = total` to the cent, in the **receipt's currency**. When the currencies
  differ, convert at the receipt day's rate (`fx_rates`, as `rate_to_base` does). That case is
  suggest-only and never automatic, because the card's own rate and fees differ.
- **Day.** Take the transaction's **purchase day**: the `dd/mm` in a card-payment description
  (`… 30/07 …`, with the year taken from the posting date, rolling back across New Year), or else
  its date. It must fall between one day before and one day after the receipt's day. When there is
  no purchase day in the text, use the posting date with a window from receipt day −1 to receipt
  day +5. The ±1 covers the OCR's local time against our UTC days.
- **Not rejected.** The pair is not in `receipt_match_rejections`.

**Score**, which orders the suggestions and breaks no ties on its own:
- Merchant: the receipt's store or merchant matches the transaction description. That is either a
  token of the store's name or an alias, or token overlap of the normalised texts. Using the live
  examples, `spar zagreb … spar` contains `spar`, and `lidl` equals `lidl`.
- Closeness of the day.

**Linking automatically.** Exactly one candidate passes the hard tests, **and** either its merchant
matches or it is the only expense of that amount within ±7 days. On top of that, the pairing must
be **mutual**: the receipt is also the only unlinked receipt that passes the hard tests for that
transaction. Two receipts of 12.50 at Spar on the same day are suggested, never guessed. A link goes
through `receipt_links::link`, so a draft made from the receipt is deleted when the real
transaction replaces it.

**Schema, also in v16:**
- `receipts.link_source TEXT` holds `'manual'` or `'auto'`, NULL when unlinked. It is server-only
  and never pushed. `receipt_links::link` takes the source.
- `receipt_match_rejections(receipt_id, transaction_id, created_at, PK both)`. `unlink` of an
  `auto` link writes one. A manual relink away from an auto link writes one too.

**When it runs:**
1. After a receipt's parse succeeds, for that receipt.
2. After a statement import books entries (`import_statement_with`, `statements/store.rs:803`),
   for the transactions it booked.
3. On `POST /receipt-pictures/match`, a backfill over every unlinked receipt (and every receipt
   linked to its own draft). It is idempotent and returns what it linked and what it suggests.

**Suggestions are computed on read, not stored:**
`GET /receipt-pictures/:id/transaction-candidates` returns the scored candidates. The desktop
picker shows them first, replacing its own ordering, which stays as the offline fallback.

**Drafts and cash.**
- The desktop import **stops creating a draft expense** (`create_receipt_transaction` on import, in
  `views/receipts.ts`), which matches what the phone now does. A card purchase gets its transaction
  from the statement, and the matcher links it.
- A receipt that stays unmatched (paid in cash, or the statement isn't imported yet) gets a **"Book
  as expense"** action on its card. It is the existing command, run on request, on an account the
  user picks (a cash leaf).

## C. Receipt line ↔ product (ingredient)

The receipt is in another language and has no barcode, so similarity alone is not trusted. What is
trusted is **what the user already matched at the same store**.

- **Migration v17:**
  - `product_aliases(store_id → stores, printed TEXT, ingredient_id → ingredients ON DELETE
    CASCADE, package_grams REAL NULL, source, created_at, updated_at, PK(store_id, printed))`.
    `printed` is the normalised printed name.
  - `receipt_line_matches(receipt_id → receipts ON DELETE CASCADE, line INTEGER, printed TEXT,
    ingredient_id, ingredient_price_id → ingredient_prices ON DELETE SET NULL, source, created_at,
    PK(receipt_id, line))`. It records which line made which price, so the desktop can show each
    line as matched or not and a re-run doesn't record a price twice. `printed` guards against
    a re-parse reordering the lines: a row whose `printed` no longer equals line `line`'s
    normalised name is treated as stale.
- **Learning:** a manual match (`POST /ingredient-prices/match`, now with `receiptId` and `line`)
  writes the line match **and** upserts the alias (store, printed) → ingredient and package grams.
  A later manual match of the same printed name to another ingredient replaces the alias.
- **Automatic:** after the store resolves (A), every line whose (store, printed) has an alias, and
  has no line match yet, records its price exactly as the manual path would (same upsert, dated on
  the receipt's day, alias package grams), with `source = 'auto'`. Undo deletes that price and the
  line match. It does **not** delete the alias, and the desktop offers "forget this alias".
- **Suggestions, never automatic:** for a line with no alias, rank the ingredients by token overlap
  of the printed name against name and brand, with the store's own brands first. It is computed on
  read (`GET /receipt-pictures/:id/line-candidates`) and shown first in the existing match dialog.
- **Not in the receipt:** a line that is a deposit, a bag or a discount is never matched. The
  normaliser marks `povratna naknada`, `vrećica`, `popust` and the like, and the desktop greys them
  out. The list lives in `src/matching/text.rs` and grows from real receipts.
- **Weighed goods:** a line like `BANANE 1,234 kg × 1.29` has no fixed package, so an alias's
  package grams would be wrong. See open question 3.

## Tasks

### Stage A — stores (migration v16, part 1)
- [ ] TA1 `src/matching/text.rs`: the normaliser and the purchase-day extractor, tested on the
      live CA, T212 and receipt strings.
- [ ] TA2 v16: `store_aliases`, `receipts.store_id`. `matching::stores::resolve` (alias → token →
      none) runs after a parse.
- [ ] TA3 `match_receipt_item` takes `receiptId` and resolves the store instead of the exact-name
      upsert. The desktop match dialog passes it.
- [ ] TA4 Desktop: the store on the receipt detail, with "auto" and Change (manual alias).

### Stage B — transactions (migration v16, part 2)
- [ ] TB1 v16: `receipts.link_source`, `receipt_match_rejections`. `receipt_links::{link,unlink}`
      take the source and write the rejection on an auto undo. The existing callers pass `manual`.
- [ ] TB2 `matching::transactions`: candidates, hard tests, score, and the mutual-unique auto rule
      as pure functions over rows, with the edge cases (New Year purchase day, two identical
      receipts, a foreign currency, a rejected pair, a receipt on its own draft) as unit tests.
- [ ] TB3 The triggers: after a parse, after a statement import, and `POST
      /receipt-pictures/match`. Plus `GET /receipt-pictures/:id/transaction-candidates`.
- [ ] TB4 Desktop:
  - the card shows "matched automatically" with Undo, and the picker lists the server's
    candidates first;
  - the import stops making drafts;
  - "Book as expense" on an unmatched receipt;
  - "Match all" runs the backfill.

### Stage C — products (migration v17)
- [ ] TC1 v17: `product_aliases`, `receipt_line_matches`. The manual match writes both.
- [ ] TC2 Automatic pricing from an alias after a parse, plus Undo
      (`DELETE /receipt-pictures/:id/lines/:line/match`).
- [ ] TC3 `GET /receipt-pictures/:id/line-candidates`. The desktop dialog shows them first. Each
      line shows matched, auto or not-a-product.

### Docs
- [ ] TD `context/receipts/matching.md` (the rules, the tables, where the code lives), a pointer
      from `account-tree.md`, an overview entry, and a context-map row.

## Done when
- Card spending: importing an export whose purchase is on a parsed receipt links the two with no
  click, shows "auto", and Undo unlinks them and keeps them unlinked across a re-import.
- Two same-amount receipts on the same day link nothing and suggest both.
- A receipt from `LIDL HRVATSKA d.o.o.` lands on the existing `lidl` store, and a price matched from
  it does not create a store.
- Matching a printed line once at a store prices the same line on the next receipt from that store
  by itself, and Undo removes that price.
- `cargo test` and `vitest` pass. `nix build .#maia-desktop` builds.

## Out of scope (later)
- Merging two stores.
- Tagging transactions with a store ("spend at Lidl" from the ledger, receipts or not).
- Asking the vision model to map the lines to the ingredient list during the parse.
- Auto-matching refunds (incomes).
- Splitting one receipt over two payments, or one payment over two receipts.

## Open questions
1. **Automatic linking at all.** The plan links on its own when there is exactly one candidate,
   with Undo. The alternative is suggest-only, where every link is a click. The recommendation is
   automatic.
2. **Cash receipts.** The plan stops the desktop's draft on import and adds "Book as expense" on
   request. Should an unmatched receipt book itself after N days instead?
3. **Weighed goods.** Should the OCR schema gain an optional `quantity` and `unit` per line
   (`ParsedItem`) so that weighed lines record a price per kg instead of a package, or should
   weighed lines stay manual and never get an alias? The recommendation is the schema change, as a
   TC0. Only the server and the desktop read the parse now.
4. **Undoing an automatic price.** Should Undo also forget the alias, or only that price, as the
   plan has it?
