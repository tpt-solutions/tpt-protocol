#![no_main]
//! Fuzz target: `KnxParser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_knx::KnxParser;

fuzz_target!(|data: &[u8]| {
    let _ = KnxParser.parse(data);
    let _ = KnxParser.try_parse(data);
});
