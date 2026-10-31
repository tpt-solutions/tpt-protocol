# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-30

### Added
- Initial release: HL7 v2.x healthcare messaging support in the TPT Protocol suite.
- Segment / field / component text parsing using the `\r` segment terminator, `|` field separator, and `^` component separator.
- `MSH` header decoding into a structured `MshSegment` (field separator, encoding characters, sending/receiving application and facility, timestamp, message type, message control ID, processing ID, version ID).
- Message-type classification for `ADT`, `ORM`, `ORU`, `MDM`, and an `Other(String)` fallback.
- UTF-8-safe parsing: non-UTF-8 input returns `Error::Parse` instead of panicking.
- State validation via the `StateValidator` trait (`Hl7Validator`), checking connection state and last-ACK bookkeeping (`Hl7ConnectionState`).
- TUP normalisation via the `DataNormalizer` trait (`Hl7Normalizer`), mapping each message to `hl7/<trigger_event>` device IDs and `Text` values carrying the message type.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
