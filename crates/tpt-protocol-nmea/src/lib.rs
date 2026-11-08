//! NMEA 0183 protocol implementation for marine navigation data.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
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

/// The sentence name for point ids. Unknown sentences keep their own letters
/// (the `Display` form is `???`).
fn sentence_label(sentence: &SentenceType) -> String {
    match sentence {
        SentenceType::Unknown(bytes) => {
            let label: String = bytes
                .iter()
                .filter(|&&b| b != 0)
                .map(|&b| if b.is_ascii_graphic() { b as char } else { '?' })
                .collect();
            if label.is_empty() {
                "???".to_string()
            } else {
                label
            }
        }
        known => known.to_string(),
    }
}

/// Parses an NMEA coordinate (`ddmm.mmmm` or `dddmm.mmmm`) with its hemisphere
/// into signed decimal degrees.
fn coordinate(value: &str, hemisphere: &str, degree_digits: usize) -> Option<f64> {
    if value.len() <= degree_digits {
        return None;
    }
    // `get` returns None instead of panicking when the split is not on a character boundary.
    let degrees: f64 = value.get(..degree_digits)?.parse().ok()?;
    let minutes: f64 = value.get(degree_digits..)?.parse().ok()?;
    let v = degrees + minutes / 60.0;
    match hemisphere {
        "N" | "E" => Some(v),
        "S" | "W" => Some(-v),
        _ => None,
    }
}

fn point(
    device_id: &str,
    sentence: &str,
    name: &str,
    value: TupValue,
    quality: Quality,
    raw: &[u8],
) -> DataPoint {
    let mut dp = DataPoint::new(
        SourceProtocol::Nmea,
        device_id.to_string(),
        format!("{sentence}.{name}"),
        0,
        value,
        quality,
    );
    dp.raw = Some(raw.to_vec());
    dp
}

/// Normalizes NMEA sentences into TUP DataPoints. Common sentences map to
/// their real fields: GGA, RMC, DBT, VHW and MWV. Other sentences keep their
/// first field.
#[derive(Clone, Debug, Default)]
pub struct NmeaNormalizer;

impl NmeaNormalizer {
    pub fn new() -> Self {
        Self
    }

    /// One data point per decoded field. Missing or empty fields are skipped.
    pub fn points(&self, frame: &NmeaFrame) -> Vec<DataPoint> {
        let device_id = format!("nmea:{}", frame.talker);
        let sentence = sentence_label(&frame.sentence);
        let raw = frame.raw.as_bytes();
        let f = |i: usize| -> Option<&str> {
            frame
                .fields
                .get(i)
                .map(|s| s.as_str())
                .filter(|s| !s.is_empty())
        };
        let num = |i: usize| f(i).and_then(|s| s.parse::<f64>().ok());
        let mut out = Vec::new();
        let mut push = |name: &str, value: TupValue, q: Quality| {
            out.push(point(&device_id, &sentence, name, value, q, raw));
        };

        match frame.sentence {
            SentenceType::Gga => {
                let good = Quality::Good;
                if let (Some(lat), Some(lon)) = (
                    f(1).zip(f(2)).and_then(|(v, h)| coordinate(v, h, 2)),
                    f(3).zip(f(4)).and_then(|(v, h)| coordinate(v, h, 3)),
                ) {
                    push("latitude", TupValue::Float(lat), good);
                    push("longitude", TupValue::Float(lon), good);
                }
                if let Some(q) = f(5).and_then(|s| s.parse::<i64>().ok()) {
                    push("fix_quality", TupValue::Int(q), good);
                }
                if let Some(n) = f(6).and_then(|s| s.parse::<i64>().ok()) {
                    push("satellites", TupValue::Int(n), good);
                }
                if let Some(alt) = num(8) {
                    push("altitude_m", TupValue::Float(alt), good);
                }
            }
            SentenceType::Rmc => {
                // Status A = valid fix. Anything else is reported, but as Bad quality.
                let q = if f(1) == Some("A") {
                    Quality::Good
                } else {
                    Quality::Bad
                };
                if let (Some(lat), Some(lon)) = (
                    f(2).zip(f(3)).and_then(|(v, h)| coordinate(v, h, 2)),
                    f(4).zip(f(5)).and_then(|(v, h)| coordinate(v, h, 3)),
                ) {
                    push("latitude", TupValue::Float(lat), q);
                    push("longitude", TupValue::Float(lon), q);
                }
                if let Some(v) = num(6) {
                    push("speed_knots", TupValue::Float(v), q);
                }
                if let Some(v) = num(7) {
                    push("course_deg", TupValue::Float(v), q);
                }
            }
            SentenceType::Dbt => {
                if let Some(v) = num(2) {
                    push("depth_m", TupValue::Float(v), Quality::Good);
                }
            }
            SentenceType::Vhw => {
                if let Some(v) = num(0) {
                    push("heading_true_deg", TupValue::Float(v), Quality::Good);
                }
                if let Some(v) = num(4) {
                    push("speed_knots", TupValue::Float(v), Quality::Good);
                }
            }
            SentenceType::Mwv => {
                if let Some(v) = num(0) {
                    push("wind_angle_deg", TupValue::Float(v), Quality::Good);
                }
                if let (Some(v), Some(unit)) = (num(2), f(3)) {
                    push(
                        &format!("wind_speed_{unit}"),
                        TupValue::Float(v),
                        Quality::Good,
                    );
                }
            }
            _ => {
                if let Some(first) = frame.fields.first() {
                    let value = match first.parse::<f64>() {
                        Ok(v) => TupValue::Float(v),
                        Err(_) => TupValue::Text(first.clone()),
                    };
                    push("field0", value, Quality::Good);
                }
            }
        }
        out
    }
}

