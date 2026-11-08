#![no_main]
//! Fuzz target: `NmeaParser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_nmea::NmeaParser;

fuzz_target!(|data: &[u8]| {
    let _ = NmeaParser.parse(data);
    let _ = NmeaParser.try_parse(data);
});
