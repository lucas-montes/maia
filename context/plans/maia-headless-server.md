# Maia Headless Server

## Change summary
Make `sync-server` deployable as a standalone NixOS service on `luctop`.

Add the missing headless binary — `sync-server` is currently a **lib crate with no `main.rs`**, started only in-process by the Tauri GUI (`maia-ui/src-tauri/src/lib.rs:1644` via `tauri::async_runtime::spawn`), so the HTTP API's lifetime is the desktop app's lifetime. Replace ad-hoc `CREATE TABLE IF NOT EXISTS` boot DDL with a `PRAGMA user_version` migration runner that snapshots before each version change and refuses to start when the DB is newer than the binary. Make all config env-driven. Fix the unrouted `POST /backup`. Rewrite `nix/module.nix`, which today runs the legacy daemon (`ExecStart = bin/maia`) and sets two env vars nothing reads.

Target deployment: NixOS system service `maia`, `0.0.0.0:3030`, DB at `/var/lib/maia/maia.db`, media served from the repo checkout.

## Success criteria
- [ ] `systemctl is-active maia` → active after `nixos-rebuild switch`; survives logout/reboot; `SIGTERM` drains cleanly
- [ ] `curl http://<lan-ip>:3030/health` → 200 from the phone on the same Wi-Fi
- [ ] `GET /media/<exercise-id>.jpg` → 200 (media served from the repo checkout, not `/var/lib`)
- [ ] `POST /backup` → 200 with a 16MB body; `GET /backup/latest` returns byte-identical output (`cmp`)
- [ ] Existing `server/maia.db` (39 tables, `user_version=0`) migrates to current version with **no data loss** and **no v1 replay**
- [ ] Adding a migration = append one `&str` to `MIGRATIONS` + bump `LATEST_VERSION`; nothing else
- [ ] Nix rollback with a newer DB → service **refuses to start** with an actionable message
- [ ] `snapshots/maia.v<N>.db` auto-created before each version change, keep 10
- [ ] Both `maia` and `lucas` can write `/var/lib/maia/maia.db` + `-wal` + `-shm`
- [ ] `cargo test --workspace`, `cargo check --workspace`, `pnpm build`, `nix build .#sync-server` all green

## Constraints and non-goals

**Constraints:**
- Schema embedded in the binary is the source of truth — no external `.sql` files
- Desktop app and service share one DB file
- Media stays in the repo at `server/exercise_media`
- Secrets come from an `EnvironmentFile`, never `maia.json`
- Deployment is a NixOS system service

**Non-goals:**
- No home-manager module
- No hot reload / dev instance (Rust has no HMR; production loop is `nixos-rebuild switch`)
- No TLS or key rotation (static Bearer on LAN, unchanged)
- No migration CLI beyond `serve`/`migrate`/`status`
- **No mobile client work** — nothing reads `endpointExport`/`endpointImport` today, so wiring `/backup` will not add an app button
- No schema redesign
- `maia-ui` / GUI behaviour untouched

## Task stack (T01..T09)

Each task is one atomic commit.

- [ ] T01: Versioned migrations keyed on `PRAGMA user_version` (status:todo)
  - Task ID: T01
  - Goal: Ordered transactional migration runner replacing `create_schema` boot DDL, with legacy-baseline detection, a Nix-rollback downgrade guard, and pre-migration `VACUUM INTO` snapshots (keep 10).
  - Boundaries (in/out of scope): In — `db.rs`: lift existing inline DDL as `MIGRATIONS[0]` (v1 verbatim), add `migrate()`/`user_version()`/`has_legacy_schema()`/`snapshot()` + rotation, **delete** `RECEIPTS_ADDED_COLUMNS`/`add_missing_columns`/`column_exists` so there is one mechanism not two; retarget `server.rs:147` **and** `maia-ui/src-tauri/src/lib.rs:151` to `migrate()`; v2 = `DROP TABLE IF EXISTS meal_ingredients` (verified orphan — no `server/**/*.rs` reference); unit tests for fresh/legacy/newer-than-binary/snapshot. Out — CLI (T03), other schema changes, docs (T09).
  - Done when: `cargo test -p sync-server` green; the 39-table legacy DB is stamped v1→v2 without replaying v1 or losing rows; `rg add_missing_columns` returns nothing.
  - Verification notes (commands or checks): `cargo test -p sync-server migrations`; on a **copy** of `server/maia.db`: `sqlite3 copy.db "PRAGMA user_version"` before/after + row-count compare; `rg add_missing_columns sync-server/`
  - Files changed: `sync-server/src/db.rs`, `sync-server/src/server.rs`, `maia-ui/src-tauri/src/lib.rs`
  - Notes: `meal_ingredients` drop is safe — the only references are in `mobile/lib/src/database/app_database.dart`, which is the **separate** Drift DB and already drops it at line 776.

