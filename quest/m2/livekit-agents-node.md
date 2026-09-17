# [M] LiveKit Agents adapter (Node)

## Goal

The Python adapter's design ported to `@livekit/agents`: an `AudioInput`
(`ReadableStream<AudioFrame>`) and `AudioOutput` (`captureFrame`, `flush`,
`clearBuffer`) over `@moq/net` and `@moq/hang`, plus the same announce-prefix
runner, published from `js/` as `@moq/livekit-agents`. Done when the stock
Node voice-agent example answers a browser publisher through a local relay.

## Plan

- `AgentSession.start({agent})` builds `RoomIO` only when a room is passed
  (`agent_session.ts`), and their tests assign `session.input.audio`
  directly, so the shape is the same as Python. `@livekit/rtc-node` stays a
  dependency for `AudioFrame`.
- Reuse the Python adapter's track naming, barge-in replacement, RTVI turn
  mapping, and tests one for one, so a browser publisher is agnostic to which
  runtime answers.

## Required

- [LiveKit Agents adapter (Python)](/quest/m2/livekit-agents-python.md) -
  settles the design this ports
