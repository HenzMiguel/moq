# [S] Active speaker

## Goal

`@moq/room` and `moq-room` expose who is talking: every `Remote` and the
`Local` carry an audio level signal and a debounced `speaking` boolean, and
the `Room` exposes the ordered active-speaker set, so a grid can highlight
the speaker and the LiveKit shim can emit `ActiveSpeakersChanged`.

## Plan

- Measure on the decoded audio the member already plays (`js/watch`'s
  audio path in `Remote`; the capture path in `Local`), not on the wire, so
  a muted member never reads as speaking and nothing new crosses the relay.
- Level is a smoothed RMS in dBFS; `speaking` uses an attack and release
  threshold with a short hold, exposed through the existing `@moq/signals`
  idiom. Rust mirrors the same numbers in `moq-room`.
- The demo at `demo/web/src/meet.html` highlights the loudest tile as proof.
