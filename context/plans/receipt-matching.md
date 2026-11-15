# Receipt matching: receipts ↔ stores, receipts ↔ transactions, lines ↔ products

> Server and desktop, plus one small phone task (TM1). It builds on the manual link
> (`src/receipt_links.rs`, `PUT`/`DELETE /receipt-pictures/:id/transaction`, abeb509) and the
> manual line match (`POST /ingredient-prices/match`). It **supersedes** the rule in
> `ingredient-price-tracking.md` that "matching is always manual — no auto-matching".
>
> The decisions of 2026-11-15 come from the user's answers to the first draft:
> - Automatic matching is fine, as long as everything stays editable.
> - The phone only photographs and sends, and keeps track of what it sent.
> - The reader reads everything on the receipt: quantities, discounts, all of it.
> - An undo also forgets the learned name → ingredient match.
> - Stores must tell Lidl Hrvatska and Lidl France apart, and recommend renaming.
> - An on-demand AI pass can improve the matching.

## Who does what

- **The phone** photographs a receipt, sends it, and keeps a log of what it sent. It creates no
  transaction, store or price, and it never reads a parse.
- **The server** owns all the logic: it reads the receipt, resolves the store, links the
  transaction, prices the lines, and books a cash receipt.
- **The desktop** shows the results and edits them. It holds no matching logic beyond ordering
  its own picker when the server can't be reached.

## What the live data looks like (2026-11-15, read-only)

- **Card payments.**
  - Crédit Agricole writes `Paiement par carte X6510 SPAR ZAGREB 30/07 - Spar`. The `30/07` is the
    purchase day, and the entry is dated 2–4 days later, when it posted. `X6510` is the card.
  - Trading 212 writes the bare merchant (`Lidl`, `Konzum`, `SPAR`) and dates it on the purchase
    day.
- **Stores.** There are 3: `konzum`, `lidl` and `spar`. All of them are really the Croatian chains.
  A receipt prints the legal entity (`LIDL HRVATSKA d.o.o. k.d.`), its tax number (OIB) and the
  branch address.
- **Products.** There are 9 ingredients, with English names and brands. The receipts are Croatian
  and abbreviated.
- **Counts.** There are 0 receipts and 0 prices, so nothing has to be migrated or kept compatible.

## Rules (every matcher)

1. **Automatic only with one candidate.** A matcher links on its own only when exactly one
   candidate passes every hard test. Otherwise it suggests and links nothing.
2. **Everything is editable.** Every link, store, alias and price that a matcher makes records
   where it came from: `source` is `auto`, `ai` or `manual`. The desktop shows it, and each one can
   be changed or undone later. A matcher never replaces a `manual` decision.
3. **An undo is remembered.** Undoing an automatic decision records the rejected pair, so it is
   never proposed again. Undoing a price that came from a learned name also forgets that name.
4. **Idempotent.** Matching runs on events and in a backfill, and a second run changes nothing.
5. **One normaliser.** `src/matching/text.rs` does all of it: lower case, strip diacritics, drop
   punctuation and legal suffixes (`d.d.`, `d.o.o.`, `k.d.`, `sa`, `sas`, `sarl`, `gmbh`, `ltd`),
   drop card boilerplate (`paiement par carte`, `X6510`, `prlvt`, `sepa`) and collapse whitespace.
   It is tested on the live strings above.

## 0. Read everything (OCR schema v2)

`receipt_ocr::ParsedReceipt` becomes a full reading. Every new field is optional, because a
receipt may not print it. The existing names (`merchant`, `date`, `total`, `currency`, `items`,
`discounts`) keep their meaning.

**The store**

| Field | Example |
|---|---|
| `merchant` | the legal name |
| `brand` | `Lidl` |
| `taxId` | OIB, SIRET, VAT number |
| `address`, `city`, `postcode`, `country` | ISO 3166, e.g. `HR` |
| `branch` | a store number when printed |

**The sale**

