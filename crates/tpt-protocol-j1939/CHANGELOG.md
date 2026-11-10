# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `J1939Encoder` (`FrameEncoder`), with round-trip tests against the parser.
- The EDP bit is part of the PGN. TC1 is PGN 0. CCVS (`0xFEF1`) and Ambient Conditions
  (`0xFEF5`) are named.
- SPN decoding in engineering units for EEC1 (engine speed), ET1 (coolant temperature),
  CCVS (wheel speed) and fuel economy (fuel rate), with `normalize_all`.
- Structured property tests over 29-bit identifiers across PDU1 and PDU2, and a runnable
  `parse_j1939` example.

### Changed
- The normalizer decodes SPN signals. It used to report the first four payload bytes as
  a number for every PGN. Point names are `pgn:EEC1.engine_speed` and similar. Other
  PGNs give their payload as bytes.
- Multi-byte fields are little-endian, as on the wire.

### Fixed
- The PDU1 destination is masked out of the PGN.
- The EDP bit was missing from the PGN.

## [0.1.0] - 2026-07-30

### Added
- Initial release: SAE J1939 heavy-duty vehicle-network protocol support in the TPT Protocol suite.
- 29-bit CAN-ID decoding into priority, Extended Data Page (EDP), Data Page (DP), PDU Format (PF), PDU Specific (PS), and Source Address (SA).
- PGN resolution with the PDU1/PDU2 distinction (`PF >= 240` includes PS in the PGN).
- PGN recognition for TC1 (0xFF01), EEC1 (0xF004), EngineTemp1 (0xFEEE), FuelEconomy (0xFEE9), CabTemp (0xFEF5), plus a `Raw(u32)` fallback.
- Payload bounds checking: minimum 4-byte CAN ID, maximum 8-byte payload.
- Incremental `try_parse` helper returning `Option<(frame, consumed)>` for streamed input.
- State validation via the `StateValidator` trait (`J1939Validator`), rejecting unconnected state, the global source address 0xFF, and `error_count > 20`.
- TUP normalisation via the `DataNormalizer` trait (`J1939Normalizer`), keyed by `j1939:<sa>` device IDs and `pgn:<pgn>` point IDs.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
