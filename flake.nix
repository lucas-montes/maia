{
  description = "Maia - Personal management tool with AI-powered features";

  inputs = {
    # Standalone default, used by `nix develop` in this repo. Consumers should
    # override it with `maia.inputs.nixpkgs.follows = "nixpkgs"`, which is how the
    # NixOS module avoids building these packages against a different channel
    # than the system's.
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    sce.url = "github:crocoder-dev/shared-context-engineering";
    crane.url = "github:ipetkov/crane";
    crane-tauri.url = "github:JPHutchins/crane-tauri";
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    flake-utils,
    crane,
    crane-tauri,
    sce,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        # Android SDK / NDK derivations require accepting Google's license.
        pkgs = import nixpkgs {
          inherit system;
          overlays = [rust-overlay.overlays.default];
          config = {
            allowUnfree = true;
            android_sdk.accept_license = true;
          };
        };

        packages = import ./nix/packages.nix {
          inherit pkgs crane-tauri;
          lib = nixpkgs.lib;
          craneLib = crane.mkLib pkgs;
        };
        devShells = import ./nix/devshells.nix {
          inherit pkgs;
          rust-bin = pkgs.rust-bin;
          sce = sce.packages.${system}.default;
        };
      in {
        devShells = {
          default = devShells.default;
        };

        packages = {
          inherit (packages) sync-server maia-desktop frontend;
        };

        apps = {
          # The headless server, which is what a deployment actually runs.
          sync-server = flake-utils.lib.mkApp {
            drv = packages.sync-server;
            name = "sync-server";
          };
          ui = flake-utils.lib.mkApp {
            drv = packages.maia-desktop;
            name = "maia";
          };
          default = self.apps.${system}.ui;
        };
      }
    )
    // {
      # System-independent outputs (NixOS module).
      # `self` cannot be resolved inside the module itself, so it is threaded in
      # here. The wrapper matters: calling the module function directly would
      # freeze its arguments at import time and leave config, lib and pkgs
      # undefined when the module system applies it.
      nixosModules.default = {
        config,
        lib,
        pkgs,
        ...
      }:
        import ./nix/module.nix {inherit self config lib pkgs;};
    };
}
