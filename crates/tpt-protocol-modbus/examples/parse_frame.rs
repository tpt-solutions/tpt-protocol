//! Parse a raw Modbus RTU frame from the wire, decode it, and normalize it into a
//! TUP `DataPoint`. Demonstrates the `FrameParser` + `DataNormalizer` flow.
//!
//! Run with: `cargo run -p tpt-protocol-modbus --example parse_frame`

use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_modbus::{ModbusNormalizer, ModbusParser};

/// Standard Modbus CRC-16 (poly 0x8005, init 0xFFFF) — included here so the
/// example is self-contained (the crate keeps its own copy private).
fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= u16::from(b);
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

fn build_rtu_frame(slave: u8, func: u8, data: &[u8]) -> Vec<u8> {
    let mut raw = Vec::new();
    raw.push(slave);
    raw.push(func);
    raw.extend_from_slice(data);
    let crc = crc16_modbus(&raw);
    raw.extend_from_slice(&crc.to_le_bytes());
    raw
}

fn main() {
    // A "Read Holding Registers" request (slave 1, func 3, addr 0, qty 10). A
    // request carries no measured value, so normalizing it is an error.
    let request = build_rtu_frame(0x01, 0x03, &[0x00, 0x00, 0x00, 0x0A]);
    let parsed_request = ModbusParser::new()
        .parse(&request)
        .expect("request should parse as a valid Modbus RTU message");
    println!(
        "request: slave={} {:?}",
        parsed_request.slave_address, parsed_request.frame
    );
    let normalizer = ModbusNormalizer::new();
    assert!(normalizer.normalize(&parsed_request).is_err());

    // The matching response: func 3, byte count 4, registers 0x000A and 0x0102.
    // The parser needs the response direction to read it.
    let response = build_rtu_frame(0x01, 0x03, &[0x04, 0x00, 0x0A, 0x01, 0x02]);
    let parsed = ModbusParser::response()
        .parse(&response)
        .expect("response should parse as a valid Modbus RTU message");
    println!(
        "response: slave={} {:?}",
        parsed.slave_address, parsed.frame
    );

    let dp = normalizer
        .normalize(&parsed)
        .expect("parsed response should normalize to a DataPoint");
    println!(
        "normalized: {} {} = {:?} ({})",
        dp.source_protocol, dp.point_id, dp.value, dp.quality
    );
}
