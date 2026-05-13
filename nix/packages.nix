{ pkgs, ... }:
let
  readCargoToml = path: builtins.fromTOML (builtins.readFile path);

  # Generic helper to build a workspace member as a release binary.
  packageData =
    name: cargoPath:
    pkgs.rustPlatform.buildRustPackage {
      pname = name;
      version = (readCargoToml cargoPath).package.version;
      src = ../.;

      cargoLock = {
        lockFile = ../Cargo.lock;
      };

      buildPhase = ''
        cargo build --release -p ${name}
      '';

      installPhase = ''
        mkdir -p $out/bin
        cp target/release/${name} $out/bin/${name}
      '';

      nativeBuildInputs = with pkgs; [ pkg-config ];
      buildInputs = with pkgs; [ openssl ];
      doCheck = false;
    };

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
  inherit maia-ui maia-chrome maia-full;
}
