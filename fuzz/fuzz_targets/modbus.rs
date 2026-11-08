#![no_main]
//! Fuzz target: `ModbusParser` must never panic on arbitrary bytes.

use libfuzzer_sys::fuzz_target;
use tpt_protocol_core::FrameParser;
use tpt_protocol_modbus::ModbusParser;

fuzz_target!(|data: &[u8]| {
    let _ = ModbusParser::new().parse(data);
    let _ = ModbusParser::new().try_parse(data);
});
