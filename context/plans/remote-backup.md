# Remote Storage Backup

## Change summary
Ship the DB backup archive and the raw files that can't be reconstructed from it (receipt images,
exercise media, the notes markdown mirror, voice clips) to an off-site destination via `rclone`,
configured through a new Settings section in the desktop app — same shape as the existing Trading 212
feed settings (`GET`/`PUT` on the overlay config, no new DB table for the setting itself). `rclone`
itself is never configured or authorized by this app: the user runs `rclone config` once per machine to
authorize whichever provider (Google Drive, OneDrive, Dropbox, Koofr, pCloud, etc.) they want, and
`maia` only ever needs the resulting remote name and a destination path.

## Success criteria
- [ ] `GET`/`PUT /remote-backup/settings` (overlay-stored, mirroring `statement-feeds/settings`):
      `enabled`, `rcloneRemote` (e.g. `gdrive:`), `destinationPath` (e.g. `maia-backups/`),
      `intervalHours`.
- [ ] A background task (same interval/debounce/sweep shape as the existing statements/notes-mirror
      watchers) that, when enabled, on the configured interval: regenerates/reuses the latest DB backup
      archive, assembles the raw-file set (receipt images, `exercise_media`, the notes `.md` mirror
      directory, `note_audio`) into a local staging tree, and runs `rclone sync <staging dir>
      <remote>:<path>` via a subprocess.
- [ ] A status surface (`GET /remote-backup/status`: last run time, success/failure, last error message)
      so a failed upload is visible rather than silent.
- [ ] A new desktop Settings section: enable toggle, remote name + destination path fields, interval,
      and a "run now" button; shows last-run status.
- [ ] `nix/module.nix`/`packages.nix` ensure `rclone` is on the service's `PATH` (it is not currently a
      dependency anywhere in this repo).
- [ ] `cargo test -p maia` covers the settings round-trip and the staging-tree assembly (given a fake
      set of files, the right ones land in the staging tree) — the actual `rclone` invocation itself is
      not unit-tested (shells out to a real binary), but the command construction is.

## Constraints and non-goals
**Constraints:**
- `rclone` authorization (OAuth login for Drive/OneDrive/Dropbox/etc.) is entirely out of band — a
  one-time manual `rclone config` the user runs themselves. `maia` only ever reads an already-configured
  remote name; it never stores provider credentials itself.
- Settings live in the overlay config file (`~/.config/maia/config.json`), like the T212 feed settings —
  not a new DB table, since this is per-machine configuration, not synced user data.
- The raw-file set backed up is read-only from `maia`'s perspective here — no changes to how those files
  are stored locally, only an additional copy shipped off-site.

**Non-goals:**
- No in-app OAuth flow, no bundling/vendoring rclone's own config management.
- No restore tooling in this plan — restoring from the remote copy (if ever needed) is a manual `rclone
  copy` back, not a button in the app. Could be a follow-up if it turns out to matter.
- No per-file incremental-upload optimization beyond whatever `rclone sync` already does — not
  reimplementing delta-sync logic.

## Assumptions
- "The DB backup archive" means the existing `/backup` endpoint's artifact — this plan triggers backup
  regeneration on its own schedule rather than only shipping whatever the phone last happened to POST
  (so remote backups don't depend on the phone having synced recently).
- The staging tree is rebuilt fresh each run (not persisted between runs) — simplest, and `rclone sync`
  already handles not re-uploading unchanged files on the remote side.
- One remote/destination at a time — not multiple simultaneous backup targets. Revisit if that turns out
  to matter.

## Task stack (T01..T05)

- [ ] T01: Overlay settings — `GET`/`PUT /remote-backup/settings` (status:open)
  - Goal: Mirror `statement-feeds/settings`'s read/write-the-overlay pattern for the new keys.
  - Done when: a PUT persists to the overlay file and a subsequent GET reflects it; `cargo test -p maia`
    green.

- [ ] T02: Staging-tree assembly (status:open)
  - Goal: A function gathering the latest backup archive + receipt images + `exercise_media` + the notes
    mirror directory + `note_audio` into one local staging directory (symlinks or copies, whichever is
    cheaper and still safe for `rclone sync` to read).
  - Done when: a unit test with a handful of fake files in each source location produces the expected
    staging-tree contents.

- [ ] T03: Scheduled upload task (status:open)
  - Goal: Interval-driven background task (enabled/interval read from the overlay settings, re-read each
    tick so a settings change takes effect without a restart) that assembles the staging tree and shells
    out to `rclone sync`, recording the outcome for T04's status endpoint.
  - Done when: a fake/stub "rclone" script in tests confirms the command is invoked with the expected
    arguments and the staging tree's path; success/failure both update the recorded status correctly.

- [ ] T04: Status endpoint (status:open)
  - Goal: `GET /remote-backup/status` surfacing last run time, outcome, and error message (stored
    alongside the settings in the overlay, or a tiny adjacent file).
  - Done when: a failed run's error message is retrievable; a successful run clears any previous error.

- [ ] T05: Desktop Settings UI + nix packaging (status:open)
  - Goal: New Settings section (enable, remote, path, interval fields; "run now" button; last-status
    display); add `rclone` to the service's runtime `PATH` in `nix/module.nix`/`packages.nix`.
  - Done when: toggling settings from the UI round-trips through T01; "run now" triggers T03's task
    immediately and the status updates once it finishes.

## Risks / gotchas
- `rclone sync` is destructive on the *remote* side (it deletes remote files not present in the source)
  — make sure the destination path is scoped to a dedicated backup folder the user isn't also using for
  other things, or a stray file there could get silently deleted on the next run. Worth calling this out
  explicitly in the Settings UI, not just the docs.
- Large raw-file sets (years of exercise media / receipt images) could make every run slow if the
  staging tree is rebuilt as real file copies rather than symlinks — prefer symlinks where the local
  filesystem allows it, falling back to copies only where `rclone` needs it (some remotes/transports
  don't handle symlinked sources well; verify before assuming this is free).
- `rclone` needs to actually be reachable from the systemd service's sandboxed environment (same family
  of concern as `notify-send` in the Waybar/notifications plan) — a `PATH` entry alone may not be enough
  if the service's filesystem sandboxing (`ProtectHome`, `ReadWritePaths`) also blocks rclone's own config
  /cache directories.
