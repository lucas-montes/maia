# Receipt matching: receipts ↔ stores, receipts ↔ transactions, lines ↔ products

**Status:** finished 2026-11-17 (server, desktop, phone) — deployed: migrations v16 and v17 are
live; the feature is idle until the first receipt arrives (0 receipts in the live database).
See the deviations at the end and `context/receipts/matching.md`.

> Server and desktop, plus one small phone task (TM1). It builds on the manual link
> (`src/receipt_links.rs`, `PUT`/`DELETE /receipt-pictures/:id/transaction`, abeb509) and the
> manual line match (`POST /ingredient-prices/match`). It **supersedes** the rule in
> `ingredient-price-tracking.md` that "matching is always manual — no auto-matching".
>
> The decisions of 2026-11-15 come from the user's answers to the first draft:
> - Automatic matching is fine, as long as everything stays editable.
> - The phone only photographs and sends. (Its "track of what it sent" was dropped in the second
>   round.)
> - The reader reads everything on the receipt: quantities, discounts, all of it.
> - An undo also forgets the learned name → ingredient match.
> - Stores must tell Lidl Hrvatska and Lidl France apart, and recommend renaming.
> - An on-demand AI pass can improve the matching.
>
> Second round, the same day:
> - The store's identifiers (OIB, SIRET, VAT number…) go on `stores` itself.
> - A receipt's page lists everything that receipt created or edited, each item with a link to
>   it, Edit, Undo and Improve with AI.
> - **A receipt is not an expense.** It is a thing of its own, never booked, and only optionally
>   linked to a transaction that came from a statement.
> - The phone deletes a receipt as soon as the server has it, and keeps no log.

## Who does what

- **The phone** photographs a receipt and sends it. Once the server accepts it, the phone deletes
  it: the row and the image. It keeps nothing else, creates no transaction, store or price, and
  never reads a parse.
- **The server** owns all the logic: it reads the receipt, resolves the store, links the
  transaction, and prices the lines. It never makes a transaction from a receipt.
  Transactions come from statements only.
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
| `taxId`, `taxIdKind` | OIB, SIRET or SIREN, and which one it is |
| `vatNumber` | the EU VAT number, when printed |
| `website` | when printed |
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

**A store is a chain in one country, so Lidl Hrvatska and Lidl France are two stores.** What
identifies a store goes on `stores` itself, as nullable columns:

| Column | Holds |
|---|---|
| `brand` | `Lidl` |
| `legal_name` | `LIDL HRVATSKA d.o.o. k.d.` |
| `country` | ISO 3166, e.g. `HR` |
| `tax_id` | the number as printed, normalised, with a partial unique index |
| `tax_id_kind` | `oib`, `siret`, `siren`, `vat`, `other` |
| `vat_number` | the EU VAT number when it is printed beside the national one |
| `address`, `city`, `postcode` | the branch on the receipt that set them, kept for display |
| `website` | |
| `source` | `manual`, `auto` or `ai`: what last set these facts |

- **Sync.** `push_stores` already names its columns (`id, name, …`), so a phone push leaves these
  alone. The pull keeps sending `id`/`name` only.
- **A phone can't undo a rename either.** `push_stores` stops overwriting the `name` of a store that
  exists; it only inserts new ones. Otherwise a phone that pushes its stores reverts an accepted
  rename. The phone's own price sheet still works, because it only adds stores.
- `store_aliases(alias PK, store_id → stores ON DELETE CASCADE, source, receipt_id, created_at)`
  holds the merchant names a store has been seen under, normalised. The tax id needs no alias; it
  is a column.
- `receipts.store_id` and `receipts.store_source` are server-only, and never in a push upsert.
- **Desktop Stores view:** every column is editable, and the view shows the receipts and the
  prices each store has.

**Resolving the store**, strongest first:
1. **Tax id.** A store whose `tax_id` equals the receipt's gives it, `auto`. A tax id is one
   legal entity, so this is safe. The VAT number counts too.
2. **Name alias.** It gives the store **unless** the store has a different tax id or country than
   the receipt. In that case it falls through to step 4.
3. **Brand and country.** Exactly one live store with that brand, and a country equal to the
   receipt's or unknown, gives it `auto`. The receipt's name is recorded as an alias, and its tax id,
   legal name and country fill the store's empty columns. A filled column is never overwritten, because a
   different value is a different store.
4. **Otherwise no store.** The desktop shows a recommendation (below).