- [ ] T02: Env-first config + explicit media/seed dirs (status:todo)
  - Task ID: T02
  - Goal: `Config` resolvable from `MAIA_*` env vars, and media/seed dirs as real config instead of CWD- and `db_path.parent()`-derived.
  - Boundaries (in/out of scope): In — `config.rs`: `media_dir`/`seed_dir` fields + `from_env()` + builders; honour `MAIA_DB_PATH`/`MAIA_BIND_ADDR`/`MAIA_API_KEY`/`MAIA_MODEL_API_KEY`; `server.rs:150-159` seeder path selection; `handlers/media.rs:11-18` `media_dir()` prefers `config.media_dir`. Out — `maia-ui/src-tauri/src/lib.rs` key/path helpers (desktop keeps `maia.json` probing), CLI (T03), packaging (T06).
  - Done when: with env set, those exact paths are used; with no env set, behaviour is byte-identical to today so the dev shell is unaffected.
  - Verification notes (commands or checks): `cargo test -p sync-server`; unit test that `from_env` prefers env over probing; `rg MAIA_DB_PATH sync-server/src/`

- [ ] T03: Headless binary + graceful shutdown (status:todo)
  - Task ID: T03
  - Goal: Give `sync-server` a real entrypoint so it runs without the GUI — `src/main.rs` with clap subcommands `serve`/`migrate`/`status`, plus SIGTERM/SIGINT shutdown of the axum listener.
  - Boundaries (in/out of scope): In — new `sync-server/src/main.rs`; `sync-server/Cargo.toml` (`clap.workspace = true`, already a workspace dep at `Cargo.toml:8`); **workspace `Cargo.toml`: add `"signal"` to tokio features — currently absent, so graceful shutdown will not compile without it**; `server.rs:160-166` `.with_graceful_shutdown(shutdown_signal())`. Out — Nix packaging (T06), the Tauri spawn path, config semantics (T02).
  - Done when: `cargo build --release -p sync-server` yields a binary; `serve` answers `/health`; `SIGTERM` exits 0 after in-flight requests drain; `status` prints `user_version` vs `LATEST_VERSION`.
  - Verification notes (commands or checks): `cargo build -p sync-server`; run + `curl localhost:3030/health` + `kill -TERM` → clean exit; `cargo test --workspace`

