# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-30

### Added
- Initial release: ISO 14229 (UDS) diagnostic-services parser in the TPT Protocol suite.
- `UdsSid` enum covering DiagnosticSessionControl (0x10), EcuReset (0x11), ClearDiagnosticInformation (0x14), ReadDataByIdentifier (0x22), ReadMemoryByAddress (0x23), SecurityAccess (0x27), CommunicationControl (0x28), WriteDataByIdentifier (0x2E), TestPresenter (0x3E), plus `Response(sid)` and `Unknown(sid)` envelopes, with `raw_value`/`is_response`/`Display`.
- `SubFunction` enum (`None`, `Request(u8)`, `Response(u8)`) parsed for the sub-function-bearing SIDs (0x10/0x11/0x14/0x27/0x28/0x3E and their 0x40 responses).
- `UdsFrame` struct (`sid`, `sub_function`, `data`, `is_request`) and `UdsParser` implementing `FrameParser` (validates non-empty input, detects request vs. response, copies payload into `data`).
- `UdsState` + `UdsValidator` implementing `StateValidator`, rejecting when not connected, `session_type > 3`, or `error_count > 10`.
- `UdsNormalizer` implementing `DataNormalizer`, mapping frames to TUP `DataPoint`s with a SID-based `point_id` and Int/Bytes values by data length.
- `#![no_std]` + `alloc` implementation.
- Unit tests for parsing, normalization, and validator states, plus a `proptest` fuzz test asserting `parse` never panics on arbitrary input.
