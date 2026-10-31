# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-30

### Added
- Initial release: CANopen (embedded motion control over Classical CAN) support in
  the TPT Protocol suite.
- COB-ID frame parsing in `CanOpenParser`, splitting the 11-bit COB-ID into function
  code (bits 10–7) and node ID (bits 6–0), with an 8-byte payload limit.
- `CanOpenFunctionCode` decoding for NMT (0), SYNC (1), Timestamp (2), PDO1-TX (3),
  PDO1-RX (4), PDO2-TX (5), PDO2-RX (6), SDO-TX (0x0B), SDO-RX (0x0C), Heartbeat
  (0x0E), with an `Unknown(u8)` fallback.
- `CanOpenState` / `CanOpenValidator` (`StateValidator`) enforcing connection, a
  valid node ID (1–127), and a consecutive-error threshold (≤ 10).
- `CanOpenNormalizer` (`DataNormalizer`) mapping frames to `tup::DataPoint`s keyed
  by `device_id = "canopen:{node_id}"` and `point_id = "fc:{function_code}"`,
  carrying the first 4 payload bytes as a big-endian integer, the raw payload bytes,
  or the integer COB-ID.
- `#![no_std]` + `alloc` support (the popular `canopen` crate requires `std` +
  Linux-only `socketcan`; this implementation is dependency-free and `no_std`).
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
