{
  pkgs,
  rust-bin,
  sce,
  ...
}: let
  # Host-only Rust toolchain (the rest of the workspace is still Rust;
  rust = rust-bin.stable.latest.default.override {
    extensions = [
      "rust-src"
      "rust-analyzer"
    ];
  };

  default = pkgs.mkShell {
    name = "maia-dev";

    nativeBuildInputs = [
      pkgs.pkg-config
      pkgs.wrapGAppsHook4
      pkgs.cargo-tauri # Optional, Only needed if Tauri doesn't work through the traditional way.
    ];

    buildInputs = [
      rust
      sce
      pkgs.openssl
      pkgs.pnpm
      pkgs.nodejs
      pkgs.sqlite
      pkgs.librsvg
      pkgs.webkitgtk_4_1
    ];

    shellHook = ''
      export XDG_DATA_DIRS="$GSETTINGS_SCHEMAS_PATH" # Needed on Wayland to report the correct display scale
    '';
  };
in {
  inherit default;
}
