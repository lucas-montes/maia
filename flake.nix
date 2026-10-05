{
  description = "Maia - Personal management tool with AI-powered features";

  inputs = {
    # Standalone default, used by `nix develop` in this repo. Consumers should
    # override it with `maia.inputs.nixpkgs.follows = "nixpkgs"`, which is how the
    # NixOS module avoids building these packages against a different channel
    # than the system's.
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    sce.url = "github:crocoder-dev/shared-context-engineering";
  };

  outputs =
    {
      self,
      nixpkgs,
      rust-overlay,
      flake-utils,
      sce,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        # Android SDK / NDK derivations require accepting Google's license.
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
          config = {
            allowUnfree = true;
            android_sdk.accept_license = true;
          };
        };

        packages = import ./nix/packages.nix {
          inherit pkgs;
          lib = nixpkgs.lib;
        };
        devShells = import ./nix/devshells.nix {
          inherit pkgs;
          rust-bin = pkgs.rust-bin;
          sce = sce.packages.${system}.default;
        };
      in
      {
        devShells = {
          default = devShells.default;
        };

        packages = {
          inherit (packages) sync-server;
          ui = packages.maia-ui;
          chrome = packages.maia-chrome;
          full = packages.maia-full;
          default = packages.maia-full;
        };

        apps = {
          # The headless server, which is what a deployment actually runs.
          sync-server = flake-utils.lib.mkApp {
            drv = packages.sync-server;
            name = "sync-server";
          };
          ui = flake-utils.lib.mkApp {
            drv = packages.maia-ui;
            name = "maia";
          };
          default = self.apps.${system}.ui;
        };
      }
    )
    // {
      # System-independent outputs (NixOS module).
      nixosModules.default = import ./nix/module.nix { inherit self; };
    };
}
