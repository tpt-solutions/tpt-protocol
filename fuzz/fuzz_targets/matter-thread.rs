#![no_main]
//! Fuzz target: `MatterParser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_matter_thread::MatterParser;

fuzz_target!(|data: &[u8]| {
    let _ = MatterParser.parse(data);
    let _ = MatterParser.try_parse(data);
});
