# Component Architecture Split — Desktop & Chrome Extension as Separate Repos

## Change summary
Two-phase split, matching how `mobile` and `server` are already fully independent sibling repos under
`/home/lucas/Projects/maia/` with no cross-references (not even flake inputs) between them:

**Phase 1 (stay in this repo):** purify the boundary first. `maia-ui/src-tauri` currently links the
`maia` crate directly and does two kinds of in-process coupling: a handful of direct calls into
`maia::config`/`maia::statements` (confirmed: `Config::shared`, `statements::SourceId`,
`config::overlay_config_path` — about a dozen call sites), and ~30 direct read-only SQLite queries
against the shared `maia.db` file for list/detail Tauri commands (notes, workouts, tasks, goals,
ingredients, receipts, etc. — writes already go through HTTP push endpoints; this was confirmed by
grep, not assumed). Both become HTTP calls against the already-running `maia` service, reusing existing
pull endpoints where they already return equivalent data and adding new ones where they don't.

**Phase 2 (the actual split):** once `maia-ui/src-tauri` has zero Rust dependency on the `maia` crate,
extract `maia-ui/` into its own sibling repo (e.g. `/home/lucas/Projects/maia/desktop/`), and `chrome_ext/`
into its own (e.g. `/home/lucas/Projects/maia/chrome-extension/`), each with history preserved via `git
filter-repo`/`git subtree split`, mirroring the existing `mobile`/`server` separation exactly.

## Success criteria
- [ ] Every one of the ~30 direct SQLite read sites in `maia-ui/src-tauri/src/lib.rs` is replaced with
      an HTTP call (via the existing `reqwest` client, same Bearer-auth pattern) to an existing or new
      sync-server endpoint.
