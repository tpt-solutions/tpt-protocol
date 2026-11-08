#![no_main]
//! Fuzz target: `OpcuaParser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_opcua::OpcuaParser;

fuzz_target!(|data: &[u8]| {
    let _ = OpcuaParser.parse(data);
    let _ = OpcuaParser.try_parse(data);
});
