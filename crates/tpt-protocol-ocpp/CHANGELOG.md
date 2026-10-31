# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-30

### Added
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
