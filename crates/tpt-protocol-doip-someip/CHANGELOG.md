# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `DoipSomeipEncoder` (`FrameEncoder`), with round-trip tests for both frame kinds.
- Structured property tests: valid DoIP and SOME/IP frames, then mutation.

### Changed
- The DoIP payload-type table follows ISO 13400-2.
- A DoIP frame is recognised by a version byte (`0x01`, `0x02`, `0x03` or `0xFF`)
  followed by its bitwise inverse. Anything else falls through to SOME/IP.
- Truncated and trailing DoIP payloads are errors, not empty payloads. The SOME/IP
  length must equal the frame length.
- The normalizer reports a DoIP payload as bytes. It used to report the payload length.
- A header-only message (for example a vehicle-identification request) returns a
  normalize error, because it carries no measured value.

### Fixed
- Payload types `0x0004`–`0x0008` and `0x8001`–`0x8003` had the wrong names.
- The payload-length overflow guard is checked, so a `u32::MAX` length cannot overflow
  the slice range.

## [0.1.0] - 2026-07-30

### Added
- Initial release: DoIP (ISO 13400) and SOME/IP automotive Ethernet support in the TPT Protocol suite.
- DoIP header parsing: protocol version, inverse version (validated as `!protocol_version`), payload type, payload length, and payload.
- SOME/IP header parsing: service ID, method ID, length, client ID, session ID, protocol version, interface version, message type, return code, and payload.
- Auto-detection of DoIP (first byte `0x02`) versus SOME/IP (minimum 16 bytes).
- DoIP payload-type decoding for GenericNACK, VehicleIdentification (Request/Response), RoutingActivation (Request/Response), AliveCheck (Request/Response), DiagnosticMessage and its positive/negative ACKs, with an `Unknown(u16)` fallback.
- SOME/IP message-type decoding for Request, RequestNoReturn, Notification, Response, Error, with an `Unknown(u8)` fallback.
- State validation via the `StateValidator` trait (`DoipSomeipValidator`), checking TCP connection, routing activation, and a consecutive-error threshold (`error_count > 10`).
- TUP normalisation via the `DataNormalizer` trait (`DoipSomeipNormalizer`), mapping DoIP frames to `doip:gateway` / `doip:<payload_type>` data points and SOME/IP frames to `someip:0x<service_id>` / `method:0x<method_id>` data points.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
- Length-integer overflow hardening: a `checked_add` guards the DoIP payload-end offset so a `u32::MAX` length cannot produce an inverted slice range.
