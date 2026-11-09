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
    Gll,
    Vtg,
    Gsa,
    Gsv,
    Hdt,
    Zda,
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
            (b'G', b'L', b'L') => Self::Gll,
            (b'V', b'T', b'G') => Self::Vtg,
            (b'G', b'S', b'A') => Self::Gsa,
            (b'G', b'S', b'V') => Self::Gsv,
            (b'H', b'D', b'T') => Self::Hdt,
            (b'Z', b'D', b'A') => Self::Zda,
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
            Self::Gll => "GLL",
            Self::Vtg => "VTG",
            Self::Gsa => "GSA",
            Self::Gsv => "GSV",
            Self::Hdt => "HDT",
            Self::Zda => "ZDA",
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
            return Err(Error::checksum(
                computed_csum as u32,
                expected_csum as u32,
                "NMEA checksum",
            ));
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
/// their real fields: GGA, RMC, GLL, DBT, VHW, VTG, HDT, MWV, GSA, GSV and ZDA.
/// Other sentences keep their first field.
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
            SentenceType::Gll => {
                // Fields: lat, N/S, lon, E/W, UTC time, status (A = valid, V = invalid).
                let q = if f(5) == Some("A") {
                    Quality::Good
                } else {
                    Quality::Bad
                };
                if let (Some(lat), Some(lon)) = (
                    f(0).zip(f(1)).and_then(|(v, h)| coordinate(v, h, 2)),
                    f(2).zip(f(3)).and_then(|(v, h)| coordinate(v, h, 3)),
                ) {
                    push("latitude", TupValue::Float(lat), q);
                    push("longitude", TupValue::Float(lon), q);
                }
            }
            SentenceType::Vtg => {
                // Fields: true course, T, magnetic course, M, knots, N, km/h, K.
                if let Some(v) = num(0) {
                    push("course_true_deg", TupValue::Float(v), Quality::Good);
                }
                if let Some(v) = num(4) {
                    push("speed_knots", TupValue::Float(v), Quality::Good);
                }
                if let Some(v) = num(6) {
                    push("speed_kmh", TupValue::Float(v), Quality::Good);
                }
            }
            SentenceType::Gsa => {
                // Fields: mode, fix type (1 = no fix, 2 = 2D, 3 = 3D), 12 satellite ids,
                // PDOP, HDOP, VDOP. A sentence without a fix reports its DOPs as Bad.
                if let Some(fix) = f(1).and_then(|s| s.parse::<i64>().ok()) {
                    push("fix_type", TupValue::Int(fix), Quality::Good);
                    let q = if fix >= 2 {
                        Quality::Good
                    } else {
                        Quality::Bad
                    };
                    if let Some(v) = num(14) {
                        push("pdop", TupValue::Float(v), q);
                    }
                    if let Some(v) = num(15) {
                        push("hdop", TupValue::Float(v), q);
                    }
                    if let Some(v) = num(16) {
                        push("vdop", TupValue::Float(v), q);
                    }
                }
            }
            SentenceType::Gsv => {
                // Fields: total messages, this message's number, satellites in view.
                // The per-satellite groups are not decoded.
                if let Some(n) = f(2).and_then(|s| s.parse::<i64>().ok()) {
                    push("satellites_in_view", TupValue::Int(n), Quality::Good);
                }
            }
            SentenceType::Hdt => {
                if let Some(v) = num(0) {
                    push("heading_true_deg", TupValue::Float(v), Quality::Good);
                }
            }
            SentenceType::Zda => {
                // Fields: hhmmss.ss, day, month, year. The time becomes seconds of the day.
                if let Some(t) = f(0) {
                    let seconds = t
                        .get(..2)
                        .and_then(|h| h.parse::<f64>().ok())
                        .zip(t.get(2..4).and_then(|m| m.parse::<f64>().ok()))
                        .zip(t.get(4..).and_then(|s| s.parse::<f64>().ok()));
                    if let Some(((h, m), s)) = seconds {
                        push(
                            "utc_seconds_of_day",
                            TupValue::Float(h * 3600.0 + m * 60.0 + s),
                            Quality::Good,
                        );
                    }
                }
                for (i, name) in [(1, "day"), (2, "month"), (3, "year")] {
                    if let Some(n) = f(i).and_then(|s| s.parse::<i64>().ok()) {
                        push(name, TupValue::Int(n), Quality::Good);
                    }
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

    /// Every decoded field of the sentence, or the sentence point when none decode.
    fn normalize_all(&self, frame: &Self::Frame) -> Result<Vec<DataPoint>, Error> {
        let points = self.points(frame);
        if points.is_empty() {
            return Ok(alloc::vec![self.normalize(frame)?]);
        }
        Ok(points)
    }

    /// The first point of the sentence. A sentence with no decodable field is an
    /// error: a point with value 0 would read as a measurement.
    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        self.points(frame).into_iter().next().ok_or_else(|| {
            Error::Normalize(format!(
                "nmea {}{} has no decodable field",
                frame.talker,
                sentence_label(&frame.sentence)
            ))
        })
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

    /// Decodes one sentence built from `fields` into its named points.
    fn decoded(sentence: &str, fields: &[&str]) -> Vec<(String, TupValue, Quality)> {
        let frame = NmeaParser::new()
            .parse(&make_nmea("GP", sentence, fields))
            .unwrap();
        NmeaNormalizer::new()
            .points(&frame)
            .into_iter()
            .map(|dp| (dp.point_id, dp.value, dp.quality))
            .collect()
    }

    fn value_of(points: &[(String, TupValue, Quality)], name: &str) -> TupValue {
        points
            .iter()
            .find(|(id, _, _)| id.rsplit('.').next() == Some(name))
            .unwrap_or_else(|| panic!("no point {name} in {points:?}"))
            .1
            .clone()
    }

    #[test]
    fn gll_position_and_validity() {
        let points = decoded(
            "GLL",
            &["4807.038", "N", "01131.000", "E", "123519", "A", "A"],
        );
        let lat = match value_of(&points, "latitude") {
            TupValue::Float(v) => v,
            other => panic!("latitude is {other:?}"),
        };
        assert!((lat - 48.1173).abs() < 1e-4, "lat {lat}");
        assert!(points.iter().all(|(_, _, q)| *q == Quality::Good));

        let void = decoded(
            "GLL",
            &["4807.038", "N", "01131.000", "E", "123519", "V", "N"],
        );
        assert!(void.iter().all(|(_, _, q)| *q == Quality::Bad));
    }

    #[test]
    fn vtg_speed_and_course() {
        let points = decoded(
            "VTG",
            &["54.7", "T", "34.4", "M", "5.5", "N", "10.2", "K", "A"],
        );
        assert_eq!(value_of(&points, "course_true_deg"), TupValue::Float(54.7));
        assert_eq!(value_of(&points, "speed_knots"), TupValue::Float(5.5));
        assert_eq!(value_of(&points, "speed_kmh"), TupValue::Float(10.2));
    }

    #[test]
    fn gsa_fix_type_and_dops() {
        // Mode A, 3D fix, twelve satellite ids (two filled), then PDOP, HDOP, VDOP.
        let fields = [
            "A", "3", "04", "05", "", "", "", "", "", "", "", "", "", "", "2.5", "1.3", "2.1",
        ];
        let points = decoded("GSA", &fields);
        assert_eq!(value_of(&points, "fix_type"), TupValue::Int(3));
        assert_eq!(value_of(&points, "pdop"), TupValue::Float(2.5));
        assert_eq!(value_of(&points, "hdop"), TupValue::Float(1.3));
        assert_eq!(value_of(&points, "vdop"), TupValue::Float(2.1));

        // With no fix the DOPs are reported as Bad, not trusted.
        let mut no_fix = fields;
        no_fix[1] = "1";
        let points = decoded("GSA", &no_fix);
        assert!(points
            .iter()
            .filter(|(id, _, _)| !id.ends_with("fix_type"))
            .all(|(_, _, q)| *q == Quality::Bad));
    }

    #[test]
    fn gsv_counts_satellites_in_view() {
        let points = decoded(
            "GSV",
            &["3", "1", "11", "03", "03", "111", "00", "04", "15", "15"],
        );
        assert_eq!(value_of(&points, "satellites_in_view"), TupValue::Int(11));
    }

    #[test]
    fn hdt_heading() {
        let points = decoded("HDT", &["274.07", "T"]);
        assert_eq!(
            value_of(&points, "heading_true_deg"),
            TupValue::Float(274.07)
        );
    }

    #[test]
    fn zda_time_of_day_and_date() {
        let points = decoded("ZDA", &["102325.50", "09", "12", "2023", "00", "00"]);
        // 10:23:25.5 is 37405.5 seconds into the day.
        assert_eq!(
            value_of(&points, "utc_seconds_of_day"),
            TupValue::Float(37405.5)
        );
        assert_eq!(value_of(&points, "day"), TupValue::Int(9));
        assert_eq!(value_of(&points, "month"), TupValue::Int(12));
        assert_eq!(value_of(&points, "year"), TupValue::Int(2023));
    }

    #[test]
    fn new_sentences_with_no_decodable_field_are_errors() {
        let frame = NmeaParser::new()
            .parse(&make_nmea("GP", "HDT", &["", "T"]))
            .unwrap();
        assert!(NmeaNormalizer::new().normalize(&frame).is_err());
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
        // ZPA is not a sentence this crate decodes, so it keeps the first field.
        let bytes = make_nmea("GP", "ZPA", &["123519", "01", "01", "2024"]);
        let frame = NmeaParser::new().parse(&bytes).unwrap();
        let dp = NmeaNormalizer::new().normalize(&frame).unwrap();
        assert_eq!(dp.point_id, "ZPA.field0");
    }

    #[test]
    fn normalize_batch_fans_out_gga_fields() {
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
        // latitude, longitude, fix quality, satellites and altitude.
        assert_eq!(
            NmeaNormalizer::new()
                .normalize_batch(&[frame])
                .unwrap()
                .len(),
            5
        );
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = NmeaParser::new();
        let normalizer = NmeaNormalizer::new();
        // A malformed position (an extra comma) decodes no field, so there is no
        // measurement to report. That is an error, not a zero.
        let bytes = make_nmea("GP", "GGA", &["123519", "4807.038,N"]);
        let frame = parser.parse(&bytes).unwrap();
        assert!(normalizer.normalize(&frame).is_err());
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
