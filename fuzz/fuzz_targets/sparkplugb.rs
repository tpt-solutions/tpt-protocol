#![no_main]
//! Fuzz target: `SparkplugParser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_sparkplugb::SparkplugParser;

fuzz_target!(|data: &[u8]| {
    let _ = SparkplugParser.parse(data);
    let _ = SparkplugParser.try_parse(data);
});
