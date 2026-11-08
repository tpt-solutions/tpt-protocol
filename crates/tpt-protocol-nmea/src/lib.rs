//! NMEA 0183 protocol implementation for marine navigation data.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// NMEA 0183 talker identifier.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Talker {
    /// GPS.
    Gp,
    /// Multi-constellation GNSS.
    Gn,
    /// GLONASS.
    Gl,
    /// Galileo.
    Ga,
    /// BeiDou.
    Bd,
    Sd,
    Gi,
    He,
    In,
    Unknown([u8; 2]),
}

impl Talker {
    fn from_bytes(b: &[u8]) -> Self {
        if b.is_empty() {
            return Self::Unknown([0, 0]);
        }
        if b.len() < 2 {
            return Self::Unknown([b[0], 0]);
        }
        match (b[0], b[1]) {
            (b'G', b'P') => Self::Gp,
            (b'G', b'N') => Self::Gn,
            (b'G', b'L') => Self::Gl,
            (b'G', b'A') => Self::Ga,
            (b'B', b'D') => Self::Bd,
            (b'S', b'D') => Self::Sd,
            (b'G', b'I') => Self::Gi,
            (b'H', b'E') => Self::He,
            (b'I', b'N') => Self::In,
            (a, b) => Self::Unknown([a, b]),
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Gp => "GP",
            Self::Gn => "GN",
            Self::Gl => "GL",
            Self::Ga => "GA",
            Self::Bd => "BD",
            Self::Sd => "SD",
            Self::Gi => "GI",
            Self::He => "HE",
            Self::In => "IN",
            Self::Unknown(_) => "??",
        }
    }
}

impl fmt::Display for Talker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// NMEA 0183 sentence type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SentenceType {
    Gga,
    Rmc,
    Dbt,
    Vhw,
    Mwv,
    Unknown([u8; 3]),
}

impl SentenceType {
    fn from_bytes(b: &[u8]) -> Self {
        if b.is_empty() {
            return Self::Unknown([0, 0, 0]);
        }
        if b.len() < 3 {
            return Self::Unknown([b[0], b.get(1).copied().unwrap_or(0), 0]);
        }
        match (b[0], b[1], b[2]) {
            (b'G', b'G', b'A') => Self::Gga,
            (b'R', b'M', b'C') => Self::Rmc,
            (b'D', b'B', b'T') => Self::Dbt,
            (b'V', b'H', b'W') => Self::Vhw,
            (b'M', b'W', b'V') => Self::Mwv,
            (a, b, c) => Self::Unknown([a, b, c]),
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Gga => "GGA",
            Self::Rmc => "RMC",
            Self::Dbt => "DBT",
            Self::Vhw => "VHW",
            Self::Mwv => "MWV",
            Self::Unknown(_) => "???",
        }
    }
}

impl fmt::Display for SentenceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A parsed NMEA 0183 sentence.
#[derive(Clone, Debug, PartialEq)]
pub struct NmeaFrame {
    pub talker: Talker,
    pub sentence: SentenceType,
    pub fields: Vec<String>,
    pub checksum: u8,
    pub checksum_valid: bool,
    pub raw: String,
}

impl fmt::Display for NmeaFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NMEA[{}{}] {} fields, csum=0x{:02X} {}",
            self.talker,
            self.sentence,
            self.fields.len(),
            self.checksum,
            if self.checksum_valid { "OK" } else { "BAD" },
        )
    }
}

/// NMEA frame parser.
#[derive(Clone, Debug, Default)]
pub struct NmeaParser;

impl NmeaParser {
    pub fn new() -> Self {
        Self
    }
}

impl FrameParser for NmeaParser {
    type Frame = NmeaFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 8 {
            return Err(Error::Parse(format!(
                "NMEA frame too short: {} bytes",
                bytes.len(),
            )));
        }

        let s = core::str::from_utf8(bytes)
            .map_err(|_| Error::Parse("NMEA: not valid UTF-8".into()))?;
        // Trim first, so the checksum range starts at the '$' we just found.
        let s = s.trim();

        if !s.starts_with('$') {
            return Err(Error::Parse("NMEA: missing '$' prefix".into()));
        }

