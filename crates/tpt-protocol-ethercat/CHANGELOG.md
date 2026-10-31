# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
