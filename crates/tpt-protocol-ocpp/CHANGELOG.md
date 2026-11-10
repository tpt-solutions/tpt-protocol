# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-11

### Added

- OCPP-J (JSON over WebSocket) parsing for OCPP 1.6 and 2.0.1: Call `[2, id, action,
  payload]`, CallResult `[3, id, payload]` and CallError `[4, id, code, description,
  details]`.
- `OcppAction` covers the common 1.6 and 2.0.1 action names, with an `Other` fallback.
- `OcppEncoder` (`FrameEncoder`): `to_bytes` encodes the same JSON, with round-trip tests.
- Structured property tests (CALL, CALLRESULT and CALLERROR arrays, then mutation), and a
  runnable `parse_ocpp` example.
- Initial release: OCPP 2.0.1 support in the TPT Protocol suite.
- Binary message framing for OCPP message types Call (2), CallResult (3), and
  CallError (4), with length-prefixed unique id, optional action (Call only), and
  payload fields.
- Action decoding for BootNotification, StatusNotification, MeterValues,
  StartTransaction, StopTransaction, Heartbeat, and Authorize, plus `Other`.
- Round-trippable serialisation via `OcppFrame::to_bytes`.
- State validation via the `StateValidator` trait (`OcppValidator` /
  `OcppConnectionState`), rejecting disconnected sessions and repeated CallError frames.
- TUP normalisation via the `DataNormalizer` trait (`OcppNormalizer`), mapping
  messages to `tup::DataPoint`s.
- `#![no_std]` + `alloc` support.
- Overflow-safe length handling (no `u32` integer overflow into an invalid slice range).
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.

### Changed

- Rewritten. The previous parser used an invented binary framing with length prefixes,
  not OCPP.
- The parser is a small scanner for the top-level JSON array. Strings are decoded,
  including escapes, and brackets inside strings are skipped. Payloads are kept as raw
  JSON text.
- Message ids are at most 36 characters, and element counts are checked for each message
  type.
- A CallError normalizes to `Quality::Bad`.
