# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
