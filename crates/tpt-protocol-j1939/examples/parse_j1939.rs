//! Parse a J1939 CAN frame and normalize it into a TUP `DataPoint`.
//!
//! The frame is an Electronic Engine Controller 1 (EEC1) message, PGN 0xF004,
//! with engine speed in the bytes that J1939 stores little-endian.
//!
//! Run with: `cargo run -p tpt-protocol-j1939 --example parse_j1939`

use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_j1939::{J1939Normalizer, J1939Parser};

/// A 29-bit identifier (priority 3, PF 0xF0, PS 0x04, source 0x12), then 8 data bytes.
fn build_frame(pf: u8, ps: u8, source: u8, payload: &[u8]) -> Vec<u8> {
    let can_id = (0x03u32 << 26) | (u32::from(pf) << 16) | (u32::from(ps) << 8) | u32::from(source);
    let mut frame = can_id.to_be_bytes().to_vec();
    frame.extend_from_slice(payload);
    frame
}

fn main() {
    let bytes = build_frame(
        0xF0,
        0x04,
        0x12,
        &[0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
    );
    println!("frame (hex): {}", hex(&bytes));

    let frame = J1939Parser::new()
        .parse(&bytes)
        .expect("a well-formed J1939 frame parses");
    println!("parsed: {frame:?}");

    match J1939Normalizer::new().normalize(&frame) {
        Ok(dp) => println!(
            "normalized: {} {} = {:?} ({})",
            dp.source_protocol, dp.point_id, dp.value, dp.quality
        ),
        Err(e) => println!("no data point: {e:?}"),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}
