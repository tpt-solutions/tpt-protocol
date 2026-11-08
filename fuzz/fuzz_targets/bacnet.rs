#![no_main]
//! Fuzz target: `BacnetParser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_bacnet::BacnetParser;

fuzz_target!(|data: &[u8]| {
    let _ = BacnetParser.parse(data);
    let _ = BacnetParser.try_parse(data);
});
