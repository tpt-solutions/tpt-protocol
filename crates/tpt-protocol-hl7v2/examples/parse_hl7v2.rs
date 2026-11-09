//! Parse an HL7 v2 ORU^R01 result message and normalize its OBX observations
//! into TUP data points. OBX segments carry the numeric results.
//!
//! Run with: `cargo run -p tpt-protocol-hl7v2 --example parse_hl7v2`

use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_hl7v2::{Hl7Normalizer, Hl7Parser};

/// Segments are separated by carriage returns, as HL7 requires.
fn build_oru() -> Vec<u8> {
    [
        "MSH|^~\\&|LAB|HOSP|EHR|HOSP|20240101010000||ORU^R01|msg002|P|2.5|",
        "PID|1||67890^^^MRN^MRN||Smith^Jane||19850615|F",
        "OBR|1|ord123|spec456|LIPID^Lipid Panel^L|||20240101000000|||||||20240101010000|||F",
        "OBX|1|NM|CHOL^Cholesterol^L||190|mg/dL|100-199|N|||F",
    ]
    .join("\r")
    .into_bytes()
}

fn main() {
    let bytes = build_oru();
    println!("frame:");
    for segment in String::from_utf8_lossy(&bytes).split('\r') {
        println!("  {segment}");
    }

    let frame = Hl7Parser
        .parse(&bytes)
        .expect("a well-formed ORU^R01 parses");
    println!("parsed: {frame:?}");

    match Hl7Normalizer.normalize_all(&frame) {
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
