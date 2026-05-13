{ pkgs, ... }:
let
  # Pin the Android SDK / NDK / tooling versions used for mobile builds.
  # Bump these when upgrading; they are pulled from nixpkgs' androidenv.
  buildToolsVersion = "34.0.0";
  platformVersion = "34";
  ndkVersion = "26.1.10909125";
  cmdLineToolsVersion = "11.0";

  androidComposition = pkgs.androidenv.composeAndroidPackages {
    cmdLineToolsVersion = cmdLineToolsVersion;
    toolsVersion = "26.1.1";
    platformToolsVersion = "35.0.2";
    buildToolsVersions = [ buildToolsVersion ];
    includeEmulator = true;
    emulatorVersion = "35.1.4";
    platformVersions = [ platformVersion ];
    includeSources = false;
    includeSystemImages = true;
    systemImageTypes = [ "google_apis_playstore" ];
    abiVersions = [
      "x86_64"
      "arm64-v8a"
    ];
    includeNDK = true;
    ndkVersions = [ ndkVersion ];
    useGoogleAPIs = true;
    useGoogleTVAddOns = false;
    includeExtras = [ ];
  };

  androidSdk = androidComposition.androidsdk;
  androidHome = "${androidSdk}/libexec/android-sdk";
  ndkHome = "${androidHome}/ndk/${ndkVersion}";
in
{
  inherit
    androidComposition
    androidSdk
    androidHome
    ndkHome
    buildToolsVersion
    platformVersion
    ndkVersion
    ;
}
