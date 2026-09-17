# [M] LiveKit shim data surfaces

## Goal

`publishData`, text streams, and RPC in the LiveKit client shim work instead
of throwing: reliable `publishData` is a `stream`-mode `binary` track and
lossy a `snapshot`-mode one, text streams are `stream`-mode `json` tracks
keyed by topic, and RPC is the data convention's request/response pair
addressed by the callee's identity path. Byte streams with progress stay
unimplemented and throw. Done when a LiveKit sample that uses each of the
three runs against a relay unchanged.

## Plan

- Every track the shim publishes is declared in the participant's
  `camera.hang` catalog, so a plain `@moq/room` client can read a LiveKit
  app's data too. The README maps each LiveKit method to its track and its
  delivery guarantee.
- Tests: a reliable and a lossy `publishData` round trip, a topic stream
  read by a late joiner (only what the stream retains), and an RPC with a
  timeout and an error reply.

## Required

- [LiveKit client shim](/quest/m2/livekit-shim.md)
- [Data convention](/quest/m2/data-convention.md)
