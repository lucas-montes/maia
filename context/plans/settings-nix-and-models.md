# Settings through Nix, secrets in the overlay, and several models per task

**Status:** done (marked 2026-11-22), committed (`0a5d67f`, revised in `2c4080d`) and being
tested. Every test suite passed at implementation: 355 server, 30 desktop, 3 `ai`, 81 `maia-ui`
vitest, and the real server was checked end to end against a stub OpenAI-compatible endpoint. The
checks against real Gemini, OpenRouter and Ollama models and the `~/.dotfiles` rollout (see
**Rollout**) are what is being tested now.

> **Revised 2026-11-22:** the editable Nix settings file is gone. The user asked to drop the idea
> of overriding the config from the app: `services.maia.settingsFile`, `save_nix_settings`, the
> pending-rebuild badges and the rebuild banner were removed. Settings shows every non-secret
> value read-only, as running, and saves only secrets. The sections below that describe editing
> the settings file are history.

> The user asked for a rework of the desktop Settings page. The decisions of 2026-11-19/20 come
> from their answers:
> - **One scrolling page, split into sections**, with a side nav and **one Save per section**.
> - Deployment-managed values are **shown normally and editable**. There was no "managed,
>   read-only" tier any more.
> - **Everything goes through Nix except the tokens.** A mutable runtime config file was proposed
>   and rejected. Settings edits *the Nix input*: a JSON file the host's flake imports.
> - After a save, the app **writes the file and shows the rebuild command**. It does not run
>   `nixos-rebuild` and does not `git commit`.
> - **Every token lives in the per-user overlay**: the sync API key, model keys and Trading 212
>   credentials. None of them go through Nix.
> - **Every path option has a default.** The split is systemd for service-only state and XDG for
>   files the user touches. The XDG homes are **module options with XDG defaults**, because Nix
>   cannot read the environment at evaluation time.
> - **Several models can be saved.** Each has a name, provider, URL, model id and key. Ollama and
>   OpenRouter must work, which means the OpenAI-compatible protocol.
> - **Each task has its own ordered fallback list**, and the review screens **show which model
>   produced each result**.

## Why

- The old page was a flat list of 11 inputs. 7 were greyed "managed" fields. 3 were dead:
  `receipts_path`, `nutriments_path` and `receipts.archived_dir` are read by nothing. Saving
  happened in two places (config and Trading 212).
- Anything that mattered (port, paths, models) could only change through Nix. Meanwhile the sync
  key and the model key went through Nix into the world-readable `/etc/maia/config.json` and the
  store.
- There was exactly one Gemini key and one model name (`model_api`/`model_name`), shared by OCR,
  matching and the Firstrade PDF reader.

## Where each value lives

| Kind | Store | Written by | Applies |
|---|---|---|---|
| Non-secret settings (port, firewall, currency, paths, Trading 212 enable/env/lookback, models, tasks) | `services.maia.settingsFile`: a JSON file in the host flake, imported into `services.maia` | Settings → `save_nix_settings` | after `nixos-rebuild switch` |
| Secrets (sync key, `models.<id>.api_key`, Trading 212 key/secret) | the overlay `${xdg.configHome}/maia/config.json` (mode 0600) | Settings → `save_secrets` | after `systemctl restart maia` and reopening the app (`Config::shared` is a `OnceLock`) |
| Everything rendered | `/etc/maia/config.json` | the module | — |

- **The overlay holds only secrets now.** Intake dir and Trading 212 enable/env/lookback used to
  be overlay-first. They are deployment-only: `Config::from_values` ignores them in the overlay.
- **The overlay's path is in the deployment file** (`overlay_file`), so the service uses the
  module's `xdg.configHome` rather than guessing from `HOME`. `overlay_config_path()` is the
  fallback for a checkout.
- **The sync key is read overlay-first, then the deployment `api_key`, then `fitfat-sync-key`.**
  The deployment fallback exists only to survive the upgrade. The module no longer renders the
  key.
- **The settings file has no default.** A pure flake can read only files tracked inside itself, so
  the path is necessarily host-specific. While it is null, the Nix fields in Settings are
  read-only and secrets can still be changed. Settings warns when the file is not tracked by git.

