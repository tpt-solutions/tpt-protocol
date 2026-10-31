# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-30

### Added
- Initial release: the TPT Unified Protocol (TUP) canonical normalized schema in the TPT Protocol suite.
- `DataPoint` model with `source_protocol`, `device_id`, `point_id`, `timestamp`, `value`, `unit`, `quality`, and `raw` fields, plus builder helpers `with_unit` / `with_raw` / `with_timestamp` / `with_quality`.
- `TupValue` enum (`Int`, `Float`, `Bool`, `Text`, `Bytes`) with `From` conversions and `Display`.
- `Quality` enum (`Good`, `Bad`, `Stale`, `Uncertain`) with string round-trip (`as_str`, `FromStr`, `from_str_opt`).
- `SourceProtocol` enum covering all 19 supported protocols with kebab-case string round-trip.
- CBOR codec via `minicbor` (`encode_to_vec` / `decode_from_bytes`) using stable integer map keys and a self-describing `[type_tag, payload]` value encoding; `#![no_std]` + `alloc` only.
- JSON mirror module (behind `std`) with `to_json_string`, `to_json_string_pretty`, and `from_json_str`; `Bytes`/`raw` fields hex-encoded with a `0x` prefix.
- `no_std` by default with `std` (default) and `alloc` Cargo features.
- Const `schema_version()` (`1`) on every `DataPoint`; decoder rejects frames with an unsupported schema version.
- `examples/roundtrip.rs` demonstrating isomorphic CBOR + JSON round-trips.
- Tests validating the `SPEC-TUP.md` §4 worked example against `schema/tup-v1.schema.json`, CBOR/JSON round-trips for every value type and quality, and rejection of truncated/garbage/oversized inputs.
