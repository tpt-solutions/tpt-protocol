#![no_main]
//! Fuzz target: `DoipSomeipParser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_doip_someip::DoipSomeipParser;

fuzz_target!(|data: &[u8]| {
    let _ = DoipSomeipParser.parse(data);
    let _ = DoipSomeipParser.try_parse(data);
});
