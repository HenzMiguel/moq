# m3: later work

## Goal

Deferred features, design studies, experiments, and hardware validation.
Exploratory quests may end with a measured no-go verdict; implementation
quests retain their stated completion criteria.

## Plan

Nothing in this milestone blocks a release. Promote work into m2 when it
joins the next agent wave, including planning work when its decisions are
worth settling now. Deferral does not by itself abandon a feature.

## Quests

- [AV1 metadata separation](/quest/m3/av1-metadata.md) - retain metadata OBUs inline while evaluating separate delivery

- [SEI separation](/quest/m3/sei/README.md) - retain inline SEI until measured savings or a metadata-only consumer justify a split

- [Catalog track identity](/quest/m3/catalog-tracks.md) - compare immutable track definitions with explicit catalog-to-group binding
- [Mobile ownership](/quest/m3/mobile-ownership.md) - decide whether Rust or platform code owns mobile capture, codecs, and rendering
- [iOS capture](/quest/m3/mobile-capture-ios.md) - camera and screen capture if the mobile ownership decision selects Rust
- [Android capture](/quest/m3/mobile-capture-android.md) - NDK/JNI capture and codec backends if the mobile ownership decision selects Rust
- [Mobile completion](/quest/m3/mobile-completion.md) - verify the selected native/mobile path before closing #700
- [C++ SDK](/quest/m3/cpp-sdk.md) - an idiomatic wrapper over libmoq's C ABI, with the OBS plugin as its first consumer
- [Linux OBS GPU input](/quest/m3/obs-linux-gpu.md) - publish OBS compositor frames without CPU readback on a validated Linux graphics/encoder combination
- [QUIC capacity probing](/quest/m3/quic-probe.md) - compare useful early retransmissions with padding and no probing

- [Embedded video](/quest/m3/video-embedded.md) - EGL import in the renderer, so moq-video presents on a Pi
- [Decoder drain](/quest/m3/decode-drain.md) - flush and finish for pipelined decoders, so no picture is lost at a track end or crosses a group boundary
- [#2819](/quest/m3/2819-moq-video-carry-pipewire-dma-bufs-safely-into-the-vulkan.md) - moq-video: carry PipeWire DMA-BUFs safely into the Vulkan renderer
- [#2893](/quest/m3/2893-video-validate-pipewire-dma-buf-capture-on-kde-hardware.md) - video: validate PipeWire DMA-BUF capture on KDE hardware
- [Video hardware validation](/quest/m3/video-hardware.md) - run the encode, capture, and zero-copy paths that were written but never run on real machines
- [Multipath spike](/quest/m3/multipath-spike.md) - whether bonded contribution over multipath QUIC is worth building, given it needs noq on both ends
- [QUIC GCC](/quest/m3/quic-gcc.md) - a measured verdict on delay-based congestion control for media egress
- [QUIC FEC](/quest/m3/quic-fec.md) - a measured verdict on transport-level FEC vs retransmission
- [Web P2P](/quest/m3/p2p/README.md) - a measured verdict on browsers serving each other over LAN-only data channels to offload the relay
- [GOP overhead](/quest/m3/gop-overhead.md) - price the I-frames a short GOP pays for, deciding whether a long GOP plus a keyframe request is worth designing
- [#703](/quest/m3/703-experimental-webgpu-renderer.md) - Experimental WebGPU renderer
- [#823](/quest/m3/823-svc-support.md) - SVC support?
- [#1838](/quest/m3/1838-tr-101-290-monitoring-requirements-broadcast-contribution.md) - TR 101 290 monitoring: requirements (broadcast/contribution health metrics)
- [Teleoperation](/quest/m3/teleop/README.md) - MoQ carries robot video down and control up on one session as a library capability
- [SIP media stack](/quest/m3/sip-stack.md) - terminate one inbound SIP audio call leg and expose it as Opus frames
- [SIP DTMF](/quest/m3/sip-dtmf.md) - RFC 4733 keypad events become a data track beside the caller's audio
- [SIP transfer](/quest/m3/sip-transfer.md) - blind transfer over REFER and warm transfer over a second leg, driven through the data convention
- [Carrier voice](/quest/m3/carrier-voice/README.md) - determine whether MoQ should be the call fabric for programmable carrier voice
- [Vision worker](/quest/m3/processor-vision.md) - a documented customer-run vision worker proves the processor contract
- [Common Access Tokens](/quest/m3/cat/README.md) - a moq-transport client presents a CAT in SETUP and `moq auth serve` admits it with the scope its `moqt` claim names
