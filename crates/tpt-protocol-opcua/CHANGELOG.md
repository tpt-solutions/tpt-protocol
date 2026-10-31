# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-30

### Added
- Initial release: OPC UA support in the TPT Protocol suite.
- Binary message-header parsing: message type (HEL/ACK/ERR/MSG/OPN/CLO), chunk
  type, message size (bounds-checked against the buffer), and secure-channel
  header (channel id, token id, sequence number, request id).
- For MSG frames, extraction of the service id (u32) and node id (u32) plus the
  remaining payload bytes.
- State validation via the `StateValidator` trait (`OpcuaValidator` /
  `OpcuaState`), rejecting disconnected sessions and zero channel/token ids.
- TUP normalisation via the `DataNormalizer` trait (`OpcuaNormalizer`), mapping
  frames to `tup::DataPoint`s keyed by channel and service/node id.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
