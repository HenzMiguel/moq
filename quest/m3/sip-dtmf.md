# [M] SIP DTMF

## Goal

`moq-sip` surfaces the caller's keypad: RFC 4733 telephone-events on the
negotiated RTP leg become a `stream`-mode data track beside the caller's
audio, declared in the catalog under the data convention, and the embedder
can send digits back on an outbound leg. In-band tone detection stays out.

## Plan

- Negotiate `telephone-event/8000` in the SDP answer beside Opus and G.711;
  decode event, end bit, and duration into one JSON frame per digit
  (`{digit, duration_ms}`) with the RTP timestamp mapped onto the audio
  clock so a transcript can interleave them.
- Sending: the embedder writes the same frames and the crate emits the RTP
  events with the standard three end packets.
- Test against a softphone and a PBX that sends both RFC 4733 and no DTMF
  at all (the track exists but stays empty).

## Required

- [SIP media stack](/quest/m3/sip-stack.md) - the leg this extends
- [Data convention](/quest/m2/data-convention.md) - the track shape and
  its catalog entry
