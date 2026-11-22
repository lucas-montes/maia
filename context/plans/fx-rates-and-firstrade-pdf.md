# FX rates from the server, and Firstrade PDF statements

**Status:** done (marked 2026-11-22; server `787c54b`, mobile `53a7b03`) and being tested. The
model step of the PDF reader had not yet run on a real statement when this was written: it needs a
model key, set in Settings → Models since `settings-nix-and-models`. Until one is set, every
Firstrade PDF lands in `failed/` with "needs a model key".

> **Superseded config (2026-11-20, `settings-nix-and-models`):** `model_api`/`model_name` are replaced by
> `services.maia.models` + `tasks.statementExtraction`, with the key set in Settings → Models.

> The user asked for two things: a way to pull FX rates periodically, and a Firstrade parser that
> reads PDFs. The decisions of 2026-11-19 come from their answers:
> - FX comes from Frankfurter (ECB daily rates, no key), **with history backfilled**. Old
>   statements need rates years back.
> - A PDF is **converted to text first, then read by the LLM**.
> - **The server is the only source of FX rates.** The phone does not push or edit them.

## Why

- Statement imports convert through `statements::store::rate_for`, which takes the latest rate on
  or before a line's date. Nothing ever *fetched* rates: `fx_rates` was only filled by a phone
  push, so every USD Firstrade line failed with `no USD/EUR FX rate on or before …`.
- Firstrade's CSV export starts on 2023-11-16. The history before that (Aug 2021 – Oct 2023) exists
  only as 21 Apex Clearing monthly/quarterly PDF statements. They end before the CSV starts, so the
  two never overlap.

## FX rates (`src/fx.rs`)

- **Loop:** `fx::spawn` starts with the other background loops in `start_server_with_shutdown`. It
  ticks every `fx.interval_hours` (default 6), with the first tick at boot, and skips missed ticks.
  The loop is off when `fx.enabled` is false; `Config::default()` (tests) keeps it off.
- **What it fetches:** `fx::plan` takes every non-base, three-letter currency used by a dated row
  (`transactions.date`, `bank_statement_lines.posted_at`, `ingredient_prices.recorded_at`) and the
  earliest day it is needed. It then fetches `[earliest, oldest stored rate)` and
  `(newest stored rate, today]`. An undated `0001-01-01` row counts as no coverage.
- **How:** `GET {fx.base_url}/{start}..{end}?base=CODE&symbols=BASE`, one calendar year per request
  (about 260 rows). Frankfurter answers daily even across years, and it includes the business day
  before `start`, which covers weekends. Its value is units of BASE per 1 CODE, which is exactly
  `rate_to_base`.
- **Writing:** `fx::store` upserts with `manual = 0`, and only touches a row whose value, manual
  flag or tombstone changed. An unchanged row keeps its `updated_at`, so the phone's `since` pull
  does not re-fetch it. Old hand-entered `manual = 1` rows are overwritten.
- **Endpoints:** `POST /fx-rates/refresh` runs one pull and returns `{"written": n}` (502 on
  failure). `GET /fx-rates` is unchanged. **`POST /fx-rates` is gone** (405): `push_fx_rates`
  and its types were deleted.
- **Config** (deployment-owned): `fx.enabled`, `fx.interval_hours` (1–168) and `fx.base_url`
  (default `https://api.frankfurter.dev/v1`).
- The puller never imports anything. Entries that already failed for a missing rate stay `error`
  until the user retries them from the review (see `statements/entry-lifecycle.md`).

## Firstrade PDF statements (`src/statements/firstrade_pdf.rs`)

- **Where it runs:** `Parser::parse` is pure and `run_upload` skips `acquire`. So the PDF is read in
  the Firstrade arms of `statements::run_file_source` and `run_upload`, and the result is handed to
  the source with `FirstradeSource::with_extract`. A failed read (no model key, no pdftotext, no
  statement period) is kept and becomes the parse error, so it shows in `failed/` and the review.
- **Steps:**
  1. **Text:** `pdftotext -layout -enc UTF-8 - -` (`pdftotext_bin`, default `pdftotext` on PATH;
     the Nix module pins `${pkgs.poppler-utils}/bin/pdftotext`).
  2. **Header, in Rust** (`read_header`): the period line and `Cash account <opening> <closing>`.
     No period means the PDF is not a statement, which is an error.
  3. **Activity, by the model** (`ai::Model::generate_json`, through `AppState::ocr`). The model
     only sees the pages with a `TRANSACTION DATE` header (`activity_text`). Each page is cut at
     `IMPORTANT INFORMATION`, and the vertical margin letters, `PAGE n OF m` lines and
     `ACCOUNT NUMBER` lines are dropped. The schema is `transactions[]` of `{date (ISO), section,
     action (verbatim), symbol?, description, quantity?, price?, debit?, credit?, withheld?,
     cusip?}`. A statement with no activity pages makes no model call.
  4. **Check** (`reconcile`): opening cash + Σ(credit − debit − withheld) must equal closing cash
     (±0.005), and every row must fall inside the period. Otherwise the statement gets a warning.
