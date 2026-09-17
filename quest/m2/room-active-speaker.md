# [S] Active speaker

## Goal

`@moq/room` exposes who is talking: every `Remote` and the `Local` carry an
audio level signal and a debounced `speaking` boolean, and the `Room`
exposes the ordered active-speaker set, so a grid can highlight the speaker
and the LiveKit shim can emit `ActiveSpeakersChanged`. `moq-room` is
media-free (announce events and chat; decoded audio lives in `moq-audio`
and the application), so the native half is a level meter in `moq-audio`
over decoded PCM with the same thresholds, which a native app wires itself;
no room media layer is added.

## Plan

- Measure on decoded audio, independent of the member's playback mute:
  `Member.muted` defaults to true and gates only the output today, so the
  level analysis runs on the decoded frames before that gate (or keeps the
  decoder subscribed while the sink is silent), and a muted-for-me member
  still reads as speaking. Measuring on the wire is not an option since the
  relay never decodes.
- Level is a smoothed RMS in dBFS; `speaking` uses an attack and release
  threshold with a short hold, exposed through the existing `@moq/signals`
  idiom. The `moq-audio` meter uses the same numbers and a test vector
  shared with the JS one.
- The demo at `demo/web/src/meet.html` highlights the loudest tile as proof,
  with every remote left muted.
