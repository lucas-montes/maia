# Nix Packaging

How the server workspace, the Rust dependency caching, the pnpm frontend, the Tauri desktop app, and the flake outputs are built. Code of truth: `nix/packages.nix`, `flake.nix`, `nix/module.nix`.

## Package graph

```
workspace src (craneLib.cleanCargoSource ../.)
        |
        +--> maia-deps        buildDepsOnly  (shared cargoArtifacts cache)
        |      |
        +<----+   (inheritCargoArtifactsHook + rsync --chmod=u+w)
        |
        +--> sync-server      buildPackage -p sync-server, reuses cargoArtifacts
        |
        +--> frontend         pnpm (pnpm_11) + vite, fetchPnpmDeps v4 offline deps
        |      |
        |<-----+   (tauri.conf build.frontendDist points at this drv)
        |
        +--> tauri (maia-ui)  crane-tauri buildTauriApp (own deps build + app build)
        |      (src = ../maia-ui, cargoRoot = ../. workspace root)
        +--> maia-desktop     tauri.wrappedApp (wrapGAppsHook3 runtime glue)
```

## Flake outputs (`flake.nix`)

- `packages.<system>.sync-server` — the HTTP API; what the NixOS service runs.
- `packages.<system>.frontend` — static vite build; exported because `tauri.conf.build.frontendDist` consumes it.
- `packages.<system>.maia-desktop` — wrapped desktop app; what `services.maia.desktop.enable` installs.
- `apps.<system>.sync-server` / `ui` / `default`.
- `nixosModules.default` — imports with `self` threaded in; referencing `self.packages` directly from the module would be circular, which is why the flake passes the packages through.
- `checks.evalModule` evals `nix/module.nix` so `nix flake check` catches option drift cheaply.

## Layout of `nix/packages.nix`

- Single `craneLib.cleanCargoSource ../.` for the whole workspace: `sync-server` depends on `maia-ui`'s `src-tauri` crate (`../../sync-server` path dep), so deps and app member builds must share one fileset and one `Cargo.lock` (workspace root; `src-tauri/` has none).
- `cargoArtifacts = craneLib.buildDepsOnly commonArgs`: one fingerprint-stable target tree for the deps. `sync-server` reuses it via `buildPackage // { inherit cargoArtifacts; }`, `crane-tauri` builds its own two (the inherit hook copies the tree with `rsync --chmod=u+w`).
- Frontend: `lib.fileset` whitelist (package.json, pnpm-lock.yaml, pnpm-workspace.yaml, tsconfig, vite config, index.html, `src/`), `pnpmDeps = pkgs.fetchPnpmDeps { fetcherVersion = 4; pnpm_11; }`,
and `pnpm build` in `buildPhase`; the install phase copies `dist/.` to the output root (asset root == drv root, no `dist/` nesting). Touching the lockfile requires updating

`frontend.passthru.pnpmDepsHash` (`nix eval .#frontend.passthru.pnpmDepsHash`, `nix hash prepend sri`).

- `tauri = crane-tauri.lib.buildTauriApp { src = ../maia-ui; cargoRoot = ../.; inherit frontend; }`: the crane-tauri layer rewrites `frontendDist` to the built frontend drv and tunes the linux bundle; `maia-desktop = tauri.wrappedApp` adds `wrapGAppHook3` so GSettings/GIO/pixbuf/GStreamer resources resolve from the Nix closure.

## Vendoring utoipa-swagger-ui's download

`utoipa-swagger-ui`'s build script can't `curl`/`reqwest` its Swagger UI bundle in the sandbox, so the GitHub `v5.17.12` archive (the version utoipa-swagger-ui 7.1.0 defaults to) is `fetchurl`ed and handed over via `SWAGGER_UI_DOWNLOAD_URL = "file://<zip>"` in `swaggerEnv`, which is merged into `commonArgs` and passed along as `craneArgs` for the Tauri build.

`swaggerZipSetup` is the load-bearing piece: the build script does `std::fs::copy(<store zip>, <out_dir>)`, and Rust's `fs::copy` prefers `copy_file_range(2)` while only falling back to a read+write loop on a few specific errors. From a read-only `/nix/store` mount (`ext4 ro` on this host) the syscall returns `EACCES`, so the build script *panics with PermissionDenied* where `cp(1)` would quietly fall back to read/write. The `preBuild` hooks therefore materialize a writable local copy (`staged-swagger-ui.zip` in the build dir) and repoint `SWAGGER_UI_DOWNLOAD_URL` at it before cargo runs. The deps build (`buildDepsOnly`) never runs the utoipa build script — `utoipa-swagger-ui` is feature-gated, so the first run is the app build — but the preBuild rides in `commonArgs` anyway.

## Gotchas hit while building

- **`fs::copy` EACCES from store**: see above. Two misdiagnoses cost most of the debugging time: (1) blaming the general-purpose flag / UT extra field in GitHub's zip (a `zip -X` re-zip was added and later removed as a red herring), and (2) blaming a stale/cached OUT_DIR from reused cargoArtifacts — in truth the deps target contains no swagger artifacts at all, because the crate is feature-gated and its build script only first runs in the app build. `cp(1)` of the same source succeeded, which is what hid the syscall-level cause.
- **Untracked source files are invisible to flakes**: `nix eval`/`nix build` on a git-flake with a dirty tree only sees *tracked* files. New files that have never been `git add`ed or committed don't hit the fileset evaluated from `nix/packages.nix`, so a fresh eval builds the previously committed (possibly stale) layout.
- **`nix develop` non-interactive shells have no stdenv functions**: `genericBuild`/`runHook` are not defined because `stdenv/setup` isn't sourced; `source "$stdenv/setup"` in an inner `bash -ic` is the escape hatch for debugging crane phases. `--keep-failed` is unreliable through the daemon on this host (`keep-failed = false` in nix.conf), so an in-place debug `preBuild` in the packaging was more productive.

## Related

- `context/plans/maia-desktop-split.md` — why the desktop app is a client of sync-server rather than embedding it (the architectural decision this packaging serves).
- `context/plans/maia-headless-server.md` — the sync-server packaging/service side: `/etc/maia/config.json`, user ownership, shared DB — the pieces `nix/module.nix` leans on.

External references worth keeping: `crane` docs (https://github.com/ipetkov/crane) for `buildDepsOnly`/`buildPackage` semantics and `cargoArtifacts` reuse; nixpkgs `fetchPnpmDeps` for `fetcherVersion 4`.
