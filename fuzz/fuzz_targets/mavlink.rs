#![no_main]
//! Fuzz target: `MavlinkParser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_mavlink::MavlinkParser;

fuzz_target!(|data: &[u8]| {
    let _ = MavlinkParser.parse(data);
    let _ = MavlinkParser.try_parse(data);
});
