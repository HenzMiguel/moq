# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.1](https://github.com/moq-dev/moq/releases/tag/moq-sock-v0.0.1) - 2026-09-21

### Added

- *(sock)* extract the shared listener plumbing and steer the uring endpoint ([#3078](https://github.com/moq-dev/moq/pull/3078))

### Other

- *(quic)* [**breaking**] keep only the noq backend ([#3811](https://github.com/moq-dev/moq/pull/3811))
- *(moq-sock)* [**breaking**] centralize reuseport group formation ([#3414](https://github.com/moq-dev/moq/pull/3414))
