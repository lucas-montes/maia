{
  self,
  config,
  lib,
  pkgs,
  ...
}: let
  cfg = config.services.maia;
  inherit (lib) mkIf mkOption mkEnableOption types optionalAttrs mapAttrs hasPrefix removePrefix;

  userHome = (config.users.users.${cfg.user} or {}).home or null;

  # Where things live, by who owns them:
  #
  #   - Service state (the database, archived receipts, notes, exercise media)
  #     lives in `dataDir`, /var/lib/maia by default: only the service and the
  #     desktop app (same uid) touch it, and systemd's StateDirectory creates,
  #     owns and sandboxes it.
  #   - Files the user drops in or browses (statement exports and their
  #     lifecycle) live under the user's XDG data home.
  #   - Secrets (the sync key, model keys, feed credentials) live in the
  #     per-user overlay under the XDG config home, written by the desktop
  #     Settings screen -- never in this module, so never in the Nix store.
  overlayDir = "${cfg.xdg.configHome}/maia";
  overlayFile = "${overlayDir}/config.json";

  # systemd only manages state directories under /var/lib; anywhere else the
  # directory is created by tmpfiles and granted with ReadWritePaths instead.
  stateDirName =
    if hasPrefix "/var/lib/" cfg.dataDir
    then removePrefix "/var/lib/" cfg.dataDir
    else null;
  insideDataDir = path: hasPrefix "${cfg.dataDir}/" path;

  statementDirs = lib.optionals (cfg.statementsRoot != null) (
    map (name: "${cfg.statementsRoot}/${name}") ["dump" "staged" "parsed" "discarded" "failed"]
  );

  # Writable directories outside the state directory: each needs a tmpfiles
  # rule (so it exists before the unit starts -- ReadWritePaths= fails on a
  # missing path) and a ReadWritePaths= grant (ProtectHome= makes /home
  # read-only).
  writableDirs =
    lib.optional (stateDirName == null) cfg.dataDir
    ++ lib.filter (path: !(insideDataDir path)) [cfg.notesDir cfg.archiveDir]
    ++ lib.optional (cfg.xdg.configHome != null) overlayDir
    ++ lib.optionals (cfg.statementsRoot != null) ([cfg.statementsRoot] ++ statementDirs)
    ++ lib.optional (cfg.statementsIntakeDir != null) cfg.statementsIntakeDir;

  # Everything the server needs, in one file. Rendered rather than assembled
  # from environment variables so that what the service reads is inspectable in
  # one place, and so the port the firewall opens and the port the server binds
  # cannot drift apart.
  settings =
    {
      database = "${cfg.dataDir}/maia.db";
      bind_addr = "0.0.0.0:${toString cfg.port}";
      # Not read by the server; rendered so Settings can show the live value.
      open_firewall = cfg.openFirewall;
      media_dir = cfg.mediaDir;
      notes_dir = cfg.notesDir;
      # Where the server moves receipt images when a receipt is archived.
      archive_dir = cfg.archiveDir;
      # Statement imports resolve FX into this currency. The phone keeps its own
      # display currency; this one is what the server stores alongside each draft.
      base_currency = cfg.baseCurrency;
      # Firstrade PDF statements are read through poppler. An absolute store
      # path, so the unit does not depend on what happens to be on PATH.
      pdftotext_bin = "${pkgs.poppler-utils}/bin/pdftotext";
      # Model endpoints and which tasks use them. Keys are not here: they are
      # joined in from the overlay by model id.
      models = mapAttrs (_: model: {
        inherit (model) provider model vision;
        base_url = model.baseUrl;
      }) cfg.models;
      tasks = {
        receipt_ocr = cfg.tasks.receiptOcr;
        receipt_matching = cfg.tasks.receiptMatching;
        statement_extraction = cfg.tasks.statementExtraction;
      };
      statements =
        {
          feeds.trading212 = {
            enabled = cfg.trading212.enable;
            environment = cfg.trading212.environment;
            lookback_days = cfg.trading212.lookbackDays;
          };
        }
        # Lifecycle dirs for dropped statements. A parsed file moves through
        # these by itself: staged while its entries await review, parsed once
        # they are imported, discarded once they are dismissed. Unset only when
        # there is no home to hang them on; the server then falls back to the
        # XDG default under its own HOME.
        // optionalAttrs (cfg.statementsRoot != null) {
          dump_dir = "${cfg.statementsRoot}/dump";
          staged_dir = "${cfg.statementsRoot}/staged";
          parsed_dir = "${cfg.statementsRoot}/parsed";
          discarded_dir = "${cfg.statementsRoot}/discarded";
          failed_dir = "${cfg.statementsRoot}/failed";
        }
        // optionalAttrs (cfg.statementsIntakeDir != null) {
          intake_dir = cfg.statementsIntakeDir;
        };
    }
    // optionalAttrs (cfg.xdg.configHome != null) {
      # The service has no XDG environment of its own, so it is told where the
      # overlay is rather than guessing from HOME.
      overlay_file = overlayFile;
    }
    // optionalAttrs (cfg.settingsFile != null) {
      # Read by the desktop Settings screen, which edits that file and shows
      # the rebuild command for this host.
      nix_settings_file = cfg.settingsFile;
      nixos_host = config.networking.hostName;
    }
    // optionalAttrs (cfg.seedDir != null) {
      seed_dir = cfg.seedDir;
    };

  configFile = "/etc/maia/config.json";

  modelType = types.submodule ({config, ...}: {
    options = {
      provider = mkOption {
        type = types.enum ["gemini" "openai"];
        default = "gemini";
        description = ''
          Wire protocol. `openai` is any OpenAI-compatible
          `/chat/completions` endpoint: OpenAI, OpenRouter, Ollama, llama.cpp,
          vLLM.
        '';
      };
      baseUrl = mkOption {
        type = types.str;
        default =
          if config.provider == "gemini"
          then "https://generativelanguage.googleapis.com/v1beta/models"
          else "https://api.openai.com/v1";
        defaultText = "the provider's public endpoint";
        example = "http://localhost:11434/v1";
        description = "API root. For OpenRouter `https://openrouter.ai/api/v1`; for a local Ollama `http://localhost:11434/v1`.";
      };
      model = mkOption {
        type = types.str;
        example = "gemini-2.5-flash";
        description = "The provider's model id.";
      };
      vision = mkOption {
        type = types.bool;
        default = false;
        description = "Whether the model reads images. Only vision models may serve `tasks.receiptOcr`.";
      };
    };
  });
