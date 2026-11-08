#![no_main]
//! Fuzz target: `IsobusParser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_isobus::IsobusParser;

fuzz_target!(|data: &[u8]| {
    let _ = IsobusParser.parse(data);
    let _ = IsobusParser.try_parse(data);
});
