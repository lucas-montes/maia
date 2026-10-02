# Receipt image → LLM → JSON → phone

## Change summary

Wire the existing half-built receipt pipeline end to end, then fix the desktop receipts view.

The mobile app already captures receipt photos, copies them to `<appDocuments>/receipts/`, writes a `receipts` Drift row, and has screens + l10n for viewing parsed data and creating a draft expense. The server already accepts a multipart upload and writes `receipt_pictures/<id>.jpg`. The seam between them is missing: `remoteReceiptOcrProvider` returns `MockRemoteReceiptOcrService`, and `sync-server` has no `ai` dependency, so no LLM call ever runs.

Make the server run Gemini vision on the uploaded image as a background job, persist the parsed JSON into `receipts.parsed_json`, and have the phone poll for the result. Separately, the desktop receipts view is fully broken against the v30 schema and its archive feature has never worked — align both.

## Success criteria

- [ ] `POST /receipt-pictures/upload` accepts multipart (`picture` binary + optional `receiptId`) and returns `202 {"id","status":"parsing","server_time"}`.
- [ ] Server writes `<db_dir>/receipt_pictures/<id><ext>` and a `receipts` row, then runs Gemini vision in a `tokio::spawn` background job that never holds the DB mutex across an `.await`.
- [ ] `GET /receipt-pictures/:id/parse` returns `pending | parsing | parsed | error` plus the `ParsedReceipt` JSON.
- [ ] `ParsedReceipt` uses `merchant` / `items` (matching the mobile draft builder) with `#[derive(StructuredOutput)]` schema generation.
- [ ] `GET /receipt-pictures/:id/picture` serves the image bytes behind the existing Bearer middleware, guarded by `is_safe_id`.
- [ ] OCR disabled (no `model_api_key`) → both endpoints return `503 {"message":"ocr not configured"}`; everything else keeps working.
- [ ] Mobile swaps `MockRemoteReceiptOcrService` for `HttpRemoteReceiptOcrService` with no change to `BudgetSyncService`'s orchestration or either screen's structure.
- [ ] Phone polls until terminal status, with a 90 s overall budget bounded by `apiTimeoutSeconds` per request.
- [ ] Desktop receipts view renders the v30 columns; `selectReceipt` receives the uuid as a string (no `Number()` coercion).
- [ ] Desktop archive moves the image to `<archived_dir>/<id>/receipt.<ext>` beside a `receipt.json` sidecar; restore is the exact inverse.
- [ ] `cargo check --workspace`, `cargo test --workspace`, `pnpm build`, `flutter gen-l10n`, `flutter analyze`, `flutter test` all pass.

## Constraints and non-goals

**Constraints:**
- Stack unchanged: `rusqlite 0.37 bundled` + `axum 0.7` + `tokio` + `serde`/`serde_json` + `uuid v7`. LLM access via the existing `ai` crate (Gemini). No `sqlx`.
- Timestamps epoch ms `int64`. `receipts.remote_path` becomes relative (`receipt_pictures/<id><ext>`) so no server filesystem path ships to the phone.
- Auth: existing Bearer middleware only. New endpoints inherit it; no new bypass except what `is_public_media` already allows.
- `Config.model_api_key` is a plain `String` (`""` = not configured), mirroring the existing `Config.api_key`. The `Option` lives on `AppState.ocr` as the runtime "is OCR available" flag.
- Mobile: `uploadStatus` is a plain `IntColumn` with no CHECK constraint, so new status codes need no Drift migration.

**Non-goals:**
- No realtime push (WebSocket / SSE / FCM). The existing manual `?since=` pull stays the only cross-device channel.
- No syncing archive state to the phone — the viewer renders from `localPath`, so moving the server-side file needs zero mobile changes.
- Receipt line items stay parsed-and-displayed; they do not become real `transactions` or `meal_ingredients`.
- No filling in the empty utoipa spec (`openapi.rs:7` is bare `paths()` with zero annotations in the codebase). `mobile/docs/api/openapi.yaml` is the maintained contract.
- No reviving `shared/src/purchases` (see Assumptions).
- No image resizing/compression; the 20 MB body limit covers phone photos.

## Assumptions

