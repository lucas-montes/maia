{ self }:
{
  config,
  lib,
  pkgs,
  ...
}:
with lib;
let
  cfg = config.services.maia;
in
{
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
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];

      environment = {
        MAIA_DB_PATH = "${cfg.dataDir}/maia.db";
        MAIA_NOTES_DIR = "${cfg.dataDir}/notes";
      };

      serviceConfig = {
        ExecStart = "${cfg.package}/bin/maia";
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

    users.groups.maia = { };
  };
}
