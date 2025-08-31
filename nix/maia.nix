{ self, nixpkgs }:
{ config, lib, pkgs, ... }:
with lib;
let
  cfg = config.services.maia;
in {
  options.services.maia = {
    enable = mkEnableOption "Maia personal management assistant";
    package = mkOption {
      type = types.package;
      default = self.packages.${pkgs.system}.full;
      description = "The Maia package to use (includes CLI, daemon, and Chrome extension).";
    };
    dataDir = mkOption {
      type = types.str;
      default = ".config/maia"; # Or use some default value passed to put it in the correct default place
      description = "Directory to store Maia data.";
    };
    chrome = {
      enable = mkEnableOption "Maia Chrome extension";
      extensionId = mkOption {
        type = types.str;
        default = "neecokkmjndnihfokcpamlnkmgihelni";
        description = "Chrome extension ID.";
      };
      nativeHostPath = mkOption {
        type = types.str;
        default = "${self.packages.${pkgs.system}.brave}";
        description = "Path to the native messaging host.";
      };
      extensionDir = mkOption {
        type = types.str;
        default = "${self.packages.${pkgs.system}.chrome}/extension";
        description = "Directory containing Chrome extension files.";
      };
      user = mkOption {
        type = types.str;
        default = "maia";
        description = "User under which the Chrome extension is installed.";
      };
    };
  };

  config = mkIf cfg.enable {
    # Maia daemon service
    systemd.services.maia = {
      description = "Maia Personal Management Assistant";
      wantedBy = ["multi-user.target"];
      after = ["network.target"];
      environment = {
        MAIA_DB_PATH = "${cfg.dataDir}/maia.db";
        MAIA_NOTES_DIR = "${cfg.dataDir}/notes";
      };
      serviceConfig = {
        ExecStart = "${cfg.package}/bin/daemon";
        Restart = "on-failure";
        User = "maia";
        Group = "maia";
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
    # set up the Maia Chrome extension in Brave or the chromium browser used
    };
  };
}
