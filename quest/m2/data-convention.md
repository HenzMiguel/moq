# [M] Data convention

## Goal

One request/response convention over the catalog's existing `json` and
`binary` data sections (#3109), so rooms, teleoperation, voice agents, and
any app stop hand-rolling reply channels: a caller's `stream`-mode track of
`{id, method, params}` answered on the callee's `stream`-mode track of
`{id, ok, result}` or `{id, error}`, with the callee advertised in its
catalog and the caller discovered from an announce prefix. The `window`
mode moq-json already implements joins the catalog's known modes, every
in-tree data track declares itself, and the hang draft carries all of it.

Today three consumers each invented a reply channel: the Pipecat
transport's fixed-name `transcript.json.z` track, teleop's proposed `rpc`
tracks, and pronto's `status` echoing a command `sequence`. `@moq/room`'s
chat window is not in the catalog at all.

## Plan

- Modes: add `window` (the retained run moq-json's `js/json/src/window`
  writes) to `KnownMode` in `js/hang/src/catalog/mode.ts` and
  `rs/hang/src/catalog/mode.rs`, so a chat window is readable by a generic
  consumer. Datagram delivery is a track `Info` property, not a mode, and
  stays out of the catalog.
- Request/response: `moq-json` gains typed `Request`/`Response` producers
  and consumers over its stream mode in JS and Rust: the caller keeps the
  correlation id and its own timeout, the callee answers each id once, and
  a lagged reader surfaces the error rather than retrying. Discovery is the
  direction teleop and Voice already use: the callee's response track is a
  `json` entry with a `schema` naming the method set, and callers are found
  under an announce prefix (operator publishes, robot subscribes;
  `request/` and `response/` subtrees).
- Reliability is what `moq-net` gives (a group is one QUIC stream; a reader
  that falls past `MAX_GROUP_CACHE` is told so). The docs say that instead
  of promising delivery. Non-goals: byte-stream file transfer, delivery
  receipts, per-participant addressing beyond a path and a scoped token,
  and a cross-host timebase.
- Adopt in the same change: `@moq/room` `Chat` declares its `window` track
  in the `json` section; `rs/moq-native/examples/chat.rs` and
  `rs/moq-json/examples/telemetry.rs` declare theirs; the draft gains the
  `window` mode and the request/response shape. A catalog without data
  sections parses unchanged, which the existing `deserialize_section`
  leniency already guarantees.
- Tests: `window` round trip in both languages, a request answered across a
  local relay, a lagged caller surfacing the error, and an unknown mode
  still passing through verbatim.

## Related

- [Robot teleoperation primitive](/quest/m3/teleop/robot.md) - the `rpc`,
  `telemetry`, and `command` tracks are the first non-room consumer
- [LiveKit client shim](/quest/m2/livekit-shim.md) - `publishData`,
  streams, and RPC map onto this
- [Catalog track identity](/quest/m3/catalog-tracks.md) - whatever it
  decides about immutable definitions applies to data entries too
