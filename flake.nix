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

        #TODO: point to the correct cargo file
        cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

        # Build the Maia CLI package
        maia-cli = pkgs.rustPlatform.buildRustPackage {
          pname = "maia-cli";
          version = cargoToml.package.version;
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          # Only build the CLI binary
          buildPhase = ''
            cargo build --release --bin maia-cli
          '';

          installPhase = ''
            mkdir -p $out/bin
            cp target/release/maia-cli $out/bin/maia
          '';

          nativeBuildInputs = with pkgs; [pkg-config];
          buildInputs = with pkgs; [openssl];
          doCheck = false;
        };

        # Build the Maia daemon package
        maia-daemon = pkgs.rustPlatform.buildRustPackage {
          pname = "maia-daemon";
          version = cargoToml.package.version;
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          # Only build the daemon binary
          buildPhase = ''
            cargo build --release --bin maia-daemon
          '';

          installPhase = ''
            mkdir -p $out/bin
            cp target/release/maia-daemon $out/bin/maia-daemon
          '';

          nativeBuildInputs = with pkgs; [pkg-config];
          buildInputs = with pkgs; [openssl];
          doCheck = false;
        };
        maia-full = pkgs.symlinkJoin {
          name = "maia-full-${cargoToml.package.version}";
          paths = [maia-cli maia-daemon];
          meta = {
            description = "Maia - Personal management tool with AI-powered features (CLI + daemon)";
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
          cli = maia-cli;
          daemon = maia-daemon;
          full = maia-full; # Combined package
          default = maia-full; # Make CLI the default
        };

        # Apps
        apps = {
          cli = flake-utils.lib.mkApp {
            drv = maia-cli;
            name = "maia";
          };
          daemon = flake-utils.lib.mkApp {
            drv = maia-daemon;
            name = "maia-daemon";
          };
          default = self.apps.${system}.cli;
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
                  ExecStart = "${self.packages.${pkgs.system}.daemon}/bin/maia-daemon";
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
