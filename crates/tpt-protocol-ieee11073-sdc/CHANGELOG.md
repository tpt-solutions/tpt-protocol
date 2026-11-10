# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-11

### Added

- A bounded XML parser for the subset SDC needs: elements, attributes, namespace scope,
  character data, comments, CDATA and processing instructions. Entity and numeric
  character references are decoded. DOCTYPE is rejected, which blocks entity expansion,
  and depth and element counts are limited.
- SOAP 1.2 envelope checks by namespace, and the WS-Addressing `Action` and `MessageID`
  headers. The body operation is read (for example `EpisodicMetricReport`).
- `NumericObservation` extraction from `NumericMetricState`. Only finite values are kept.
- `SdcEncoder` (`FrameEncoder`), with round-trip tests against the parser.
- `normalize_all` gives one point per numeric observation.
- Structured property tests (valid messages, then mutation).
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

### Changed

- Rewritten. SDC messages are SOAP 1.2 envelopes with BICEPS XML bodies. The previous
  binary layout was invented, so it is gone (including its MDIB, waveform, alert and
  context decoders).

### Notes

- The BICEPS element names and namespace version are not yet checked against a real
  device. Waveforms, alerts and context states are not decoded yet.
