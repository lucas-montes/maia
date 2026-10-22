{
  pkgs,
  lib,
  crane-tauri,
  craneLib,
  ...
}: let
  readCargoToml = path: builtins.fromTOML (builtins.readFile path);

  # Only cargo-relevant files, so `target`, the 19 GB of media/data churn and
  # `context/tmp` never reach the store, and a dependency fingerprint never
  # depends on an unrelated working-tree file.
  src = craneLib.cleanCargoSource ../.;

  # utoipa-swagger-ui's build script fetches the Swagger UI bundle from GitHub
  # (`curl` when the `reqwest` feature is off), which neither works in the Nix
  # sandbox. So the archive is vendored at build time and handed over as a
  # file:// URL -- the one scheme the build script will copy from disk. The tag
  # has to track the version utoipa-swagger-ui defaults to (v5.17.12 for 7.1.0).
  swaggerUi = pkgs.fetchurl {
    url = "https://github.com/swagger-api/swagger-ui/archive/refs/tags/v5.17.12.zip";
    hash = "sha256-HK4z/JI+1yq8BTBJveYXv9bpN/sXru7bn/8g5mf2B/I=";
  };

  swaggerEnv = {
    SWAGGER_UI_DOWNLOAD_URL = "file://${swaggerUi}";
  };

  # The build script does std::fs::copy() on the zip. Rust's fs::copy prefers
  # copy_file_range(2) and only falls back to a read+write loop on a few
  # specific errors -- a source on a read-only store mount yields EACCES and
  # aborts (cp(1) would silently fall back). So before cargo runs, turn the
  # store file into a plain writable copy and point the build script at it.
  # Needed wherever utoipa-swagger-ui's build script can run: the deps build
  # never compiles it (feature-gated), but the app builds do.
  swaggerZipSetup = ''
    if [ -f "''${SWAGGER_UI_DOWNLOAD_URL#file://}" ]; then
      local_zip="$PWD/staged-swagger-ui.zip"
      cat "''${SWAGGER_UI_DOWNLOAD_URL#file://}" > "$local_zip"
      chmod 644 "$local_zip"
      export SWAGGER_UI_DOWNLOAD_URL="file://$local_zip"
    fi
  '';

  commonArgs = {
    inherit src;
    strictDeps = true;

    nativeBuildInputs = [
      pkgs.pkg-config
    ];

    buildInputs =
      [
        pkgs.openssl
      ]
      ++ lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
        pkgs.libiconv
      ];

    preBuild = swaggerZipSetup;
  }
  // swaggerEnv;

  # Build *just* the cargo dependencies, so all of that work can be reused
  # (e.g. via cachix) when running in CI.
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;

  # The HTTP API, headless. Pure axum plus bundled SQLite, so it builds and runs
  # without a desktop session -- this is what the NixOS service runs.
  sync-server = craneLib.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoExtraArgs = "-p sync-server";
    }
  );

  # ---------------------------------------------------------------------
  # Desktop application (Tauri v2, via crane-tauri)
  # ---------------------------------------------------------------------

  pnpm = pkgs.pnpm_11;

  frontend = pkgs.stdenv.mkDerivation (finalAttrs: {
    pname = "maia-ui-frontend";
    version = "0.1.0";

    # Only the manifests and sources vite actually reads: node_modules and the
    # dist that a previous dev build left behind must stay out of the store.
    src = lib.fileset.toSource {
      root = ../maia-ui;
      fileset = lib.fileset.unions [
        ../maia-ui/package.json
        ../maia-ui/pnpm-lock.yaml
        ../maia-ui/pnpm-workspace.yaml
        ../maia-ui/tsconfig.json
        ../maia-ui/vite.config.ts
        ../maia-ui/index.html
        ../maia-ui/src
      ];
    };

    pnpmDeps = pkgs.fetchPnpmDeps {
      inherit (finalAttrs) pname version src;
      inherit pnpm;
      # 4 is the current fetcher: bundle the store as a reproducible tarball and
      # dump the v11 SQLite index to SQL so the hash is platform-independent.
      # Older versions are deprecated and scheduled for removal in 26.11.
      fetcherVersion = 4;
      hash = "sha256-Qe07si03npOOpJRYLIbWqxqpxOTKE0vYf2iXmyiWMTc=";
    };

    nativeBuildInputs = [
      pkgs.pnpmConfigHook
      pnpm
      pkgs.nodejs-slim
    ];

    # `build` is `tsc && vite build`; run it through pnpm so the workspace
    # settings (pnpm-workspace.yaml, which allows esbuild's install script)
    # apply. Entering the Tauri config from the build would drag the whole
    # tauri.conf.json into the fileset for no benefit.
    buildPhase = ''
      runHook preBuild
      pnpm build
      runHook postBuild
    '';

    # buildTauriApp points tauri.conf.json's `build.frontendDist` at this
    # derivation, so the output root itself is the asset root -- no dist/
    # nesting.
    installPhase = ''
      runHook preInstall
      cp -r dist/. "$out"
      runHook postInstall
    '';

    passthru.pnpmDepsHash = finalAttrs.pnpmDeps.outputHash;
  });

  tauri = crane-tauri.lib.buildTauriApp {inherit pkgs craneLib;} {
    pname = "maia-ui";
    version = (readCargoToml ../maia-ui/src-tauri/Cargo.toml).package.version;
    # The Tauri app root (whatever directly contains src-tauri/).
    src = ../maia-ui;
    # Widened to the workspace root: src-tauri depends on ../../sync-server, so
    # the fileset has to reach the sibling crates.
    cargoRoot = ../.;
    inherit frontend;
    craneArgs = swaggerEnv // {preBuild = swaggerZipSetup;};
  };

  # The same app wrapped with wrapGAppsHook3, so the runtime lookups the
  # libraries declare (pixbuf loaders, GSettings schemas, GIO/TLS modules,
  # gstreamer prefixes) resolve from the Nix closure instead of ambient
  # environment. This is what the module installs.
  maia-desktop = tauri.wrappedApp;

  passthru = {
    inherit frontend;
    inherit (tauri) app cargoArtifacts tauriConfig;
  };
in {
  inherit
    sync-server
    maia-desktop
    ;
}
// passthru
