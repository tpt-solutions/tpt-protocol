# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-30

### Added
- Initial release: Sparkplug B support in the TPT Protocol suite.
- Binary metric-payload parsing: version, message type, sequence (0–255),
  timestamp (u64), and a metric TLV list.
- Message-type decoding: NBIRTH, DBIRTH, NCMD, DCMD, NDATA, DDATA, NDEATH, DDEATH.
- Metric value decoding for Int32, Int64, Float, Double, Boolean, and String
  (unknown data types return an error).
- State validation via the `StateValidator` trait (`SparkplugValidator` /
  `SparkplugConnectionState`).
- TUP normalisation via the `DataNormalizer` trait (`SparkplugNormalizer`),
  mapping frames to `tup::DataPoint`s (preserving the frame timestamp).
- `#![no_std]` + `alloc` support.
- Wire-controlled metric count capped at `MAX_SPARKPLUG_METRICS = 1024` to
  prevent unbounded allocation.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