- [ ] T04: Fix backup route + write logic (status:todo)
  - Task ID: T04
  - Goal: Expose `POST /backup` and harden the write path; replace the fake test with a real round-trip.
  - Boundaries (in/out of scope): In — `server.rs`: route `POST /backup` with its own `DefaultBodyLimit` (axum's 2MB default rejects a real DB — mirror the `MAX_UPLOAD_BYTES` pattern from `/receipt-pictures/upload`); `handlers/backup.rs`: write `.tmp` + `rename` for atomicity, stop swallowing the `create_dir_all` error (`let _ =` today), add `Content-Disposition: attachment` to `get_backup`; `lib.rs:535`: rewrite `backup_roundtrip` to POST-then-GET asserting byte equality. Out — mobile client, auth model (already behind global `bearer_auth`), retention/scheduling.
  - Done when: `POST /backup` → 200 (was 404); `GET /backup/latest` returns the exact bytes; oversized body → 413 not 500; an interrupted write cannot leave a corrupt `fitfat.sqlite`.
  - Verification notes (commands or checks): `cargo test -p sync-server backup`; `curl -X POST --data-binary @server/maia.db -H "Authorization: Bearer $KEY" localhost:3030/backup -w '%{http_code}'` → 200; `curl .../backup/latest -o /tmp/out.sqlite && cmp /tmp/out.sqlite server/maia.db`

- [ ] T05: Flake nixpkgs `follows` (repo `server`) (status:todo)
  - Task ID: T05
  - Goal: Stop `server/flake.nix` hardcoding `nixos-25.11` so a 26.05 system can consume its module and package.
  - Boundaries (in/out of scope): In — `server/flake.nix` only. Out — packaging, module options, the dotfiles flake.
  - Done when: evaluating from a 26.05 flake builds against the 26.05 stdenck.
  - Verification notes (commands or checks): `nix eval --raw .#packages.x86_64-linux.default.drvPath`; `rg "nixos-25.11" flake.nix` → empty

- [ ] T06: Nix package for `sync-server` (repo `server`) (status:todo)
  - Task ID: T06
  - Goal: Add a derivation for the module to run, and make `packageData` use cargo's build hooks so it caches.
  - Boundaries (in/out of scope): In — `nix/packages.nix`: new `sync-server` via `packageData "sync-server" ../sync-server/Cargo.toml`, exposed as `packages.sync-server`; replace the hand-rolled `cargo build --release` + `cp` with `cargoBuildHook`/`cargoInstallHook`. Out — `module.nix` (T07), devshell.
  - Done when: `nix build .#sync-server` succeeds, `$out/bin/sync-server --help` runs, and it builds with **no** GTK/WebKit/Tauri deps (pure axum + bundled rusqlite).
  - Verification notes (commands or checks): `nix build .#sync-server && ./result/bin/sync-server --help`; `nix why-depends .#sync-server webkitgtk_4_1` → not found

- [ ] T07: Rewrite the NixOS module (repo `server`) (status:todo)
  - Task ID: T07
  - Goal: Replace `services.maia` with a correct unit for the headless server — real binary, env wiring, shared-DB group permissions, declarative firewall.
  - Boundaries (in/out of scope): In — `nix/module.nix`: options `enable`/`package`/`dataDir`/`sourceDir`/`port`/`apiKeyFile`/`openFirewall`; unit with `ExecStart = lib.getExe cfg.package "sync-server" serve`, `StateDirectory = "maia"`, `UMask = "0007"`, `ProtectHome = "read-only"` (media stays in repo), `ProtectSystem = "strict"`, `ReadWritePaths = [dataDir]`, `environmentFiles = [apiKeyFile]`; `users.users.maia` + `users.groups.maia` with **`lucas` as member** and `dataDir` group-owned setgid `2775`; `openFirewall` → `allowedTCPPorts`; **remove** the dead `MAIA_NOTES_DIR` and the legacy-daemon `ExecStart`. Out — dotfiles side (T08), creating `secrets.env` (documented there).
  - Done when: unit shows the sync-server ExecStart; both `maia` and `lucas` can write the DB and its `-wal`/`-shm` (the setgid + `UMask` combination is what makes this work under `ProtectSystem = "strict"`).
  - Verification notes (commands or checks): eval the module in a scratch config; `sudo -u maia test -w /var/lib/maia`; `sudo -u lucas sqlite3 /var/lib/maia/maia.db "PRAGMA user_version;"`

- [ ] T08: Enable on luctop (repo `~/.dotfiles`) (status:todo)
  - Task ID: T08
  - Goal: Consume the module and retire the hardcoded firewall rule.
  - Boundaries (in/out of scope): In — `~/.dotfiles/flake.nix`: `maia.url = "path:/home/lucas/Projects/maia"` + `maia.inputs.nixpkgs.follows = "nixpkgs"`; new `~/.dotfiles/services/maia.nix` mirroring `services/bandtrack.nix`; `hosts/luctop/configuration.nix`: import module + `services.maia` config and **remove** the uncommitted hand-written `networking.firewall.allowedTCPPorts = [ 3030 ]` in favour of `openFirewall`. Out — lucver/raspi4, creating `/var/lib/maia/secrets.env` (one-time manual: `MAIA_API_KEY`, `MAIA_MODEL_API_KEY`, `0600 root:maia`).
  - Done when: `nixos-rebuild switch --flake ~/.dotfiles#luctop` starts `maia`; phone reaches `/health` over LAN.
  - Verification notes (commands or checks): rebuild + `systemctl status maia` + `curl http://<lan-ip>:3030/health`

- [ ] T09: Docs sync + validation (repo `server`, plus dotfiles verification) (status:todo)
  - Task ID: T09
  - Goal: Bring `context/` in line with reality and run the full validation pass including a rollback rehearsal.
  - Boundaries (in/out of scope): In — `context/overview.md` (39 tables not 38; headless server is the deployment target; migrations exist), mark `context/plans/sqlite-schema.md` obsolete (it proposes AUTOINCREMENT + a `migrations` table, both contradicting the real UUID-v7/TEXT-PK + `user_version` design), correct the inaccurate "POST /backup 200" evidence line in `fitfat-server-health.md`; full `cargo test --workspace`, `cargo check --workspace`, `pnpm build`, `nix build .#sync-server`; live smoke of `/health`, `/media/<id>`, `POST /backup`; **Nix rollback rehearsal proving the downgrade guard fires**. Out — new features.
  - Done when: all checks green; a Nix rollback with a newer DB refuses to start with an actionable message; docs describe the real deployment.
  - Verification notes (commands or checks): full command list + `nixos-rebuild switch --rollback` → assert service exits with the guard message, then restore via `sqlite3 /var/lib/maia/maia.db ".restore snapshots/maia.v<N>.db"`

## Open questions

None blocking. Assumptions recorded per the plan-authoring skill:

- `keep 10` is the chosen snapshot rotation count; snapshots live in `<dataDir>/snapshots/` and are **excluded** from Nix management (mutable state stays in `/var/lib`).
- `meal_ingredients` drop (v2) is safe based on zero server-side references. If it must be kept as a no-op safety net, make v2 a comment-only version bump instead.
- `openFirewall` defaults to `true`, replacing the hand-written `allowedTCPPorts = [ 3030 ]` in `hosts/luctop/configuration.nix`.
- Module type is NixOS (not home-manager); migration mechanism is `user_version`; snapshot policy is pre-migration + keep 10; media stays in the repo; shared-DB permissions use the `maia` group with `lucas` as a member, setgid `dataDir`, and `UMask = "0007"`.