# [M] C++ SDK

## Goal

C++ developers (robotics tooling, engines, native apps) get a thin,
idiomatic wrapper over `libmoq`'s C ABI: RAII handles, `std::span` payloads,
callbacks or futures for announce and subscribe, and hang audio, video, and
data tracks, installed as a CMake package beside `moq.pc`. The OBS plugin
(`cpp/obs`) moves onto it as the first consumer, and the wrapper's surface
tracks the Go, Swift, and Kotlin bindings one for one.

## Plan

- Header-first in `cpp/moq`, generated where the C ABI already carries the
  shape (cbindgen output in `rs/libmoq`), handwritten where ownership or
  callbacks need C++ semantics. C++20, no exceptions across the boundary.
- Ship through `libmoq`'s existing release pipeline (`build.sh`, CMake
  config), with a smoke that publishes and watches a clock through a local
  relay on Linux, macOS, and Windows.
- Unity is deliberately not a target; a C# binding is a separate decision.

## Required

- [#2152](/quest/m2/2152-libmoq-c-abi-catch-up-with-the-moq-ffi-surface.md) -
  the C ABI has to carry sessions before a wrapper is worth publishing
