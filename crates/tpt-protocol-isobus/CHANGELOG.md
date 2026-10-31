# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-30

### Added
- Initial release: ISO 11783 (ISOBUS) agricultural-vehicle protocol support in the TPT Protocol suite.
- 29-bit CAN-ID decoding into priority, data page, PDU Format (PF), PDU Specific (PS), and Source Address (SA).
- PGN resolution with the PDU1/PDU2 distinction (`PF >= 240` includes PS in the PGN).
- PGN recognition for Request for Rep (0xEC00), PTO (0xFEBA), Working Set Master (0xFE12), Task Controller (0xFEAB), plus a `Raw(u32)` fallback.
- Payload bounds checking: minimum 4-byte CAN ID, maximum 8-byte payload.
- Implement-ID (payload bytes 0-1) and device-class (payload byte 2) extraction.
- State validation via the `StateValidator` trait (`IsobusValidator`), rejecting unconnected state, reserved/global source addresses (0xFE/0xFF), zero implement ID, and `error_count > 15`.
- TUP normalisation via the `DataNormalizer` trait (`IsobusNormalizer`), keyed by `isobus:<sa>:<implement_id>` device IDs and `pgn:<pgn>` point IDs.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