- **`shared/src/purchases` is dead and unusable.** `shared/src/lib.rs:8` declares `mod purchases;` — private, so `sync-server` cannot name `schema::Receipt`. And `shared/src/purchases/models.rs:32-38` derives `Crud` onto `receipts` with columns `id: Option<i64>, store, date, total, currency`, while the real table (`sync-server/src/db.rs:337-348`) is `id TEXT, local_path, remote_path, upload_status, parsed, parsed_json, transaction_id, created_at, updated_at, deleted_at`. `create()` would fail at runtime. The OCR schema is therefore **redefined inside `sync-server`**; `shared` keeps its copy untouched for the deprecated daemon. This also keeps `shared`'s `rust_decimal`/`rand`/`maia-macros` tree out of `sync-server`'s dependency graph.
- `ai-derive` is behind a `derive` feature that is in `default` (`ai/Cargo.toml:13-16`), so `ai = { path = "../ai" }` needs no feature plumbing.
- **A model-key settings field already exists and is write-only.** `CONFIG_FIELD_DEFS` (`maia-ui/src/main.ts:199`) defines `{ key: "model_api", label: "Model API Key" }`, round-tripped to maia.json as a flat `"model_api": "<string>"`. Nothing in Rust reads it. We read that flat key rather than the daemon's `models_api: Vec<ModelApi>` shape (`src/config.rs:29-32`), because `buildConfigJson` (`main.ts:233-252`) structurally cannot emit an array — it only produces flat keys and `parts.length === 2` nested objects.
- **`buildConfigJson` wipes unknown maia.json keys on save** — it rebuilds the file from only the keys in `CONFIG_FIELD_DEFS`. That is why `"models_api": []` is all that survives today. Reading the flat `model_api` needs no `CONFIG_FIELD_DEFS` change.
- **`DefaultBodyLimit` is a live bug.** `handlers/receipts.rs:40` takes `body: Bytes`, which axum 0.7 caps at 2 MB. Phone receipt photos routinely exceed that. T05 fixes it.
- **The Sync hub has never uploaded a receipt image.** `data_push_service.dart:191` reads `data['picturePath']` but Drift emits `localPath`, so the multipart branch never fires.
- **Archiving won't break the phone.** The viewer renders from `localPath` (`receipt_viewer_screen.dart:56`); `remotePath` is only displayed as text (`_StatusRow`, :228-240).
- **The whole desktop receipts cluster is broken, not just `read_receipt`.** `list_receipts` (`lib.rs:368-373`) is already correct; everything downstream reads columns that no longer exist. `renderReceiptList` reads `merchant`/`date`/`total`/`status`/`current_path` — none of which `list_receipts` returns, so rows render blank. `selectReceipt(Number(uuid))` at `main.ts:1219` yields `NaN`, so the detail panel never opens. `archive_receipt` is registered (`lib.rs:1167`) but never invoked from TS, and its `UPDATE` writes `archived_path = current_path` (the source path) while putting the destination in `current_path` — inverted.
- The embedded sync server binds `0.0.0.0` on the LAN (`server.rs:13`), so the phone on the same Wi-Fi can reach the new endpoints. `android/app/src/main/AndroidManifest.xml` already sets `usesCleartextTraffic="true"`.

## Decisions

| Decision | Choice |
|---|---|
| OCR timing | Background job (`tokio::spawn`) + phone polling |
| Delivery | HTTP polling response + existing `GET /receipt-pictures?since=` cursor |
| Gemini key | Flat `model_api` in maia.json → `Config.model_api_key: String`; `AppState.ocr: Option<Arc<Model>>` |
| JSON contract | Rename Rust schema to match mobile: `store`→`merchant`, `products`→`items` |
| Archive layout | Per-receipt folder `<archived_dir>/<id>/` holding `receipt.<ext>` + `receipt.json` |
| Archive reversibility | Yes — `restore_receipt` moves it back and clears the archive fields |
| Archive before parse | Refused; button gated until `parsed = 1` |
| Archive dir | Keep `get_archive_dir()` reading `receipts.archived_dir`, fallback `./receipts_archive` |

## API contract

```
POST /receipt-pictures/upload    multipart: receiptId (text, optional → uuid v7), picture (binary)
  202 → { id, status: "parsing", server_time }      503 → { message: "ocr not configured" }
GET  /receipt-pictures/:id/parse
  200 → { id, status: pending|parsing|parsed|error, parsed: ParsedReceipt|null, error, server_time }
GET  /receipt-pictures/:id/picture   → image bytes (reuse is_safe_id from media.rs:67)
```

