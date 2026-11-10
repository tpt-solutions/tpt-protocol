# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-11

### Added

- C ABI: `tpt_decode_json` and `tpt_free_string`, declared in `include/tpt_protocol.h`. The
  header is hand-written, and a unit test checks that it matches the exports.
- WebAssembly build (`--features wasm`), with `decode_hex` and `protocols`, pinned to
  `wasm-bindgen` 0.2.126, and a browser playground in `www/`.
- Python module (`--features python`), built with maturin as `tpt_protocol`, with `decode`
  and `protocols`. An unknown protocol raises `ValueError`.
- A CI job, `python-wheel`, that builds the wheel on every push. Nothing is uploaded.
- All three front ends decode through one dispatch table, and return the daemon's JSON
  form for every data point. All 19 protocols are included.
- Unit tests for the dispatch and the C ABI (error codes, null pointers, the header).
