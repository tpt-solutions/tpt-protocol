//! Parse a UDS (ISO 14229) negative response and normalize it. A negative
//! response (0x7F) names the rejected service and a negative response code,
//! and normalizes to a `Bad`-quality point.
//!
//! Run with: `cargo run -p tpt-protocol-uds --example parse_uds`

use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_uds::{UdsNormalizer, UdsParser};

fn main() {
    // 0x7F = negative response, 0x22 = the rejected ReadDataByIdentifier,
    // 0x31 = requestOutOfRange.
    let bytes = [0x7F, 0x22, 0x31];
    println!("frame (hex): {}", hex(&bytes));

    let frame = UdsParser::new()
        .parse(&bytes)
        .expect("a well-formed negative response parses");
    println!("parsed: {frame:?}");

    match UdsNormalizer::new().normalize(&frame) {
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
