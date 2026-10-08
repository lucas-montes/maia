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

  # The desktop app and the service open the same SQLite file. In WAL mode
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

  # Everything the server needs, in one file. Rendered rather than assembled
  # from environment variables so that what the service reads is inspectable in
  # one place, and so the port the firewall opens and the port the server binds
  # cannot drift apart.
  settings = {
    database = "${cfg.dataDir}/maia.db";
    bind_addr = "0.0.0.0:${toString cfg.port}";
    # Read from sourceDir rather than derived from the database location, which
    # is what lets the database live in /var/lib while the images stay in the
    # working tree.
    media_dir = "${cfg.sourceDir}/exercise_media";
    notes_dir = "${cfg.dataDir}/notes";
    api_key = cfg.apiKey;
    model_api = cfg.modelApiKey;
    model_name = cfg.modelName;
  } // optionalAttrs (cfg.seedDir != null) {
    seed_dir = cfg.seedDir;
  };

  configFile = "/etc/maia/config.json";
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
        Read-only checkout. Holds the exercise images served by `GET /media/*`
        and, when `seedDir` is set, the catalogue used to seed it.
      '';
    };

    seedDir = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = ''
        Where to import the exercise catalogue from, looked for as `<dir>/parsed`,
        when the `exercises` table is empty. Null disables seeding, which is the
        right default: seeding copies thousands of media files and runs before the
        listener binds, so it would exceed the systemd start timeout on a first
        boot. Import an existing database instead.
      '';
    };

    port = mkOption {
      type = types.port;
      default = 3030;
      description = ''
        Port the API listens on. Written into `bind_addr` in the generated config
        and used for the firewall rule, so the two cannot disagree.
      '';
    };

    apiKey = mkOption {
      type = types.str;
      # No default on purpose: falling back silently would leave the API
      # reachable on the LAN with a key that is in the source.
      default = "";
      description = ''
        Bearer token the API requires. Rendered into the generated config file,
        which is a symlink into /nix/store and therefore readable by any local
        user -- worth knowing, and no worse than the compiled-in fallback, but
        set it rather than relying on the default.
      '';
    };

    modelApiKey = mkOption {
      type = types.str;
      default = "";
      description = "Vision-model key for receipt OCR. Empty leaves the parse endpoints answering 503.";
    };

    modelName = mkOption {
      type = types.str;
      default = "gemini-2.5-flash";
      description = "Vision model id used for receipt OCR.";
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

    desktop = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Install the desktop application, so it can be launched on demand while
          the API keeps running as a service. It reads the same configuration and
          the same database as the service, so there is nothing extra to start.
        '';
      };

      package = mkOption {
        type = types.package;
        default = self.packages.${pkgs.stdenv.hostPlatform.system}.maia-desktop;
        defaultText = "maia-desktop package";
        description = "The Tauri desktop application to install.";
      };
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

    environment.etc."maia/config.json" = {
      source = pkgs.writeText "maia-config.json" (builtins.toJSON settings);
    };

    # Created here as well as by StateDirectory, so it exists before the unit runs.
    # StateDirectory only creates it in the exec child, which leaves the desktop app
    # unable to open the database whenever the service is not currently running --
    # and the two are decoupled now. The mode and group match StateDirectoryMode so
    # systemd and tmpfiles do not disagree.
    systemd.tmpfiles.rules = [
      "d ${cfg.dataDir} 2775 ${maiaUser} ${maiaGroup} - - "
    ];

    systemd.services.maia = {
      description = "Maia sync server";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];

      serviceConfig = {
        # `migrate` first: it is sandboxed, so systemd creates the StateDirectory
        # for it, which means the directory exists by the time the privileged
        # permission fix below runs.
        ExecStartPre = [
          # Applies pending migrations, and exits non-zero when the database is
          # newer than this binary. That is what turns a Nix rollback into a
          # visible failure here instead of a server that cannot read the schema.
          "${cfg.package}/bin/sync-server --config ${configFile} migrate"
          # `+` runs this with full privileges: the files it fixes predate the unit
          # and are not all owned by the service user.
          "+${fixPermissions} ${cfg.dataDir}"
        ];
        ExecStart = "${cfg.package}/bin/sync-server --config ${configFile} serve";
        # `systemctl reload maia` migrates without dropping the listener.
        ExecReload = "${cfg.package}/bin/sync-server --config ${configFile} migrate";

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
        # No ReadWritePaths for dataDir: systemd makes StateDirectory writable
        # under ProtectSystem, and naming it here instead is fatal on a first boot.
        # The mount namespace is built before the directory is created, and a
        # missing path in ReadWritePaths is an error rather than a warning.
      };
    };

    networking.firewall = mkIf cfg.openFirewall {
      allowedTCPPorts = [ cfg.port ];
    };

    # The desktop half is optional and independent of the service: it is a client
    # of the API and a reader of the database, so it needs neither the port nor
    # the service to be up in order to install.
    environment.systemPackages = mkIf cfg.desktop.enable [ cfg.desktop.package ];

    environment.etc."applications/maia.desktop" = mkIf cfg.desktop.enable {
      source = pkgs.writeText "maia.desktop" ''
        [Desktop Entry]
        Name=Maia
        Comment=Personal management suite
        Exec=maia-ui
        Icon=${cfg.desktop.package}/share/icons/hicolor/128x128/apps/maia-ui.png
        Type=Application
        Categories=Utility;
        Terminal=false
        StartupNotify=true
      '';
      mode = "0644";
    };
  };
}