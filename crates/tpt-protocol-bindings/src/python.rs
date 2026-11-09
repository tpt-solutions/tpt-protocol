//! The Python module, built with `--features python`.
//!
//! ```python
//! import tpt_protocol
//! tpt_protocol.protocols()                       # ['modbus', 'j1939', ...]
//! tpt_protocol.decode("modbus", bytes.fromhex("01060001000A580D"))
//! # '[{"schema_version":1,"source_protocol":"modbus", ... "value":10, ...}]'
//! ```

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

/// Decodes one frame with the named protocol and returns the data points as a JSON array.
#[pyfunction]
fn decode(protocol: &str, data: &[u8]) -> PyResult<String> {
    crate::decode_json(protocol, data).map_err(PyValueError::new_err)
}

/// The protocol names this build understands.
#[pyfunction]
fn protocols() -> Vec<&'static str> {
    crate::PROTOCOLS.to_vec()
}

#[pymodule]
fn tpt_protocol(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(decode, m)?)?;
    m.add_function(wrap_pyfunction!(protocols, m)?)?;
    Ok(())
}
