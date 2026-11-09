//! Parse an NMEA 0183 GGA sentence (a GPS fix) and normalize it into TUP
//! data points: position, fix quality, satellites and altitude.
//!
//! Run with: `cargo run -p tpt-protocol-nmea --example parse_nmea`

use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_nmea::{NmeaNormalizer, NmeaParser};

/// `$<talker><sentence>,<fields>*<checksum>\r\n`, where the checksum is the XOR
/// of every byte between `$` and `*`.
fn build_sentence(talker: &str, sentence: &str, fields: &[&str]) -> Vec<u8> {
    let mut body = format!("{talker}{sentence}");
    for field in fields {
        body.push(',');
        body.push_str(field);
    }
    let checksum = body.bytes().fold(0u8, |acc, b| acc ^ b);
    format!("${body}*{checksum:02X}\r\n").into_bytes()
}

fn main() {
    let bytes = build_sentence(
        "GP",
        "GGA",
        &[
            "123519",
            "4807.038",
            "N",
            "01131.000",
            "E",
            "1",
            "08",
            "0.9",
            "545.4",
            "M",
            "46.9",
            "M",
            "",
            "",
        ],
    );
    println!("frame: {}", String::from_utf8_lossy(&bytes).trim_end());

    let frame = NmeaParser::new()
        .parse(&bytes)
        .expect("a well-formed GGA sentence parses");
    println!("parsed: {frame:?}");

    match NmeaNormalizer::new().normalize_all(&frame) {
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
