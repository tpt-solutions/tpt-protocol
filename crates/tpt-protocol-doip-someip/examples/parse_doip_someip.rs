//! Parse a DoIP diagnostic message (ISO 13400-2, payload type 0x8001) and show
//! the result. DoIP carries UDS requests over Ethernet; the payload here is the
//! source address, the target address, and a UDS request.
//!
//! Run with: `cargo run -p tpt-protocol-doip-someip --example parse_doip_someip`

use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_doip_someip::{DoipSomeipNormalizer, DoipSomeipParser};

/// Generic DoIP header: protocol version 0x02, inverse version 0xFD, payload
/// type (big-endian), payload length (big-endian), then the payload.
fn build_diagnostic_message(payload: &[u8]) -> Vec<u8> {
    let mut frame = vec![0x02, 0xFD, 0x80, 0x01];
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

fn main() {
    // Source 0x0E80 (tester), target 0x1000 (ECU), then UDS ReadDataByIdentifier 0xF190 (VIN).
    let payload = [0x0E, 0x80, 0x10, 0x00, 0x22, 0xF1, 0x90];
    let bytes = build_diagnostic_message(&payload);
    println!("frame (hex): {}", hex(&bytes));

    let frame = DoipSomeipParser::new()
        .parse(&bytes)
        .expect("a well-formed DoIP diagnostic message parses");
    println!("parsed: {frame:?}");

    match DoipSomeipNormalizer::new().normalize(&frame) {
        Ok(dp) => println!(
            "normalized: {} {} = {:?} ({})",
            dp.source_protocol, dp.point_id, dp.value, dp.quality
        ),
        Err(e) => println!("no data point (a request has no measured value): {e:?}"),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}
