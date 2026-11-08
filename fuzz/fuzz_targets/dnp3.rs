#![no_main]
//! Fuzz target: `Dnp3Parser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_dnp3::Dnp3Parser;

fuzz_target!(|data: &[u8]| {
    let _ = Dnp3Parser.parse(data);
    let _ = Dnp3Parser.try_parse(data);
});