```jsonc
// ParsedReceipt — shared/src/purchases/schema.rs, renamed for mobile
{ "merchant", "date": "2026-08-15 14:30:00", "total", "currency",
  "items": [{name, price, currency}],   // was: products
  "discounts": [{description, amount}] }
```

## Archive layout

```
<archived_dir>/<id>/receipt.jpg   # the image (moved, not copied)
<archived_dir>/<id>/receipt.json  # { id, archivedAt, source, uploadStatus, parsed,
                                   #   transactionId, parsedJson: {…} }
```

The id-keyed folder removes the ad-hoc filename dedup at `lib.rs:512-528`. `receipts` gains `archived_path TEXT` + `archived_at INTEGER`; **`remote_path` is left untouched**, so the pre-move location stays known and restore is a move back. Archived-ness is derived from `archived_at IS NOT NULL` — deliberately *not* folded into `upload_status`, which is the phone's 0–3 lifecycle code and orthogonal to archiving.

## Task stack

Ordered T01→T02→T03→T04→T05. T06–T09 and all M-tasks are independent of each other.

- [x] T01: Make the `ai` crate callable from a server (status:done)
  - Task ID: T01
  - Completed: 2026-10-02
  - Goal: Remove the debug `assert_eq!` that panics on any model override, make the model name configurable, add a client timeout, and add a bytes-based `extract_image_bytes` so a server handler can reuse already-held image bytes instead of re-reading from disk.
  - Boundaries (in/out of scope): In - `ai/src/google.rs` (delete `assert_eq!` at :140-143; `Model::new(api_key, name, base_url)` with trailing-slash trim; `reqwest::Client::builder().timeout(90s)`), `ai/src/model.rs` (new `Vllm::extract_image_bytes(&self, bytes, mime)`, reimplement `extract_image(&Path)` on top of it, drop the blocking `std::fs::read` from the async body, parameterize the hardcoded `"image/jpeg"` MIME at :71 and :113). Out - prompt/template changes, streaming, multi-image batching, non-Gemini providers.
  - Done when: `cargo check -p ai` passes and `ai::Model::new` accepts a custom model name and base URL.
  - Verification notes: `cargo check -p ai`; `cargo test -p ai-derive`

- [x] T02: Model key in sync-server config and state (status:done)
  - Task ID: T02
  - Completed: 2026-10-02
  - Goal: Give `sync-server` a way to reach Gemini, sourced from the flat `model_api` key that the desktop settings UI already writes.
  - Boundaries (in/out of scope): In - `sync-server/Cargo.toml` (add `ai = { path = "../ai" }`), `sync-server/src/config.rs` (`Config.model_api_key: String`, `Config.model_name: String`, `with_model()` builder beside `with_bind_addr`, `Default` → empty key), `sync-server/src/server.rs` (`AppState.ocr: Option<Arc<ai::Model>>`, built in `start_server` :112-145 and the test helper :147-172). Out - the Tauri plumbing (T08), the schema (T03).
  - Done when: `cargo check -p sync-server` passes and `start_server` populates `state.ocr` from a non-empty key, leaving it `None` for an empty one.
  - Verification notes: `cargo check -p sync-server`; `cargo test -p sync-server`

- [x] T03: OCR output schema inside sync-server (status:done)
  - Task ID: T03
  - Completed: 2026-10-02
  - Goal: Define the Gemini structured-output schema that matches what the mobile draft builder already reads, rather than reusing the dead `shared` copy.
  - Boundaries (in/out of scope): In - new `sync-server/src/receipt_ocr.rs` with `ParsedReceipt { merchant, date, total, currency, items, discounts }` deriving `StructuredOutput` + `Serialize`/`Deserialize`, copying the `#[description]` and `#[gemini(format = "date-time")]` attributes from `shared/src/purchases/schema.rs:6-52` with renamed fields; a `#[cfg(test)]` module porting the assertions from `schema.rs:54-185`. Out - any edit to `shared/src/purchases/*`.
  - Done when: the schema test passes and asserts `merchant`/`items` with the expected `required` and `propertyOrdering`.
  - Verification notes: `cargo test -p sync-server receipt_ocr`

