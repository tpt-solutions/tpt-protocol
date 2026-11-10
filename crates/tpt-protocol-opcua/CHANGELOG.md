# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-11

### Added

- `OpcuaEncoder` (`FrameEncoder`), with round-trip tests against the parser.
- A MSG body is decoded as an `ExtensionObject`: its TypeId (two-byte, four-byte and
  numeric NodeId forms), encoding byte and length-prefixed body.
- `try_parse` for streams: `Ok(None)` for an incomplete message, otherwise the consumed
  length.
- Structured property tests (valid messages, then mutation), and a runnable `parse_opcua`
  example.
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

### Changed

- OPC UA Binary is little-endian throughout. The parser read big-endian.
- `message_size` must equal the frame length, so bytes after a message are never read as
  its payload.
- Chunk type (`F`, `C`, `A`) and message type are validated.
- `OpcuaFrame::node_id` is replaced by `type_namespace`. The service and node fields were
  read from fixed offsets.
