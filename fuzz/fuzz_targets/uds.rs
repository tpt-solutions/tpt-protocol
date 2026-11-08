#![no_main]
//! Fuzz target: `UdsParser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_uds::UdsParser;

fuzz_target!(|data: &[u8]| {
    let _ = UdsParser.parse(data);
    let _ = UdsParser.try_parse(data);
});
