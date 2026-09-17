# [L] LiveKit client shim

## Goal

A drop-in `livekit-client`-compatible JS package (e.g. `@moq/livekit`) that
runs a multi-participant room entirely over MoQ. The v1 surface is media plus
core events: Room connect/disconnect, local and remote participants,
camera/mic/screenshare publish, auto-subscribe, and the TrackSubscribed event
family; data surfaces (publishData, streams, RPC) are stubbed. The token slot
takes an ordinary moq-auth token, no LiveKit JWT parsing. Done when an
off-the-shelf LiveKit JS sample runs against a MoQ relay with only the import
and the connect URL/token changed.

## Plan

- The shim is a LiveKit-API facade over `@moq/room` (`js/room`, landed in
  #3634): the room is a path prefix in the connection URL and token root,
  participants are discovered from the announce stream, identity is the path
  before `camera.hang` and `screen.hang`, and a screenshare's
  announce/unannounce is its lifecycle. The shim groups the two broadcasts
  per identity into one RemoteParticipant and maps catalog entries to
  TrackPublications.
- Build on `@moq/publish` and `@moq/watch`. LiveKit quality hints map to
  the receiver-driven pixel target where they can (`setVideoQuality` picks
  the rendition, `adaptiveStream` follows the rendered size); a hint the
  target cannot express (`setVideoFPS`, per-layer bitrate caps) throws a
  clear unsupported error rather than no-oping, so an app never believes a
  limit is active.
- v1 is identity-only: `participant.identity` comes from the path and muted
  state derives from catalog track presence. Names and coarse state come
  from `@moq/room`'s `hang/*.json` metadata in a follow-up, not a rival
  scheme; `ActiveSpeakersChanged` waits on
  [active speaker](/quest/m2/room-active-speaker.md).
- Tokens come from `@moq/room` `claims()`: publish under `<identity>/**`
  only, so participants cannot publish at each other's paths.
- `publishData`, text and byte streams, and RPC throw a clear
  not-implemented error in v1; [shim data surfaces](/quest/m2/livekit-shim-data.md)
  maps them onto the data convention afterwards.

## Related

- [LiveKit Agents adapter (Python)](/quest/m2/livekit-agents-python.md) -
  the agent-side half of the same migration
