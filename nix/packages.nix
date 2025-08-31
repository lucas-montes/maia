{ pkgs }:
let

        readCargoToml = path: builtins.fromTOML (builtins.readFile path);

        packageData = name: cargoPath: pkgs.rustPlatform.buildRustPackage {
          pname = name;
          version = (readCargoToml cargoPath).package.version;
          src = ./.;

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

          nativeBuildInputs = with pkgs; [pkg-config];
          buildInputs = with pkgs; [openssl];
          doCheck = false;
        };


in
 {
          maia-cli = packageData "maia" ../Cargo.toml;
        maia-daemon = packageData "daemon" ../daemon/Cargo.toml;
        maia-chrome = packageData "chrome" ../chrome/Cargo.toml;
}
