{ pkgs, lib, ... }:
let
  readCargoToml = path: builtins.fromTOML (builtins.readFile path);

  # Nix does not honour .gitignore for `builtins.path`, so the working tree would
  # otherwise land in the store whole: `target`, 3 GB of `data`, 3 GB of
  # `exercise_media`, `node_modules` and the live database add up to ~19 GB per
  # build. None of it is needed to compile the workspace.
  excludedDirs = [
    "target"
    "node_modules"
    "data"
    "exercise_media"
    "receipt_pictures"
    "receipts_archive"
    "result"
    ".direnv"
    ".env"
    # Built here, not shipped: `pnpm build` regenerates it and tauri-build embeds
    # whatever is present, so a stale copy in the tree would be embedded silently.
    "dist"
  ];

  # Local configuration. `maia.json` can hold `model_api` with an API key, so it
  # must never end up baked into a store path.
  excludedFiles = [
    "maia.json"
    ".env"
  ];

  sourceFilter = path: type:
    let
      rel = lib.removePrefix (toString ./.) (toString path);
      name = baseNameOf (toString path);
      excluded =
        builtins.elem name excludedDirs
        || builtins.elem name excludedFiles
        || lib.hasSuffix ".db" name
        || lib.hasSuffix ".db-wal" name
        || lib.hasSuffix ".db-shm" name
        # Diff traces from the shared-context hooks, ~90 MB of churn.
        || lib.hasPrefix "context/tmp" rel
        || rel == "context/tmp";
    in
    !excluded;

  source = lib.cleanSourceWith {
    src = ../.;
    filter = sourceFilter;
    name = "maia-source";
  };

  # utoipa-swagger-ui downloads the Swagger UI bundle from GitHub in its build
  # script, using `curl` when the `reqwest` feature is off. Neither works inside
  # the Nix sandbox, so the archive is vendored and handed over as a file:// URL,
  # which is the one scheme its build script copies from disk. The tag has to track
  # the version utoipa-swagger-ui defaults to (v5.17.12 for 7.1.0).
  swaggerUi = pkgs.fetchurl {
    url = "https://github.com/swagger-api/swagger-ui/archive/refs/tags/v5.17.12.zip";
    hash = "sha256-HK4z/JI+1yq8BTBJveYXv9bpN/sXru7bn/8g5mf2B/I=";
  };

  # Generic helper to build one workspace member as a release binary.
  packageData =
    name: cargoPath:
    pkgs.rustPlatform.buildRustPackage {
      pname = name;
      version = (readCargoToml cargoPath).package.version;
      src = source;

      cargoLock = {
        lockFile = ../Cargo.lock;
      };

      # Scoped to one member so building `sync-server` does not drag in the Tauri
      # toolchain: it needs no GTK, WebKit or JavaScript at all.
      cargoBuildFlags = [
        "-p"
        name
      ];

      # Referenced by store path only. Listing the archive in nativeBuildInputs
      # puts it on PATH, where the stdenv tries to execute it.
      postPatch = ''
        export SWAGGER_UI_DOWNLOAD_URL="file://${swaggerUi}"
      '';

      nativeBuildInputs = with pkgs; [ pkg-config ];
      buildInputs = with pkgs; [ openssl ];
      doCheck = false;
    };

  # The HTTP API. Pure axum plus bundled SQLite, so it builds and runs without
  # a desktop session -- this is what the NixOS service runs.
  sync-server = packageData "sync-server" ../sync-server/Cargo.toml;

  # ---------------------------------------------------------------------
  # Desktop application
  # ---------------------------------------------------------------------

  # Only the manifests: enough for `pnpm fetch` to populate a store, without
  # dragging the whole working tree into it.
  frontendManifest = lib.cleanSourceWith {
    src = ../maia-ui;
    name = "maia-ui-manifest";
    filter = path: type:
      let
        name = baseNameOf (toString path);
        rel = lib.removePrefix (toString ../maia-ui) (toString path);
      in
      builtins.elem name [ "package.json" "pnpm-lock.yaml" "pnpm-workspace.yaml" ]
      || rel == "/pnpm-workspace.yaml";
  };

  # The frontend's packages, fetched once into a store derivation so the build
  # below can install without network access.
  frontendDeps = pkgs.stdenv.mkDerivation {
    pname = "maia-ui-frontend-deps";
    version = (readCargoToml ../maia-ui/src-tauri/Cargo.toml).package.version;
    src = frontendManifest;
    nativeBuildInputs = [
      pkgs.nodejs
      pkgs.pnpm
      # Node has no CA bundle of its own and does not read /etc/ssl, so without
      # this every registry request fails with UNABLE_TO_GET_ISSUER_CERT_LOCALLY.
      pkgs.cacert
    ];
    installPhase = ''
      runHook preInstall
      export HOME=$TMPDIR
      export SSL_CERT_FILE="${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt"
      export NODE_EXTRA_CA_CERTS="$SSL_CERT_FILE"
      export npm_config_cafile="$SSL_CERT_FILE"
      pnpm config set store-dir "$out"
      pnpm fetch --frozen-lockfile
      # pnpm caches a per-project entry keyed by the checkout's absolute path,
      # which leaves a symlink into /build behind. Nothing reads it, and Nix
      # rejects the output for it.
      rm -rf "$out"/v10/projects
      runHook postInstall
    '';
    outputHashMode = "recursive";
    outputHash = "sha256-XBLtDV1KjPYF+gfsatg/TW4SiP4DKPs7oRgrrAM2IHQ=";
  };

  # Launchers find desktop entries through $XDG_DATA_DIRS/applications, and NixOS
  # keeps /etc off that list, so an entry written to /etc/applications is invisible
  # to rofi and friends. Shipping it inside the package puts it in the system
  # profile's share/applications instead, which is where every working entry on a
  # NixOS system comes from.
  #
  # The icon is referenced by name rather than by store path, so it survives a
  # rebuild: it resolves through the hicolor theme against the copy installed
  # below.
  desktopItem = pkgs.makeDesktopItem {
    name = "maia";
    desktopName = "Maia";
    comment = "Personal management suite";
    exec = "maia-ui";
    icon = "maia-ui";
    categories = [ "Utility" ];
    terminal = false;
  };

  # The Tauri desktop app. Unlike `sync-server` this genuinely needs a desktop
  # stack at both build and run time.
  maia-desktop = pkgs.rustPlatform.buildRustPackage {
    pname = "maia-ui";
    version = (readCargoToml ../maia-ui/src-tauri/Cargo.toml).package.version;
    src = source;

    cargoLock = {
      lockFile = ../Cargo.lock;
    };

    cargoBuildFlags = [
      "-p"
      "maia-ui"
      # Without this the webview loads build.devUrl (http://localhost:1420)
      # rather than the embedded assets, and the packaged app opens a blank
      # window saying it cannot connect to localhost. The Tauri CLI passes it
      # automatically; calling cargo directly does not.
      "--features"
      "custom-protocol"
    ];

    # The app depends on sync-server, which pulls in utoipa-swagger-ui.
    postPatch = ''
      export SWAGGER_UI_DOWNLOAD_URL="file://${swaggerUi}"
    '';

    # tauri-build embeds `frontendDist` at compile time, so the frontend has to be
    # built before cargo runs. preBuildHook is the last hook before buildPhase.
    preBuildHook = ''
      export HOME="$TMPDIR"
      pnpm --dir maia-ui config set store-dir "${frontendDeps}"
      pnpm --dir maia-ui install --frozen-lockfile --offline
      pnpm --dir maia-ui build
    '';

    nativeBuildInputs = [
      pkgs.nodejs
      pkgs.pnpm
      pkgs.pkg-config
      pkgs.wrapGAppsHook4
    ] ++ (with pkgs; [ frontendDeps ]);

    buildInputs = with pkgs; [
      glib
      gtk3
      at-spi2-atk
      cairo
      pango
      gdk-pixbuf
      libsoup_3
      webkitgtk_4_1
      librsvg
      openssl
      sqlite
      # Exercise videos carry WebVTT subtitles, which only gst-plugins-bad can
      # render. wrapGAppsHook4 turns the gst_* buildInputs into
      # GST_PLUGIN_SYSTEM_PATH_1_0, so an installed binary finds them without an
      # extra wrapper -- which would only nest a second one around this.
      gst_all_1.gstreamer
      gst_all_1.gst-plugins-base
      gst_all_1.gst-plugins-bad
    ];

    postInstall = ''
      # Shipped with the package so any launcher can find both by name, whichever
      # way it was installed.
      mkdir -p "$out/share/icons/hicolor/128x128/apps"
      cp maia-ui/src-tauri/icons/128x128.png \
        "$out/share/icons/hicolor/128x128/apps/maia-ui.png"

      mkdir -p "$out/share/applications"
      ln -s ${desktopItem}/share/applications/maia.desktop \
        "$out/share/applications/maia.desktop"
    '';

    meta = {
      description = "Maia desktop application";
      mainProgram = "maia-ui";
    };
  };

  # The legacy daemon: a file watcher on a Unix socket, superseded by the Tauri
  # app. Kept so existing installs do not break.
  maia-ui = packageData "maia" ../Cargo.toml;
  maia-chrome = packageData "chrome" ../chrome/Cargo.toml;

  maia-full = pkgs.symlinkJoin {
    name = "maia-full";
    paths = [ maia-ui ];
    meta = {
      description = "Maia - Personal management tool with AI-powered features";
      mainProgram = "maia";
    };
  };
in
{
  inherit
    sync-server
    maia-desktop
    maia-ui
    maia-chrome
    maia-full
    source
    ;
}