# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-30

### Added
- Initial release: IEEE 11073 SDC bedside medical-device protocol support in the TPT Protocol suite.
- Binary frame parsing: message type (1 byte) + timestamp (8 bytes) + body.
- Message-type decoding for `Mdib` (0), `Waveform` (1), `Alert` (2), and `Context` (3).
- `NumericMetrics` body decoder: length-prefixed handle/descriptor-handle, 8-byte `f64` value, and length-prefixed unit string.
- `Waveforms` body decoder: length-prefixed handle, 4-byte `f32` sample rate, `u16` sample count, and packed `f32` samples.
- `Alerts` body decoder: length-prefixed handle/descriptor-handle, 1-byte `active` flag, and 1-byte `priority`.
- `ContextInfo` body decoder flattening metric handle/unit pairs.
- Bounded allocation: wire-controlled element and sample counts are capped at `MAX_SDC_ELEMENTS` (1024) via `with_capacity`.
- State validation via the `StateValidator` trait (`SdcValidator`), rejecting unconnected state or `metric_count > 1000`.
- TUP normalisation via the `DataNormalizer` trait (`SdcNormalizer`), mapping numeric metrics to `Float` + unit, waveform averages to `Float` (`mV`), alerts to `Bool`, and context to `Text`.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
