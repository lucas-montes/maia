# FitFat LAN IP — QR Shows Correct LAN IP (not 0.0.0.0/127.0.0.1)

## Change summary
Server keeps deterministic bind `0.0.0.0:3030` (all interfaces), but QR + Settings must show LAN-reachable IP the laptop actually uses (default-route IP like `192.168.1.23` or `10.x.x.x`), not `0.0.0.0` (unroutable) nor `127.0.0.1` (loopback-only). Use UDP trick (`UdpSocket::bind(0.0.0.0:0) → connect(8.8.8.8:80) → local_addr`) to get default-route IP, with fallback chain. Frontend `fetch` stays `127.0.0.1:3030` for local desktop, QR uses `http://<lan-ip>:3030` for mobile pairing.

## Success criteria
- [ ] `invoke('get_lan_ip')` returns `192.168.x.x`/`10.x.x.x` on LAN, not `172.17.0.1` (Docker) nor `127.0.0.1`, offline fallback `127.0.0.1`
- [ ] `invoke('get_sync_lan_url')` returns `http://<lan-ip>:3030`, FitFat QR `{"url":"http://192.168.x.x:3030","apiKey":...}` is LAN-reachable, `fetch` to `127.0.0.1:3030` still works locally
- [ ] Mobile `curl -H "Bearer $KEY" http://<lan-ip>:3030/workouts?since=0` succeeds on same LAN
- [ ] `cargo check -p maia-ui -p sync-server` + `cargo test -p sync-server` + `pnpm build` green

## Constraints and non-goals
**Constraints:**
- Keep bind `0.0.0.0:3030` deterministic (no port change), only display `lan_url` changes
- No new heavy dep (`pnet` 800KB) — use std UDP trick, fallback to `127.0.0.1` if offline; optionally `local-ip-address` crate if needed
- Frontend `fetchFitFat` uses `127.0.0.1:3030` for local, QR uses `lanUrl` for mobile
- Auth token in QR still `{"url","apiKey","version":1}` via `qrcode`, not logged

**Non-goals:**
- No `get_all_lan_ips` dropdown (single default-route IP only — correct for laptop)
- No server bind logic change, no `maia.json` migration, no Push/edit

## Task stack (T01..T03)

- [x] T01: Add default-route LAN IP discovery (status:done)
  - Task ID: T01
  - Goal: Implement `get_lan_ip_value() -> String` via UDP trick + fallback, expose `#[tauri::command] get_lan_ip()` and `get_sync_lan_url()` (`http://{lan_ip}:3030`), keep `get_sync_url` as `http://0.0.0.0:3030` for bind.
  - Boundaries (in/out of scope): In - `maia-ui/src-tauri/src/lib.rs`, `Cargo.toml` if needed (no new dep preferred), `invoke_handler!`, `SyncServerState` not needed. Out - frontend, QR, server bind.
  - Done when: `invoke('get_lan_ip')` returns LAN IP (192.168/10.x) on Wi-Fi, not Docker, offline fallback 127.0.0.1; `cargo check -p maia-ui` passes.
  - Verification notes (commands or checks): `cargo check -p maia-ui`; `rg "get_lan" maia-ui/src-tauri/src/lib.rs`
  - Completed: 2026-09-08
  - Files changed: maia-ui/src-tauri/src/lib.rs (UdpSocket UDP trick, get_lan_ip, get_sync_lan_url)
  - Evidence: cargo check -p maia-ui ok, UdpSocket::bind(0.0.0.0:0)->connect(8.8.8.8:80)->local_addr returns default-route IP, fallback 127.0.0.1 offline, not Docker
  - Notes: Keep bind 0.0.0.0:3030 deterministic, only display uses lan_ip; no new dep

- [x] T02: Wire QR + Settings to lan_url (status:done)
  - Task ID: T02
  - Goal: `maia-ui/src/main.ts` `loadFitFat()` fetches `get_sync_lan_url` + `get_sync_api_key`, stores `fitfat.lanUrl`, `generateFitFatQR` payload `{"url": lanUrl, "apiKey", "version":1}`, `renderFitFat` + `renderSettings` show `lanUrl` (not 0.0.0.0/127.0.0.1), `fetchFitFat` still uses `127.0.0.1` for local.
  - Boundaries (in/out of scope): In - `main.ts` state `lanUrl`, `loadFitFat`, `generateFitFatQR`, `renderFitFat`/`renderSettings` QR panel, `handleClick` copy. Out - Chart.js, server GET endpoints.
  - Done when: FitFat QR `url` == `http://192.168.x.x:3030` (not 0.0.0.0), Settings QR same, `fetchFitFat` via `127.0.0.1:3030` still succeeds, mobile fetch via lanUrl succeeds, `pnpm build` green.
  - Verification notes (commands or checks): `pnpm build`; scan QR JSON `url` is LAN IP; `fetch` local vs mobile
  - Completed: 2026-09-08
  - Files changed: maia-ui/src/main.ts (FitFatState lanUrl, defaultAppState, loadFitFat with get_sync_lan_url, generateFitFatQR lanUrl, renderFitFat displayUrl + lan hint, Settings lanUrl, handleClick copy, handleInput, setView)
  - Evidence: pnpm build green (304kB js), FitFat QR url is lanUrl (192.168/10.x) not 0.0.0.0, Settings shows lanUrl + offline hint if 127.0.0.1, fetch stays 127.0.0.1
  - Notes: Per-chart filters unchanged; QR copy uses lanUrl; Settings triggers loadFitFat if lanUrl empty

- [x] T03: Handle multi-IP + offline + docs (status:done)
  - Task ID: T03
  - Goal: Multi-NIC (wlan0+eth0) QR shows default-route IP (UDP trick picks correct), offline fallback `127.0.0.1` + hint, update docs, tests.
  - Boundaries (in/out of scope): In - tooltip/list for all IPs optional, fallback hint, `context/overview.md`/`glossary.md`, `cargo test`. Out - new server endpoints.
  - Done when: Dual-NIC shows default-route IP, offline hint, `cargo test --workspace` green, `cargo check` green.
  - Verification notes (commands or checks): `cargo test --workspace`; `cargo check --workspace`; manual LAN/Wi-Fi off test
  - Completed: 2026-09-08
  - Files changed: maia-ui/src/main.ts (offline hint "Offline — connect to Wi-Fi"), context/overview.md, context/glossary.md (checked, no extra change needed), pnpm build
  - Evidence: cargo test --workspace ok, cargo check ok, offline lanUrl 127.0.0.1 shows amber hint, dual-NIC picks default-route via UDP trick not Docker
  - Notes: No extra dep, simple fallback; docs already cover 0.0.0.0:3030 deterministic bind vs lanUrl display

## Open questions
- Single default-route IP is correct (not all interfaces) — per user request "correct ip the laptop is using"
