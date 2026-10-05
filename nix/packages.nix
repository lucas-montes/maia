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
  inherit sync-server maia-ui maia-chrome maia-full source;
}