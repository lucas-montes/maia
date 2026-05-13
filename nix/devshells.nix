{
  pkgs,
  rust-bin,
  sce,
  android,
  ...
}:
let
  # Host-only Rust toolchain (the rest of the workspace is still Rust;
  # mobile/desktop UI work goes through Flutter, so no Android cross targets).
  rust = rust-bin.stable.latest.default.override {
    extensions = [
      "rust-src"
      "rust-analyzer"
    ];
  };

  default = pkgs.mkShell {
    name = "maia-dev";

    # Android tooling expects these to be set in the environment.
    ANDROID_HOME = android.androidHome;
    ANDROID_SDK_ROOT = android.androidHome;
    ANDROID_NDK_HOME = android.ndkHome;
    ANDROID_NDK_ROOT = android.ndkHome;
    NDK_HOME = android.ndkHome;

    GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${android.androidHome}/build-tools/${android.buildToolsVersion}/aapt2";

    JAVA_HOME = "${pkgs.jdk17}/lib/openjdk";

    # Used by `flutter run -d chrome`.
    CHROME_EXECUTABLE = "${pkgs.chromium}/bin/chromium";

    nativeBuildInputs = with pkgs; [
      pkg-config
      cmake
      ninja
      clang
    ];

    buildInputs =
      [
        rust
        sce
        android.androidSdk
      ]
      ++ (with pkgs; [
        # Host build deps
        openssl
        sqlite

        # General Rust workflow tooling
        cargo-watch
        cargo-nextest

        # JVM toolchain required by Gradle / Android builds
        jdk17
        gradle

        # Flutter SDK (mobile + Linux desktop + web)
        flutter

        # Linux desktop build deps for `flutter run -d linux`
        gtk3
        glib
        pcre2
        libepoxy
        libsysprof-capture
        mesa
        xorg.libX11

        # Web target
        chromium

        # Device interaction / debugging
        scrcpy
        usbutils
      ]);

    shellHook = ''
      echo "🛠  Maia dev shell"
      echo "  ANDROID_HOME      = $ANDROID_HOME"
      echo "  ANDROID_NDK_HOME  = $ANDROID_NDK_HOME"
      echo "  JAVA_HOME         = $JAVA_HOME"
      echo "  CHROME_EXECUTABLE = $CHROME_EXECUTABLE"
      echo ""
      echo "Try:  cargo build                 # Rust workspace"
      echo "      flutter doctor"
      echo "      flutter run -d linux        # desktop"
      echo "      flutter run -d <device-id>  # Android (see: flutter devices)"
    '';
  };
in
{
  inherit default;
}
