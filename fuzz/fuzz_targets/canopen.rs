#![no_main]
//! Fuzz target: `CanOpenParser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_canopen::CanOpenParser;

fuzz_target!(|data: &[u8]| {
    let _ = CanOpenParser.parse(data);
    let _ = CanOpenParser.try_parse(data);
});
