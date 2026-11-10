# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- cEMI `L_Data` parsing: message code, control fields (priority and hop count), source
  individual address, group or individual destination, NPDU length and the APCI service.
- Explicit datapoint decoders: `decode_dpt1`, `decode_dpt5_percent` and `decode_dpt9`
  (with the `0x7FFF` invalid marker). Tests use known DPT 9.001 values.
- `KnxEncoder` (`FrameEncoder`), with round-trip tests against the parser.
- Structured property tests, and a runnable `parse_knx` example.

### Changed
- The previous parser read an invented layout that is not KNX. It is replaced.
- The normalizer uses the source individual address as the device and the destination
  group address as the point.
- `DptValue` is now `Short(u8)` (a six-bit value in a one-octet telegram) or `Data(Vec<u8>)`.

## [0.1.0] - 2026-07-30

### Added
- Initial release: KNX smart-building protocol support in the TPT Protocol suite.
- cEMI-like binary frame parsing: control byte, source address, group address, group flag, service type, and DPT payload.
- Group-address decoding into the `main(5) / middle(3) / sub(8)` structure, with public `from_u16` / `to_u16` round-trip conversion.
- Service-type decoding for `GroupValueRead` (0x00), `GroupValueResponse` (0x40), `GroupValueWrite` (0x80), with a public `as_str` helper.
- Datapoint Type (DPT) decoding into `Switch(bool)`, `Scaling(u8)`, `Temperature(f32)`, `Percent(f32)`, and `Raw(u16)`.
- State validation via the `StateValidator` trait (`KnxValidator`), rejecting unconnected state and out-of-range group addresses.
- TUP normalisation via the `DataNormalizer` trait (`KnxNormalizer`), keyed by `knx/<source>` device IDs and `<main>/<middle>/<sub>` point IDs.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
