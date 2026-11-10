# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Matter TLV decoding (Appendix A) with `decode_tlv`: all element types, the context,
  profile and fully-qualified tag forms, and nested containers with a depth limit.
- The message header and protocol header for unsecured (session 0) messages. Secured
  payloads are reported as encrypted.
- Interaction Model `ReportData`, decoded into attribute reports (endpoint, cluster,
  attribute, value). `normalize_all` gives one point per scalar attribute.
- Structured property tests, and a runnable `parse_matter` example.

### Changed
- Rewritten. The previous parser used an invented binary layout, not Matter.
- Unsigned values above `i64::MAX` are errors, not truncated values.

### Notes
- The Interaction Model opcode values follow the connectedhomeip implementation. They
  have not been checked against a real capture.

## [0.1.0] - 2026-07-30

### Added
- Initial release: Matter/Thread support in the TPT Protocol suite.
- Binary frame parsing for a Matter/Thread command frame: protocol layer
  (Matter/Thread), cluster id, command id, endpoint, data version, interaction
  type, and a simplified TLV payload.
- Cluster id decoding for OnOff (0x0006), LevelControl (0x0008),
  TemperatureMeasurement (0x0402), OccupancySensing (0x0406), plus
  `ClusterId::Other(u16)` for unknown clusters.
- Interaction type decoding: Read (0), Write (1), Invoke (2), Subscribe (3).
- TLV value decoder for Bool, Int (i64), Float (f64), Text, Bytes, and Null.
- State validation via the `StateValidator` trait (`MatterValidator` /
  `MatterConnectionState`).
- TUP normalisation via the `DataNormalizer` trait (`MatterNormalizer`),
  mapping frames to `tup::DataPoint`s.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