**Recommendations.** They are computed on read and shown on the receipt and the Stores view.
Accepting one is `manual`.
- **Rename:** "`lidl` → `Lidl Hrvatska`". This appears when the store's name is only the brand and
  the receipts show one country. Accepting it renames the store, which syncs, and fills its
  identifier columns from the receipt.
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

**A receipt is not an expense.** Nothing ever books one.
- The desktop import stops creating a draft expense. `create_receipt_transaction` and its `api.ts`
  binding are deleted, and so is the phone's old draft path, which is already gone.
- A receipt paid in cash, or one whose statement isn't imported yet, simply stays unlinked. That is
  a normal state, not an error, and the list does not nag about it.
- **Drafts already made from receipts** (the old desktop and phone flows; 0 of them live today)
  are left alone. `receipt_links::link` still deletes a receipt's own draft when it relinks it to
  a statement transaction.

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

## E. What a receipt did, and getting to it

Every change that a receipt causes is recorded, so its page can list each one with a link to the
change, an edit and an undo.

**`receipt_effects`** (v16, server-only). It is append-only, and an undone row is marked rather
than deleted.

| Column | Holds |
|---|---|
| `id` | |
| `receipt_id` | → `receipts` |
| `kind` | `store_resolved`, `store_created`, `store_renamed`, `store_filled`, `alias_learned`, `transaction_linked`, `price_recorded`, `product_alias_learned` |
| `target_table`, `target_id` | the row it touched (the store, the transaction, the `ingredient_prices` row, the alias) |
| `before_json`, `after_json` | the fields it changed, so a rename can be undone to the old name |
| `source` | `auto`, `ai`, `manual` |
| `created_at`, `undone_at` | |

- **Writers.** Each matcher and each accept endpoint writes its effect in the same database
  transaction as the change. The manual endpoints that act for a receipt write one too: the link,
  the line match, and the store change.
