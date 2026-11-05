# Notes — Markdown Mirror + Editor + Audio Round-Trip

## Change summary
Keep SQLite as the sync source of truth for notes, but have the server mirror every note to a `.md` file
in the (currently unused) `notes_dir` config (`<dataDir>/notes` per the Nix module). Edits made directly to
the files (in any external editor) are read back into the DB via a file watcher; deleting a file is
non-destructive (the note is re-exported on the next scan). On the mobile (fitfat/Flutter) side, replace the
plain title+body form with a raw-markdown editor plus a preview toggle, and make note voice clips
(`note_audio`) round-trip by adding a byte endpoint and pulling them down. Sync remains manual via the sync
hub; conflict resolution stays last-write-wins on `updated_at`. Tags participate fully in the file mirror.

## Success criteria
- [ ] Every non-deleted note exists as `<notes_dir>/<date>-<slug>-<short-id>.md` with YAML frontmatter
      (`id`, `title`, `created_at`, `updated_at`, `tags: [...]`) and the markdown body below it.
- [ ] Editing a `.md` file in place updates the note in the server DB within ~1s (watcher), bumps
      `updated_at` from file mtime, and shows up on the phone after a manual pull (LWW).
- [ ] Creating a `.md` file without a known `id` in frontmatter adopts it into the DB with a new UUIDv7.
- [ ] Same-id file rename is detected from frontmatter and renamed in place (not delete+recreate).
- [ ] Deleting a `.md` file recreates the note file (DB is authoritative on deletes; DB tombstones still
      remove the file and are not recreated).
- [ ] Server's own export writes do not re-import (self-write suppression via content-hash/mtime signature
      map, rebuilt on startup reconcile scan).
- [ ] Notes are exported/updated through the mirror on every push/pull handler touch and local note mutation,
      even when no phone is connected.
- [ ] `GET /notes/audio/:id` serves the stored mp3 bytes with Bearer auth (404 for missing/deleted).
- [ ] `PullNotesResponse` includes a `noteAudio` list (rows + `note_id`); mobile applies them and downloads
      missing clip bytes to its existing local clips directory so players/list UIs work unchanged.
- [ ] Mobile note editor: title + raw markdown body (monospace) with a preview toggle rendering markdown;
      save persists via the existing repository; tags picker unchanged.
- [ ] `cargo test/check` green; mobile `flutter analyze`/build green.

## Constraints and non-goals
**Constraints:**
- Server stack additions: `notify` (file watcher); parse frontmatter with a minimal hand-rolled loader or a
  small YAML crate only if needed (avoid heavy deps).
- SQLite DB remains the single source of truth; the mirror is a derived projection with import feedback.
- Timestamps epoch ms everywhere; file imports set `updated_at` from file mtime (LWW, single user).
- Mobile markdown rendering uses a maintained 2026 package — `flutter_markdown` is discontinued; preferred
  `flutter_markdown_plus` (drop-in) or `markdown_widget` (pick at implementation based on current state).
- Sync stays manual (sync hub); no automatic/background sync.
- Full markdown file layout: one file per note, filename `<ISO-date>-<title-slug>-<id-prefix(8)>.md`;
  files without matching `id` but with valid frontmatter id are matched by id.

**Non-goals:**
- No automatic/background sync, no incremental push, no conflict-resolution UI, no line-level merge.
- No desktop (Tauri) note editing UI changes (it edits the same files on disk for free).
- No audio bytes inside the markdown files (frontmatter may list clip refs; bytes stay in
  `<db_dir>/note_audio/`).
- No markdown rendering of existing exercise bold-hack code paths.

## Assumptions
- User approved: hybrid storage (DB truth + file mirror), file-watcher readback, safe (recreate) file deletes,
  frontmatter file format, manual sync, LWW conflicts, full tags+audio round-trip, raw+preview mobile editor —
  clarifications 2026-11-02.
- `notes_dir` config already exists (`server/sync-server/src/config.rs:92`, Nix module sets `<dataDir>/notes`)
  and must finally be consumed.
- Mobile API client already supports byte downloads (`getBytes`) and multipart upload; push side of audio is
  complete and stays as-is.
- Server-side migrations go through the append-only `MIGRATIONS` in `db.rs` (`user_version`, currently v4);
  mobile schema (Drift v39) likely needs no version bump (audio table already exists).

## Task stack (T01..T07)

- [x] T01: Markdown mirror module — export/import (status:done)
  - Task ID: T01
  - Goal: Create `sync-server/src/notes_mirror.rs`: export non-deleted notes to `notes_dir` as
    `<date>-<slug>-<short-id>.md` with frontmatter (id/title/created/updated/tags) + body; parse frontmatter
    + body on import (update title/body/tags, bump `updated_at` from mtime; adopt unknown ids with UUIDv7;
    upsert tags by name into `tags`/`note_tags`); full reconcile scan (import changed/new, export missing,
    tombstoned notes delete the file) run at server start before routes serve.
  - Boundaries (in/out of scope): In - `notes_mirror.rs`, config consumption of `notes_dir`, startup
    reconcile call. Out - watcher (T02), HTTP route changes (T04/T05), mobile.
  - Done when: unit tests cover export round-trip (export→import→export idempotent), frontmatter parse,
    unknown-id adoption, tag upsert; `cargo test -p sync-server` passes.
  - Verification notes (commands or checks): `cargo test -p sync-server notes_mirror`; manual run shows
    `.md` files in `notes_dir` with correct frontmatter.

