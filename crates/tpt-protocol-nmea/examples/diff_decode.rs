//! Reads NMEA 0183 sentences from stdin, one per line, and prints the points the
//! normalizer gives for each, for the differential test in
//! `scripts/differential/nmea_pynmea2.py`.
//!
//! Each sentence starts with a marker line, `#<n>`, where `n` counts the sentences
//! from 0. Then come one line per point, `<point id> <value>`. Floats are printed
//! with six decimals, integers as integers, and text in double quotes. A sentence
//! that fails to parse, or that carries no decodable field, gets the single line
//! `error` after its marker.
//!
//! Run with: `cargo run -q -p tpt-protocol-nmea --example diff_decode < input.txt`

use std::io::{self, BufRead, Write};

use tpt_protocol_core::{Error, FrameParser};
use tpt_protocol_nmea::{NmeaNormalizer, NmeaParser};
use tpt_protocol_tup::TupValue;

fn main() -> io::Result<()> {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let mut index = 0usize;
    for line in io::stdin().lock().lines() {
        let line = line?;
        let sentence = line.trim();
        if sentence.is_empty() {
            continue;
        }
        writeln!(out, "#{index}")?;
        index += 1;
        match points(sentence) {
            Ok(lines) => {
                for text in lines {
                    writeln!(out, "{text}")?;
                }
            }
            Err(_) => writeln!(out, "error")?,
        }
    }
    Ok(())
}

fn points(sentence: &str) -> Result<Vec<String>, Error> {
    let frame = NmeaParser::new().parse(sentence.as_bytes())?;
    let points = NmeaNormalizer::new().points(&frame);
    if points.is_empty() {
        return Err(Error::Parse("no decodable field".into()));
    }
    Ok(points
        .iter()
        .map(|dp| {
            let value = match &dp.value {
                TupValue::Float(v) => format!("{v:.6}"),
                TupValue::Int(v) => v.to_string(),
                TupValue::Bool(v) => v.to_string(),
                TupValue::Text(v) => format!("\"{v}\""),
                TupValue::Bytes(_) => "bytes".to_string(),
            };
            format!("{} {value}", dp.point_id)
        })
        .collect())
}
