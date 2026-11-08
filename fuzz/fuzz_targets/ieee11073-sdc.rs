#![no_main]
//! Fuzz target: `SdcParser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_ieee11073_sdc::SdcParser;

fuzz_target!(|data: &[u8]| {
    let _ = SdcParser.parse(data);
    let _ = SdcParser.try_parse(data);
});
