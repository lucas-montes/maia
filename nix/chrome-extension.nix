{ pkgs, maia-chrome }:
{ extensionId ? "nmnehdcbpdcljhliiegidjdlngjljbhc", nativeHostPath ? "${maia-chrome}/bin/chrome", srcDir ? "${pkgs.brave.config}/Brave-Browser/NativeMessagingHosts" }:
let
  # Native Messaging manifest
  nativeMessagingManifest = pkgs.writeText "maia.chrome.json" (builtins.toJSON {
    name = "maia.chrome";
    description = "Native Messaging Host for Maia Chrome Extension";
    path = nativeHostPath;
    type = "stdio";
    allowed_origins = ["chrome-extension://${extensionId}/"];
  });

  # Package extension files
  extensionFiles = pkgs.stdenv.mkDerivation {
    name = "maia-chrome-extension";
    src = srcDir;
    installPhase = ''
      cp ${nativeMessagingManifest} $out
    '';
  };
in
pkgs.symlinkJoin {
  name = "maia-chrome-full";
  paths = [maia-chrome extensionFiles];
  meta = {
    description = "Maia Chrome extension with native messaging host";
  };
}
