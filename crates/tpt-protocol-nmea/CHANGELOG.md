# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-11

### Added

- Normalized fields for GGA (position, fix quality, satellites, altitude), RMC (position,
  speed, course), DBT (depth), VHW (heading and speed) and MWV (wind angle and speed).
  Coordinates are signed degrees.
- Decoders for GLL, VTG, GSA, GSV, HDT and ZDA.
- Talkers GN, GL, GA and BD, in addition to the earlier ones.
- NMEA 2000 (`nmea2000` module): a CAN frame parser and encoder, a fast-packet
  reassembler, and decoders for PGNs 129025, 127250, 130306, 128267 and 129029. The
  field layouts follow the published definitions and are not yet checked against a bus
  capture.
- `NmeaEncoder` (`FrameEncoder`), `normalize_all` (one point per decoded field), and
  structured property tests with checksums that are right or off by one.
- A differential example (`examples/diff_decode.rs`), checked against pynmea2 in CI, and
  a runnable `parse_nmea` example.
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

### Changed

- A checksum mismatch is `Error::Checksum`, as with Modbus and DNP3. It used to be a frame
  with `checksum_valid = false`.
- The checksum range is computed on the trimmed sentence, so leading whitespace no longer
  shifts it off the `$`.
- A sentence with no decodable field is an error. A length or a zero is never reported as
  a reading.

### Fixed

- Coordinate parsing split a string at a byte index inside a multi-byte character, which
  panicked. It now uses `str::get`.
