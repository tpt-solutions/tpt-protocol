//! Reads Modbus frames from stdin and prints a canonical summary of each, for the
//! differential test in `scripts/differential/modbus_pymodbus.py`.
//!
//! Each input line is `req <hex>` (a request) or `resp <hex>` (a response). The
//! frame may be RTU or TCP; the parser tells them apart. Each output line is one of:
//!
//! ```text
//! req unit=U fc=3 addr=A qty=Q
//! req unit=U fc=6 addr=A value=V
//! req unit=U fc=16 addr=A qty=Q values=v1,v2,...
//! resp unit=U fc=3 values=v1,v2,...
//! resp unit=U fc=6 addr=A value=V
//! resp unit=U fc=16 addr=A qty=Q
//! resp unit=U fc=F exception=E
//! error
//! ```
//!
//! Run with: `cargo run -q -p tpt-protocol-modbus --example diff_decode < input.txt`

use std::io::{self, BufRead, Write};

use tpt_protocol_core::FrameParser;
use tpt_protocol_modbus::{ModbusFrame, ModbusParser, ModbusRtuFrame};

fn main() -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in io::stdin().lock().lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (direction, hex) = line.split_once(' ').unwrap_or(("req", line));
        let summary = match decode_hex(hex) {
            Some(bytes) => {
                let parser = if direction == "resp" {
                    ModbusParser::response()
                } else {
                    ModbusParser::new()
                };
                match parser.parse(&bytes) {
                    Ok(frame) => describe(direction, &frame),
                    Err(_) => "error".to_string(),
                }
            }
            None => "error".to_string(),
        };
        writeln!(out, "{summary}")?;
    }
    Ok(())
}

fn describe(direction: &str, frame: &ModbusRtuFrame) -> String {
    let unit = frame.slave_address;
    let kind = if direction == "resp" { "resp" } else { "req" };
    let list = |values: &[u16]| {
        values
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };
    match &frame.frame {
        ModbusFrame::ReadHoldingRegisters { address, quantity } if kind == "req" => {
            format!("req unit={unit} fc=3 addr={address} qty={quantity}")
        }
        ModbusFrame::ReadInputRegisters { address, quantity } if kind == "req" => {
            format!("req unit={unit} fc=4 addr={address} qty={quantity}")
        }
        ModbusFrame::WriteSingleRegister { address, value } => {
            format!("{kind} unit={unit} fc=6 addr={address} value={value}")
        }
        ModbusFrame::WriteMultipleRegisters {
            address,
            quantity,
            values,
        } => format!(
            "req unit={unit} fc=16 addr={address} qty={quantity} values={}",
            list(values)
        ),
        ModbusFrame::ReadResponse {
            function_code,
            bytes,
        } => {
            let values: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|w| u16::from_be_bytes([w[0], w[1]]))
                .collect();
            format!(
                "resp unit={unit} fc={function_code} values={}",
                list(&values)
            )
        }
        ModbusFrame::WriteMultipleResponse { address, quantity } => {
            format!("resp unit={unit} fc=16 addr={address} qty={quantity}")
        }
        ModbusFrame::Exception {
            function_code,
            exception_code,
        } => format!("resp unit={unit} fc={function_code} exception={exception_code}"),
        _ => "error".to_string(),
    }
}

/// Decodes a hex string (no separators). `None` for odd length or a non-hex digit.
fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if text.len() % 2 != 0 {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}
