//! Reads UDS messages (ISO 14229) from stdin, one hex string per line, and prints a
//! canonical summary of each, for the differential test in
//! `scripts/differential/uds_udsoncan.py`.
//!
//! Each input line is a payload in hex. Each output line is one of:
//!
//! ```text
//! req sid=S sub=B|- suppress=true|false data=HEX
//! resp sid=S sub=B|- data=HEX
//! neg sid=S nrc=N
//! error
//! ```
//!
//! `S`, `B` and `N` are two hex digits. `sub` is the sub-function byte with the
//! suppress bit removed. Lines are numbered by the input order with a `#<n>` marker
//! before each, so a file with several payloads can be read back.
//!
//! Run with: `cargo run -q -p tpt-protocol-uds --example diff_decode < input.txt`

use std::io::{self, BufRead, Write};

use tpt_protocol_core::FrameParser;
use tpt_protocol_uds::{SubFunction, UdsFrame, UdsParser};

fn main() -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut index = 0usize;
    for line in io::stdin().lock().lines() {
        let line = line?;
        let hex = line.trim();
        if hex.is_empty() {
            continue;
        }
        writeln!(out, "#{index}")?;
        index += 1;
        let summary = match decode_hex(hex) {
            Some(bytes) => match UdsParser::new().parse(&bytes) {
                Ok(frame) => describe(&frame),
                Err(_) => "error".to_string(),
            },
            None => "error".to_string(),
        };
        writeln!(out, "{summary}")?;
    }
    Ok(())
}

fn describe(frame: &UdsFrame) -> String {
    let sid = frame.sid.value();
    let data_hex = hex(&frame.data);
    // A negative response keeps the requested SID and the NRC as its two data bytes.
    if sid == 0x7F {
        return match frame.data.as_slice() {
            [requested, nrc] => format!("neg sid={requested:02x} nrc={nrc:02x}"),
            _ => "error".to_string(),
        };
    }
    let sub = match frame.sub_function {
        SubFunction::None => "-".to_string(),
        SubFunction::Request(v) | SubFunction::Response(v) => format!("{v:02x}"),
    };
    if frame.is_request {
        format!(
            "req sid={sid:02x} sub={sub} suppress={} data={data_hex}",
            frame.suppress_positive_response
        )
    } else {
        format!("resp sid={sid:02x} sub={sub} data={data_hex}")
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}