- [x] T04: Guarded receipts table migrations (status:done)
  - Task ID: T04
  - Completed: 2026-10-02
  - Goal: Add the three new `receipts` columns to both fresh and pre-existing databases.
  - Boundaries (in/out of scope): In - `sync-server/src/db.rs`: one helper reading `PRAGMA table_info(receipts)` and `ALTER TABLE ... ADD COLUMN` for each missing column — `parse_error TEXT` (T05), `archived_path TEXT` and `archived_at INTEGER` (T09) — placed after the `CREATE TABLE IF NOT EXISTS receipts` block at :337-348. Out - new tables, other columns.
  - Done when: running against a pre-existing DB adds the three columns and re-running is a no-op.
  - Verification notes: `cargo test -p sync-server migrations_idempotent`

- [x] T05: OCR endpoints and background job (status:done)
  - Task ID: T05
  - Completed: 2026-10-02
  - Goal: Receive the uploaded image, kick off vision parsing in the background, and expose the result.
  - Boundaries (in/out of scope): In - new `sync-server/src/handlers/receipt_ocr.rs` (`upload_receipt_picture(Multipart)` → 202 + `tokio::spawn(run_ocr)`; `run_ocr` writing `parsed`/`parsed_json`/`parse_error`; `get_receipt_parse(Path(id))`; `get_receipt_picture(Path(id))` reusing `is_safe_id` from `handlers/media.rs:67`), `sync-server/src/handlers/mod.rs`, `sync-server/src/server.rs:29-61` (three routes, with `.layer(DefaultBodyLimit::max(20 * 1024 * 1024))` on the upload route). Out - rewriting the hand-rolled parser in `handlers/receipts.rs` (still used by bulk sync; `multer 3.1.0` is already in `Cargo.lock`), utoipa annotations.
  - Done when: uploading a JPEG returns 202, the file and row exist, and the parse endpoint reaches a terminal status. The DB mutex guard is never held across an `.await`.
  - Verification notes: `cargo test -p sync-server receipt_ocr`

- [x] T06: Fix path leak and dead code in the legacy handler (status:done)
  - Task ID: T06
  - Completed: 2026-10-02
  - Goal: Stop shipping absolute server filesystem paths to the phone, and confirm a phone push does not clobber archive state.
  - Boundaries (in/out of scope): In - `sync-server/src/handlers/receipts.rs`: :78 stores `path.to_string_lossy()` (absolute) into `remote_path` which `pull_receipts` (`pull.rs:429-435`) ships to the phone → store the relative form; delete the no-op `if part.windows(...).any(|_| false)` at :122; add a test asserting `archived_at` survives the `ON CONFLICT DO UPDATE` push path. Out - restructuring the parser.
  - Done when: `remote_path` is relative and a receipt push preserves `archived_path`/`archived_at`.
  - Verification notes: `cargo test -p sync-server receipt`

- [x] T07: Server tests for the OCR pipeline (status:done)
  - Task ID: T07
  - Completed: 2026-10-02
  - Goal: Cover the new endpoints in the existing real-TCP test style.
  - Boundaries (in/out of scope): In — `sync-server/src/lib.rs`, using `start_test_server_with_db` (`server.rs:147-172`): 401 without auth; 503 without a model key; 202 writes row + file; parse reports status; push preserves `archived_at`; `#[ignore]`d `receipt_ocr_live_gemini` gated on `MAIA_TEST_GEMINI_KEY`. Out - snapshot/visual tests, load testing.
  - Done when: all non-live receipt tests pass under `cargo test --workspace`.
  - Verification notes: `cargo test --workspace`

- [x] T08: maia-ui model key plumbing (status:done)
  - Task ID: T08
  - Completed: 2026-10-02
  - Goal: Feed the already-collected `model_api` setting into the embedded sync server.
  - Boundaries (in/out of scope): In — `maia-ui/src-tauri/src/lib.rs`: `get_model_api_key_value()` reading the flat `model_api` key from maia.json mirroring `get_sync_api_key_value` (:163-179), optionally resolving a leading `$` from the env to match the existing `"e.g. $GEMINI_API"` placeholder; pass `.with_model(key, name)` at :1129-1152. Out - changing `CONFIG_FIELD_DEFS`, reviving the daemon's `models_api` array.
  - Done when: `pnpm build` passes and a key entered in the desktop settings reaches `sync_server::Config`.
  - Verification notes: `cd server/maia-ui && pnpm build`

