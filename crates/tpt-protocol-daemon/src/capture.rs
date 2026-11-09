//! Reads CAN captures in the `candump -L` text format, one frame per line:
//!
//! ```text
//! (1700000000.123456) can0 0CF00412#0102000000000000
//! (1700000000.223456) can0 181#01020000
//! ```
//!
//! Identifiers of up to three hex digits are standard, longer ones are
//! extended. Remote frames (`#R`) and CAN FD frames (`##`) are skipped, since
//! none of the CAN protocols here carries them.

/// One CAN frame from a capture.
#[derive(Debug, Clone, PartialEq)]
pub struct CanRecord {
    /// Capture time in milliseconds since the Unix epoch.
    pub timestamp_ms: u64,
    pub interface: String,
    pub id: u32,
    pub data: Vec<u8>,
}

/// Parses a `candump -L` file. Blank lines and unsupported frames are skipped;
/// a malformed line is an error that names its line number.
pub fn parse_candump(text: &str) -> Result<Vec<CanRecord>, String> {
    let mut records = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match parse_line(line) {
            Ok(Some(record)) => records.push(record),
            Ok(None) => {}
            Err(e) => return Err(format!("line {}: {e}", index + 1)),
        }
    }
    Ok(records)
}

fn parse_line(line: &str) -> Result<Option<CanRecord>, String> {
    let rest = line
        .strip_prefix('(')
        .ok_or("expected a '(timestamp)' prefix")?;
    let (stamp, rest) = rest
        .split_once(')')
        .ok_or("missing ')' after the timestamp")?;
    let seconds: f64 = stamp
        .trim()
        .parse()
        .map_err(|_| format!("bad timestamp '{stamp}'"))?;
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(format!("timestamp out of range: {stamp}"));
    }

    let mut parts = rest.split_whitespace();
    let interface = parts.next().ok_or("missing interface name")?.to_string();
    let frame = parts.next().ok_or("missing frame")?;
    if parts.next().is_some() {
        return Err("unexpected text after the frame".into());
    }

    if frame.contains("##") {
        return Ok(None); // CAN FD: not carried by the protocols here
    }
    let (id_text, data_text) = frame
        .split_once('#')
        .ok_or_else(|| format!("frame '{frame}' has no '#' separator"))?;
    if data_text == "R" || data_text.starts_with('R') {
        return Ok(None); // remote frame: no payload
    }

    let id = u32::from_str_radix(id_text, 16).map_err(|_| format!("bad identifier '{id_text}'"))?;
    let limit = if id_text.len() <= 3 {
        0x7FF
    } else {
        0x1FFF_FFFF
    };
    if id > limit {
        return Err(format!("identifier {id_text} is out of range"));
    }
    let data = parse_hex_pairs(data_text)?;
    if data.len() > 8 {
        return Err(format!(
            "{} data bytes; classic CAN carries at most 8",
            data.len()
        ));
    }
    Ok(Some(CanRecord {
        timestamp_ms: (seconds * 1000.0).round() as u64,
        interface,
        id,
        data,
    }))
}

fn parse_hex_pairs(text: &str) -> Result<Vec<u8>, String> {
    if text.len() % 2 != 0 {
        return Err(format!("odd number of data digits in '{text}'"));
    }
    (0..text.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&text[i..i + 2], 16)
                .map_err(|_| format!("bad data byte in '{text}'"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_and_extended_frames() {
        let text =
            "(1700000000.123456) can0 181#0102\n\n(1700000000.5) vcan0 0CF00412#01020000000000FF\n";
        let records = parse_candump(text).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].id, 0x181);
        assert_eq!(records[0].data, vec![0x01, 0x02]);
        assert_eq!(records[0].timestamp_ms, 1_700_000_000_123);
        assert_eq!(records[1].id, 0x0CF0_0412);
        assert_eq!(records[1].interface, "vcan0");
        assert_eq!(records[1].data.len(), 8);
    }

    #[test]
    fn remote_and_fd_frames_are_skipped() {
        let text = "(1.0) can0 181#R\n(2.0) can0 181##1ABC\n(3.0) can0 182#00\n";
        let records = parse_candump(text).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, 0x182);
    }

    #[test]
    fn malformed_lines_name_their_line() {
        let err = parse_candump("(1.0) can0 181#01\n(oops) can0 181#01\n").unwrap_err();
        assert!(err.starts_with("line 2:"), "{err}");
        assert!(
            parse_candump("(1.0) can0 181#0").is_err(),
            "odd digit count"
        );
        assert!(
            parse_candump("(1.0) can0 181#010203040506070809").is_err(),
            "more than 8 bytes"
        );
        assert!(
            parse_candump("(1.0) can0 800#00").is_err(),
            "standard id above 0x7FF"
        );
    }
}
