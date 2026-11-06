# Meeting Recording (server-side capture + transcription)

## Change summary
The `maia` service itself captures microphone audio (not the Tauri desktop app, not mobile) — started
and stopped via a new API the desktop UI calls, since it runs on the same machine as the user and the
service's lifetime is independent of whether the desktop window happens to be open. A new `meetings`
entity (distinct from notes, per explicit direction) holds the recording and, once captured, its
Gemini-generated transcript — reusing the existing `ai` crate's generic `InlineData` (mime type + base64
payload) the same way receipt OCR already sends images, just with an audio mime type instead.

## Success criteria
- [ ] A new `meetings` table (id, title, started_at, ended_at, audio_path, transcript, transcript_status,
      transcript_error, created_at, updated_at, deleted_at).
- [ ] `POST /meetings/start` begins a server-side microphone capture (via `cpal` or equivalent),
      returning a meeting id immediately; `POST /meetings/:id/stop` ends capture, finalizes the audio
      file, and kicks off background transcription (same `tokio::spawn` pattern as receipt OCR's
      background parse job — fire-and-poll, not a blocking request).
- [ ] `GET /meetings/:id` reports capture/transcript status (recording / stopped / transcribing /
      transcribed / error) so the desktop UI can poll it exactly like it already polls receipt parse
      status.
- [ ] Transcription sends the captured audio to Gemini via the existing `ai` crate's `InlineData` path
      with an audio mime type, requesting a plain-text transcript (not necessarily structured JSON —
      unlike `ParsedReceipt`, a transcript is just text).
- [ ] A desktop "Meetings" screen: start/stop recording button, a list of past meetings with
      transcript/status, and a detail view showing the transcript and an audio player for the
      recording.
- [ ] `cargo test -p maia` covers the meetings CRUD/status transitions directly; the actual microphone
      capture and real Gemini call are exercised manually (same as the existing `receipt_ocr_live_gemini`
      ignored-by-default test pattern for anything needing a live key/hardware).

## Constraints and non-goals
**Constraints:**
- Capture happens in the `maia` service process, not the Tauri app and not mobile — per explicit
  direction, even though this means the headless service needs real-time microphone access, which is
  the same family of "system service touching user-session resources" problem as the Waybar plan's
  `notify-send` wiring.
- `meetings` is its own table, not a note with an attachment — per explicit direction. It does not
  reuse `note_audio`.
- Reuse the existing `ai` crate's request-building (`InlineData`, `Model`) rather than adding a second
  Gemini client path.

**Non-goals:**
- No real-time/live transcription while recording — transcription happens after `stop`, once the full
  audio file exists, matching how receipt OCR already works (upload, then background parse).
- No speaker diarization, no meeting-notes summarization beyond the raw transcript (could be a
  follow-up once this exists and the knowledge-base similarity-search plan wants to index it).
- No calendar-event linkage in this pass (e.g. auto-attaching a recording to a synced Outlook event) —
  revisit once the calendar-sync plan lands if that turns out to matter.

## Assumptions
- Audio is captured to WAV (uncompressed) — simplest to implement with `cpal` and no new
  encoding dependency; worth revisiting for file size once real meeting lengths are seen in practice
  (an hour of WAV is tens of megabytes, not huge for local storage but non-trivial if it ever needs to
  ship anywhere).
- Only one recording can be active at a time per service instance — starting a second while one is in
  progress is rejected rather than queued or run concurrently.
- The microphone device is whatever the OS default input device is at capture start — no device
  picker in this pass.

## Task stack (T01..T05)

- [ ] T01: Schema — `meetings` table (status:open)
  - Goal: Migration adding the table described above.
  - Done when: migration applies cleanly; `cargo test -p maia` green.

- [ ] T02: Server-side audio capture (status:open)
  - Goal: `cpal`-based (or equivalent) capture module: start writes a WAV file incrementally, stop
    finalizes it; reject a second concurrent start.
  - Done when: a start/stop cycle produces a valid, playable WAV file of the expected rough duration.

- [ ] T03: Start/stop/status routes + background transcription (status:open)
  - Goal: Wire capture into `POST /meetings/start`, `POST /meetings/:id/stop`; on stop, spawn a
    background task sending the audio to Gemini via the `ai` crate and storing the resulting transcript
    (or error) on the row.
  - Done when: status polling reflects each state transition; a fake/stubbed Gemini response in tests
    confirms the transcript (or error) lands on the correct row.

- [ ] T04: `GET /meetings`/`GET /meetings/:id` (status:open)
  - Goal: List + detail endpoints, consistent with how other resources expose pull-style reads.
  - Done when: a completed meeting's transcript and audio path are both retrievable.

- [ ] T05: Desktop Meetings screen (status:open)
  - Goal: Start/stop button, meetings list, detail view with transcript + audio player.
  - Done when: a full start → talk → stop → see transcript walkthrough works end to end against a real
    microphone and a real Gemini key.

## Risks / gotchas
- **Microphone access from a system service is the central risk here**, same family as the Waybar
  plan's `notify-send` problem but likely worse: PipeWire/PulseAudio are normally scoped to a logged-in
  user session, and a `systemd` system unit may need explicit `Environment=`/socket access
  (`XDG_RUNTIME_DIR`, the PipeWire/Pulse socket) or even to run as the interactive user rather than a
  dedicated service user. Worth a small standalone spike (can the service even open the mic at all,
  under the actual deployed unit) before building the rest of this plan on top of an assumption that
  might not hold.
- An hour-long WAV sent to Gemini as inline base64 data could be a large request — check the API's
  practical size limits for inline audio before assuming this scales to long meetings; a long recording
  might need chunking.
- If recording fails mid-meeting (capture error, disk full, service restart), there's currently no
  partial-save/resume story — worth deciding whether a crash should at least preserve whatever was
  captured so far, rather than losing the whole recording.
