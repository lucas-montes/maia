{
  description = "Maia - Personal management tool with AI-powered features";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.05";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [(import rust-overlay)];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        rust-bin-custom = pkgs.rust-bin.stable.latest.default.override {
          extensions = ["rust-src"];
        };

        readCargoToml = path: builtins.fromTOML (builtins.readFile path);

        packageData = name: cargoPath: pkgs.rustPlatform.buildRustPackage {
          pname = name;
          version = (readCargoToml cargoPath).package.version;
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          buildPhase = ''
            cargo build --release -p ${name}
          '';

          installPhase = ''
            mkdir -p $out/bin
            cp target/release/${name} $out/bin/${name}
          '';

          nativeBuildInputs = with pkgs; [pkg-config];
          buildInputs = with pkgs; [openssl];
          doCheck = false;
        };

        maia-ui = packageData "maia" ./Cargo.toml;
        maia-daemon = packageData "daemon" ./daemon/Cargo.toml;
        maia-chrome = packageData "chrome" ./chrome/Cargo.toml;

        # Native Messaging manifest generator
        nativeMessageManifest = { hostPath, allowedOrigins ? [] }:
          pkgs.writeTextFile {
            name = "maia.chrome.json";
            text = builtins.toJSON {
              name = "maia.chrome";
              description = "Maia Native Messaging Host";
              path = hostPath;
              type = "stdio";
              allowed_origins = allowedOrigins;
            };
          };

        maia-full = pkgs.symlinkJoin {
          name = "maia-full";
          paths = [maia-ui maia-daemon];
          meta = {
            description = "Maia - Personal management tool with AI-powered features (ui + daemon)";
            mainProgram = "maia";
          };
        };
      in {
        # Development shell
        devShells.default = with pkgs;
          mkShell {
            buildInputs = [
              openssl
              pkg-config
              rust-bin-custom
            ];
          };

        packages = {
          ui = maia-ui;
          daemon = maia-daemon;
          full = maia-full; # Combined package
          default = maia-full; # Make ui the default
          chrome = maia-chrome;
        };

        # Apps
        apps = {
          ui = flake-utils.lib.mkApp {
            drv = maia-ui;
            name = "maia";
          };
          daemon = flake-utils.lib.mkApp {
            drv = maia-daemon;
            name = "maia-daemon";
          };
          default = self.apps.${system}.ui;
        };

        # NixOS module for system-wide installation
        nixosModules.default = {
          config,
          lib,
          pkgs,
          ...
        }:
          with lib; let
            cfg = config.services.maia;
          in {
            options.services.maia = {
              enable = mkEnableOption "Maia personal management assistant";

              package = mkOption {
                type = types.package;
                default = self.packages.${pkgs.system}.default;
                description = "The Maia package to use.";
              };

              dataDir = mkOption {
                type = types.str;
                default = "/var/lib/maia";
                description = "Directory to store Maia data.";
              };
            };

            config = mkIf cfg.enable {
              systemd.services.maia = {
                description = "Maia Personal Management Assistant";
                wantedBy = ["multi-user.target"];
                after = ["network.target"];

                environment = {
                  MAIA_DB_PATH = "${cfg.dataDir}/maia.db";
                  MAIA_NOTES_DIR = "${cfg.dataDir}/notes";
                  # Add GEMINI_API_KEY here if you want to configure it system-wide
                };

                serviceConfig = {
                  ExecStart = "${self.packages.${pkgs.system}.daemon}/bin/daemon";
                  Restart = "on-failure";
                  User = "maia";
                  Group = "maia";

                  # Lockdown
                  PrivateTmp = true;
                  ProtectSystem = "strict";
                  ProtectHome = true;
                  ReadWritePaths = cfg.dataDir;
                };
              };

              users.users.maia = {
                isSystemUser = true;
                group = "maia";
                description = "Maia service user";
                home = cfg.dataDir;
                createHome = true;
              };

              users.groups.maia = {};
            };
          };
      }
    );
}
