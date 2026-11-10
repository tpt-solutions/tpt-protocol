//! Reads HL7 v2 messages from stdin, one per line, and prints the fields the parser
//! keeps, for the differential test in `scripts/differential/hl7_hl7py.py`.
//!
//! A message is one line, with its segments separated by the two characters `\r`
//! (a backslash and an r), because a line holds one message. Each message starts
//! with a marker line, `#<n>`, counting the messages from 0. Then come one line per
//! field, `<SEG>-<n> <text>`, for the non-empty fields. MSH-1 and MSH-2 are not
//! listed, and MSH-8 is not kept by the parser, so it is not listed either. A
//! message that does not parse gets the single line `error`.
//!
//! Run with: `cargo run -q -p tpt-protocol-hl7v2 --example diff_decode < input.txt`

use std::io::{self, BufRead, Write};

use tpt_protocol_core::FrameParser;
use tpt_protocol_hl7v2::{Hl7Frame, Hl7Parser};

fn main() -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut index = 0usize;
    for line in io::stdin().lock().lines() {
        let line = line?;
        let message = line.trim_end();
        if message.is_empty() {
            continue;
        }
        writeln!(out, "#{index}")?;
        index += 1;
        // The two characters backslash and r stand for a segment terminator.
        let text = message.replace("\\r", "\r");
        match Hl7Parser.parse(text.as_bytes()) {
            Ok(frame) => {
                for field in describe(&frame) {
                    writeln!(out, "{field}")?;
                }
            }
            Err(_) => writeln!(out, "error")?,
        }
    }
    Ok(())
}

fn describe(frame: &Hl7Frame) -> Vec<String> {
    let msh = &frame.msh;
    let mut out = Vec::new();
    // The MSH fields the parser keeps, under their HL7 numbers. MSH-8 is dropped.
    let msh_fields = [
        (3, &msh.sending_app),
        (4, &msh.sending_facility),
        (5, &msh.receiving_app),
        (6, &msh.receiving_facility),
        (7, &msh.date_time),
        (9, &msh.message_type),
        (10, &msh.message_control_id),
        (11, &msh.processing_id),
        (12, &msh.version_id),
    ];
    for (number, value) in msh_fields {
        if !value.is_empty() {
            out.push(format!("MSH-{number} {value}"));
        }
    }
    for segment in &frame.segments {
        // Field 0 of the parsed list is the empty piece before the first separator,
        // so index i is HL7 field i.
        for (i, field) in segment.fields.iter().enumerate().skip(1) {
            let text = field.components.join("^");
            if !text.is_empty() {
                out.push(format!("{}-{i} {text}", segment.name));
            }
        }
    }
    out
}
