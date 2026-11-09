//! Parse a DNP3 data-link frame carrying an application response with data (an
//! answer to a READ) and normalize it. A bare READ request has no measured value,
//! so the response is the frame that carries one. The frame is built here with the CRCs that
//! DNP3 requires: one on the header and one per 16-byte block of user data.
//!
//! Run with: `cargo run -p tpt-protocol-dnp3 --example parse_dnp3`

use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_dnp3::{Dnp3Normalizer, Dnp3Parser};

/// CRC-16/DNP: polynomial 0x3D65 (reflected 0xA6BC), init 0, output XOR 0xFFFF.
fn crc16_dnp(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &byte in data {
        crc ^= u16::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xA6BC
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// Start bytes, length, control, destination, source, header CRC, then the user
/// data in blocks of up to 16 bytes, each followed by its CRC.
fn build_frame(dest: u16, src: u16, ctrl: u8, user: &[u8]) -> Vec<u8> {
    let mut out = vec![0x05, 0x64, (user.len() + 5) as u8, ctrl];
    out.extend_from_slice(&dest.to_le_bytes());
    out.extend_from_slice(&src.to_le_bytes());
    let header_crc = crc16_dnp(&out[0..8]);
    out.extend_from_slice(&header_crc.to_le_bytes());
    for block in user.chunks(16) {
        out.extend_from_slice(block);
        out.extend_from_slice(&crc16_dnp(block).to_le_bytes());
    }
    out
}

fn main() {
    // Transport byte 0xC0 (FIN and FIR), application control 0xC0, function 0x81
    // (RESPONSE), internal indications 0x0000, then object data (an analog input).
    let user = [0xC0, 0xC0, 0x81, 0x00, 0x00, 0x1E, 0x02, 0x00, 0x01, 0x2A];
    let bytes = build_frame(0x000A, 0x0001, 0x44, &user);
    println!("frame (hex): {}", hex(&bytes));

    // The CRC of the standard check string is a fixed value; checking it proves
    // the helper above matches DNP3.
    assert_eq!(crc16_dnp(b"123456789"), 0xEA82);

    let frame = Dnp3Parser::new()
        .parse(&bytes)
        .expect("a well-formed DNP3 frame parses");
    println!("parsed: {frame:?}");

    match Dnp3Normalizer::new().normalize(&frame) {
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
