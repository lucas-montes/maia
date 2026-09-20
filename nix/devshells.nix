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
      pkgs.gst_all_1.gstreamer
      pkgs.gst_all_1.gst-plugins-base
      pkgs.gst_all_1.gst-plugins-bad
    ];

    shellHook = ''
      export XDG_DATA_DIRS="$GSETTINGS_SCHEMAS_PATH" # Needed on Wayland to report the correct display scale
      export GST_PLUGIN_PATH="${pkgs.gst_all_1.gst-plugins-bad}/lib/gstreamer-1.0:${pkgs.gst_all_1.gst-plugins-base}/lib/gstreamer-1.0:${pkgs.gst_all_1.gstreamer}/lib/gstreamer-1.0:$GST_PLUGIN_PATH"
    '';
  };
in {
  inherit default;
}
