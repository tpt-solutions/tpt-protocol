# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
