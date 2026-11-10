# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `CoapEncoder` (`FrameEncoder`), with round-trip tests against the parser.
- Structured property tests (valid frames, then mutation).

### Changed
- Uri-Path is one UTF-8 segment per option. Invalid UTF-8 is a parse error. It used to
  be stored as an empty string.
- Max-Age (4 bytes), Observe (3) and Content-Format (2) reject longer values instead
  of truncating them.

### Fixed
- Option-number arithmetic is checked, so an overflow is a parse error. A debug-mode
  `u16` overflow in extended-length parsing (`+ 269` on a 2-byte extension) is fixed.
- The `0xFF` payload marker must be followed by at least one byte (RFC 7252 §3).
- Bracketed indices in doc comments (`[0]`, `[1]`) are escaped, so `cargo doc` passes
  with `-D warnings`.

## [0.1.0] - 2026-07-30

### Added
- Initial release: CoAP (RFC 7252) constrained-IoT support in the TPT Protocol suite.
- Binary header parsing in `CoapParser`: version (must be 1), message type
  (Confirmable/NonConfirmable/Acknowledgement/Reset), token length, code, 16-bit
  message ID, token, options, and payload-marker-delimited payload.
- `CoapCode` decoding for methods Get (1) / Post (2) / Put (3) / Delete (4) and the
  2.x/4.x/5.x response classes, with `is_method` / `is_success` / `is_client_error`
  / `is_server_error` helpers, plus `Unknown(u8)`.
- Delta/length option decoding with 4-bit, 1-byte (13+), and 2-byte (269+) escape
  extensions, and typed `CoapOption` extraction: UriPath (11), ContentFormat (12),
  MaxAge (14), ETag (4), Observe (6), and `Other`.
- `CoapConnectionState` / `CoapValidator` (`StateValidator`) enforcing a connected link.
- `CoapNormalizer` (`DataNormalizer`) mapping frames to `tup::DataPoint`s keyed by
  `device_id = "coap/{type}"` and `point_id = "mid:{message_id}"`, carrying the
  integer code when the payload is empty, or the raw payload bytes.
- `#![no_std]` + `alloc` support (the popular `coap`/`coap-rs` crate requires `std`
  + `tokio`; this implementation is dependency-free and `no_std`).
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
