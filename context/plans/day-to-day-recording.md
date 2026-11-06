# Day-to-Day Recording (speculative — "very later on")

This is the roadmap's explicitly deferred item. Written as an idea-capture plan rather than a full
implementation breakdown — the point is to not lose the idea and to note what would actually need
deciding before this becomes buildable, not to pretend false precision on something years out.

## Change summary
Continuous personal lifelogging: record the laptop's screen, plus audio/video of the surroundings
(either the laptop's own webcam/mic, or a separate device). The idea sits closer to self-hosted
"Rewind.ai"-style activity recording than to the meeting-recording plan — much higher volume, much
longer retention horizon, and real privacy/storage questions that don't have obvious answers yet.

## What's reasonably clear
- Two capture streams: (1) screen recording of the laptop, (2) ambient audio/video of the physical
  surroundings, from either the laptop's own camera/mic or a separate device.
- This is explicitly lower priority than everything else on the roadmap ("to do very later on").

## Open questions to resolve before this is buildable
- **Capture cadence:** continuous video, periodic screenshots/snapshots, or something adaptive
  (recording only during active use, pausing when idle)? Continuous full-res screen+video recording for
  a full day is a lot of data — this alone probably needs resolving before anything else.
- **Storage volume and retention:** a day of continuous screen+webcam video is easily tens of gigabytes;
  what's the retention policy (keep everything forever, roll off after N days, compress/downsample
  aggressively)? This has real implications for where it's stored (local disk, the planned remote
  backup target, something else entirely).
- **"Another device" for surroundings capture:** what device, concretely — a phone propped up, a
  dedicated webcam, something else? This determines whether this plan needs its own capture client or
  can lean on the existing mobile app.
- **Privacy/pause controls:** continuous recording of your screen and surroundings is sensitive by
  nature even for personal use — at minimum needs an obvious, fast way to pause/stop (e.g. a visible
  indicator and a single hotkey), and probably some thought about what happens to video/screen content
  that includes other people (meetings, calls, people walking into frame) who didn't consent to being
  recorded.
- **What this is actually *for*:** lifelogging for later recall/search (which would eventually want the
  same kind of indexing as the knowledge-base plan), or something narrower (e.g. just a personal activity
  log, no retrieval ambitions)? The answer changes whether this needs OCR/transcription/search built on
  top, or is just "record it and maybe look at it later."

## Non-goals (for now)
- No implementation task stack yet — intentionally, given the open questions above.
- Not assumed to reuse the meeting-recording plan's server-side capture architecture, even though both
  involve audio/video capture — the volume, retention, and privacy shape here are different enough that
  this deserves its own design pass once it's actually prioritized.

## Revisit when
This roadmap item gets picked up for real — at that point, work through the open questions above as a
proper clarifying round (the same way every other plan in this batch got one) before writing an actual
task stack.
