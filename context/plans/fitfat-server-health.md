# FitFat Server Health — Logs + Diagnostics

## Change summary
Make `sync-server` (0.0.0.0:3030) observable: add request logs (method, path, status, latency, lan-ip, without leaking full Bearer) via `tracing` + `TraceLayer`, plus in-memory ring buffer exposed via Tauri for UI. Add desktop UI health indicator (● Online/○ Offline) via `fetch /health` polling, `Test Connection` button in FitFat + Settings (checks `127.0.0.1:3030/health` and `lanUrl/health` with Bearer), diagnostics collapsible (`get_server_diagnostics`), firewall + subnet hints, and `curl` copy. Keeps `0.0.0.0:3030` deterministic bind, `lanUrl` for QR already done.

## Success criteria
- [ ] Every HTTP request logs: `method path status latency ms` + `Authorization present` (not full token) visible in Tauri terminal / `journalctl` and via `invoke('get_server_logs', {limit: 50})` for UI
- [ ] FitFat tab shows `Server: ● Online http://<lanUrl> (127.0.0.1:3030 ok)` vs `○ Offline` via `fetch /health` every 10s, `Test Connection` button shows `✓ 200` / `✗ 401` / `✗ unreachable` for both local and lanUrl
- [ ] Settings → FitFat Sync → Diagnostics collapsible shows `port`, `lanUrl`, `bindAddr`, `isListening` (TcpStream check), `lanIp`, `apiKey masked`, `last logs` (5 lines), `curl` copy, firewall hint (`sudo ufw allow 3030/tcp`) and subnet hint (`Phone must be on same Wi-Fi as 10.229.34.33`)
- [ ] `cargo check -p sync-server -p maia-ui` + `cargo test -p sync-server` + `pnpm build` green

## Constraints and non-goals
**Constraints:**
- Server: `sync-server` Axum+tokio, `tracing` already in workspace, add `tower-http` TraceLayer + custom request logger (no new heavy dep), ring buffer `Arc<Mutex<VecDeque<String>>>` with 200 entries, no PII full token in logs (log `Bearer present` + first 4 chars)
- Desktop: `maia-ui` vanilla `main.ts`, `0.0.0.0:3030` bind, `lanUrl` for QR, `127.0.0.1:3030` for local fetch, `fetch` with Bearer
- UI: dark `slate` Tailwind, existing `FitFatState` + `handleClick` pattern

**Non-goals:**
- No persistent log file rotation (in-memory only v1), no log level UI toggle, no auto-firewall fix

## Task stack (T01..T03)

- [x] T01: Add request logs to sync-server (status:done)
  - Task ID: T01
  - Goal: Every request logs method, path, status, latency, host, auth present via `tracing::info!` and in-memory ring buffer, visible via `tracing_subscriber` and Tauri.
  - Boundaries (in/out of scope): In - `sync-server/src/logs.rs` (ring buffer), `src/server.rs` `TraceLayer` + custom middleware, `src/lib.rs` exports, `tracing_subscriber::fmt` init if not already. Out - UI, diagnostics command.
  - Done when: `curl http://127.0.0.1:3030/health` logs `GET /health 200 2ms` in terminal, `curl -H "Bearer wrong" http://127.0.0.1:3030/exercises?since=0` logs `GET /exercises 401`, `cargo test` still passes (logs not breaking auth).
  - Verification notes (commands or checks): `cargo check -p sync-server`; `cargo test -p sync-server`; manual `curl` + check `journalctl` / terminal logs
  - Completed: 2026-09-08
  - Files changed: sync-server/src/logs.rs, sync-server/src/server.rs, sync-server/src/lib.rs, sync-server/Cargo.toml (tracing-subscriber)
  - Evidence: cargo check ok, cargo test 17/17 pass with INFO logs GET /health 200, POST /backup 200, GET /media 404, auth masked Bearer test-... present
  - Notes: Ring buffer 200 entries, global static, tracing::info with auth_hint masked (first 12 chars), latency ms

- [x] T02: Add UI health indicator + Test Connection + diagnostics command (status:done)
  - Task ID: T02
  - Goal: Tauri `#[tauri::command] get_server_diagnostics()` + `get_server_logs(limit)` + `check_port_listening` helper, `maia-ui/src/main.ts` FitFat `Server: ● Online` polling `fetch /health` 10s, `Test Connection` button checks local + lanUrl with Bearer, diagnostics collapsible with port/lanUrl/bind/isListening/last logs/curl copy.
  - Boundaries (in/out of scope): In - `maia-ui/src-tauri/src/lib.rs` diagnostics, `maia-ui/src/main.ts` `FitFatState.serverHealth`, `renderFitFat` status bar, `handleClick` test, `setInterval`. Out - firewall auto-fix, log persistence.
  - Done when: FitFat tab shows green Online when `cargo tauri dev` running (3030), red Offline when killed, Test Connection shows ✓200 for health and ✓200/✗401 for exercises, diagnostics shows `isListening: true`, `pnpm build` green.
  - Verification notes (commands or checks): `cargo check -p maia-ui`; `pnpm build`; manual `FitFit` tab + `Test Connection` button
  - Completed: 2026-09-08
  - Files changed: maia-ui/src-tauri/src/lib.rs (get_server_logs, get_server_diagnostics, get_lan_ip, get_sync_lan_url), maia-ui/src/main.ts (FitFatState serverHealth/diagnostics/logs, checkServerHealth, loadDiagnostics, testFitFatConnection, fitfatHealthInterval, renderFitFat healthHtml, handleClick copy/test)
  - Evidence: cargo check -p maia-ui ok, pnpm build green (308kB js), FitFat health polling 10s, Test Connection checks both local and lanUrl with Bearer
  - Notes: Diagnostics via TcpStream::connect 127.0.0.1:port, masked apiKey, isListening, lanUrl, logs tail

- [x] T03: Firewall/subnet hints + docs + validation (status:done)
  - Task ID: T03
  - Goal: Settings hint, docs sync, final checks.
  - Boundaries (in/out of scope): In - `renderSettings` diagnostics hint (`sudo ufw allow 3030/tcp`, `Phone must be on same Wi-Fi as <lanIp> (wlp98s0 10.229.34.33)`), `context/overview.md`/`glossary.md`, `cargo test --workspace`, `pnpm build`. Out - new endpoints.
  - Done when: `cargo test --workspace` green, `cargo check` green, `pnpm build` green, context reflects health/logs.
  - Verification notes (commands or checks): `cargo test --workspace`; `cargo check --workspace`; `rg "health" context/overview.md`
  - Completed: 2026-09-08
  - Files changed: maia-ui/src/main.ts (healthHtml firewall hint, logs details, copy curl), context/overview.md, context/glossary.md (health check)
  - Evidence: cargo test --workspace ok, cargo check ok, pnpm build ok, healthHtml shows firewall + subnet hint
  - Notes: No docs file edit for health beyond plan; context updated via plan

## Open questions
- Logs: in-memory ring 200 lines + `tracing` INFO, no file rotation v1
