#![no_main]
//! Fuzz target: `Hl7Parser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_hl7v2::Hl7Parser;

fuzz_target!(|data: &[u8]| {
    let _ = Hl7Parser.parse(data);
    let _ = Hl7Parser.try_parse(data);
});