## Path layout (decision: systemd for app state, XDG for user files)

| Path | Default | Why |
|---|---|---|
| `dataDir` | `/var/lib/maia` (StateDirectory) | Only the service and the app (same uid) touch it |
| `mediaDir` | `${dataDir}/exercise_media` | Read-only to the service, so it may point into a checkout (luctop does) |
| `notesDir`, `archiveDir` | under `dataDir` | Service state |
| `statementsRoot` | `${xdg.dataHome}/maia/statements` | The user's files |
| `statementsIntakeDir` | `${statementsRoot}/intake` | A dedicated folder, so draining it is safe by default. It can still be `~/Downloads` |
| overlay | `${xdg.configHome}/maia/config.json` | Secrets |
| `seedDir` | null | Seeding is opt-in. It copies GBs of media and outruns the start timeout |

- `xdg.dataHome` and `xdg.configHome` default to `${home}/.local/share` and `${home}/.config`.
- A `dataDir` outside `/var/lib` gets no StateDirectory. tmpfiles creates it and `ReadWritePaths`
  grants it.
- Every writable directory outside `dataDir` gets a tmpfiles rule plus a `ReadWritePaths` entry.
  That covers the overlay dir, the statement roots, intake, and notes or archive when moved.
- **`dataDir` is not editable from Settings.** It holds the database, so moving it means moving
  the database by hand.
- **Removed options error out** (`mkRemovedOptionModule`) with a pointer to the replacement:
  - `apiKey`, `modelApiKey`: Settings.
  - `modelName`: `models` + `tasks`.
  - `sourceDir`: `mediaDir`.

## Models (`ai` crate)

- **Providers:** `ai::Provider::{Gemini, OpenAi}`.
  - `OpenAi` is any `POST {base}/chat/completions` endpoint: OpenAI, OpenRouter, Ollama,
    llama.cpp, vLLM (`ai/src/openai.rs`).
  - Images go as `image_url` data URIs.
  - Structured output asks for `response_format: json_schema` (strict false). On a 400 it retries
    once with `json_object` and the schema in the system prompt, which covers servers without
    schema support.
  - An answer wrapped in a Markdown fence is accepted.
  - An empty key sends no `Authorization` header (local Ollama).
- **Schema dialect:** the derive macro emits Gemini's `responseSchema`. It already writes
  nullability as `["T","null"]`, so `ai::schema::to_json_schema` only strips `propertyOrdering`.
- **`ModelSpec`** is the config entry: `id`, `provider`, `base_url`, `model`, `api_key`, `vision`.
  `Model::from_spec` builds the client. The old `Model::with_base_url` constructor is kept for
  tests.
- **`ModelChain`** tries each model in order and returns `Generated { value, model: ModelRef,
  attempts }`.
  - **Every failure falls through:** transport error, any non-2xx, an empty answer, or one that
    does not fit the schema. Each of those is specific to the model that gave it.
  - `ChainError::AllFailed` names every attempt. A retry inside one model is not a fallback.
  - `ModelRef` displays as `"<id> (<provider model>)"`. That string is what gets recorded.
- **Tasks:**

  | Task | Used by | Rule |
  |---|---|---|
  | `receipt_ocr` | `handlers/receipt_ocr.rs` | Vision models only; others are skipped |
  | `receipt_matching` | `handlers/matching.rs::assist` | — |
  | `statement_extraction` | `statements/firstrade_pdf.rs` | — |

  - `AppState` holds one `Option<Arc<ModelChain>>` per task: `ocr`, `matching` and
    `statement_extraction`. An empty chain means the task is off, answered with 503 or a config
    error.
  - Config: deployment `models.<id>.{provider,base_url,model,vision}` and `tasks.<task>: [ids]`.
    Keys come from overlay `models.<id>.api_key`.
  - Unknown ids, unknown providers and non-vision OCR entries are skipped with a warning. The
    module asserts both conditions so a rebuild fails, and so does `save_nix_settings`.
  - **Legacy fallback:** a deployment with no `models` but `model_api`/`model_name` becomes one
    Gemini model, `default`, on all three tasks.

## Provenance (which model produced what)

- **Migration v18** (`add_model_provenance`) adds `receipts.parsed_model` and
  `receipt_effects.model`. Both are TEXT and server-only.
