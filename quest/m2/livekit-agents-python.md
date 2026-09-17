# [M] LiveKit Agents adapter (Python)

## Goal

A `livekit-agents` voice agent runs against a MoQ relay instead of a LiveKit
room, with its STT, LLM, TTS, turn detection, and plugins unchanged: a
`moq-livekit-agents` package in `py/` supplies an `AudioInput` and
`AudioOutput` over MoQ and a runner that starts one `AgentSession` per
announced request broadcast and publishes its response. Done when a stock
LiveKit voice-agent example answers a browser publisher through a local relay
with only the I/O wiring changed.

## Plan

- The framework already runs without a room. `AgentSession.start(agent)`
  with `session.input.audio` and `session.output.audio` assigned is how its
  own test harness (`tests/fake_io.py`) and `console` mode
  (`cli/tcp_console.py`) work, and `get_job_context(required=False)` keeps a
  session independent of a Worker. Subclass `voice/io.py`'s `AudioInput`
  (async iterator of `rtc.AudioFrame`) and `AudioOutput` (`capture_frame`,
  `flush`, `clear_buffer`, and `on_playback_finished` when the track drains).
- Media the way Pipecat's MoQ transport does it (`pipecat.transports.moq`):
  the `moq` package encodes and decodes Opus inside the FFI, the agent's
  audio is one hang audio track replaced by a fresh one on barge-in so
  catalog-aware players follow the successor, and frames cross as int16 at
  the session's sample rate. `livekit-rtc` stays a dependency for the frame
  type; no WebRTC connection is ever opened.
- The runner replaces dispatch: consume an announce prefix, start a session
  per `request/<x>` broadcast, publish at `response/<x>`, end the session on
  unannounce. The disjoint subtrees are the loop guard. A scoped token
  (subscribe `request/**`, publish `response/**`) is the whole credential;
  there is no registration and no dispatch API.
- Client turn signals ride the same RTVI JSON track the Pipecat transport
  uses, mapped onto `session.interrupt()` and `session.commit_user_turn()`,
  so one browser publisher works with either framework's agent.
- The README states what `RoomIO` provides and this does not: participant
  attributes, `lk.chat` text input, transcription forwarding, `SessionHost`,
  and RPC. Text and transcript surfaces follow the
  [data convention](/quest/m2/data-convention.md) when it lands.
- Tests: a fake `AgentSession` I/O round trip over a local relay, barge-in
  replacing the output track, and unannounce ending the session. Pin
  `livekit-agents` to the 1.8 line and record the `io.py` surface it relies
  on, since that module is not covered by their stability promise.

## Related

- [LiveKit Agents adapter (Node)](/quest/m2/livekit-agents-node.md)
- [LiveKit client shim](/quest/m2/livekit-shim.md) - the client-side half
