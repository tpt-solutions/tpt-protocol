#![no_main]
//! Fuzz target: `EthercatParser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_ethercat::EthercatParser;

fuzz_target!(|data: &[u8]| {
    let _ = EthercatParser.parse(data);
    let _ = EthercatParser.try_parse(data);
});
