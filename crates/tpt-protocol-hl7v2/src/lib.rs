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

        let field_sep: String = if msh_raw.len() > 3 {
            msh_raw[3..4].into()
        } else {
            return Err(Error::Parse("hl7: MSH too short".into()));
        };

        let msh_body = if msh_raw.len() > 4 { &msh_raw[4..] } else { "" };
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
            let seg_name: String = if seg_raw.len() >= 3 {
                seg_raw[..3].into()
            } else {
                continue;
            };
            let seg_body = if seg_raw.len() > 3 { &seg_raw[3..] } else { "" };
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

impl DataNormalizer for Hl7Normalizer {
    type Frame = Hl7Frame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = alloc::format!("hl7/{}", frame.trigger_event);
        let point_id = frame.msh.message_control_id.clone();
        let value = TupValue::Text(frame.msh.message_type.clone());

        Ok(DataPoint::new(
            SourceProtocol::Hl7v2,
            device_id,
            point_id,
            0,
            value,
            Quality::Good,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_adt_message() -> Vec<u8> {
        "MSH|^~\\&|SEND_APP|SEND_FAC|RECV_APP|RECV_FAC|20240101000000||ADT^A01|ctrl123|P|2.5|\rEVN|A01|20240101000000|\rPID|1||12345^^^MRN^MRN||Doe^John||19700101|M|||123 Main St^^Metropolis^NY^10001^USA||(555)123-4567|||S|".as_bytes().to_vec()
    }

    fn make_oru_message() -> Vec<u8> {
        "MSH|^~\\&|LAB|HOSP|EHR|HOSP|20240101010000||ORU^R01|msg002|P|2.5|\rPID|1||67890^^^MRN^MRN||Smith^Jane||19850615|F|||456 Oak Ave^^Springfield^IL^62701^USA|||||\rOBR|1|ord123|spec456|LIPID^Lipid Panel^L|||20240101000000|||||||20240101010000|||F\rOBX|1|NM|CHOL^Cholesterol^L||190|mg/dL|100-199|N||F".as_bytes().to_vec()
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
        let bytes = make_adt_message();
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Hl7v2);
        assert_eq!(dp.point_id, "ctrl123");
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
