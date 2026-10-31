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
//! * **Modbus validator thresholds** — the connection-health state machine only
//!   rejects once `error_count > 10` and never while `connected == true`.
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

/// Load every `.telos` file from `dir` (relative to the crate root) and verify
/// each one with the tpt-telos pipeline.
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

/// Verifies all bundled `.telos` specs and panics on any failed check.
///
/// Intended to be called from a `#[test]` so verification runs in CI.
#[cfg(feature = "verify")]
pub fn verify_bundled() -> Result<(), TelosError> {
    let reports = verify_dir("telos")?;
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
}