- [x] T09: Fix the desktop receipts view and resurrect archiving (status:done)
  - Task ID: T09
  - Completed: 2026-10-02
  - Goal: Make the receipts view work against the v30 schema, and rebuild the archive feature on top of it so the picture and its parsed JSON stay together.
  - Boundaries (in/out of scope): **9a** — `maia-ui/src-tauri/src/lib.rs`: replace `struct Receipt` (:46-62) with the v30 shape (`id: String`, `local_path`, `remote_path`, `upload_status`, `parsed`, `parsed_json`, `transaction_id`, `created_at`, `updated_at`, `archived_path`, `archived_at`) with `#[serde(rename_all = "camelCase")]`; make `list_receipts` return `Vec<Receipt>`; `read_receipt(state, id: String)` with v30 SQL + `AND deleted_at IS NULL`. `maia-ui/src/main.ts`: `interface Receipt` (:22-38) to the v30 shape; `ReceiptsState.selectedId` (:94) to `string | null`; `selectReceipt(id: string)`; delete the `Number()` coercion at :1219; add an epoch-ms formatter (`formatDate` :1611 appends `"Z"` and expects ISO); rewrite `renderReceiptList` (:1705-1730) and `renderReceiptDetail` (:1732+) from v30 fields with a *derived* display status (`archived_at` → archived, else `parsed`, else the `upload_status` code) and `JSON.parse` of the `parsed_json` string. **9b** — keep `get_archive_dir()` (:438-455) as-is; `archive_receipt(state, id: String) -> CommandResult<Receipt>` resolving the source via `get_database_path().parent().join(remote_path)`, refusing unless `parsed = 1`, moving to `<archived_dir>/<id>/receipt.<ext>`, writing the `receipt.json` sidecar, setting `archived_path`/`archived_at` and leaving `remote_path` untouched; `restore_receipt(state, id: String)` as the inverse; `read_receipt_picture(id) -> Vec<u8>` checking `archived_path` then `db_dir.join(remote_path)`; Archive/Restore buttons via the existing `data-action='…'` delegation (:1224 pattern). Out — deleting `archive_receipt`, changing `receipts.archived_dir` resolution, serving images over HTTP to the desktop.
  - Done when: the list shows real rows, clicking one opens the detail panel, archive produces `<archived_dir>/<id>/{receipt.<ext>,receipt.json}`, and restore reverses it exactly.
  - Verification notes: `cd server/maia-ui && pnpm build` (runs `tsc`, so the TS type changes are compiler-checked); then exercise archive → inspect folder → restore. Splits cleanly into two commits (9a, 9b).

- [x] M01: ApiClient.postMultipart must send filenames (status:done)
  - Task ID: M01
  - Completed: 2026-10-02
  - Goal: Send proper file parts so the server's `axum::extract::Multipart` extractor sees filenames and content types.
  - Boundaries (in/out of scope): In — `mobile/lib/src/network/api_client.dart`: change the `files` parameter from `Map<String, Uint8List>` to a small record carrying bytes + filename + content type; update `MockApiClient.postMultipart` (:286) and both call sites (`sync/data_push_service.dart:93-129` noteAudio, `:181-217` receipts). Out - changing the `ApiClient` interface beyond that parameter, adding a dio/retrofit stack.
  - Done when: multipart parts carry `filename=` and a per-part content type, and all `flutter analyze`/`flutter test` pass.
  - Verification notes: `cd mobile && flutter analyze && flutter test`

- [x] M02: New parsing receipt status (status:done)
  - Task ID: M02
  - Completed: 2026-10-02
  - Goal: Represent the 2–10 s window where a receipt exists but its parse has not landed, so the viewer stops offering a re-upload of a running job.
  - Boundaries (in/out of scope): In — `mobile/lib/src/budget/models/receipt.dart:5-24`: add `parsing` with code **4** (existing 0–3 unchanged) and update `fromCode`. Out — a Drift migration (none needed: `uploadStatus` is a plain `IntColumn` at `tables.dart:648-666` with no CHECK constraint).
  - Done when: `ReceiptStatus.fromCode(4) == ReceiptStatus.parsing` and 0–3 still round-trip.
  - Verification notes: `cd mobile && flutter test`

