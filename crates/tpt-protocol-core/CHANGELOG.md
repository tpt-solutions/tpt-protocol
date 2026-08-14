# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-30

### Added
- Initial release: the core trait definitions and shared error type for the TPT Protocol suite.
- `FrameParser` trait (`parse` required, `try_parse` with a default that reports bytes consumed) for parsing raw bytes into a protocol-specific `Frame`.
- `StateValidator` trait for validating connection/link state.
- `DataNormalizer` trait (`normalize` required, `normalize_batch` with a default implementation) for converting parsed frames into TUP `DataPoint`s.
- `Error` enum (`#[non_exhaustive]`) with string variants `Parse`, `State`, `Normalize`, `Cbor`, `Transport`, `Config`, and structured variants `Checksum`, `Overflow`, `InvalidFrame`, plus ergonomic constructors (`parse`, `state`, `normalize`, `cbor`, `transport`, `config`, `checksum`, `overflow`, `invalid_frame`) and `Display`/`From<Utf8Error>`/`std::error::Error` impls.
- `ProtocolPlugin` trait (behind the `std` feature) emitting a `futures_util` stream of TUP `DataPoint`s, used by the daemon's plugin architecture.
- `#![no_std]` core (no `alloc` required by the three parser traits) with an `std` feature (default) enabling `tpt-protocol-tup/std` and `futures-util`.
- Unit tests for all three traits and the `examples/custom_protocol.rs` runnable example showing how to add a new protocol.
