# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-30

### Added
- Initial release: NMEA 0183 support in the TPT Protocol suite.
- ASCII sentence parsing for the `$<talker><sentence>,<fields>*<checksum_hex>`
  envelope, validating UTF-8 and the `$` prefix and `*` separator.
- Talker-id decoding: GP, SD, GI, HE, IN (plus `Unknown`).
- Sentence-type decoding: GGA, RMC, DBT, VHW, MWV (plus `Unknown`).
- XOR checksum computation with validity reported on the parsed frame (not rejected).
- State validation via the `StateValidator` trait (`NmeaValidator` / `NmeaState`),
  checking connection, signal-quality range (0–5), and error threshold.
- TUP normalisation via the `DataNormalizer` trait (`NmeaNormalizer`), attaching
  the raw sentence to the produced `DataPoint`.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
