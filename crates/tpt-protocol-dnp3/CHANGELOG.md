# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- User data is parsed in 16-byte blocks, each checked with its own CRC. Transport
  header (`transport_first`, `transport_final`, `transport_seq`) and application
  header (`application_control`) are parsed; the application header is present only in
  the first fragment.
- Function codes Confirm (0), Response (0x81) and UnsolicitedResponse (0x82).
- `Dnp3Encoder` (`FrameEncoder`), with round-trip tests.
- `try_parse` reports the exact consumed length, and `Ok(None)` for an incomplete frame.
- Structured property tests that build frames with per-block CRCs, then mutate them.

### Changed
- The normalizer reports the data body as bytes. It used to report the frame length.
- A frame with no body (a bare READ) returns a normalize error, because it carries no
  measured value.

### Fixed
- The data-link header CRC covers all eight bytes before it, the start bytes included,
  and is checked against the published CRC-16/DNP vector.

## [0.1.0] - 2026-07-30

### Added
- Initial release: DNP3 (IEEE 1815 electrical-grid SCADA) support in the TPT Protocol suite.
- Data-link-layer frame parsing in `Dnp3Parser`: start `0x0564`, length, control,
  2-byte little-endian destination/source, and CRC-16/DNP validation over bytes 2–7
  (`crc16_dnp`), with a truncation guard covering the trailing CRC so a small `length`
  field cannot cause an out-of-bounds read.
- Transport-layer parsing: final-segment flag (bit 7) and 6-bit sequence number.
- Application function-code decoding (`Dnp3FunctionCode`): Read (1), Write (2),
  Select (3), Operate (4), DirectOperate (5), DirectOperateNoAck (6), Freeze (7),
  FreezeNoAck (8), FreezeClear (9), FreezeClearNoAck (10), with an `Unknown(u8)` fallback.
- `Dnp3State` / `Dnp3Validator` (`StateValidator`) enforcing connection, a valid
  source address (not `0` / not `0xFFFF`), and a consecutive-error threshold (≤ 10).
- `Dnp3Normalizer` (`DataNormalizer`) mapping frames to `tup::DataPoint`s keyed by
  `device_id = "dnp3:{source}"` and `point_id = "func:{app_function}"`, carrying the
  raw payload bytes or the integer `length`.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