- **Receipts:** `run_ocr` writes `parsed_model`. `GET /receipt-pictures/:id/parse` returns it as
  `model`, and `GET /receipt-pictures/:id/matching` as `readBy`.
- **AI assists:** `POST …/assist` returns `model` and `attempts` with its proposals. The accept
  endpoints (`PUT …/store`, `POST …/store`, `PUT …/lines/:line`, `PUT …/transaction`) take an
  optional `model`. It is kept only with `source: "ai"`.
- **How an accept gets its model:** the model is known only at the endpoint, not in the matchers
  that write effects. `effects::tag_model` stamps it afterwards, in the same transaction, on this
  receipt's `source = 'ai'` effects created at this `now` with no model yet. That avoided
  threading a parameter through every `NewEffect` writer.
- **Firstrade PDFs:** `PdfExtract.extracted_by` is cached in `<hash>.extract.json` and surfaces
  as `parsed.extractedBy` on the statement.
- **Desktop:**
  - The receipt page shows "Read by …", "AI proposals from … after X failed", and the model on
    each AI effect.
  - The statement review shows "Read by …".

## Desktop

- **`maia-ui/src-tauri/src/settings.rs`** replaces `read_config` / `save_config` /
  `DEPLOYMENT_OWNED`:
  - `read_settings` returns:
    - `effective`: `/etc/maia/config.json` read fresh, not the cached config, in the module's
      camelCase vocabulary.
    - `pending`: the settings file.
    - `rebuildCommand`: `sudo nixos-rebuild switch --flake <git toplevel>#<nixos_host>`.
    - `settingsFileTracked`.
    - `secrets`: booleans only.
  - `save_nix_settings(patch)`:
    - Validates like the module types.
    - `null` removes a key, which brings the module default back.
    - `trading212` and `tasks` merge key by key; `models` is replaced whole so a deletion sticks.
    - Checks task references, then writes atomically.
  - `save_secrets(patch)`: absent keeps, `null` clears, blank keeps. Uses
    `maia::config::update_overlay`, which moved from the server's statements handler.
  - `test_model(probe)`: one tiny structured request against the form's values, which need not be
    rebuilt yet.
- **Removed:**
  - The server route `PUT /statement-feeds/settings`. Only the desktop called it; `GET` stays.
  - The Tauri commands `get_statement_feed_settings` / `save_statement_feed_settings`.
- **`views/settings.ts`** has six sections: Server, Models, Tasks, Statements, Trading 212,
  Storage.
  - Each field shows a Nix or secret chip, and "pending rebuild · live: X" when the file differs
    from what is running.
  - A page banner shows the rebuild command and the "not tracked by git" warning. After a secret
    save it shows the restart reminder.
  - Removing a model also removes it from the tasks in the same save.
- **Module rendering:** `open_firewall`, `nix_settings_file` and `nixos_host` are rendered for
  Settings only. The server ignores them.

## Rollout (order matters)

1. Commit and push maia.
2. **Before rebuilding**, put the sync key in the overlay: save it in Settings → Server, or copy
   the current `api_key` from `/etc/maia/config.json` into `~/.config/maia/config.json` so the
   phone keeps working. Otherwise the new module renders no `api_key`, the server falls back to
   `fitfat-sync-key`, and the phone is locked out.
3. In `~/.dotfiles/services/maia.nix`, drop `apiKey` and `sourceDir` and set `mediaDir` to the
   checkout's `exercise_media`, plus `models` and `tasks` if wanted.
4. `nix flake update maia`, rebuild, restart the app, then add the model keys in Settings →
   Models.

## Known gaps

- Nothing has run against a real provider. In particular, which Ollama and OpenRouter models honour
  `json_schema`, and whether the `json_object` retry is enough for the ones that don't.
- `ReadWritePaths` for a moved `notesDir`/`archiveDir` and a non-`/var/lib` `dataDir` was checked
  by evaluation only, not on a running unit.
- The intake dir can no longer be turned off from Settings: an empty field means "default", not
  null.
- First-boot exercise seeding falls back to `./data` when `seed_dir` is unset (existing behaviour).
  A test server started from `server/` copied about 2.9 GB of media into its data dir.
