#![no_main]
//! Fuzz target: JSON mirror parse must never panic on arbitrary text.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_tup::json;

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = std::str::from_utf8(data) {
        let _ = json::from_json_str(text);
    }
});
