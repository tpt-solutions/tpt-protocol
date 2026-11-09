//! Parse a KNX cEMI L_Data group telegram and normalize it. The telegram is a
//! 1-bit switch write (DPT 1) to group address 0x0801 (1/0/1).
//!
//! Run with: `cargo run -p tpt-protocol-knx --example parse_knx`

use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_knx::{KnxNormalizer, KnxParser};

/// cEMI L_Data.ind with no additional info: message code, info length, control 1,
/// control 2 (group destination, hop count 6), source, destination, NPDU length
/// minus one, then the NPDU.
fn build_group_write(destination: u16, npdu: &[u8]) -> Vec<u8> {
    let mut out = vec![0x29, 0x00, 0xBC, 0xE0, 0x11, 0x01, 0x00, 0x00];
    out[6..8].copy_from_slice(&destination.to_be_bytes());
    out.push((npdu.len() - 1) as u8);
    out.extend_from_slice(npdu);
    out
}

fn main() {
    // NPDU: GroupValue_Write with a 1-bit value of 1 (the low bit of the second byte).
    let bytes = build_group_write(0x0801, &[0x00, 0x81]);
    println!("frame (hex): {}", hex(&bytes));

    let frame = KnxParser
        .parse(&bytes)
        .expect("a well-formed cEMI telegram parses");
    println!("parsed: {frame:?}");

    match KnxNormalizer.normalize(&frame) {
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
