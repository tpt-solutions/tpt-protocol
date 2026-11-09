//! Parse an EtherCAT frame carrying one LRD (logical read) datagram and show the
//! parsed datagrams. The frame is built here: a little-endian header with the
//! length and the type field, then the datagrams, each with a working counter.
//!
//! Run with: `cargo run -p tpt-protocol-ethercat --example parse_ethercat`

use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_ethercat::{EthercatNormalizer, EthercatParser};

/// One datagram: command, index, address, data length and more-follows flag,
/// IRQ, the data, then the working counter. All fields are little-endian.
fn datagram(command: u8, index: u8, address: u32, data: &[u8]) -> Vec<u8> {
    let mut out = vec![command, index];
    out.extend_from_slice(&address.to_le_bytes());
    out.extend_from_slice(&(data.len() as u16).to_le_bytes()); // no more-follows bit
    out.extend_from_slice(&[0x00, 0x00]); // IRQ
    out.extend_from_slice(data);
    out.extend_from_slice(&0x0003u16.to_le_bytes()); // working counter
    out
}

/// Header: length in the low 11 bits, type 1 (EtherCAT) in bits 12-15.
fn frame(datagrams: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = datagrams.concat();
    let header = (body.len() as u16 & 0x07FF) | (1 << 12);
    let mut out = header.to_le_bytes().to_vec();
    out.extend_from_slice(&body);
    out
}

fn main() {
    // LRD (command 10): logical address 0x0001_0000, reading two bytes of process data.
    let bytes = frame(&[datagram(10, 0x01, 0x0001_0000, &[0xAA, 0xBB])]);
    println!("frame (hex): {}", hex(&bytes));

    let parsed = EthercatParser::new()
        .parse(&bytes)
        .expect("a well-formed EtherCAT frame parses");
    println!("parsed: {parsed:?}");

    match EthercatNormalizer::new().normalize(&parsed) {
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
