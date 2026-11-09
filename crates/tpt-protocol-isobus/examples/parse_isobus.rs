//! Parse an ISOBUS (ISO 11783) CAN frame and normalize it into a TUP `DataPoint`.
//!
//! The frame is a PTO-style message with two little-endian data bytes.
//!
//! Run with: `cargo run -p tpt-protocol-isobus --example parse_isobus`

use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_isobus::{IsobusNormalizer, IsobusParser};

/// A 29-bit identifier (priority 3, PF, PS, source address), then the payload.
fn build_frame(pf: u8, ps: u8, source: u8, payload: &[u8]) -> Vec<u8> {
    let can_id = (0x03u32 << 26) | (u32::from(pf) << 16) | (u32::from(ps) << 8) | u32::from(source);
    let mut frame = can_id.to_be_bytes().to_vec();
    frame.extend_from_slice(payload);
    frame
}

fn main() {
    let bytes = build_frame(0xF0, 0x04, 0x30, &[0x01, 0x02, 0x00, 0x00]);
    println!("frame (hex): {}", hex(&bytes));

    let frame = IsobusParser::new()
        .parse(&bytes)
        .expect("a well-formed ISOBUS frame parses");
    println!("parsed: {frame:?}");

    match IsobusNormalizer::new().normalize(&frame) {
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
