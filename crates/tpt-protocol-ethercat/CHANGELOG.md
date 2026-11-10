# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `EthercatEncoder` (`FrameEncoder`), with round-trip tests against the parser.
- The whole datagram chain is parsed (the more-follows bit), and `EthercatFrame` exposes
  every datagram in `datagrams`.
- `try_parse` for streams: `Ok(None)` for an incomplete frame, otherwise the exact
  consumed length.
- Structured property tests: valid frames, then mutation.
- Published to crates.io. Renamed from `out-protocol-ethercat` (`publish = false` removed).

### Changed
- The command codes follow IEC 61158-4-12 (APWR = 2, FPWR = 5, BWR = 8, LRD = 10,
  LWR = 11), and the read/write variants are added.
- The EtherCAT header is little-endian, with an 11-bit length and a type field in bits
  12–15. Datagram fields (ADP, ADO, length flags, IRQ) are little-endian.
- The chain must end exactly at the section length. Trailing Ethernet padding is ignored.
- The normalizer uses the first datagram.

### Fixed
- The command table and byte order did not match the standard. Datagram chains were not
  walked, so only one datagram was read.

## [0.1.0] - 2026-07-30

### Added
- Initial release: EtherCAT (EtherType `0x88A4` datagram layer) support in the TPT Protocol suite.
- Datagram frame parsing in `EthercatParser`, decoding the EtherCAT header
  (`eth_length` + `eth_type`) and the 10-byte datagram header (command, index,
  4-byte addressing, length, reserved), plus the data region and the
  little-endian working counter (WKC), with truncation guards.
- `EcCmd` decoding for the eight standard datagram commands — APRD (1), APWR (5),
  FPRD (4), FPWR (8), BRD (7), BWR (11), LRD (12), LWR (13) — with an
  `Unknown(u8)` fallback.
- `EthercatState` / `EthercatValidator` (`StateValidator`) enforcing connection,
  OPERATIONAL state, non-zero slave count, and a consecutive-error threshold (≤ 10).
- `EthercatNormalizer` (`DataNormalizer`) mapping frames to `tup::DataPoint`s keyed
  by `device_id = "ethercat:master"` and `point_id = "cmd:{command}"`, carrying the
  raw datagram data (or the integer WKC when empty).
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
