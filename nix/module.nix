{
  self,
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.maia;
  inherit (lib) mkIf mkOption mkEnableOption types optionalAttrs;

  # The service and the desktop app open the same SQLite file, so they run as the
  # same user. That makes owner permissions apply to both, and stops SQLite's
  # -wal and -shm sidecars from being a problem: they are created with the
  # creating process's umask, so with two different uids and two different umasks
  # whichever wrote first would lock the other out.
  #
  # This repairs whatever already exists, which needs privilege because an imported
  # database typically arrives owned by root -- `cp` as root preserves root
  # ownership and the source's mode, and SQLite opens read-write, so a 0644 copy
  # leaves nobody able to write it.
  fixOwnership = pkgs.writeShellScript "maia-fix-ownership" ''
    set -euo pipefail
    target="''$1"
    owner="''$2"
    chown -R "$owner":"$owner" "$target"
    chmod -R u+rwX,go+rX "$target"
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
        backups. Created by systemd's StateDirectory and owned by `user`.
      '';
    };

    user = mkOption {
      type = types.str;
      default = "lucas";
      description = ''
        The user to run as. It must be the same user that runs the desktop
        application, because both open the same SQLite file: under one uid,
        owner permissions apply to both and SQLite's `-wal`/`-shm` sidecars
        cannot lock either of them out.
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
    environment.etc."maia/config.json" = {
      source = pkgs.writeText "maia-config.json" (builtins.toJSON settings);
    };

    # Created here as well as by StateDirectory, so it exists before the unit runs.
    # StateDirectory only creates it in the exec child, which would leave the desktop
    # app unable to open the database whenever the service is not currently running --
    # and the two are separate processes. Owner and mode match StateDirectory so
    # systemd and tmpfiles do not disagree.
    systemd.tmpfiles.rules = [
      "d ${cfg.dataDir} 0755 ${cfg.user} users - - "
    ];

    systemd.services.maia = {
      description = "Maia sync server";
      wantedBy = [ "multi-user.target" ];
      after = [ "network.target" ];

      serviceConfig = {
        # Ownership first: `migrate` opens the database read-write, so on a
        # database imported with `sudo cp` -- root-owned, 0644, since cp keeps the
        # source's mode -- it would fail and abort the unit before the repair
        # below could run.
        ExecStartPre = [
          # `+` runs this with full privileges: an imported database is typically
          # owned by root, and the unit cannot chown its own files.
          "+${fixOwnership} ${cfg.dataDir} ${cfg.user}"
          # Applies pending migrations, and exits non-zero when the database is
          # newer than this binary. That is what turns a Nix rollback into a
          # visible failure here instead of a server that cannot read the schema.
          "${cfg.package}/bin/sync-server --config ${configFile} migrate"
        ];
        ExecStart = "${cfg.package}/bin/sync-server --config ${configFile} serve";
        # `systemctl reload maia` migrates without dropping the listener.
        ExecReload = "${cfg.package}/bin/sync-server --config ${configFile} migrate";

        Restart = "on-failure";
        # Room for the WAL checkpoint during a clean SIGTERM drain.
        TimeoutStopSec = 30;

        # Same uid as the desktop app, so both can write the shared database and
        # neither is locked out by the other's SQLite sidecars.
        User = cfg.user;
        StateDirectoryMode = "0755";

        # The exercise images live in a home directory, so /home has to be
        # readable. Nothing under it is writable: ProtectSystem makes the whole
        # hierarchy read-only apart from StateDirectory.
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
    # the service to be up in order to install. The desktop entry rides along
    # inside the package, so it lands in the system profile's share/applications,
    # which is on XDG_DATA_DIRS -- unlike /etc/applications.
    environment.systemPackages = mkIf cfg.desktop.enable [ cfg.desktop.package ];
  };
}