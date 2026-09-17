# [L] Data convention

## Goal

One convention for application data beside media, so rooms, teleoperation,
voice agents, and any app stop hand-rolling it: a `data` section in the hang
catalog that makes a JSON or binary track discoverable and typed, and one
request/response shape with correlation ids carried over those tracks.
Implemented in `js/hang`, `rs/hang`, and `moq-json`, adopted by `@moq/room`
chat, and written into `draft-lcurley-moq-hang`.

Today the catalog root is audio and video only, `moq-json` carries snapshot,
stream, and window modes with no way to advertise them, and three consumers
each invented a reply channel: the Pipecat transport's fixed-name
`transcript.json.z` track, teleop's proposed `rpc` tracks, and pronto's
`status` echoing a command `sequence`.

## Plan

- Catalog: a `data` map in the root beside `video` and `audio`, keyed by
  track name, each entry carrying `mode` (`snapshot`, `stream`, `window`, or
  `datagram`), a free-form `schema` identifier the app owns, and the track
  `Info` the reader needs (`timescale`, `priority`, `latency_max`). Rust
  keeps `CatalogExt` for app-private sections; this is the shared,
  browser-visible one.
- Request/response: a caller publishes a `stream`-mode track of
  `{id, method, params}` and the callee answers on its own `stream`-mode
  track of `{id, ok, result}` or `{id, error}`. The callee advertises its
  response track in its catalog; the caller is discovered from an announce
  prefix, the direction teleop and Voice already use (operator publishes,
  robot subscribes; `request/` and `response/` subtrees). Timeouts are the
  caller's, and a lagged reader is told so; recovery after a reconnect
  belongs to the application, never to a retry loop in the library.
- Reliability is what `moq-net` gives: a group is one QUIC stream, so frames
  in it are ordered and exactly once for a reader that keeps up, and
  `MAX_GROUP_CACHE` bounds how far behind one may fall. Say that in the docs
  instead of promising delivery.
- Non-goals: byte-stream file transfer, delivery receipts, per-participant
  addressing (a path and a scoped token already do that), and a cross-host
  timebase (teleop's correlation quest).
- Adopt it in-tree in the same change: `@moq/room` `Chat` declares its
  window track in the section, and the JS and Rust examples
  (`rs/moq-native/examples/chat.rs`, `rs/moq-json/examples/telemetry.rs`)
  move onto it. The draft gains the section and the request/response shape.
- Tests: catalog round trip in both languages, a request answered across a
  local relay, a lagged caller surfacing the error, and an unknown `mode`
  refused at parse.

## Related

- [Robot teleoperation primitive](/quest/m3/teleop/robot.md) - the `rpc`,
  `telemetry`, and `command` tracks are the first non-room consumer
- [LiveKit client shim](/quest/m2/livekit-shim.md) - `publishData`,
  streams, and RPC map onto this
- [Catalog track identity](/quest/m3/catalog-tracks.md) - whatever it
  decides about immutable definitions applies to `data` entries too