- **Cache:** the reading is saved as `<statements_store_dir>/firstrade/<hash>.extract.json` (0600),
  where the hash is the usual `hash_documents` of the PDF bytes. A re-drop reuses it, which costs no
  model call and gives the same fingerprints. Deleting the file forces a fresh reading.
- **Lowering** (`firstrade.rs::pdf_row`): rows go through the same loop as CSV rows, so a PDF
  line's fingerprint, dedupe key, trade and category are built the CSV way. The CSV fingerprints are
  unchanged.
  - `amount = credit − debit − withheld`, so a dividend is **net**, as the CSV records it.
    `withheld`, `cusip`, `section` and `format: "pdf"` go into the line metadata.
  - `WIRE`/`ACH`/`FUNDS` become `Wire In` or `Wire Out`. A `JOURNAL` that costs money or mentions a
    fee becomes `Fee` (the ADR custody fee). `BOUGHT`, `SOLD`, `DIVIDEND` and `INTEREST` pass
    through.
  - A non-trade row that moves no cash (account-type journals, securities received or delivered)
    is `security_only`: a trade line with no trade details, which is informational.
  - Dates are ISO or `MM/DD/YY`, and anything before 2000 is refused (`%m/%d/%Y` would read
    `03/30/23` as the year 23).
- **The statement:** `account_number` stays `firstrade`, so the PDF and CSV land in one account.
  `source_key` is `firstrade:firstrade:<hash>`. The period is the printed one, the opening and
  closing balances are the printed cash, and `parsed.format` is `pdf`.

## Phone (mobile repo)

The Settings exchange-rate list is read-only: `_editRate`, `_RateEditDialog` and the "Manual"
badge are gone. `FxRepository.setRate` and the unused `replaceAll` are removed, and
`upsertRate` always takes the server's value. The `settingsRateEdit`/`settingsRateManual` strings
are removed. The `manual` column stays, always false, so there is no migration.

## Tasks

- [x] F1 `fx` config, `pdftotext_bin` config
- [x] F2 `src/fx.rs`: plan, yearly chunks, fetch, store, loop, `POST /fx-rates/refresh`
- [x] F3 delete `POST /fx-rates` and its tests; add `fx_rates_cannot_be_pushed`
- [x] F4 `firstrade_pdf.rs`: pdftotext, header, activity text, model call, cache, reconcile
- [x] F5 `firstrade.rs`: PDF branch, `pdf_row`, `security_only`/`extra`, printed period and cash
- [x] F6 Nix: `pdftotext_bin` in the module, poppler-utils in the dev shell
- [x] F7 phone: read-only rate list, no local writers
- [ ] F8 a real model reading of the 21 PDFs (blocked on a `model_api` key)

## Verification

- 346 server tests. Fixture: `fixtures/statements/firstrade-2023-03.txt`, an anonymised
  `pdftotext` output. 344 mobile tests, run with nix sqlite on `LD_LIBRARY_PATH`.
- Standalone server on a throwaway config:
  - A live Frankfurter refresh wrote 951 USD→EUR rows (2023-03-01 – 2026-11-19), and a second
    refresh wrote 0. `POST /fx-rates` returned 405.
  - With no key, the March 2023 PDF went to `failed/` with the model-key error.
  - With a hand-written cached reading, it staged: period 2023-03-01 – 2023-03-31, cash
    726.86 → 2350.47, dividends 49.64 and 5.06, interest 0.21, the BABA sale 1568.70, and no
    warning. It imported at each day's rate (for example 0.93598 on 2023-03-01) and the file moved
    to `parsed/`. A re-drop reused the cache and created nothing.

## Deviations from the first draft

- The header (period, cash) moved from the model to Rust, which leaves the model only the rows.
- A re-drop is handled by the reading cache, not a "skip if already staged" lookup.
- The CSV row loop was fed rows rather than factored into a `lower_rows` helper.
- No shared FX upsert helper: the push handler was deleted rather than refactored.
- The brochure and disclosure PDFs turned out to be statements too, so none are expected to fail.

## Known gaps

- **No model key** on the deployed box. Set `model_api` (receipt OCR needs it too).
- Dropping all 21 PDFs is 21 model calls, one per statement.
- A re-drop under a new file name stages nothing, but the copy stays in `staged/`. That is the
  watcher's existing behaviour for every source.
- The model's `symbol` is best effort: from the holdings table, or a ticker it is sure of, or null.