| Field | Meaning |
|---|---|
| `date` | with the time |
| `receiptNumber` | |
| `paymentMethod` | `card`, `cash` or `other` |
| `cardLast4` | `6510` |
| `subtotal`, `total`, `currency` | |
| `vat[]` | `{rate, base, amount}` |
| `discounts[]` | `{description, amount}`, applied to the whole receipt |
| `loyalty` | a loyalty card or points line, when printed |

**Each item**

| Field | Example |
|---|---|
| `name` | as printed |
| `quantity` | `2`, `1.234` |
| `unit` | `pcs`, `kg`, `g`, `l` |
| `unitPrice` | |
| `price` | the line total paid, after the line's discount |
| `discount` | the line's own discount, e.g. `−0.50` |
| `vatRate` | |
| `code` | the article code or barcode when printed |
| `kind` | `product`, `deposit`, `bag`, `fee` or `other` |

- **Re-parse:** `POST /receipt-pictures/:id/parse` runs the reader again on the stored image. The
  desktop gets a "Read again" button for a receipt read by the old schema or read badly. Matching
  re-runs after it.
- **Tests:** a schema test like `schema_uses_merchant_and_items`, and a round-trip on a realistic
  Croatian receipt.

## A. Receipt ↔ store

**A store is a chain in one country, so Lidl Hrvatska and Lidl France are two stores.** The
`stores` table syncs with the phone, so its extra facts live in server-only tables:

- `store_details(store_id PK → stores ON DELETE CASCADE, brand, country, tax_id, updated_at)`
- `store_aliases(alias PK, store_id → stores ON DELETE CASCADE, source, created_at)`, where
  `alias` is a normalised merchant name or `tax:<taxId>`.
- `receipts.store_id` and `receipts.store_source`. Both are server-only and never in a push upsert.

**Resolving the store**, strongest first:
1. **Tax id.** An alias `tax:<taxId>` gives that store, `auto`. A tax id is one legal entity, so this
   is safe.
2. **Name alias.** It gives the store **unless** the store has a different tax id or country than
   the receipt. In that case it falls through to step 4.
3. **Brand and country.** Exactly one live store with that brand, and a country equal to the
   receipt's or unknown, gives it `auto`. The receipt's tax id and name are recorded as aliases.
4. **Otherwise no store.** The desktop shows a recommendation (below).

**Recommendations.** They are computed on read and shown on the receipt and the Stores view.
Accepting one is `manual`.
- **Rename:** "`lidl` → `Lidl Hrvatska`". This appears when the store's name is only the brand and
  the receipts show one country. Accepting it renames the store, which syncs, and fills
  `store_details`.
- **New store:** "This receipt is from Lidl France (FR, SIRET …). Create *Lidl France*?" This
  appears when the brand matches but the country or tax id differs.
- **Same store:** "`LIDL HRVATSKA` looks like `lidl`. Link it?" This appears for a brand or token
  match that rule 3 could not settle.
- **Merge two stores** is a later task (see the end of the plan).

`match_receipt_item` takes a `receiptId` and uses the receipt's store. It no longer finds or makes
a store by exact name.

## B. Receipt ↔ transaction

**Candidates:** live `expense` transactions that are not trades, whose `receipt_id` is empty or is
the receipt's own draft. Refunds (`income`) are never matched automatically.

**Hard tests**
- **Amount.** It matches the receipt's `total` to the cent, in the receipt's currency. When the
  currencies differ, the amount is converted at the day's rate, and the result is only a
  suggestion.
- **Day.** The transaction's purchase day must fall within ±1 day of the receipt's. The purchase
  day is the `dd/mm` in a card description, with the year taken from the posting date and rolled
  back across New Year. When the description has no `dd/mm`, the posting date must be from 1 day
  before the receipt to 5 days after.
- **Not rejected.** The pair is not in `receipt_match_rejections`.

**Score**, used for ordering:
- The merchant: the store's name, its aliases or its brand appear among the description's tokens.
- **The card**: `cardLast4` appears in the description (`X6510`), or the account is the one that
  card belongs to.
- How close the days are.
- The payment method: a `cash` receipt never auto-links to a card payment.

**Automatic link.** Exactly one candidate passes. Its merchant or card matches, or it is the only
expense of that amount within ±7 days. The pairing is also mutual: no other unlinked receipt passes
for that transaction. The link goes through `receipt_links::link`, so a draft made from the receipt
is deleted.

