#![no_main]
//! Fuzz target: `J1939Parser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_j1939::J1939Parser;

fuzz_target!(|data: &[u8]| {
    let _ = J1939Parser.parse(data);
    let _ = J1939Parser.try_parse(data);
});
