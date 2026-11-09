//! Formal verification of TPT Protocol critical state machines using tpt-telos.
//!
//! This crate holds the `.telos` specifications for the safety-critical
//! invariants that the protocol crates must uphold, and a verification harness
//! that discharges them with `tpt-telos-verifier` (QF_LRA Fourier–Motzkin, no
//! external SMT solver required).
//!
//! The `.telos` sources live in the `telos/` directory. They encode the
//! mathematical contracts behind:
//!
//! * **Modbus CRC framing** — byte-index bounds so the CRC window can never
//!   read before the start of the frame or past its end (no out-of-bounds read
//!   on truncated/garbage input).
//! * **Modbus validator thresholds** — the connection-health state machine
//!   rejects a device that is not connected, or whose `error_count` is above 10.
//!   It accepts a connected device with `error_count <= 10`.
//! * **Register normalizer bounds** — a `WriteMultipleRegisters` frame with a
//!   declared quantity of `N` yields at most `N` register values and is
//!   rejected when the payload is too short.
//!
//! Run the verifier as part of the test suite:
//!
//! ```text
//! cargo test -p tpt-protocol-telos
//! ```
#![cfg_attr(docsrs, feature(doc_cfg))]

/// Errors produced by the verification harness.
#[derive(Debug, Clone, PartialEq)]
pub enum TelosError {
    /// A `.telos` source failed to parse.
    Parse(String),
    /// Constraint extraction failed.
    Extract(String),
    /// A verification check failed (with a counter-example description).
    CheckFailed(String),
}

impl core::fmt::Display for TelosError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "telos parse error: {e}"),
            Self::Extract(e) => write!(f, "telos extract error: {e}"),
            Self::CheckFailed(e) => write!(f, "telos verification failed: {e}"),
        }
    }
}

impl std::error::Error for TelosError {}

/// One verified `.telos` module plus its pass/fail summary.
#[derive(Debug, Clone)]
pub struct VerificationReport {
    pub func_name: String,
    pub all_passed: bool,
    pub failures: Vec<String>,
}

/// Load every `.telos` file from `dir` and verify each one with the tpt-telos
/// pipeline. `dir` is used as given: a relative path is resolved against the
/// process's current directory, so prefer an absolute path (see
/// [`bundled_dir`]) when the caller is not the crate itself.
#[cfg(feature = "verify")]
pub fn verify_dir(dir: &str) -> Result<Vec<VerificationReport>, TelosError> {
    let mut reports = Vec::new();

    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| TelosError::Parse(format!("cannot read {dir}: {e}")))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "telos").unwrap_or(false))
        .collect();
    entries.sort();

    for path in entries {
        let src = std::fs::read_to_string(&path)
            .map_err(|e| TelosError::Parse(format!("read {}: {e}", path.display())))?;
        let report = verify_source(&src)
            .map_err(|e| TelosError::Parse(format!("{}: {e}", path.display())))?;
        reports.push(report);
    }

    Ok(reports)
}

/// Parse, extract, and verify a single `.telos` source string.
#[cfg(feature = "verify")]
pub fn verify_source(src: &str) -> Result<VerificationReport, TelosError> {
    use tpt_telos_ir::extract;
    use tpt_telos_parser::parse;
    use tpt_telos_verifier::verify;

    let modules = parse(src).map_err(TelosError::Parse)?;
    let problems = extract(&modules).map_err(TelosError::Extract)?;

    let mut failures = Vec::new();
    let mut all_passed = true;
    let mut func_name = String::new();

    for problem in &problems {
        func_name = problem.func_name.clone();
        let result = verify(problem);
        if !result.all_passed {
            all_passed = false;
            for check in result.checks.iter().filter(|c| !c.passed) {
                failures.push(check.description.clone());
            }
        }
    }

    if all_passed {
        Ok(VerificationReport {
            func_name,
            all_passed: true,
            failures,
        })
    } else {
        Err(TelosError::CheckFailed(failures.join("; ")))
    }
}

/// The bundled `telos/` directory, resolved from this crate's manifest so it
/// does not depend on the current directory.
pub fn bundled_dir() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/telos")
}

