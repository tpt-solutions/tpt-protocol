//! The WebAssembly exports, built with `--features wasm`. The `www/` playground
//! calls these.

use wasm_bindgen::prelude::*;

/// Decodes a frame given as hex text. Returns the data points as a JSON array.
#[wasm_bindgen]
pub fn decode_hex(protocol: &str, hex_text: &str) -> Result<String, JsValue> {
    crate::decode_hex(protocol, hex_text).map_err(|e| JsValue::from_str(&e))
}

/// The protocol names this build understands.
#[wasm_bindgen]
pub fn protocols() -> Vec<String> {
    crate::PROTOCOLS
        .iter()
        .map(|name| name.to_string())
        .collect()
}
