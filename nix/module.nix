{
  self,
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.maia;
  inherit (lib) mkIf mkOption mkEnableOption types optionalAttrs;

  maiaUser = "maia";
  maiaGroup = "maia";

  # The desktop app opens the same SQLite file as the service user. In WAL mode
  # SQLite needs write access to the database, to the -wal and -shm sidecars, and
  # to the directory they live in.
  #
  # New files get the directory's group thanks to the setgid bit, and UMask =
  # "0007" makes them group writable. That covers what either process creates
  # from here on; this fixes up whatever already exists, which needs privilege
  # because some of it is owned by the desktop user rather than the service user.
  fixPermissions = pkgs.writeShellScript "maia-fix-permissions" ''
    set -euo pipefail
    target="''$1"
    chgrp -R ${maiaGroup} "$target"
    chmod -R g+rwX "$target"
  '';
in
{
  options.services.maia = {
    enable = mkEnableOption "Maia sync server";

    package = mkOption {
      type = types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.sync-server;
      description = "The sync-server package to run.";
    };

    dataDir = mkOption {
      type = types.path;
      default = "/var/lib/maia";
      description = ''
        Writable state: the database, pre-migration snapshots and uploaded
        backups. Created by systemd's StateDirectory, so it needs no imperative
        setup.
      '';
    };

    sourceDir = mkOption {
      type = types.path;
      default = "/home/lucas/Projects/maia/server";
      description = ''
        Read-only checkout. Holds the exercise images served by
        `GET /media/*`: the media directory is otherwise derived from the
        database location, so this is what lets the database live in /var/lib
        while the images stay in the working tree.
      '';
    };

    port = mkOption {
      type = types.port;
      default = 3030;
      description = "Port the API listens on. Must match the phone's configured endpoint.";
    };

    apiKeyFile = mkOption {
      type = types.path;
      default = "${cfg.dataDir}/secrets.env";
      defaultText = "/var/lib/maia/secrets.env";
      description = ''
        File read as the unit's EnvironmentFile, holding MAIA_API_KEY and
        optionally MAIA_MODEL_API_KEY. Create it before enabling this module,
        owned by root:${maiaGroup} with mode 0600. It is not managed by Nix
        because a store path cannot hold a secret.
      '';
    };

    seedExercises = mkOption {
      type = types.bool;
      default = false;
      description = ''
        Seed the exercise catalogue from `sourceDir/data` when the `exercises`
        table is empty. Off by default: seeding copies thousands of media files
        and runs before the listener binds, which would blow past systemd's
        start timeout on a first boot. Import an existing database instead.
      '';
    };

    openFirewall = mkOption {
      type = types.bool;
      default = true;
      description = "Open `port` so the phone can reach the server on the LAN.";
    };

    extraUsers = mkOption {
      type = with types; listOf str;
      default = [ ];
      description = ''
        Users that need write access to the shared database, added to the
        ${maiaGroup} group. The desktop app is why this exists: it runs as a
        normal user and opens the same SQLite file.
      '';
    };
  };

  config = mkIf cfg.enable {
    users.users.${maiaUser} = {
      isSystemUser = true;
      group = maiaGroup;
      description = "Maia sync server";
    };

    # Declared with its members rather than as an empty group above, so the two
    # definitions do not collide.
    users.groups.${maiaGroup}.members = [ maiaUser ] ++ cfg.extraUsers;

    systemd.services.maia = {
      description = "Maia sync server";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];

      environment = {
        MAIA_DB_PATH = "${cfg.dataDir}/maia.db";
        MAIA_BIND_ADDR = "0.0.0.0:${toString cfg.port}";
        # Read from sourceDir rather than derived from the database location.
        MAIA_MEDIA_DIR = "${cfg.sourceDir}/exercise_media";
        RUST_LOG = "info";
      } // optionalAttrs cfg.seedExercises {
        MAIA_SEED_DIR = "${cfg.sourceDir}/data";
      };

      serviceConfig = {
        # Carries MAIA_API_KEY and MAIA_MODEL_API_KEY.
        environmentFiles = [ cfg.apiKeyFile ];

        # `+` runs this with full privileges: the files it fixes predate the unit
        # and are not all owned by the service user.
        ExecStartPre = [
          "+${fixPermissions} ${cfg.dataDir}"
          # Applies pending migrations, and exits non-zero when the database is
          # newer than this binary. That is what turns a Nix rollback into a
          # visible failure here instead of a server that cannot read the schema.
          "${cfg.package}/bin/sync-server migrate"
        ];
        ExecStart = "${cfg.package}/bin/sync-server serve";
        # `systemctl reload maia` migrates without dropping the listener.
        ExecReload = "${cfg.package}/bin/sync-server migrate";

        Restart = "on-failure";
        # Room for the WAL checkpoint during a clean SIGTERM drain.
        TimeoutStopSec = 30;

        User = maiaUser;
        Group = maiaGroup;
        # setgid so files either process creates stay in the shared group.
        StateDirectoryMode = "2775";
        UMask = "0007";

        # The exercise images live in a home directory, so /home has to be
        # readable. Nothing under it is writable, thanks to ProtectSystem plus
        # ReadWritePaths.
        ProtectHome = "read-only";
        ProtectSystem = "strict";
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        PrivateTmp = true;
        PrivateDevices = true;
        NoNewPrivileges = true;
        RestrictSUIDSGID = true;
        ReadWritePaths = [ cfg.dataDir ];
      };
    };

    networking.firewall = mkIf cfg.openFirewall {
      allowedTCPPorts = [ cfg.port ];
    };
  };
}