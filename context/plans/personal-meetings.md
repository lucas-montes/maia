# Plan: Meeting Recording

**Status:** Proposed
**Depends on:** system-daemon-wiring (for socket and DB)

## Summary
Add a simple meeting recording system: start/stop a meeting session, save notes and timestamp, optionally attach audio or a transcript reference.

## Tasks

### T01 — Meetings database table
- Create `meetings` table: id, title, date, start_time, end_time, participants TEXT, notes TEXT, audio_path TEXT (optional), created_at
- Derive `Crud` for the model
- Add migration in `initialize_schema()`

### T02 — Meeting recording CLI
- CLI: `meeting start <title>` — creates a meeting record, sets start_time = now
- CLI: `meeting end [<meeting_id>]` — sets end_time = now, prompts for notes
- CLI: `meeting list` — show recent meetings
- CLI: `meeting show <id>` — show full details
- Wire through socket to daemon CRUD handlers

### T03 — Quick notes during meeting
- CLI: `meeting note <meeting_id> <text>` — append a timestamped note line
- Notes stored as newline-separated entries in the `notes` field or a separate `meeting_notes` table (if we want richer editing)
- Simple approach: append to notes field with `[HH:MM] note text`

### T04 — Audio recording integration (optional)
- CLI: `meeting record <meeting_id>` — starts `arecord`/`ffmpeg` in background
- CLI: `meeting stop-record <meeting_id>` — stops recording, saves to `audio_path`
- No transcription yet (future: feed to AI for summarization)
- Make this opt-in (requires audio tools on system)

### T05 — Meeting summary output
- CLI: `meeting summary <id>` — formatted output: title, date, duration, participants, notes
- CLI: `meeting export <id> --format md` — export as markdown for the knowledge base
- Link the exported markdown into the knowledge base notes

### T06 — Validation & cleanup
- Test start/end flow calculates correct duration
- Test note appending with timestamps
- Test list/show/summary outputs
- Verify empty states (no meetings yet, no notes, etc.)

## Done checks
- `meeting start/end/list/show` works end-to-end
- Notes are timestamped and retrievable
- Duration is calculated correctly
- Export produces valid markdown