**Schema**
- `receipts.link_source` holds `manual`, `auto` or `ai`.
- `receipt_match_rejections(receipt_id, transaction_id, created_at)`. An undo of an automatic link,
  or a manual relink away from one, writes a row.

**When it runs**
- After a parse or a re-parse.
- After a statement import books entries (`import_statement_with`, `statements/store.rs:803`).
- `POST /receipt-pictures/match`, a backfill.
- `GET /receipt-pictures/:id/transaction-candidates` feeds the picker.

**Cash and drafts. Nobody drafts any more.**
- The desktop import stops creating a draft expense (`create_receipt_transaction`, called from
  `views/receipts.ts`).
- A receipt that stays unmatched gets **"Book as expense"**, an action the server performs:
  `POST /receipt-pictures/:id/book {accountId}`. It replaces the desktop's
  `create_receipt_transaction`.
  - The amount, currency, day and category come from the reading.
  - It is linked as `manual`.
  - A `cash` receipt suggests the cash leaf.

## C. Receipt line ↔ product (ingredient)

The receipt is in another language and the reader rarely gets a barcode, so similarity only
suggests. What is trusted is a name the user already matched at the same store, or a code.

**Schema**
- `product_aliases(store_id, printed, ingredient_id → ingredients ON DELETE CASCADE, source,
  created_at, updated_at, PK(store_id, printed))`. `printed` is the normalised printed name, or
  `code:<code>`.
- `receipt_line_matches(receipt_id → receipts ON DELETE CASCADE, line, printed, ingredient_id,
  ingredient_price_id → ingredient_prices ON DELETE SET NULL, source, created_at, PK(receipt_id,
  line))`. A row whose `printed` no longer matches its line after a re-parse is stale.

**The price recorded** comes from the reading itself:
- `price` is the line total after the line's discount, in the line's currency.
- `package_grams` comes from the quantity:
  - for `kg` or `g`, the weight (`1.234 kg` → 1234 g, at the line total);
  - for `pcs`, the price is `unitPrice`, and `package_grams` comes from the ingredient's known pack
    size when one was entered before. It is asked for at a manual match, as today.
- It is dated on the receipt's day.

**Learning.** A manual match (`POST /ingredient-prices/match`, now with `receiptId` and `line`)
writes the line match and upserts the alias for the printed name, and for the code if there is one.

**Automatic.** After the store resolves, each `product` line with an alias and no line match is
priced as above, `auto`.
- **Undo** deletes that price, the line match **and the alias**, and records the rejection, so the
  name is not relearned from the same line.
- **Change** re-points the alias and re-prices the line.

**Suggestions.** For a line with no alias, ingredients are ranked by the overlap of the printed
name with each ingredient's name and brand, the store's own brands first. They are computed on read
(`GET /receipt-pictures/:id/line-candidates`) and shown first in the match dialog.

**Never matched:** a line of kind `deposit`, `bag`, `fee` or `other`. The desktop greys it out, and
it can be overridden.

## D. "Improve with AI" (on demand)

When the rules leave things unmatched, a button on the receipt (and "for all unmatched" on the
list) asks the model. The server sends `POST /receipt-pictures/:id/assist` with:
- the reading;
- the store candidates and their details;
- the transaction candidates;
- the live ingredients (name, brand, nutrition per 100 g, for context);
- the store's existing aliases.

It asks for structured output (the `ai` crate, the same key as the parse):
- the store, existing or new, with its proposed name, brand and country;
- the transaction id, or none;
- for each line: the ingredient id, or "new ingredient" with a proposed name, or "not a product",
  each with a confidence and a short reason.

**Proposals are never applied by themselves.** They come back as suggestions marked `ai`. The
desktop shows them inline and offers "accept" or "accept all". Accepting one writes it like a
manual decision with `source = 'ai'`, and it teaches the aliases the same way. A proposal for a new
ingredient opens the ingredient form, prefilled.

The prompt and the schema live in `src/matching/assist.rs`. A test with a stubbed model checks
that a proposal pointing at an unknown id is dropped.