- [x] M03: HttpRemoteReceiptOcrService (status:done)
  - Task ID: M03
  - Completed: 2026-10-02
  - Goal: Replace the mock with a real HTTP implementation behind the existing abstraction, without touching the orchestration or the screens.
  - Boundaries (in/out of scope): In — **change the seam first**: `RemoteReceiptOcrService.upload` from `upload(String localPath)` to `upload(String receiptId, String localPath)` (`remote_receipt_ocr.dart:19-25`, one call site at `budget_sync.dart:44`) so the uuid stays stable across phone and server, which the `?since=` pull depends on. New `mobile/lib/src/budget/services/http_remote_receipt_ocr.dart` with `upload` (`postMultipart('$endpoint/upload', fields: {'receiptId': …}, files: {'picture': (bytes, filename, contentType)})` → `ReceiptUploadResult(response['id'])`) and `fetchParse` (`getJson`, poll every 2 s up to 90 s, per-request bounded by `apiTimeoutSeconds`, throw a typed error on `status == 'error'` or timeout). `mobile/lib/src/budget/providers/services.dart:8-10` builds it from `remoteSyncBaseUrl` + `remoteSyncApiKey`, deriving sub-paths from `settings.endpointReceiptPictures` (`settings.dart:172`). Update the stale doc comment at `remote_receipt_ocr.dart:16-18`. Out — using `apiClientProvider` (it reads a compile-time `API_BASE_URL` at `api_client.dart:296-302`; follow the `sync_service.dart:55-57` pattern instead), deleting the mock (keep it for tests/offline dev).
  - Done when: `flutter analyze` passes and the mock is no longer wired into `remoteReceiptOcrProvider`.
  - Verification notes: `cd mobile && flutter analyze`

- [x] M04: Viewer UI handles the parsing state (status:done)
  - Task ID: M04
  - Completed: 2026-10-02
  - Goal: Show progress instead of an Upload button while the OCR job is running.
  - Boundaries (in/out of scope): In — `mobile/lib/src/budget/screens/receipt_viewer_screen.dart:66-73`: gate the Upload button on `status != ReceiptStatus.parsing` and render a progress indicator while parsing; add a `receiptParsing` key to `lib/l10n/app_{en,fr,es}.arb`. Out — any other screen's layout.
  - Done when: `flutter gen-l10n` succeeds for all three locales and `flutter analyze` is clean.
  - Verification notes: `cd mobile && flutter gen-l10n && flutter analyze`

- [x] M05: Retry and resume sweep for stuck receipts (status:done)
  - Task ID: M05
  - Completed: 2026-10-02
  - Goal: Stop stranding receipts whose upload died mid-flight, which `receipt_list_screen.dart:_capture` currently does with a bare `try {} catch (_) {}` around a detached `uploadAndParse`.
  - Boundaries (in/out of scope): In — `mobile/lib/src/budget/providers/receipts.dart`: `receiptPendingProvider` for receipts with `uploadStatus IN (uploading, parsing, error)` and `parsed == false`. `mobile/lib/src/budget/providers/services.dart`: a resume entry point guarded by an in-flight `Set<String>` so the list and viewer sweeps cannot race the same id. Trigger from `ReceiptListScreen`'s `initState` and when the viewer loads a non-parsed receipt. Out — app-startup sweeps, background isolate workers.
  - Done when: a receipt left in `error` is automatically retried when the list is opened.
  - Verification notes: `cd mobile && flutter test`

- [x] M06: Fix the two dead bugs in data_push_service (status:done)
  - Task ID: M06
  - Completed: 2026-10-02
  - Goal: Make the Sync hub's receipt push actually send image bytes, under a consistent field name.
  - Boundaries (in/out of scope): In — `mobile/lib/src/sync/data_push_service.dart`: :191 reads `data['picturePath']` but Drift emits `localPath` (`tables.dart:649`), so the multipart branch never executes and every push silently falls through to JSON → fix the key; :200 uses field name `'receipt'` (singular) while the JSON fallback uses `'receipts'` → unify, and give the part a filename now that M01 supports it. Out — restructuring the push service, adding new entity types.
  - Done when: the multipart branch is exercised by a test and the field name matches the server's `parse_multipart` expectations.
  - Verification notes: `cd mobile && flutter analyze && flutter test`

