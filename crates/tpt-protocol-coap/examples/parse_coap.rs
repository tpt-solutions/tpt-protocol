//! Parse a CoAP response (RFC 7252) and normalize it into a TUP `DataPoint`.
//!
//! The message is an ACK carrying `Content` (2.05) with a JSON payload.
//!
//! Run with: `cargo run -p tpt-protocol-coap --example parse_coap`

use tpt_protocol_coap::{CoapNormalizer, CoapParser};
use tpt_protocol_core::{DataNormalizer, FrameParser};

fn build_response() -> Vec<u8> {
    let mut buf = Vec::new();
    buf.push(0b0110_0000); // version 1, ACK, token length 0
    buf.push(69); // code 2.05 Content
    buf.extend_from_slice(&0x0001u16.to_be_bytes()); // message id
    buf.push(0xFF); // payload marker
    buf.extend_from_slice(b"{\"state\":true}");
    buf
}

fn main() {
    let bytes = build_response();
    println!("frame (hex): {}", hex(&bytes));

    let frame = CoapParser
        .parse(&bytes)
        .expect("a well-formed CoAP response parses");
    println!("parsed: {frame:?}");

    match CoapNormalizer.normalize(&frame) {
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