## Tasks

### Stage 0: the reading (no migration)
- [ ] T0.1 OCR schema v2 (section 0), the prompt, and the tests.
- [ ] T0.2 Re-parse: `POST /receipt-pictures/:id/parse`, and the desktop's "Read again".
- [ ] T0.3 The desktop receipt detail shows the new fields: store block, payment, per-line
      quantity, unit and discount, VAT.

### Phone
- [ ] TM1 The phone keeps a **sent log**.
  - Once the server accepts the upload (`202`), the row becomes `sent`: id, the time it was sent,
    and what the server answered. The image is deleted at that point, and the phone stops polling
    the parse.
  - A failed upload stays and is retried.
  - The receipt list shows what was sent and when.
  - It replaces the hard delete after the parse (mobile `budget_sync.dart`).

### Stage A: stores (migration v16)
- [ ] TA1 `src/matching/text.rs`: the normaliser and the purchase-day extractor.
- [ ] TA2 v16: `store_details`, `store_aliases`, `receipts.store_id` and `store_source`.
      `matching::stores::resolve` runs after a parse.
- [ ] TA3 The recommendations (rename, new store, same store), and the endpoints to accept each
      one. `match_receipt_item` takes `receiptId`.
- [ ] TA4 Desktop: the store on a receipt, with its source and Change, and the recommendations on
      the receipt and in the Stores view, where details can be edited too.

### Stage B: transactions (migration v16)
- [ ] TB1 v16: `receipts.link_source`, `receipt_match_rejections`. `receipt_links` takes the
      source and writes a rejection on an automatic undo.
- [ ] TB2 `matching::transactions` as pure functions. Unit tests cover:
  - a purchase day across New Year;
  - two identical receipts;
  - a foreign currency;
  - the card number;
  - a cash receipt;
  - a rejected pair;
  - a receipt on its own draft.
- [ ] TB3 The triggers, `POST /receipt-pictures/match`, the candidates endpoint, and `POST
      /receipt-pictures/:id/book`.
- [ ] TB4 Desktop:
  - the source badge, with Undo or Change;
  - the server's candidates in the picker;
  - no more drafts on import;
  - "Book as expense";
  - "Match all".

### Stage C: products (migration v17)
- [ ] TC1 v17: `product_aliases`, `receipt_line_matches`. A manual match writes both, and records
      the price from the reading's quantity and unit.
- [ ] TC2 Automatic pricing from an alias, with Undo (which forgets the alias) and Change.
- [ ] TC3 The line candidates endpoint. The dialog shows them first, and each line shows its state
      and kind.

### Stage D: AI assist
- [ ] TD1 `matching::assist`: the prompt, the schema, and dropping unknown ids. `POST
      /receipt-pictures/:id/assist`.
- [ ] TD2 Desktop: "Improve with AI" per receipt and for all unmatched ones. Inline proposals with
      accept and accept all. A new-ingredient proposal opens the prefilled form.

### Docs
- [ ] TDoc: `context/receipts/matching.md`, a pointer from `account-tree.md`, an overview entry,
      and an update to the context-map row.

## Done when
- A receipt read again shows its quantities, discounts, store address, tax id and card.
- A receipt from `LIDL HRVATSKA d.o.o.` lands on the existing Lidl store, which is then recommended
  for a rename to *Lidl Hrvatska*. A receipt from Lidl France recommends a new store and is never
  put on the Croatian one.
- Importing a card export whose purchase is on a parsed receipt links the two with no click, marked
  `auto`. Undo keeps them apart across a re-import. Two identical receipts link nothing.
- A printed line matched once at a store prices the same line on the next receipt, by weight when
  it is weighed. Undo removes the price and the alias.
- "Improve with AI" proposes a store, a transaction and the lines, and nothing changes until a
  proposal is accepted.
- The phone shows its sent log and never creates anything.
- `cargo test`, `vitest` and the mobile tests pass. `nix build .#maia-desktop` builds.

## Out of scope (later)
- Merging two stores.
- Tagging ledger transactions with a store.
- Matching refunds automatically.
- A receipt split over two payments.
- Running the AI pass automatically.