- [ ] The dozen direct `maia::config`/`maia::statements` calls are replaced: config resolution the
      desktop needs (API key, bind address, overlay path) goes through whatever minimal mechanism makes
      sense without re-reading the server's config file directly — likely the desktop keeps reading its
      *own* copy of the overlay file (it's the Settings form that owns writing it, after all) without
      needing the `maia` crate's helper function, just a small local duplicate or a shared tiny
      "config-paths" crate if duplication is too lossy.
- [ ] `maia-ui/src-tauri/Cargo.toml` no longer depends on `maia` (`path = "../.."`) or directly on
      `rusqlite` for its own queries (keeping `rusqlite` only if some other narrow need remains after
      the above, otherwise dropped entirely).
- [ ] `maia-ui/` and `chrome_ext/` are extracted into their own git repos with history preserved,
      verified by `git log` on the extracted repos showing the original commits, not a single "import"
      commit.
- [ ] The root `Cargo.toml` workspace no longer lists `maia-ui/src-tauri` as a member; the root repo's
      Nix packaging (`nix/packages.nix`, `flake.nix`) stops building `maia-desktop` (that build moves to
      the new desktop repo's own flake) — `chrome-ext`'s packaging moves similarly.
- [ ] `cargo check -p maia` (now the only Rust package in this repo, plus `ai`) stays green throughout
      Phase 1; the new desktop repo gets its own working `cargo check`/`nix build` before Phase 2 is
      considered done.

## Constraints and non-goals
**Constraints:**
- Phase 1 must land completely before Phase 2 starts — splitting a repo that still has a cross-repo
  Rust path dependency just trades one coupling problem for an uglier one (git/path dependencies across
  repos).
- The server stays the single database writer and the single source of truth; this plan only removes
  the desktop's *read* shortcut, it doesn't change who owns writes (already HTTP-only).
- Preserve git history on the extraction — a fresh `git init` losing the file history for `maia-ui/`
  and `chrome_ext/` is not acceptable given the repo's existing convention (`mobile`/`server` both have
  their full history).

**Non-goals:**
- No behavior changes visible to the user — this is purely an internal architecture/repo-boundary
  cleanup. Every existing desktop feature should work identically before and after.
- Does not address the Nix flake's cross-package build orchestration beyond what's needed to keep
  `maia-desktop` buildable from its new home — a deeper look at whether the three repos should be wired
  together via flake inputs (vs. fully independent, like `mobile` is today) is a separate decision, not
  assumed here.

## Assumptions
- The new desktop/chrome-extension repo locations are siblings of `server`/`mobile` under
  `/home/lucas/Projects/maia/` — matching the existing layout, not nested inside `server/`.
- Config resolution duplication (desktop re-deriving the overlay config path itself instead of asking
  the `maia` crate) is acceptable — it's a handful of lines, not worth a shared crate just for this. If
  more shared logic surfaces during Phase 1, revisit whether a tiny shared `maia-config-paths` crate
  (published as its own thing, or vendored) is worth it instead of duplicating.
- "New endpoints where none exist yet" will be discovered during Phase 1's actual work, not fully
  enumerated up front — some of the 30 read sites (e.g. ingredient detail with nested pictures+prices)
  may need a new endpoint shaped exactly like what the Tauri command currently assembles by hand.

## Task stack (T01..T06)

- [ ] T01: Inventory the exact gap per read site (status:open)
  - Goal: For each of the ~30 direct-query Tauri commands, confirm whether an existing pull endpoint
    already returns equivalent data, or note what a new endpoint needs to return.
  - Done when: a concrete table (command → existing endpoint reused / new endpoint needed) exists to
    drive T02-T03.

- [ ] T02: Replace direct SQLite reads with HTTP calls (status:open)
  - Goal: Rewrite each Tauri command per T01's table; add the handful of new sync-server endpoints
    identified as missing.
  - Done when: every desktop screen that reads data still works, now backed by HTTP instead of a direct
    `rusqlite::Connection`.

- [ ] T03: Replace the remaining direct `maia::` calls (status:open)
  - Goal: Resolve config/statement-source-enum needs without the crate dependency — likely local
    duplication of `overlay_config_path`-equivalent logic and a local copy of the small `SourceId` enum
    (or an HTTP endpoint returning the known source list, if that's cleaner).
  - Done when: `grep -rn "maia::" maia-ui/src-tauri/src/` is empty.

- [ ] T04: Drop the dependency (status:open)
  - Goal: Remove `maia = { path = "../.." }` from `maia-ui/src-tauri/Cargo.toml`; remove `rusqlite` too
    if nothing else needs it; remove `maia-ui/src-tauri` from the root workspace `members`.
  - Done when: `cargo check -p maia` (root) builds with no knowledge of `maia-ui` at all; `maia-ui/
    src-tauri` builds standalone (still inside this repo, as its own independent Cargo project) with no
    workspace membership.

- [ ] T05: Extract `maia-ui/` into its own repo (status:open)
  - Goal: `git filter-repo`/`subtree split` preserving history, push to a new sibling repo location, set
    up that repo's own flake (porting the relevant bits of `nix/packages.nix`/`flake.nix`), remove
    `maia-ui/` and the desktop-building Nix code from this repo.
  - Done when: the new repo builds (`nix build .#maia-desktop` or equivalent) standalone; this repo no
    longer contains `maia-ui/`.

- [ ] T06: Extract `chrome_ext/` into its own repo (status:open)
  - Goal: Same extraction treatment for the Chrome extension.
  - Done when: the new repo exists with preserved history; this repo no longer contains `chrome_ext/`
    or its Nix packaging.

## Risks / gotchas
- T02 is where real behavior regressions could sneak in — a hand-assembled direct-SQL response and an
  existing HTTP endpoint's response shape won't always match field-for-field (different casing,
  missing/extra fields, different default-value handling). Diff the actual JSON shapes before assuming
  a drop-in swap, not just "it's the same table."
- History-preserving extraction (`git filter-repo`) is unforgiving of mistakes — work on a throwaway
  clone first, verify the extracted history looks right, before touching the real repo or deleting
  anything from it.
- Until T05/T06 land, this repo is temporarily carrying dead weight (a desktop app and chrome extension
  that build but are slated to move) — fine as a transitional state, but don't let Phase 1 linger
  indefinitely with the extraction perpetually "later."