- **Reading.** `GET /receipt-pictures/:id/effects` returns them, joined to what they point at
  (a store's name, a transaction's label, amount and day, an ingredient's name and a price),
  together with what is still open: an unmatched line, a recommendation, no store, no
  transaction.
- **Undo.** `POST /receipt-pictures/:id/effects/:effect/undo` reverses one effect from its
  `before_json`, and follows the undo rules (rule 3: it records the rejection, and a price undo
  forgets the alias). It refuses when the target changed after the effect, for instance a store
  renamed again by hand, and says so. Fixing it then means editing the target.

**The receipt page on the desktop** is laid out as:
1. **The image and the reading.** "Read again" sits here.
2. **What this receipt did.** One row per live effect, newest last: what it is, a badge for its
   source (`auto`/`ai`/`manual`), and three actions:
   - **Open**, which goes to the store in Stores, the transaction in Transactions, or the
     ingredient's price history;
   - **Edit**, which opens the same picker or form inline;
   - **Undo**.

   Undone effects are folded away under "show undone".
3. **Still open.** Unmatched lines, store recommendations, and no transaction yet, each with its
   own action.
4. **"Improve with AI"**, for the whole receipt, with a per-row variant on an open item or an
   `auto` effect. Proposals appear inline in sections 2 and 3 (section D).

**Back links.** A store, a transaction and an ingredient price made by a receipt each show "from
receipt …", which opens that receipt's page. Transactions and stores get it through the effect
rows. Prices already have it through `receipt_line_matches`.

## Tasks

### Stage 0: the reading (no migration)
- [x] T0.1 OCR schema v2 (section 0), the prompt, and the tests.
- [x] T0.2 Re-parse: `POST /receipt-pictures/:id/parse`, and the desktop's "Read again".
- [x] T0.3 The desktop receipt detail shows the new fields: store block, payment, per-line
      quantity, unit and discount, VAT.

### Phone
- [x] TM1 The phone forgets a receipt once it is sent.
  - When the server accepts the upload (`202`), the phone hard-deletes the row and the image, with
    no tombstone, and stops polling the parse. Today it waits for the parse first
    (`budget_sync.dart`, `uploadAndParse`).
  - A failed upload stays and is retried, which is the only state the phone keeps.
  - The phone's copy of the OCR client (`fetchParse`) goes, along with its tests.

### Stage A: stores (migration v16)
- [x] TA1 `src/matching/text.rs`: the normaliser and the purchase-day extractor.
- [x] TA2 v16: the identifier columns on `stores`, `store_aliases`, `receipts.store_id` and
      `store_source`. `push_stores` becomes insert-only for the name.
      `matching::stores::resolve` runs after a parse.
- [x] TA3 The recommendations (rename, new store, same store), and the endpoints to accept each
      one. `match_receipt_item` takes `receiptId`.
- [x] TA4 Desktop: the store on a receipt, with its source and Change, and the recommendations on
      the receipt and in the Stores view, where details can be edited too.

### Stage B: transactions (migration v16)
- [x] TB1 v16: `receipts.link_source`, `receipt_match_rejections`. `receipt_links` takes the
      source and writes a rejection on an automatic undo.
- [x] TB2 `matching::transactions` as pure functions. Unit tests cover:
  - a purchase day across New Year;
  - two identical receipts;
  - a foreign currency;
  - the card number;
  - a cash receipt;
  - a rejected pair;
  - a receipt on its own draft.
- [x] TB3 The triggers, `POST /receipt-pictures/match` and the candidates endpoint.
- [x] TB4 Desktop:
  - the source badge, with Undo or Change;
  - the server's candidates in the picker;
  - no more drafts on import (`create_receipt_transaction` deleted);
  - "Match all".

### Stage C: products (migration v17)
- [x] TC1 v17: `product_aliases`, `receipt_line_matches`. A manual match writes both, and records
      the price from the reading's quantity and unit.
- [x] TC2 Automatic pricing from an alias, with Undo (which forgets the alias) and Change.
- [x] TC3 The line candidates endpoint. The dialog shows them first, and each line shows its state
      and kind.

### Stage D: AI assist
- [x] TD1 `matching::assist`: the prompt, the schema, and dropping unknown ids. `POST
      /receipt-pictures/:id/assist`.
- [x] TD2 Desktop: "Improve with AI" per receipt and for all unmatched ones. Inline proposals with
      accept and accept all. A new-ingredient proposal opens the prefilled form.

### Stage E: the receipt's page (built alongside A–D)
- [x] TE1 `receipt_effects` in v16. Every writer from A, B and C records its effect, and the
      effects, open items and undo endpoints exist. Stage A starts with this, so every later
      writer has somewhere to record.
- [x] TE2 Desktop: the receipt page laid out as in section E, with Open, Edit and Undo per
      effect, the open items, and the "from receipt" back links on stores, transactions and
      ingredient prices.

### Docs
- [x] TDoc: `context/receipts/matching.md`, a pointer from `account-tree.md`, an overview entry,
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
- A receipt's page lists every store, alias, link and price it caused, and each one opens its
  target, can be edited, and can be undone. An undo is refused once the target was changed after.
- The phone deletes a receipt once the server accepts it, and never creates anything.
- `cargo test`, `vitest` and the mobile tests pass. `nix build .#maia-desktop` builds.

## Out of scope (later)
- Merging two stores.
- Tagging ledger transactions with a store.
- Matching refunds automatically.
- A receipt split over two payments.
- Running the AI pass automatically.

## Deviations from the plan (2026-11-15)

- **Endpoints.** The page is served as one endpoint, `GET
  /receipt-pictures/:id/matching`, instead of separate `effects`,
  `transaction-candidates` and `line-candidates` endpoints. The undo is
  `POST /receipt-pictures/:id/effects/:effect/undo`, and the back links come
  from `GET /receipt-effects?table=&id=`. Lines are matched through `PUT`/`DELETE
  /receipt-pictures/:id/lines/:line`, and `POST /ingredient-prices/match` is
  left as it was for other callers.
- **Rejections.** `receipt_match_rejections` became one `receipt_rejections`
  table, with a `kind` (store, transaction, ingredient) and a `line`, so every
  undo is remembered the same way.
- **Reading keys** are snake_case (`tax_id`, `card_last4`), the Rust field
  names, rather than the camelCase the tables above show.
- **Store pushes** only add stores. They also name a placeholder, a store named
  after its own id that a price push made, which an existing test relies on.
  They never revive a deleted store.
- **The desktop's draft-expense command** (`create_receipt_transaction`) and
  the old link commands were removed. The page goes through one
  `matching_request` command, limited to the matching routes.
- **Checked:**
  - `nix build .#maia` and `.#maia-desktop` both build;
  - a copy of the live database migrates from v15 to v17, and its three stores are intact.
- **Not checked yet:**
  - the desktop at runtime;
  - the live model with the v2 schema and the assist prompt (only the stubbed-Gemini
    tests cover them).
