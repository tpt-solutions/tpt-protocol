# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-30

### Added
- Initial release: formal-verification specifications and harness for TPT Protocol critical state machines in the TPT Protocol suite.
- `TelosError` error type (`Parse`, `Extract`, `CheckFailed`) implementing `Display` and `std::error::Error`.
- `VerificationReport` struct summarising one verified module's pass/fail and failures.
- Verification harness behind the `verify` feature: `verify_dir` (loads all `*.telos` under a directory), `verify_source` (verifies a single spec string), and `verify_bundled` (verifies the shipped `telos/` specs).
- Integration with the upstream `tpt-telos` toolchain (`tpt-telos-parser`, `tpt-telos-ir`, `tpt-telos-verifier`) gated behind the `verify` feature, using the QF_LRA Fourier–Motzkin verifier (no external Z3).
- Three bundled `.telos` specs under `telos/`: `modbus_crc.telos` (CRC-16 window in-bounds for `len >= 4`, no OOB read), `modbus_validator.telos` (non-negative error-count threshold arithmetic), and `modbus_normalizer.telos` (`WriteMultipleRegisters` yields at most the declared quantity `N`).
- A `#[test]` (`bundled_specs_verify`) that panics on any failed check so verification runs and fails loudly in CI.
