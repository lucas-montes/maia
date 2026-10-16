{
  pkgs,
  lib,
  crane-tauri,
  craneLib,
  ...
}: let
  readCargoToml = path: builtins.fromTOML (builtins.readFile path);
  src = craneLib.cleanCargoSource ../.;

  # Common arguments can be set here to avoid repeating them later
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
        # Additional darwin specific inputs can be set here
        pkgs.libiconv
      ];

    # Additional environment variables can be set directly
    # MY_CUSTOM_VAR = "some value";
  };

  # Build *just* the cargo dependencies, so we can reuse
  # all of that work (e.g. via cachix) when running in CI
  cargoArtifacts = craneLib.buildDepsOnly commonArgs;

  # Build the actual crate itself, reusing the dependency
  # artifacts from above.
  sync-server = craneLib.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoExtraArgs = "-p sync-server";
    }
  );

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
    categories = ["Utility"];
    terminal = false;
  };
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

  frontend = pkgs.buildNpmPackage {
    pname = "maia-ui-frontend";
    version = "0.1.0";

    src = lib.fileset.toSource {
      root = ../maia-ui;
      fileset = lib.fileset.unions [
        ../maia-ui/package.json
        ../maia-ui/pnpm-lock.yaml
        ../maia-ui/tsconfig.json
        # ../tsconfig.node.json
        ../maia-ui/vite.config.ts
        ../maia-ui/index.html
        ../maia-ui/src
        # ../public
      ];
    };

    npmDepsHash = "sha256-...";

    installPhase = ''
      runHook preInstall
      cp -r dist $out
      runHook postInstall
    '';
  };

  tauri = crane-tauri.lib.buildTauriApp {inherit pkgs craneLib;} {
    pname = "maia-ui";
    version = (readCargoToml ../maia-ui/src-tauri/Cargo.toml).package.version;
    src = ./.;
    inherit frontend;
  };
in {
  inherit
    sync-server
    ;
  maia-desktop = tauri.app;
}
#TODO: fix paths and so on