/// Verifies all bundled `.telos` specs and panics on any failed check.
///
/// Intended to be called from a `#[test]` so verification runs in CI.
#[cfg(feature = "verify")]
pub fn verify_bundled() -> Result<(), TelosError> {
    let reports = verify_dir(bundled_dir())?;
    let mut failed = Vec::new();
    for r in &reports {
        if !r.all_passed {
            failed.push(format!("{}: {}", r.func_name, r.failures.join(", ")));
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(TelosError::CheckFailed(failed.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "verify")]
    fn bundled_specs_verify() {
        verify_bundled().expect("all bundled .telos specs must verify");
    }

    #[test]
    fn error_display() {
        let e = TelosError::CheckFailed("invariant violated".into());
        assert!(e.to_string().contains("invariant violated"));
    }

    #[cfg(feature = "verify")]
    #[test]
    fn bundled_dir_is_absolute_and_loads_without_cwd() {
        // The bundled path is anchored to the manifest, so it does not depend on
        // where the test binary runs from.
        assert!(std::path::Path::new(bundled_dir()).is_absolute());
        let reports = verify_dir(bundled_dir()).expect("bundled specs must load");
        assert_eq!(reports.len(), 3, "expected the three bundled Modbus specs");
    }
}

/// Conformance tests: the real `tpt-protocol-modbus` code against the contracts
/// the `.telos` specs state. They sample inputs, so they check behaviour, not
/// a proof. A spec that describes something the code does not do fails here.
#[cfg(test)]
mod conformance {
    use tpt_protocol_core::{DataNormalizer, FrameEncoder, FrameParser, StateValidator};
    use tpt_protocol_modbus::{
        ModbusEncoder, ModbusFrame, ModbusNormalizer, ModbusParser, ModbusRtuFrame, ModbusState,
        ModbusValidator,
    };

    /// Small deterministic generator, so the tests need no extra dependency.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u8 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) as u8
        }
    }

    /// ModbusCrc: for any input of length >= 4, a frame is accepted only when its
    /// trailing two bytes are the CRC of the bytes before them. The parser must
    /// never read outside the buffer.
    #[test]
    fn crc_window_is_in_bounds_and_only_valid_frames_are_accepted() {
        let parser = ModbusParser::new();
        let encoder = ModbusEncoder;
        let mut rng = Lcg(7);
        for len in 0..=64usize {
            for _ in 0..200 {
                let bytes: Vec<u8> = (0..len).map(|_| rng.next()).collect();
                if let Ok(frame) = parser.parse(&bytes) {
                    // An accepted frame re-encodes to exactly the input bytes,
                    // which can only happen when the CRC window is correct.
                    let re = encoder.encode(&frame).expect("accepted frame must encode");
                    assert_eq!(re, bytes, "accepted frame must round-trip with a valid CRC");
                }
            }
        }
    }

    /// ModbusNormalizer: a WriteMultipleRegisters frame with quantity N yields at
    /// most N values, and a payload shorter than N values is rejected.
    #[test]
    fn write_multiple_registers_yields_at_most_quantity_values() {
        let parser = ModbusParser::new();
        let encoder = ModbusEncoder;
        let normalizer = ModbusNormalizer::new();
        for quantity in 1u16..=20 {
            let values: Vec<u16> = (0..quantity).map(|v| v * 3).collect();
            let frame = ModbusRtuFrame {
                slave_address: 1,
                frame: ModbusFrame::WriteMultipleRegisters {
                    address: 0,
                    quantity,
                    values,
                },
                crc: 0,
                transaction_id: None,
            };
            let bytes = encoder.encode(&frame).expect("encode");
            let parsed = parser.parse(&bytes).expect("a well-formed request parses");
            let points = normalizer.normalize_all(&parsed).unwrap_or_default();
            assert!(
                points.len() <= quantity as usize,
                "quantity {quantity} yielded {} values",
                points.len()
            );

            // A payload with one byte too few must not parse.
            let mut short = bytes.clone();
            let n = short.len();
            short.remove(n - 3); // drop a data byte, keep the length byte stale
            assert!(
                parser.parse(&short).is_err(),
                "short payload must be rejected: quantity {quantity}, bytes {short:02X?}, parsed {:?}",
                parser.parse(&short)
            );
        }
    }

    /// Validator thresholds: a device is rejected exactly when it is not connected
    /// or its error count is above 10.
    #[test]
    fn validator_rejects_exactly_when_disconnected_or_over_threshold() {
        let validator = ModbusValidator::new();
        for connected in [true, false] {
            for error_count in 0u64..=20 {
                let state = ModbusState {
                    connected,
                    error_count,
                    ..ModbusState::default()
                };
                let expect_err = !connected || error_count > 10;
                assert_eq!(
                    validator.validate(&state).is_err(),
                    expect_err,
                    "connected={connected} error_count={error_count}"
                );
            }
        }
    }
}
