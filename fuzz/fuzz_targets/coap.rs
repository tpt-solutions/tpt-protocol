#![no_main]
//! Fuzz target: `CoapParser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_coap::CoapParser;

fuzz_target!(|data: &[u8]| {
    let _ = CoapParser.parse(data);
    let _ = CoapParser.try_parse(data);
});