        // Find checksum separator
        let csum_star = s
            .rfind('*')
            .ok_or_else(|| Error::Parse("NMEA: missing '*' checksum separator".into()))?;
        let body = &s[1..csum_star];
        let csum_str = &s[csum_star + 1..];

        let expected_csum = u8::from_str_radix(csum_str, 16)
            .map_err(|_| Error::Parse("NMEA: invalid checksum hex".into()))?;

        let computed_csum = compute_nmea_checksum(&s.as_bytes()[1..csum_star]);
        if computed_csum != expected_csum {
            return Err(Error::Parse(format!(
                "NMEA checksum mismatch: computed 0x{computed_csum:02X}, got 0x{expected_csum:02X}"
            )));
        }
        // Mismatches are rejected above, so every parsed frame has a valid checksum.
        let checksum_valid = true;

        // Split body into talker+sentence and fields
        let talker_part = body.get(..2).unwrap_or("");
        let sentence_part = body.get(2..5).unwrap_or("");
        let fields_str = body.get(6..).unwrap_or(""); // skip sentence + leading comma

        let talker = Talker::from_bytes(talker_part.as_bytes());
        let sentence = SentenceType::from_bytes(sentence_part.as_bytes());

        let fields: Vec<String> = if fields_str.is_empty() {
            Vec::new()
        } else {
            fields_str.split(',').map(|f| f.into()).collect()
        };

        Ok(NmeaFrame {
            talker,
            sentence,
            fields,
            checksum: expected_csum,
            checksum_valid,
            raw: s.into(),
        })
    }
}

/// NMEA connection state.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct NmeaState {
    pub connected: bool,
    pub signal_quality: u8,
    pub last_fix_timestamp: u64,
    pub error_count: u64,
}

/// Validates NMEA connection health.
#[derive(Clone, Debug, Default)]
pub struct NmeaValidator;

impl NmeaValidator {
    pub fn new() -> Self {
        Self
    }
}

impl StateValidator for NmeaValidator {
    type State = NmeaState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("NMEA device not connected".into()));
        }
        if state.signal_quality > 5 {
            return Err(Error::State(format!(
                "NMEA invalid signal quality: {}",
                state.signal_quality,
            )));
        }
        if state.error_count > 20 {
            return Err(Error::State(format!(
                "NMEA device has {} consecutive errors",
                state.error_count,
            )));
        }
        Ok(())
    }
}

/// XOR checksum over the given bytes (the NMEA body between `$` and `*`).
fn compute_nmea_checksum(body: &[u8]) -> u8 {
    body.iter().fold(0u8, |acc, &b| acc ^ b)
}

#[derive(Clone, Debug, Default)]
pub struct NmeaNormalizer;

impl NmeaNormalizer {
    pub fn new() -> Self {
        Self
    }
}

impl DataNormalizer for NmeaNormalizer {
    type Frame = NmeaFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = format!("nmea:{}", frame.talker);
        let point_id = format!("{}", frame.sentence);
        let timestamp = 0;

        let value = if let Some(first) = frame.fields.first() {
            if let Ok(v) = first.parse::<f64>() {
                TupValue::Float(v)
            } else {
                TupValue::Text(first.clone())
            }
        } else {
            TupValue::Int(0)
        };

        let mut dp = DataPoint::new(
            SourceProtocol::Nmea,
            device_id,
            point_id,
            timestamp,
            value,
            if frame.checksum_valid {
                Quality::Good
            } else {
                Quality::Bad
            },
        );
        dp.raw = Some(frame.raw.as_bytes().to_vec());
        Ok(dp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_nmea(talker: &str, sentence: &str, fields: &[&str]) -> Vec<u8> {
        let mut s = alloc::string::String::from("$");
        s.push_str(talker);
        s.push_str(sentence);
        for f in fields {
            s.push(',');
            s.push_str(f);
        }
        let csum = compute_nmea_checksum(&s.as_bytes()[1..]);
        s.push('*');
        s.push_str(&alloc::format!("{:02X}", csum));
        s.push_str("\r\n");
        s.into_bytes()
    }

    #[test]
    fn valid_parse_gga() {
        let bytes = make_nmea(
            "GP",
            "GGA",
            &[
                "123519",
                "4807.038,N",
                "01131.000,E",
                "1",
                "08",
                "0.9",
                "545.4,M",
                "46.9,M",
                "",
                "0000",
            ],
        );
        let parser = NmeaParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.talker, Talker::Gp);
        assert_eq!(frame.sentence, SentenceType::Gga);
        assert!(frame.checksum_valid);
        assert_eq!(frame.fields[0], "123519");
    }

