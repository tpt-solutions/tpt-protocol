//! HL7 v2 protocol implementation for healthcare data exchange.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

#[derive(Debug, Clone, PartialEq)]
pub enum Hl7MessageType {
    Adt,
    Orm,
    Oru,
    Mdm,
    Other(String),
}

impl Hl7MessageType {
    fn from_str(s: &str) -> Self {
        match s {
            "ADT" => Self::Adt,
            "ORM" => Self::Orm,
            "ORU" => Self::Oru,
            "MDM" => Self::Mdm,
            other => Self::Other(other.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MshSegment {
    pub field_separator: String,
    pub encoding_chars: String,
    pub sending_app: String,
    pub sending_facility: String,
    pub receiving_app: String,
    pub receiving_facility: String,
    pub date_time: String,
    pub message_type: String,
    pub message_control_id: String,
    pub processing_id: String,
    pub version_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hl7Field {
    pub components: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hl7Segment {
    pub name: String,
    pub fields: Vec<Hl7Field>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hl7Frame {
    pub message_type: Hl7MessageType,
    pub trigger_event: String,
    pub msh: MshSegment,
    pub segments: Vec<Hl7Segment>,
    pub raw: String,
}

pub struct Hl7Parser;

impl FrameParser for Hl7Parser {
    type Frame = Hl7Frame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        let raw =
            core::str::from_utf8(bytes).map_err(|_| Error::Parse("hl7: invalid utf-8".into()))?;

        if raw.is_empty() {
            return Err(Error::Parse("hl7: empty message".into()));
        }

        let seg_term: char = '\r';
        let segments_raw: Vec<&str> = raw.split(seg_term).filter(|s| !s.is_empty()).collect();

        if segments_raw.is_empty() {
            return Err(Error::Parse("hl7: no segments".into()));
        }

        let msh_raw = segments_raw[0];
        if !msh_raw.starts_with("MSH") {
            return Err(Error::Parse("hl7: first segment must be MSH".into()));
        }

        // Byte-indexed slicing of `&str` panics on non-char boundaries, so every
        // slice below goes through `str::get`. The field separator must be a
        // single ASCII byte (it is in every valid HL7 v2 message).
        let field_sep: String = match msh_raw.get(3..4) {
            Some(sep) if sep.is_ascii() => sep.into(),
            Some(_) => return Err(Error::Parse("hl7: non-ASCII field separator".into())),
            None => return Err(Error::Parse("hl7: MSH too short".into())),
        };

        let msh_body = msh_raw.get(4..).unwrap_or("");
        let msh_fields: Vec<&str> = msh_body.split(&field_sep).collect();

        let empty = "";
        let encoding_chars = msh_fields.first().copied().unwrap_or(empty).into();
        let sending_app = msh_fields.get(1).copied().unwrap_or(empty).into();
        let sending_facility = msh_fields.get(2).copied().unwrap_or(empty).into();
        let receiving_app = msh_fields.get(3).copied().unwrap_or(empty).into();
        let receiving_facility = msh_fields.get(4).copied().unwrap_or(empty).into();
        let date_time = msh_fields.get(5).copied().unwrap_or(empty).into();
        let msg_type_full: String = msh_fields.get(7).copied().unwrap_or(empty).into();
        if msg_type_full.is_empty() {
            return Err(Error::Parse("hl7: missing message type".into()));
        }
        let message_control_id: String = msh_fields.get(8).copied().unwrap_or(empty).into();
        let processing_id: String = msh_fields.get(9).copied().unwrap_or(empty).into();
        let version_id: String = msh_fields.get(10).copied().unwrap_or(empty).into();

        let msg_type_parts: Vec<&str> = msg_type_full.split('^').collect();
        let message_type_str = msg_type_parts.first().copied().unwrap_or("");
        let trigger_event: String = msg_type_parts.get(1).copied().unwrap_or("").into();
        let message_type = Hl7MessageType::from_str(message_type_str);

        let msh = MshSegment {
            field_separator: field_sep.clone(),
            encoding_chars,
            sending_app,
            sending_facility,
            receiving_app,
            receiving_facility,
            date_time,
            message_type: msg_type_full,
            message_control_id,
            processing_id,
            version_id,
        };

        let comp_sep = '^';
        let mut segments = Vec::new();

        for seg_raw in segments_raw.iter().skip(1) {
            // Malformed segments (too short, or a non-char-boundary name) are skipped.
            let Some(seg_name) = seg_raw.get(..3) else {
                continue;
            };
            let seg_name: String = seg_name.into();
            let seg_body = seg_raw.get(3..).unwrap_or("");
            let fields_raw: Vec<&str> = seg_body.split(&field_sep).collect();
            let mut fields = Vec::new();

            for f in fields_raw {
                let components: Vec<String> = f.split(comp_sep).map(|c| c.into()).collect();
                fields.push(Hl7Field { components });
            }

            segments.push(Hl7Segment {
                name: seg_name,
                fields,
            });
        }

        Ok(Hl7Frame {
            message_type,
            trigger_event,
            msh,
            segments,
            raw: raw.into(),
        })
    }
}

pub struct Hl7Validator;

impl StateValidator for Hl7Validator {
    type State = Hl7ConnectionState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("hl7: not connected".into()));
        }
        if (state.messages_processed > 0 && state.last_ack_received)
            || state.messages_processed == 0
        {
            Ok(())
        } else {
            Err(Error::State("hl7: awaiting ack".into()))
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hl7ConnectionState {
    pub connected: bool,
    pub messages_processed: u64,
    pub last_ack_received: bool,
}

pub struct Hl7Normalizer;

/// The first component of field `index` of a segment, or "" when it is absent.
/// Segment fields are indexed so that `fields[n]` is field `n` (OBX-n).
fn field_text(segment: &Hl7Segment, index: usize) -> &str {
    segment
        .fields
        .get(index)
        .and_then(|f| f.components.first())
        .map(String::as_str)
        .unwrap_or("")
}

impl Hl7Normalizer {
    /// One point per OBX observation. Numeric results (value type NM) are floats;
    /// other results are text. The result status (OBX-11) sets the quality: `F`
    /// or empty is final, `X` and `D` (not done, deleted) are bad, and anything
    /// else (preliminary, corrected) is uncertain.
    fn observations(frame: &Hl7Frame) -> Vec<DataPoint> {
        frame
            .segments
            .iter()
            .filter(|segment| segment.name == "OBX")
            .map(|obx| {
                let code = field_text(obx, 3);
                let point_id = if code.is_empty() {
                    alloc::format!("obs-{}", field_text(obx, 1))
                } else {
                    alloc::string::String::from(code)
                };
                let raw = field_text(obx, 5);
                let value = if field_text(obx, 2) == "NM" {
                    raw.parse::<f64>()
                        .map(TupValue::Float)
                        .unwrap_or_else(|_| TupValue::Text(alloc::string::String::from(raw)))
                } else {
                    TupValue::Text(alloc::string::String::from(raw))
                };
                let quality = match field_text(obx, 11) {
                    "" | "F" => Quality::Good,
                    "X" | "D" => Quality::Bad,
                    _ => Quality::Uncertain,
                };
                let mut dp = DataPoint::new(
                    SourceProtocol::Hl7v2,
                    alloc::format!("hl7/{}", frame.trigger_event),
                    point_id,
                    0,
                    value,
                    quality,
                );
                let unit = field_text(obx, 6);
                if !unit.is_empty() {
                    dp = dp.with_unit(unit);
                }
                dp
            })
            .collect()
    }
}

impl DataNormalizer for Hl7Normalizer {
    type Frame = Hl7Frame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        Self::observations(frame).into_iter().next().ok_or_else(|| {
            Error::Normalize(alloc::format!(
                "HL7 {} has no OBX observation",
                frame.msh.message_type
            ))
        })
    }

    fn normalize_all(&self, frame: &Self::Frame) -> Result<Vec<DataPoint>, Error> {
        let points = Self::observations(frame);
        if points.is_empty() {
            return Err(Error::Normalize(alloc::format!(
                "HL7 {} has no OBX observation",
                frame.msh.message_type
            )));
        }
        Ok(points)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_adt_message() -> Vec<u8> {
        "MSH|^~\\&|SEND_APP|SEND_FAC|RECV_APP|RECV_FAC|20240101000000||ADT^A01|ctrl123|P|2.5|\rEVN|A01|20240101000000|\rPID|1||12345^^^MRN^MRN||Doe^John||19700101|M|||123 Main St^^Metropolis^NY^10001^USA||(555)123-4567|||S|".as_bytes().to_vec()
    }

    fn make_oru_message() -> Vec<u8> {
        "MSH|^~\\&|LAB|HOSP|EHR|HOSP|20240101010000||ORU^R01|msg002|P|2.5|\rPID|1||67890^^^MRN^MRN||Smith^Jane||19850615|F|||456 Oak Ave^^Springfield^IL^62701^USA|||||\rOBR|1|ord123|spec456|LIPID^Lipid Panel^L|||20240101000000|||||||20240101010000|||F\rOBX|1|NM|CHOL^Cholesterol^L||190|mg/dL|100-199|N|||F".as_bytes().to_vec()
    }

    #[test]
    fn multibyte_field_separator_no_panic() {
        // 'é' is two bytes at index 3, so `msh_raw[3..4]` would split a char.
        let parser = Hl7Parser;
        assert!(parser
            .parse("MSHé|a|b|c|d|e|f|ADT^A01|1|P|2.5".as_bytes())
            .is_err());
    }

    #[test]
    fn multibyte_segment_name_no_panic() {
        // "PIé" — slicing `seg_raw[..3]` lands inside 'é'. A malformed segment
        // is skipped (like short segments), so the message still parses.
        let parser = Hl7Parser;
        let msg = "MSH|^~\\&|A|B|C|D|20240101||ADT^A01|1|P|2.5|\rPIé|1|x";
        let frame = parser.parse(msg.as_bytes()).unwrap();
        assert!(frame.segments.iter().all(|s| s.name.is_ascii()));
    }

    #[test]
    fn valid_parse_adt() {
        let parser = Hl7Parser;
        let bytes = make_adt_message();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.message_type, Hl7MessageType::Adt);
        assert_eq!(frame.trigger_event, "A01");
        assert_eq!(frame.msh.message_control_id, "ctrl123");
        assert!(frame.segments.iter().any(|s| s.name == "PID"));
    }

    #[test]
    fn valid_parse_oru() {
        let parser = Hl7Parser;
        let bytes = make_oru_message();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.message_type, Hl7MessageType::Oru);
        assert_eq!(frame.trigger_event, "R01");
        assert_eq!(frame.msh.sending_app, "LAB");
    }

    #[test]
    fn invalid_frame_no_msh() {
        let parser = Hl7Parser;
        let result = parser.parse(b"BAD|no|msh\rPID|1||id");
        assert!(result.is_err());
    }

    #[test]
    fn truncation() {
        let parser = Hl7Parser;
        let result = parser.parse(b"MSH|^~");
        assert!(result.is_err());
    }

    #[test]
    fn garbage() {
        let parser = Hl7Parser;
        let result = parser.parse(b"\xff\xfe\x00\x01 garbage data \x00");
        assert!(result.is_err());
    }

    #[test]
    fn empty_message() {
        let parser = Hl7Parser;
        let result = parser.parse(b"");
        assert!(result.is_err());
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = Hl7Parser;
        let normalizer = Hl7Normalizer;
        // An ADT carries no observations, so there is no value to report.
        let frame = parser.parse(&make_adt_message()).unwrap();
        assert!(normalizer.normalize(&frame).is_err());

        // An ORU reports its OBX result: CHOL 190 mg/dL, final.
        let frame = parser.parse(&make_oru_message()).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Hl7v2);
        assert_eq!(dp.point_id, "CHOL");
        assert_eq!(dp.value, TupValue::Float(190.0));
        assert_eq!(dp.unit.as_deref(), Some("mg/dL"));
        assert_eq!(dp.quality, Quality::Good);
    }

    #[test]
    fn result_status_sets_the_quality() {
        let parser = Hl7Parser;
        let normalizer = Hl7Normalizer;
        let text = make_oru_message();
        // Preliminary (P) is uncertain; not done (X) is bad.
        for (status, expected) in [("P", Quality::Uncertain), ("X", Quality::Bad)] {
            let edited = String::from_utf8(text.clone())
                .unwrap()
                .replace("|N|||F", &alloc::format!("|N|||{status}"));
            let frame = parser.parse(edited.as_bytes()).unwrap();
            let dp = normalizer.normalize(&frame).unwrap();
            assert_eq!(dp.quality, expected, "status {status}");
        }
    }

    #[test]
    fn validator_ok() {
        let v = Hl7Validator;
        let s = Hl7ConnectionState {
            connected: true,
            messages_processed: 5,
            last_ack_received: true,
        };
        assert!(v.validate(&s).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        let v = Hl7Validator;
        let s = Hl7ConnectionState {
            connected: false,
            messages_processed: 0,
            last_ack_received: false,
        };
        assert!(v.validate(&s).is_err());
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let parser = Hl7Parser;
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }

    /// A message shaped like an HL7 v2 interface feed: an MSH header, then segments
    /// of common types, each field from a small alphabet that includes the component
    /// and repeat separators. Segments end in carriage returns.
    fn message() -> impl Strategy<Value = Vec<u8>> {
        let segment_name =
            proptest::sample::select(alloc::vec!["EVN", "PID", "PV1", "OBR", "OBX", "NTE", "ZZZ"]);
        let field = "[A-Za-z0-9 ^~&.\\-]{0,12}";
        (
            proptest::collection::vec(
                (segment_name, proptest::collection::vec(field, 0..10)),
                0..6,
            ),
            proptest::collection::vec(field, 0..12),
        )
            .prop_map(|(segments, msh_fields)| {
                let mut out = String::from("MSH|^~\\&|");
                out.push_str(&msh_fields.join("|"));
                out.push('\r');
                for (name, fields) in segments {
                    out.push_str(name);
                    out.push('|');
                    out.push_str(&fields.join("|"));
                    out.push('\r');
                }
                out.into_bytes()
            })
    }

    /// One random byte overwrite, one truncation, or one inserted non-ASCII byte.
    /// The last two reach the UTF-8 and character-boundary paths.
    fn mutated_message() -> impl Strategy<Value = Vec<u8>> {
        (message(), any::<usize>(), any::<u8>(), 0u8..3).prop_map(|(mut bytes, idx, val, kind)| {
            if bytes.is_empty() {
                return bytes;
            }
            let i = idx % bytes.len();
            match kind {
                0 => bytes.truncate(i),
                1 => bytes[i] = val,
                _ => bytes.insert(i, 0xC3 | (val & 0x01)),
            }
            bytes
        })
    }

    proptest! {
        #[test]
        fn structured_messages_never_panic(bytes in mutated_message()) {
            let parser = Hl7Parser;
            if let Ok(frame) = parser.parse(&bytes) {
                let _ = Hl7Normalizer.normalize_all(&frame);
            }
        }
    }
}
