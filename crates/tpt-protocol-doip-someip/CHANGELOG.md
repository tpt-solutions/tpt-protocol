# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
