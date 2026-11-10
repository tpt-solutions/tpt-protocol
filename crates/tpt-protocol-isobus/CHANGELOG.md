# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-11

### Added

- `IsobusName`: the 64-bit ECU NAME from Address Claimed frames (`0xEE00`), decoded with
  the ISO 11783-5 bit layout. A test builds each field at its bit position.
- `IsobusEncoder` (`FrameEncoder`), with round-trip tests against the parser.
- Named PGNs for TP.CM (`0xEC00`), TP.DT (`0xEB00`) and Request.
- Signals for EEC1, ET1, CCVS and fuel economy, in their units, with `normalize_all`.
  Other PGNs give their payload as bytes.
- Structured property tests, and a runnable `parse_isobus` example.
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

### Changed

- Identity (`implement_id`, `device_class`) comes from the NAME in Address Claimed
  frames, not from arbitrary payload bytes. `implement_id` is little-endian.
- PDU1 frames (PF below 240) do not include the destination address in the PGN, the same
  as J1939.

### Fixed

- A frame with no payload is an error, not a zero.
- The TP.CM and TP.DT names, and the Request PGN, were wrong in the earlier table.