- [x] M07: Mobile tests for the receipt pipeline (status:done)
  - Task ID: M07
  - Completed: 2026-10-02
  - Goal: Cover the OCR client and the orchestration, neither of which has any test today.
  - Boundaries (in/out of scope): In — `mobile/test/receipt_ocr_service_test.dart` (script both endpoints via `MockApiClient(onRequest: …)`; assert multipart field name + filename, `merchant`/`items` round-trip, poll-until-parsed, `status: error` surfaces the message, timeout path); `mobile/test/budget_sync_test.dart` (in-memory Drift via `AppDatabase.forTesting(NativeDatabase.memory())` as in `test/note_repository_test.dart:1-18`; assert the `local→uploading→uploaded→parsed` chain, `parsedJson` contents, and that the draft gets `category == merchant`, `isDraft == true`, `receiptId` set, `accountId == null`); `mobile/test/receipt_status_test.dart` (`fromCode` for 0–4). Follow the scripted-HTTP idiom from `test/catalog_selective_sync_test.dart:18-81`. Out — golden/widget tests, integration_test.
  - Done when: `flutter test` passes with the new files green.
  - Verification notes: `cd mobile && flutter test`

- [x] M08: Update the contract and context docs (status:done)
  - Task ID: M08
  - Completed: 2026-10-02
  - Goal: Keep the maintained contract and the AI memory files accurate.
  - Boundaries (in/out of scope): In — `mobile/docs/api/openapi.yaml`: add the three endpoints and the `ParsedReceipt` schema next to the existing `/receipt-pictures` at :341-384. `mobile/context/network/network.md:53`: it currently states "`remoteReceiptOcrProvider` is still a mock (unchanged)". `server/context/overview.md`: changelog entry. `server/context/plans/`: note that `shared/src/purchases` is dead code superseded by `sync-server/src/receipt_ocr.rs`, plus the archive folder layout. Out - regenerating the Rust utoipa spec.
  - Done when: every path and field named in the docs matches the code.
  - Verification notes: cross-check each referenced path and symbol against the source

## Files changed

**server/**
- `ai/src/google.rs`, `ai/src/model.rs`
- `sync-server/Cargo.toml`
- `sync-server/src/config.rs`, `server.rs`, `db.rs`
- `sync-server/src/receipt_ocr.rs` *(new)*
- `sync-server/src/handlers/receipt_ocr.rs` *(new)*, `handlers/mod.rs`, `handlers/receipts.rs`
- `sync-server/src/lib.rs` (tests)
- `maia-ui/src-tauri/src/lib.rs`, `maia-ui/src/main.ts`
- `context/overview.md`, `context/plans/`

**mobile/**
- `lib/src/network/api_client.dart`
- `lib/src/budget/models/receipt.dart`
- `lib/src/budget/services/remote_receipt_ocr.dart`, `http_remote_receipt_ocr.dart` *(new)*
- `lib/src/budget/services/budget_sync.dart`
- `lib/src/budget/providers/services.dart`, `receipts.dart`
- `lib/src/budget/screens/receipt_viewer_screen.dart`, `receipt_list_screen.dart`
- `lib/src/sync/data_push_service.dart`
- `lib/l10n/app_{en,fr,es}.arb`
- `test/receipt_ocr_service_test.dart`, `budget_sync_test.dart`, `receipt_status_test.dart` *(new)*
- `docs/api/openapi.yaml`, `context/network/network.md`

## Verification summary

```bash
cd server        && cargo check --workspace && cargo test --workspace
cd server/maia-ui && pnpm build
cd mobile        && flutter gen-l10n && flutter analyze && flutter test
```

Manual end-to-end: run the desktop app, enter a Gemini key in settings, pair the phone, capture a receipt photo, and watch `202 → parsing → parsed`; confirm the draft expense appears; then archive the receipt from the desktop and inspect `<archived_dir>/<id>/`.

## Notes and risks

- **No Gemini key exists anywhere in the repo.** T08's read of the existing `model_api` field is the first path to one. Everything through T07 is testable without a key; only the `#[ignore]`d live test and manual end-to-end verification need it.
- **M06 will make the Sync-hub push send real image bytes for the first time.** Watch payload sizes; the 20 MB limit from T05 applies there too.
- **Archiving moves a file the HTTP layer serves.** After archiving, `GET /receipt-pictures/:id/picture` 404s. Acceptable — the desktop reads from the archive dir — but if that endpoint should follow archives, it is a two-line change in T05.
- **`pnpm build` runs `tsc`**, so the T09 TypeScript type changes are compiler-checked rather than string edits.
- **T09 is the largest single task** and splits cleanly into two commits (9a schema alignment, 9b archiving).
