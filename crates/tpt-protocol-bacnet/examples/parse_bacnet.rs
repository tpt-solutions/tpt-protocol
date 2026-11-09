//! Parse a BACnet/IP ReadProperty answer (a ComplexACK carrying a REAL) and
//! normalize it into a TUP `DataPoint`. The request that asks for this value
//! carries no measurement, so the answer is the frame that does.
//!
//! Run with: `cargo run -p tpt-protocol-bacnet --example parse_bacnet`

use tpt_protocol_bacnet::{BacnetNormalizer, BacnetParser};
use tpt_protocol_core::{DataNormalizer, FrameParser};

/// BVLC Original-Unicast-NPDU (0x0A), with the length that makes the frame exact.
fn build_complex_ack(packed_object: u32, invoke_id: u8, value: f32) -> Vec<u8> {
    let mut body = vec![0x01, 0x00]; // NPDU: version 1, no addresses, normal priority
    body.extend_from_slice(&[0x30, invoke_id, 0x0C]); // APDU: complex ACK, ReadProperty
    body.push(0x0C); // context tag 0, length 4: object identifier
    body.extend_from_slice(&packed_object.to_be_bytes());
    body.extend_from_slice(&[0x19, 85]); // context tag 1, length 1: Present_Value
    body.push(0x3E); // opening tag 3
    body.push(0x44); // application tag REAL, length 4
    body.extend_from_slice(&value.to_be_bytes());
    body.push(0x3F); // closing tag 3

    let len = (4 + body.len()) as u16;
    let mut frame = vec![0x81, 0x0A];
    frame.extend_from_slice(&len.to_be_bytes());
    frame.extend_from_slice(&body);
    frame
}

fn main() {
    // Analog Input (type 0), instance 1, reporting 21.5 degrees.
    let bytes = build_complex_ack(0x0000_0001, 1, 21.5);
    println!("frame (hex): {}", hex(&bytes));

    let frame = BacnetParser::new()
        .parse(&bytes)
        .expect("a well-formed BACnet/IP frame parses");
    println!("parsed: {frame:?}");

    match BacnetNormalizer::new().normalize(&frame) {
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