    #[test]
    fn valid_parse_rmc() {
        let bytes = make_nmea(
            "GP",
            "RMC",
            &[
                "225446.00",
                "A",
                "4916.45,N",
                "12311.12,W",
                "5.7",
                "280.5",
                "151222",
                "25.8,E",
            ],
        );
        let parser = NmeaParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.talker, Talker::Gp);
        assert_eq!(frame.sentence, SentenceType::Rmc);
        assert!(frame.checksum_valid);
    }

    #[test]
    fn valid_parse_dbt() {
        let bytes = make_nmea("SD", "DBT", &["12.3", "f", "3.75", "M", "2.05", "F"]);
        let parser = NmeaParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.talker, Talker::Sd);
        assert_eq!(frame.sentence, SentenceType::Dbt);
    }

    #[test]
    fn invalid_no_prefix() {
        let parser = NmeaParser::new();
        let result = parser.parse(b"GPGGA,1,2,3*5C\r\n");
        assert!(result.is_err());
    }

    #[test]
    fn truncated_input() {
        let parser = NmeaParser::new();
        let result = parser.parse(b"$GP");
        assert!(result.is_err());
    }

    #[test]
    fn garbage_input() {
        let parser = NmeaParser::new();
        let result = parser.parse(b"$XXXXXXX*ZZ\r\n");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_checksum() {
        let bytes = make_nmea("GP", "GGA", &["1", "2"]);
        let mut modified = bytes.clone();
        // Corrupt the checksum
        let len = modified.len();
        if len > 4 {
            modified[len - 4] = b'0';
            modified[len - 3] = b'0';
        }
        // A checksum that does not match is an error, not a Bad-quality frame.
        assert!(NmeaParser::new().parse(&modified).is_err());
    }

    #[test]
    fn checksum_covers_the_sentence_after_leading_whitespace() {
        // Leading whitespace must not shift the checksum range off the '$'.
        let bytes = make_nmea("GP", "GGA", &["1", "2"]);
        let mut padded = alloc::vec![b' ', b' '];
        padded.extend_from_slice(&bytes);
        let frame = NmeaParser::new().parse(&padded).unwrap();
        assert!(frame.checksum_valid);
        assert_eq!(frame.talker, Talker::Gp);
    }

    #[test]
    fn multi_constellation_talkers_are_recognized() {
        for (code, expected) in [
            ("GN", Talker::Gn),
            ("GL", Talker::Gl),
            ("GA", Talker::Ga),
            ("BD", Talker::Bd),
        ] {
            let frame = NmeaParser::new()
                .parse(&make_nmea(code, "GGA", &["1"]))
                .unwrap();
            assert_eq!(frame.talker, expected, "{code}");
        }
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = NmeaParser::new();
        let normalizer = NmeaNormalizer::new();
        let bytes = make_nmea("GP", "GGA", &["123519", "4807.038,N"]);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Nmea);
        assert_eq!(dp.device_id, "nmea:GP");
        assert_eq!(dp.point_id, "GGA");
    }

    #[test]
    fn validator_ok() {
        let validator = NmeaValidator::new();
        let state = NmeaState {
            connected: true,
            signal_quality: 3,
            last_fix_timestamp: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        let validator = NmeaValidator::new();
        let state = NmeaState::default();
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn validator_bad_signal_quality() {
        let validator = NmeaValidator::new();
        let state = NmeaState {
            connected: true,
            signal_quality: 10,
            last_fix_timestamp: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_err());
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let parser = NmeaParser::new();
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }
}
