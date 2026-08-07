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
    // A Modbus "Read Holding Registers" request: slave 1, func 3, addr 0, qty 10.
    let frame = build_rtu_frame(0x01, 0x03, &[0x00, 0x00, 0x00, 0x0A]);

    let parser = ModbusParser::new();
    let parsed = parser
        .parse(&frame)
        .expect("frame should parse as a valid Modbus RTU message");
    println!("parsed: slave={} {:?}", parsed.slave_address, parsed.frame);

    let normalizer = ModbusNormalizer::new();
    let dp = normalizer
        .normalize(&parsed)
        .expect("parsed frame should normalize to a DataPoint");
    println!(
        "normalized: {} {} = {:?} ({})",
        dp.source_protocol,
        dp.point_id,
        dp.value,
        dp.quality
    );
}
