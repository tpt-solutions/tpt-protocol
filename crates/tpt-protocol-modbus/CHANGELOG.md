# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Responses: `ModbusParser::response()` parses read responses (`ReadResponse`), write
  echoes, `WriteMultipleResponse`, and exception responses (`Exception`, function
  code with bit 7 set). Exceptions were previously `Raw`.
- `ModbusEncoder` (RTU) and `ModbusTcpEncoder` (MBAP, for master-side requests), with
  round-trip tests. The RTU encoder refuses frames over the 256-byte RTU limit.
- `try_parse` for streams: `Ok(None)` for an incomplete frame, otherwise the exact
  consumed length. TCP frames expose their MBAP transaction id.
- Differential test against pymodbus 3.6.9 (`examples/diff_decode.rs`, run in CI), a
  criterion benchmark (`benches/parse.rs`), and structured MBAP property tests.

### Changed
- Requests are range-checked: FC 0x05 accepts only `0xFF00` and `0x0000`, and FC 0x10
  checks its byte count.
- Read requests normalize to an error, because a request carries no measured value.
  They used to report their quantity.
- Read responses normalize to the first coil, bit or register of the response, named
  by its index in the response (`holding_register[0]`).
- CRC failures return `Error::Checksum`, which carries the 16-bit CRC values.

### Fixed
- A whole-buffer parse requires the MBAP length to match the input. Trailing bytes
  were accepted before.
- An MBAP length below 2 no longer panics. A frame with a valid RTU CRC is never
  parsed as TCP.

## [0.1.0] - 2026-07-30

### Added
- Initial release: Modbus RTU/TCP support in the TPT Protocol suite.
- Frame parsing for Modbus RTU (CRC-16/Modbus validated) and Modbus TCP (MBAP
  header) transports, auto-detected by the parser into a single
  `ModbusRtuFrame` model.
- Decoding of function codes 0x01 (Read Coils), 0x02 (Read Discrete Inputs),
  0x03 (Read Holding Registers), 0x04 (Read Input Registers), 0x05 (Write
  Single Coil), 0x06 (Write Single Register), 0x07 (Read Exception Status),
  0x08 (Diagnostics), and 0x10 (Write Multiple Registers); unrecognised codes
  captured as `ModbusFrame::Raw`.
- State validation via the `StateValidator` trait (`ModbusValidator` /
  `ModbusState`), rejecting disconnected devices and error thresholds.
- TUP normalisation via the `DataNormalizer` trait (`ModbusNormalizer`),
  mapping frames to `tup::DataPoint`s keyed by slave address and register/coil
  address.
- `#![no_std]` + `alloc` support.
- Wire-controlled register counts capped at `MAX_MODBUS_REGISTERS = 1024` to
  prevent unbounded allocation.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
