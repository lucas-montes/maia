{
  description = "Maia - Personal management tool with AI-powered features";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
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

        packages = import ./nix/packages.nix { inherit pkgs; };
        android = import ./nix/android.nix { inherit pkgs; };
        devShells = import ./nix/devshells.nix {
          inherit pkgs android;
          rust-bin = pkgs.rust-bin;
          sce = sce.packages.${system}.default;
        };
      in
      {
        devShells = {
          default = devShells.default;
        };

        packages = {
          ui = packages.maia-ui;
          chrome = packages.maia-chrome;
          full = packages.maia-full;
          default = packages.maia-full;
        };

        apps = {
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
