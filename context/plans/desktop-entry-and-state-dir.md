# Desktop Entry Discovery and First-Boot State Directory

## Change summary
Two defects found while bringing `services.maia` up on luctop.

The desktop app never appeared in rofi: the module wrote its `.desktop` file to
`/etc/applications`, which is not on `XDG_DATA_DIRS` and is therefore scanned by
nobody. Separately, the service never started at all — it failed on every boot with
`status=226/NAMESPACE` because `ReadWritePaths` named a `StateDirectory` that systemd
has not created yet at the point it builds the mount namespace.

## Evidence

- `XDG_DATA_DIRS` contains `/run/current-system/sw/share` but not `/etc`. Rofi's `drun`
  scans `$XDG_DATA_HOME/applications` and `$XDG_DATA_DIRS/applications`.
  `/etc/applications` contains exactly one file, `maia.desktop`, and it is invisible.
  Every entry that does appear ships inside its package: see
  `/run/current-system/sw/share/applications/blueman-manager.desktop` with `Icon=blueman`
  and `blueman.png` in `share/icons/hicolor/128x128/apps/`.
- `systemctl status maia`: `Active: failed (Result: start-limit-hit)`, with
  `ExecStartPre=+maia-fix-permissions /var/lib/maia → status=226/NAMESPACE` and
  `Failed to set up mount namespacing: /var/lib/maia: No such file or directory`.
  `/var/lib/maia` does not exist.
- `StateDirectory` is already made writable under `ProtectSystem=strict`, so the
  `ReadWritePaths` entry was redundant as well as fatal: of twelve nixpkgs modules
  combining the two, ten omit it; `usbguard`, one of the two that do not, uses
  `ReadWritePaths` for `/dev/shm` and `/tmp`. On this machine `bluetooth.service` and
  `systemd-timesyncd.service` both run `ProtectSystem=strict` with a `StateDirectory`
  and an empty `ReadWritePaths`.

## Root causes

**Discovery.** `environment.etc."applications/maia.desktop"` puts the file in
`/etc/applications`. NixOS keeps `/etc` off `XDG_DATA_DIRS`, so no XDG-conforming
launcher — rofi among them — reads that directory.

**State directory.** systemd applies the mount namespace (`ProtectSystem`,
`ReadWritePaths`) before creating `StateDirectory` in the exec child. On a first boot
the path does not exist, and systemd 260 treats a missing `ReadWritePaths` entry as
fatal rather than warning about it.

## Success criteria
- [ ] `systemctl is-active maia` → active, surviving reboot
- [ ] `curl http://<lan-ip>:3030/health` → 200
- [ ] `Maia` appears in rofi and its icon renders
- [ ] `/run/current-system/sw/share/applications/maia.desktop` exists; `/etc/applications/maia.desktop` does not
- [ ] `/var/lib/maia` exists after a switch, with mode `2775` and group `maia`, before the service starts
- [ ] `lucas` can still read and write `maia.db` and its WAL sidecars
- [ ] `ExecStartPre` order: `migrate` before the privileged permission fix

## Constraints and non-goals
**Constraints:** no new options; the desktop entry ships with the package so it also
works for anyone installing it outside this module; `dataDir` ownership keeps matching
`StateDirectoryMode` so systemd and tmpfiles do not fight.
**Non-goals:** no change to the config-file design or the API surface; no keybinding in
the Hyprland config; no verification of the Tauri window itself, which needs a live
session.

## Task stack

- [ ] T01: Let the service start on a fresh system (status:todo)
  - Task ID: T01
  - Goal: Remove the fatal `ReadWritePaths` reference and make `/var/lib/maia` exist at activation.
  - Boundaries (in/out of scope): In — `nix/module.nix`: drop `ReadWritePaths = [ cfg.dataDir ]`; add `systemd.tmpfiles.rules` creating `/var/lib/maia` as `2775 maia maia`; swap the two `ExecStartPre` entries so the sandboxed `migrate` runs before the privileged `chgrp`/`chmod`. Out — the desktop entry (T02), packaging, dotfiles.
  - Done when: the unit evaluates, `/var/lib/maia` exists after a switch without starting the service, and the service comes up.
  - Verification notes (commands or checks): `nixos-rebuild switch`; `systemctl is-active maia`; `ls -ld /var/lib/maia`; `journalctl -u maia -n 20`; `curl -H "Authorization: Bearer $KEY" localhost:3030/health`; `sudo -u lucas sqlite3 /var/lib/maia/maia.db "PRAGMA user_version;"`

- [ ] T02: Ship the desktop entry inside the package (status:todo)
  - Task ID: T02
  - Goal: Put `maia.desktop` where launchers look, and stop writing one to `/etc/applications`.
  - Boundaries (in/out of scope): In — `nix/packages.nix`: a `pkgs.makeDesktopItem` (`name = "maia"`, `exec = "maia-ui"`, `icon = "maia-ui"`, `categories = [ "Utility" ]`) symlinked into `$out/share/applications/`; `nix/module.nix`: remove `environment.etc."applications/maia.desktop"`. The icon keeps its existing `share/icons/hicolor/128x128/apps/maia-ui.png` install, referenced by name so the entry does not embed a store path. Out — `makeDesktopItem` as a module output, which does not exist in this nixpkgs.
  - Done when: the entry is in the system profile's `share/applications`, rofi lists Maia, and nothing is left in `/etc/applications`.
  - Verification notes (commands or checks): `ls -l /run/current-system/sw/share/applications/maia.desktop`; `ls /etc/applications/` shows no maia entry; `rofi -show drun` lists Maia with its icon; `nix why-depends .#maia-desktop gtk3` still resolves the icon theme at runtime.

- [ ] T03: Re-lock, rebuild and verify together (status:todo)
  - Task ID: T03
  - Goal: Get the fixes onto the running system and confirm both symptoms are gone.
  - Boundaries (in/out of scope): In — `~/.dotfiles`: `nix flake update maia`, then `nixos-rebuild switch --flake ~/.dotfiles#luctop`; confirm service active, rofi entry present, firewall still `[ 22 3030 ]`. Out — new configuration.
  - Done when: `systemctl is-active maia` is active and Maia is listed by rofi.
  - Verification notes (commands or checks): `systemctl is-active maia`; `ls /run/current-system/sw/share/applications/maia.desktop`; `curl -H "Authorization: Bearer $KEY" http://<lan-ip>:3030/health`

## Open questions

None. The two causes were established from the running system rather than inferred, so
no design choice remains open.
