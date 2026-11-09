//! Parse an OPC UA Binary message chunk (a `MSG` chunk with a sequence header
//! and a service body) and show the parsed fields.
//!
//! Run with: `cargo run -p tpt-protocol-opcua --example parse_opcua`

use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_opcua::{OpcuaNormalizer, OpcuaParser};

/// Message header (type, chunk flag, size), the optional sequence header, then the body.
/// The size, little-endian, covers the whole message.
fn build_message(
    msg_type: &[u8; 3],
    chunk: u8,
    sequence: Option<(u32, u32, u32, u32)>,
    body: &[u8],
) -> Vec<u8> {
    let mut out = msg_type.to_vec();
    out.push(chunk);
    out.extend_from_slice(&[0, 0, 0, 0]); // size, filled in below
    if let Some((channel, token, seq, req)) = sequence {
        out.extend_from_slice(&channel.to_le_bytes());
        out.extend_from_slice(&token.to_le_bytes());
        out.extend_from_slice(&seq.to_le_bytes());
        out.extend_from_slice(&req.to_le_bytes());
    }
    out.extend_from_slice(body);
    let size = out.len() as u32;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    out
}

/// A service body: a two-byte NodeId (42), a binary body encoding, a length, then the data.
fn service_body(data: &[u8]) -> Vec<u8> {
    let mut body = vec![0x00, 0x2A];
    body.push(0x01);
    body.extend_from_slice(&(data.len() as i32).to_le_bytes());
    body.extend_from_slice(data);
    body
}

fn main() {
    let bytes = build_message(
        b"MSG",
        b'F',
        Some((0x1234, 0x5678, 1, 2)),
        &service_body(&[0xAA]),
    );
    println!("frame (hex): {}", hex(&bytes));

    let frame = OpcuaParser::new()
        .parse(&bytes)
        .expect("a well-formed OPC UA chunk parses");
    println!("parsed: {frame:?}");

    match OpcuaNormalizer::new().normalize(&frame) {
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