in {
  imports = let
    removed = name: why: lib.mkRemovedOptionModule ["services" "maia" name] why;
  in [
    (removed "apiKey" ''
      The sync API key is a secret and no longer goes through Nix (it ended up
      in the world-readable /etc/maia/config.json and the store). Set it in the
      desktop app: Settings -> Server, which writes the per-user overlay.'')
    (removed "modelApiKey" ''
      Model keys are secrets and no longer go through Nix. Declare models with
      `services.maia.models`, assign them with `services.maia.tasks`, and set
      each key in the desktop app: Settings -> Models.'')
    (removed "modelName" ''
      Replaced by `services.maia.models.<id>.model` plus `services.maia.tasks`.'')
    (removed "sourceDir" ''
      Only the exercise media was read from the checkout. Set
      `services.maia.mediaDir` to `<checkout>/exercise_media` instead.'')
  ];

  options.services.maia = {
    enable = mkEnableOption "Maia sync server";

    package = mkOption {
      type = types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.maia;
      description = "The maia package to run.";
    };

    xdg = {
      dataHome = mkOption {
        type = types.nullOr types.str;
        default =
          if userHome != null
          then "${userHome}/.local/share"
          else null;
        defaultText = "\${home}/.local/share";
        description = ''
          The service user's XDG data home. Read here rather than from the
          environment, which Nix cannot see at evaluation time; set it from
          home-manager's `config.xdg.dataHome` if yours is elsewhere.
        '';
      };
      configHome = mkOption {
        type = types.nullOr types.str;
        default =
          if userHome != null
          then "${userHome}/.config"
          else null;
        defaultText = "\${home}/.config";
        description = "The service user's XDG config home, which holds the secrets overlay `maia/config.json`.";
      };
    };

    dataDir = mkOption {
      type = types.str;
      default = "/var/lib/maia";
      description = ''
        Service state: the database, pre-migration snapshots, archived
        receipts, notes and uploaded backups. Under /var/lib it is systemd's
        StateDirectory; anywhere else it is created by tmpfiles and granted to
        the unit. Owned by `user`.

        Nothing here changes ownership of what is already present, so an
        imported database has to arrive writable: copy it as `user`, or follow
        it with `chown -R $(id -un): dataDir`. A `sudo cp` leaves the copy owned
        by root, and SQLite opens read-write.
      '';
    };

    mediaDir = mkOption {
      type = types.str;
      default = "${cfg.dataDir}/exercise_media";
      defaultText = "\${dataDir}/exercise_media";
      description = ''
        Exercise images and videos served by `GET /media/*`. Read-only to the
        service, so it may point into a checkout to keep the media (several GB)
        out of /var/lib.
      '';
    };

    notesDir = mkOption {
      type = types.str;
      default = "${cfg.dataDir}/notes";
      defaultText = "\${dataDir}/notes";
      description = "Where notes are mirrored as Markdown.";
    };

    archiveDir = mkOption {
      type = types.str;
      default = "${cfg.dataDir}/receipts_archive";
      defaultText = "\${dataDir}/receipts_archive";
      description = "Where the server moves receipt images when a receipt is archived.";
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

    seedDir = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = ''
        Where to import the exercise catalogue from, looked for as `<dir>/parsed`,
        when the `exercises` table is empty. Null disables seeding, which is the
        right default: seeding copies thousands of media files and runs before the
        listener binds, so it would exceed the systemd start timeout on a first
        boot. Import an existing database instead.
      '';
    };

    settingsFile = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "/home/lucas/.dotfiles/services/maia-settings.json";
      description = ''
        Absolute path of the JSON file this host's configuration imports into
        `services.maia` (`builtins.fromJSON (builtins.readFile ./that.json)`).
        The desktop Settings screen edits it and shows the rebuild command.

        No default: it must sit inside the flake that configures this host,
        and be tracked by git, because a pure flake cannot read anything else.
        While null, Settings shows these values read-only.
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

    baseCurrency = mkOption {
      type = types.strMatching "[A-Za-z]{3}";
      default = "EUR";
      description = ''
        Currency statement imports resolve into. Stored on each draft
        transaction as `amount_base`, using the latest FX snapshot on or before
        the line's posting date. The phone keeps its own display currency; this
        one is the server's bookkeeping currency.
      '';
    };

    models = mkOption {
      type = types.attrsOf modelType;
      default = {};
      example = {
        gemini-flash = {
          model = "gemini-2.5-flash";
          vision = true;
        };
        local = {
          provider = "openai";
          baseUrl = "http://localhost:11434/v1";
          model = "qwen2.5vl:7b";
          vision = true;
        };
      };
      description = ''
        Saved model endpoints, by id. Keys are set per id in the desktop
        Settings screen and kept in the overlay, never here.
      '';
    };

    tasks = let
      task = what: mkOption {
        type = types.listOf types.str;
        default = [];
        description = ''
          Model ids for ${what}, in fallback order: each is tried until one
          answers, and the one that did is recorded beside the result. Empty
          turns the feature off.
        '';
      };
    in {
      receiptOcr = task "reading receipt images (vision models only)";
      receiptMatching = task "proposing receipt store, transaction and line matches";
      statementExtraction = task "reading activity out of statement PDFs";
    };

    statementsRoot = mkOption {
      type = types.nullOr types.str;
      # A string rather than `types.path` on purpose: a path value is copied
      # into the Nix store when the config is rendered, which would publish
      # personal bank exports to every local user.
      default =
        if cfg.xdg.dataHome != null
        then "${cfg.xdg.dataHome}/maia/statements"
        else null;
      defaultText = "\${xdg.dataHome}/maia/statements";
      description = ''
        Root of the statement lifecycle: files are dropped into
        `<root>/dump/<source>/` and staged automatically by the server, which
        then keeps each file where the state of its statements says it belongs
        -- `<root>/staged/<source>/` while entries await review,
        `<root>/parsed/<source>/` once they are imported,
        `<root>/discarded/<source>/` once they are dismissed, or
        `<root>/failed/<source>/` with a reason beside them when the parse
        never succeeded. Each root and subdirectory is created by the module
        and granted to the service.
      '';
    };

    statementsIntakeDir = mkOption {
      type = types.nullOr types.str;
      default =
        if cfg.statementsRoot != null
        then "${cfg.statementsRoot}/intake"
        else null;
      defaultText = "\${statementsRoot}/intake";
      description = ''
        A drop folder drained into the dump tree: name an export for its
        source (`firstrade-2026-11.csv`, `t212_feb_aug.csv`, `ca-20261027.csv`)
        and drop it here. The service only ever moves recognised files out, so
        this may also point at a directory used for other things, such as
        ~/Downloads. Null turns the drain off.
      '';
    };

    trading212 = {
      enable = mkOption {
        type = types.bool;
        default = false;
        description = ''
          Poll the Trading 212 history API. Credentials are deliberately not a
          NixOS option: they live in the per-user overlay and are written by the
          desktop Settings screen, so they never land in the world-readable
          /etc/maia/config.json or in the Nix store.
        '';
      };

      environment = mkOption {
        type = types.enum ["live" "demo"];
        default = "live";
        description = "Which Trading 212 API to talk to.";
      };

      lookbackDays = mkOption {
        type = types.addCheck types.int (days: days >= 1 && days <= 3650);
        default = 90;
        description = "How far back each sync fetches, in days.";
      };
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
    assertions = let
      tasks = {
        "tasks.receiptOcr" = cfg.tasks.receiptOcr;
        "tasks.receiptMatching" = cfg.tasks.receiptMatching;
        "tasks.statementExtraction" = cfg.tasks.statementExtraction;
      };
      unknown = lib.concatLists (lib.mapAttrsToList (task: ids:
        map (id: "${task} names `${id}`, which is not in services.maia.models")
        (lib.filter (id: !(cfg.models ? ${id})) ids))
      tasks);
      notVision =
        map (id: "tasks.receiptOcr names `${id}`, which is not a vision model (set models.${id}.vision = true)")
        (lib.filter (id: cfg.models ? ${id} && !cfg.models.${id}.vision) cfg.tasks.receiptOcr);
    in
      map (message: {
        assertion = false;
        message = "services.maia: ${message}";
      }) (unknown ++ notVision);

    environment.etc."maia/config.json" = {
      source = pkgs.writeText "maia-config.json" (builtins.toJSON settings);
    };

    # The data directory is created here as well as by StateDirectory, so it
    # exists before the unit runs: StateDirectory only creates it in the exec
    # child, which would leave the desktop app unable to open the database
    # whenever the service is not running. Owner and mode match StateDirectory
    # so systemd and tmpfiles do not disagree.
    systemd.tmpfiles.rules =
      ["d ${cfg.dataDir} 0755 ${cfg.user} users - -"]
      ++ lib.optional (insideDataDir cfg.mediaDir) "d ${cfg.mediaDir} 0755 ${cfg.user} users - -"
      ++ map (dir: "d ${dir} ${if dir == overlayDir then "0700" else "0750"} ${cfg.user} users - -")
      (lib.unique (lib.filter (dir: dir != cfg.dataDir) writableDirs
        ++ lib.filter insideDataDir [cfg.notesDir cfg.archiveDir]));

    systemd.services.maia = {
      description = "Maia sync server";
      wantedBy = ["multi-user.target"];
      after = ["network.target"];

      serviceConfig = {
        # No ExecStartPre: `serve` applies pending migrations on the way up, via
        # init_db, and refuses to bind if the database is newer than the binary --
        # which is what turns a Nix rollback into a visible failure instead of a
        # server that cannot read the schema.
        ExecStart = "${cfg.package}/bin/maia --config ${configFile} serve";
        # `systemctl reload maia` migrates without dropping the listener.
        ExecReload = "${cfg.package}/bin/maia --config ${configFile} migrate";

        Restart = "on-failure";
        # Room for the WAL checkpoint during a clean SIGTERM drain.
        TimeoutStopSec = 30;

        # Same uid as the desktop app, so both can write the shared database and
        # neither is locked out by the other's SQLite sidecars.
        User = cfg.user;

        # The due-task notifier (`notifier::spawn`) calls `notify-send` from
        # this process, which talks to the *session* D-Bus bus, not the
        # system bus every other systemd system unit gets for free. Without
        # these two variables `notify-send` fails outright with
        # "GDBus.Error:org.freedesktop.DBus.Error.NoReply: Message recipient
        # disconnected from message bus without replying" -- confirmed by
        # running it from a shell with both unset.
        #
        # This only works at all because `User=` above is the same uid as
        # the logged-in desktop session (required anyway, for the shared
        # SQLite file) -- `/run/user/%U/bus` is *that* session's bus, not a
        # new one. A service running as a uid with no logged-in session has
        # nothing to connect to here and `notify-send` will keep failing;
        # that failure is caught and logged, not propagated, so it cannot
        # take the rest of the service down.
        #
        # `%U` is the numeric uid of `User=`, expanded by systemd itself, so
        # this does not need to hardcode this machine's uid (1000) or depend
        # on `cfg.user`'s uid being known at Nix eval time.
        #
        # Verified empirically on this machine (not yet verified end-to-end
        # against the live deployed unit -- see the T03 note in
        # context/plans/waybar-and-notifications.md and the implementation
        # commit message):
        #   - `notify-send` fails with the D-Bus error above when
        #     `DBUS_SESSION_BUS_ADDRESS`/`XDG_RUNTIME_DIR` are unset.
        #   - Setting both manually to the values below fixes it.
        #   - The fix still works wrapped in `bwrap --ro-bind / /`, which
        #     approximates this unit's `ProtectSystem=strict` +
        #     `ProtectHome=read-only` (the latter documented to mount
        #     `/run/user` read-only, not inaccessible) for the purpose of
        #     *connecting* to an already-existing socket.
        Environment = [
          "XDG_RUNTIME_DIR=/run/user/%U"
          "DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/%U/bus"
        ];

        # Exercise images may live in a home directory, so /home has to be
        # readable. ProtectSystem makes the whole hierarchy read-only apart from
        # the state directory and the paths below.
        ProtectHome = "read-only";
        # The overlay (which the server rewrites when a secret changes), the
        # statement lifecycle and intake (which the server empties and fills),
        # and any state kept outside /var/lib. Without these a write fails with
        # EROFS and only shows up at the next restart.
        ReadWritePaths = writableDirs;
        ProtectSystem = "strict";
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        PrivateTmp = true;
        PrivateDevices = true;
        NoNewPrivileges = true;
        RestrictSUIDSGID = true;
      }
      // optionalAttrs (stateDirName != null) {
        # Required, and not implied by StateDirectoryMode: ProtectSystem mounts
        # the whole hierarchy read-only, and StateDirectory is what carves
        # `dataDir` back out as a writable bind mount owned by `user`. Without
        # it SQLite cannot create its -wal and -shm sidecars and startup dies
        # with SQLITE_CANTOPEN. Not listed in ReadWritePaths: on a first boot
        # the mount namespace is built before the directory exists.
        StateDirectory = stateDirName;
        StateDirectoryMode = "0755";
      };
    };

    networking.firewall = mkIf cfg.openFirewall {
      allowedTCPPorts = [cfg.port];
    };

    # The desktop half is optional and independent of the service: it is a client
    # of the API and a reader of the database, so it needs neither the port nor
    # the service to be up in order to install. The desktop entry rides along
    # inside the package, so it lands in the system profile's share/applications,
    # which is on XDG_DATA_DIRS -- unlike /etc/applications.
    environment.systemPackages = mkIf cfg.desktop.enable [cfg.desktop.package];
  };
}