- [x] T02: File watcher readback + self-write suppression (status:done)
  - Task ID: T02
  - Goal: Watch `notes_dir` with `notify` (create/modify/delete/rename); import changed files within ~1s;
    maintain signature map `note_id -> (content hash, mtime)` of our own exports and ignore events whose
    content hash matches what we wrote (no ping-pong); rebuild map on startup scan. File delete → recreate
    the note file (erase from signature map) unless the note is tombstoned in DB (then keep deletion).
    Detect same-id rename via frontmatter and rename in place (update export filename state).
  - Boundaries (in/out of scope): In - watcher integration, suppression map, delete-recreate, rename
    detection, wiring touchpoints into push/pull handlers and local note mutations. Out - mobile, audio.
  - Done when: integration test shows edit→DB update, own-write drawdown (no import of own export), delete
    →recreate, tombstone→file removed; file rename propagates without deleting/recreating the note.
  - Verification notes (commands or checks): `cargo test -p sync-server notes_mirror_watcher`; manual test
    editing a file with an external editor and confirming DB/`GET /notes` change.

- [x] T03: Audio byte endpoint + noteAudio in pull (status:done)
  - Task ID: T03
  - Goal: Add `GET /notes/audio/:id` serving the mp3 from `<db_dir>/note_audio/` (Bearer-authed, 404 for
    missing/deleted); extend `PullNotesResponse` (`pull.rs`) with a `noteAudio` list containing row fields (`id`, `note_id`, `duration_ms`, `position`,
    `created_at`, `updated_at`; no `audio_path` bytes), selected alongside notes.
  - Boundaries (in/out of scope): In - `handlers/pull.rs`, route, cached byte reader. Out - push changes
    (complete), mobile (T06).
  - Done when: pull response includes `noteAudio` rows for a seeded note; audio bytes endpoint returns 200
    with identical bytes and 404 with wrong/missing id; auth enforced.
  - Verification notes (commands or checks): `cargo test -p sync-server`: pull contains `noteAudio`;
    `curl -i -H "Authorization: Bearer $KEY" .../notes/audio/<id>` → 200 matching bytes; no auth → 401.

- [x] T04: Mirror hookups in push/pull handlers (status:done)
  - Task ID: T04
  - Goal: Call mirror export/update from `POST /notes` push handler (created/updated/deleted notes), pull
    handler (nothing to mirror on read, but tags/tombstones ensure consistency), and any local note mutation
    path, so the directory stays current without a phone.
  - Boundaries (in/out of scope): In - `handlers/push.rs`, `handlers/pull.rs` touchpoints. Out - T01/T02
    internals.
  - Done when: pushing a new note from the phone produces its `.md` file immediately; deleting from the
    phone removes the file (no recreate).
  - Verification notes (commands or checks): cargo integration test posting a note then reading the file;
    manual sync hub test.

- [x] T05: Mobile audio pull (status:done)
  - Task ID: T05
  - Goal: `PersonalPullService.pullNotes`: apply `noteAudio` rows via `insertOnConflictUpdate`; after pull,
    download bytes for clip ids whose local file is missing/empty via `apiClient.getBytes("notes/audio/<id>")`
    into the existing local clips dir used by `note_repository` (so players/list UIs work unchanged); accept
    the `noteAudio` field in the pull model.
  - Boundaries (in/out of scope): In - `personal_pull_service.dart`, pull models, local dir cache fill.
    Out - push services, editor UI (T06).
  - Done when: a clip recorded+pushed on the phone, then wiped locally (reinstall sim), reappears playable
    after manual pull; no double downloads within one session.
  - Verification notes (commands or checks): `flutter analyze`; manual sync hub pull test with a deleted
    local clip.

- [x] T06: Mobile markdown editor (status:done)
  - Task ID: T06
  - Goal: Rework `note_editor_screen.dart`: title field + monospace raw markdown body `TextField` +
    icon-button preview toggle rendering the body (maintained markdown package: `flutter_markdown_plus` or
    `markdown_widget`); keep save flow, tags picker, voice clips (`VoiceNotesField`) unchanged. Cheap extra:
    tiny toolbar inserting `**bold**`, `_italic_`, heading, list markers.
  - Boundaries (in/out of scope): In - `note_editor_screen.dart`, `pubspec.yaml` deps. Out - new features
    beyond toolbar, notes list screen changes.
  - Done when: preview renders headings/bold/lists/links from the raw text; existing save/sync flow
    unchanged; `flutter analyze` clean.
  - Verification notes (commands or checks): `flutter analyze`; manual editor walkthrough on device/emulator.

- [ ] T07: End-to-end verification + docs (status:open)
  - Task ID: T07
  - Goal: Full manual e2e: create note on phone (tags+clip) → push → file appears on server; edit file in
    external editor → watcher imports → pull on phone shows edit (LWW); edit on phone → push → file updated;
    desktop/Tauri or filesystem delete → recreated; DB delete → file removed. Update `server/context/overview.md`
    notes section with the mirror contract.
  - Boundaries (in/out of scope): In - verification, docs tweak. Out - new code paths.
  - Done when: all above flows pass; docs state the mirror semantics (truth = DB, deletes = recreate,
    suppress self-writes).
  - Verification notes (commands or checks): checklist run-through; `server/context/overview.md` updated.

## Risks / gotchas
- Self-write suppression is the trickiest part: refine events to content-hash match, else capability for
  import/export fighting.
- File mtime granularity can coarse to seconds on ext4/msdos — acceptable for LWW but two edits within one
  tick may pick a slightly older file; fine for a single user.
- `flutter_markdown` discontinued → do not add it; choose `markdown_widget` (flexible) or
  `flutter_markdown_plus` (drop-in) after checking pub versions at implementation time.
- Watcher recursion/backpressure: debounce events (e.g. 300–500 ms) per file id.
