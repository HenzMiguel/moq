# [M] SIP transfer

## Goal

`moq-sip` moves a live call: a blind transfer sends REFER for a target URI
and follows the NOTIFY subscription to completion, and a warm transfer holds
the caller, originates a second leg to the target, and bridges the two legs
when the embedder says so. The embedder drives both through the same
request/response track the data convention defines, so an agent or an app
can transfer without SIP-shaped code.

## Plan

- Blind: REFER with `Refer-To`, handle the implicit subscription and the
  `sipfrag` NOTIFYs, and end the original dialog on success. Refuse REFER
  from the far end (405) in v1.
- Warm: reuse the origination quest's outbound leg; hold is re-INVITE with
  `a=sendonly`; bridging is the embedder swapping which leg's Opus feeds
  which, not a media mixer.
- Test blind against a PBX that accepts REFER and one that rejects it, and
  warm end to end with a softphone as the target.

## Required

- [SIP media stack](/quest/m3/sip-stack.md)
- [SIP call origination](/quest/m3/carrier-voice/sip-originate.md) - warm
  transfer needs the outbound leg
- [Data convention](/quest/m2/data-convention.md) - the control channel
