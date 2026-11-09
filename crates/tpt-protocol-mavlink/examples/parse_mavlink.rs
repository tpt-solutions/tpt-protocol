//! Parse a MAVLink v1 HEARTBEAT and normalize it into TUP data points.
//!
//! The frame is built here so the example needs no flight-controller. The CRC
//! uses the message's CRC_EXTRA seed (50 for HEARTBEAT), as the MAVLink spec
//! requires.
//!
//! Run with: `cargo run -p tpt-protocol-mavlink --example parse_mavlink`

use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_mavlink::{MavlinkNormalizer, MavlinkParser};

/// CRC_EXTRA for HEARTBEAT (message id 0). Each MAVLink message has its own seed.
const HEARTBEAT_CRC_EXTRA: u8 = 50;

/// MAVLink's CRC-16/MCRF4XX (X.25) over `bytes`, then the CRC_EXTRA seed.
fn crc_with_extra(bytes: &[u8], extra: u8) -> u16 {
    fn accumulate(crc: u16, byte: u8) -> u16 {
        let tmp = byte ^ (crc & 0xFF) as u8;
        let tmp = tmp ^ (tmp << 4);
        (crc >> 8) ^ (u16::from(tmp) << 8) ^ (u16::from(tmp) << 3) ^ u16::from(tmp >> 4)
    }
    let crc = bytes.iter().fold(0xFFFFu16, |crc, &b| accumulate(crc, b));
    accumulate(crc, extra)
}

/// A v1 frame: magic 0xFE, length, sequence, system id, component id, message
/// id, payload, then the little-endian CRC over everything after the magic byte.
fn build_heartbeat(system: u8, component: u8) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&0u32.to_le_bytes()); // custom_mode
    payload.push(2); // type: quadrotor
    payload.push(3); // autopilot: ArduPilotMega
    payload.push(0x81); // base_mode: armed, custom mode enabled
    payload.push(4); // system_status: active
    payload.push(3); // mavlink_version

    let mut frame = vec![0xFE, payload.len() as u8, 0, system, component, 0];
    frame.extend_from_slice(&payload);
    let crc = crc_with_extra(&frame[1..], HEARTBEAT_CRC_EXTRA);
    frame.extend_from_slice(&crc.to_le_bytes());
    frame
}

fn main() {
    let bytes = build_heartbeat(1, 1);
    println!("frame (hex): {}", hex(&bytes));

    let frame = MavlinkParser::new()
        .parse(&bytes)
        .expect("a well-formed HEARTBEAT parses");
    println!(
        "parsed: system {} component {}",
        frame.system_id, frame.component_id
    );

    match MavlinkNormalizer::new().normalize_all(&frame) {
        Ok(points) => {
            for dp in points {
                println!(
                    "normalized: {} {} = {:?} ({})",
                    dp.source_protocol, dp.point_id, dp.value, dp.quality
                );
            }
        }
        Err(e) => println!("no data points: {e:?}"),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}
