//! Parse a Matter (CSA) unsecured Interaction Model ReportData message carrying
//! one attribute value: OnOff (cluster 0x0006) attribute 0x0000 on endpoint 1 is
//! true. The message is built here with Matter's TLV encoding.
//!
//! Run with: `cargo run -p tpt-protocol-matter-thread --example parse_matter`

use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_matter_thread::{MatterNormalizer, MatterParser};

/// Interaction Model protocol id, and the ReportData opcode within it.
const PROTOCOL_INTERACTION_MODEL: u16 = 0x0001;
const OPCODE_REPORT_DATA: u8 = 0x05;

// Matter TLV: a control byte holds the element type in its low five bits, and
// bit 5 says a one-byte context tag follows it.

fn put_control(out: &mut Vec<u8>, tag: Option<u8>, element_type: u8) {
    match tag {
        Some(t) => {
            out.push(0x20 | element_type);
            out.push(t);
        }
        None => out.push(element_type),
    }
}

fn put_uint(out: &mut Vec<u8>, tag: Option<u8>, v: u64) {
    let (element_type, size) = if v <= 0xFF {
        (0x04, 1)
    } else if v <= 0xFFFF {
        (0x05, 2)
    } else {
        (0x06, 4)
    };
    put_control(out, tag, element_type);
    out.extend_from_slice(&v.to_le_bytes()[..size]);
}

fn put_bool(out: &mut Vec<u8>, tag: Option<u8>, v: bool) {
    put_control(out, tag, if v { 0x09 } else { 0x08 });
}

fn start(out: &mut Vec<u8>, tag: Option<u8>, element_type: u8) {
    put_control(out, tag, element_type);
}

fn end(out: &mut Vec<u8>) {
    out.push(0x18);
}

/// A ReportData payload with one attribute report whose value is written by `value`.
fn report_data(
    endpoint: u64,
    cluster: u64,
    attribute: u64,
    value: impl FnOnce(&mut Vec<u8>),
) -> Vec<u8> {
    let mut out = Vec::new();
    start(&mut out, None, 0x15); // ReportData structure
    start(&mut out, Some(1), 0x16); // AttributeReportIBs array
    start(&mut out, None, 0x15); // AttributeReportIB
    start(&mut out, Some(1), 0x15); // AttributeData
    put_uint(&mut out, Some(0), 7); // DataVersion
    start(&mut out, Some(1), 0x17); // AttributePath list
    put_uint(&mut out, Some(2), endpoint);
    put_uint(&mut out, Some(3), cluster);
    put_uint(&mut out, Some(4), attribute);
    end(&mut out);
    value(&mut out); // tag 2 is written by the closure
    end(&mut out); // AttributeData
    end(&mut out); // AttributeReportIB
    end(&mut out); // array
    end(&mut out); // ReportData
    out
}

/// Message header, a protocol header (exchange flags, opcode, exchange id,
/// protocol id), then the payload. A source node id makes the header longer.
fn unsecured_message(source: u64, protocol_id: u16, opcode: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = vec![0x04, 0x00]; // flags: source node present; security flags
    out.extend_from_slice(&0u16.to_le_bytes()); // session id
    out.extend_from_slice(&42u32.to_le_bytes()); // message counter
    out.extend_from_slice(&source.to_le_bytes());
    out.push(0x05); // exchange flags: initiator, reliability
    out.push(opcode);
    out.extend_from_slice(&0x1234u16.to_le_bytes()); // exchange id
    out.extend_from_slice(&protocol_id.to_le_bytes());
    out.extend_from_slice(payload);
    out
}

fn main() {
    let payload = report_data(1, 0x0006, 0x0000, |out| put_bool(out, Some(2), true));
    let bytes = unsecured_message(
        0x1122,
        PROTOCOL_INTERACTION_MODEL,
        OPCODE_REPORT_DATA,
        &payload,
    );
    println!("frame (hex): {}", hex(&bytes));

    let frame = MatterParser
        .parse(&bytes)
        .expect("a well-formed Matter message parses");
    println!(
        "parsed: encrypted={} attributes={}",
        frame.encrypted,
        frame.attributes.len()
    );

    match MatterNormalizer.normalize_all(&frame) {
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