impl DataNormalizer for NmeaNormalizer {
    type Frame = NmeaFrame;

    /// The first point of the sentence. A sentence with no decodable field
    /// yields a point for the sentence itself, with value 0.
    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        if let Some(first) = self.points(frame).into_iter().next() {
            return Ok(first);
        }
        Ok(point(
            &format!("nmea:{}", frame.talker),
            &sentence_label(&frame.sentence),
            "empty",
            TupValue::Int(0),
            Quality::Good,
            frame.raw.as_bytes(),
        ))
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
    fn gga_yields_position_quality_satellites_and_altitude() {
        // Standard example: 48 deg 07.038 min N, 11 deg 31.000 min E.
        let bytes = make_nmea(
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
        let frame = NmeaParser::new().parse(&bytes).unwrap();
        let points = NmeaNormalizer::new().points(&frame);
        let get = |name: &str| {
            points
                .iter()
                .find(|p| p.point_id == format!("GGA.{name}"))
                .unwrap()
                .value
                .clone()
        };
        let TupValue::Float(lat) = get("latitude") else {
            panic!()
        };
        let TupValue::Float(lon) = get("longitude") else {
            panic!()
        };
        assert!((lat - 48.1173).abs() < 0.0001, "{lat}");
        assert!((lon - 11.516666).abs() < 0.0001, "{lon}");
        assert_eq!(get("fix_quality"), TupValue::Int(1));
        assert_eq!(get("satellites"), TupValue::Int(8));
        assert_eq!(get("altitude_m"), TupValue::Float(545.4));
    }

    #[test]
    fn non_ascii_coordinate_does_not_panic() {
        // 'é' is two bytes, so byte 2 falls inside it: the split must not panic.
        assert_eq!(coordinate("1é2.5", "N", 2), None);
        assert_eq!(coordinate("12é5", "N", 2), None);
    }

    #[test]
    fn southern_and_western_coordinates_are_negative() {
        let bytes = make_nmea(
            "GP",
            "GGA",
            &[
                "123519",
                "3349.000",
                "S",
                "15112.000",
                "W",
                "1",
                "08",
                "0.9",
                "10.0",
                "M",
                "",
                "",
                "",
                "",
            ],
        );
        let frame = NmeaParser::new().parse(&bytes).unwrap();
        let points = NmeaNormalizer::new().points(&frame);
        let lat = points
            .iter()
            .find(|p| p.point_id == "GGA.latitude")
            .unwrap();
        let lon = points
            .iter()
            .find(|p| p.point_id == "GGA.longitude")
            .unwrap();
        assert!(matches!(lat.value, TupValue::Float(v) if v < 0.0));
        assert!(matches!(lon.value, TupValue::Float(v) if v < 0.0));
    }

    #[test]
    fn rmc_without_a_valid_fix_is_bad_quality() {
        let bytes = make_nmea(
            "GP",
            "RMC",
            &[
                "123519",
                "V",
                "4807.038",
                "N",
                "01131.000",
                "E",
                "0.5",
                "84.4",
                "230394",
                "",
                "",
                "N",
            ],
        );
        let frame = NmeaParser::new().parse(&bytes).unwrap();
        let points = NmeaNormalizer::new().points(&frame);
        assert!(points.iter().all(|p| p.quality == Quality::Bad));
        assert!(points.iter().any(|p| p.point_id == "RMC.speed_knots"));
    }

    #[test]
    fn rmc_with_a_valid_fix_is_good_quality() {
        let bytes = make_nmea(
            "GP",
            "RMC",
            &[
                "123519",
                "A",
                "4807.038",
                "N",
                "01131.000",
                "E",
                "22.4",
                "84.4",
                "230394",
                "",
                "",
                "A",
            ],
        );
        let frame = NmeaParser::new().parse(&bytes).unwrap();
        let points = NmeaNormalizer::new().points(&frame);
        assert!(points.iter().all(|p| p.quality == Quality::Good));
        assert!(points.iter().any(|p| p.point_id == "RMC.course_deg"));
    }

    #[test]
    fn dbt_depth_is_in_metres() {
        let bytes = make_nmea("SD", "DBT", &["12.4", "f", "3.8", "M", "2.1", "F"]);
        let frame = NmeaParser::new().parse(&bytes).unwrap();
        let dp = NmeaNormalizer::new().normalize(&frame).unwrap();
        assert_eq!(dp.point_id, "DBT.depth_m");
        assert_eq!(dp.value, TupValue::Float(3.8));
    }

    #[test]
    fn mwv_wind_speed_keeps_its_unit() {
        let bytes = make_nmea("WI", "MWV", &["045", "R", "12.5", "N", "A"]);
        let frame = NmeaParser::new().parse(&bytes).unwrap();
        let points = NmeaNormalizer::new().points(&frame);
        assert!(points
            .iter()
            .any(|p| p.point_id == "MWV.wind_speed_N" && p.value == TupValue::Float(12.5)));
    }

    #[test]
    fn unknown_sentence_keeps_the_first_field() {
        let bytes = make_nmea("GP", "ZDA", &["123519", "01", "01", "2024"]);
        let frame = NmeaParser::new().parse(&bytes).unwrap();
        let dp = NmeaNormalizer::new().normalize(&frame).unwrap();
        assert_eq!(dp.point_id, "ZDA.field0");
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = NmeaParser::new();
        let normalizer = NmeaNormalizer::new();
        // A malformed position (an extra comma) decodes no field, so the
        // normalizer falls back to a point for the sentence itself.
        let bytes = make_nmea("GP", "GGA", &["123519", "4807.038,N"]);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Nmea);
        assert_eq!(dp.device_id, "nmea:GP");
        assert_eq!(dp.point_id, "GGA.empty");
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
