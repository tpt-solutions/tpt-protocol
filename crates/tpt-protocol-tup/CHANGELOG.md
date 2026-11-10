# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `cddl_conformance` tests, which check encoder output against the CDDL rules and check
  that the protocol and quality lists match the Rust enums. The checker is hand-written,
  not a general CDDL validator.
- The JSON mirror tags bytes and non-finite floats as objects, so they round-trip.

### Changed
- CBOR matches `schema/tup-v1.cddl`: integer keys and `[tag, value]` values.
- Non-canonical CBOR is rejected: indefinite-length maps and arrays, extra array elements,
  duplicate keys, and `schema_version` 0.
- The JSON mirror rejects invalid hex, unsupported `schema_version` values and integers
  above `u64`.
- `Quality` and `SourceProtocol` are `#[non_exhaustive]`.
- `serde` is optional and enabled by `std`. CBOR is gated behind `alloc`, since it encodes
  into a `Vec`. Default features are empty.
- `hex` is optional, behind `std`.

### Fixed
- Schema tests run on `jsonschema` 0.61 without its `http` feature, which pulled in
  `reqwest` and `h2` (RUSTSEC-2026-0258). The schema is self-contained.

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
