//! Parse a CANopen frame (a TPDO1 from node 16) and normalize it into a TUP
//! `DataPoint`. CANopen carries its COB-ID in the first two bytes, and the
//! payload is little-endian.
//!
//! Run with: `cargo run -p tpt-protocol-canopen --example parse_canopen`

use tpt_protocol_canopen::{CanOpenNormalizer, CanOpenParser};
use tpt_protocol_core::{DataNormalizer, FrameParser};

/// COB-ID = function code << 7 | node id, big-endian, then the payload.
fn build_frame(function_code: u8, node: u8, payload: &[u8]) -> Vec<u8> {
    let cob_id = (u16::from(function_code) << 7) | u16::from(node);
    let mut frame = cob_id.to_be_bytes().to_vec();
    frame.extend_from_slice(payload);
    frame
}

fn main() {
    // TPDO1 (function code 3) from node 16. The payload 0x0201 is little-endian.
    let bytes = build_frame(0x03, 0x10, &[0x01, 0x02, 0x00, 0x00]);
    println!("frame (hex): {}", hex(&bytes));

    let frame = CanOpenParser::new()
        .parse(&bytes)
        .expect("a well-formed CANopen frame parses");
    println!("parsed: {frame:?}");

    match CanOpenNormalizer::new().normalize(&frame) {
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
