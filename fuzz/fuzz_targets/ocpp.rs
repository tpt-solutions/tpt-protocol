#![no_main]
//! Fuzz target: `OcppParser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_ocpp::OcppParser;

fuzz_target!(|data: &[u8]| {
    let _ = OcppParser.parse(data);
    let _ = OcppParser.try_parse(data);
});
