//! OCPP (Open Charge Point Protocol) for EV charging station communication.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OcppMessageType {
    Call,
    CallResult,
    CallError,
}

impl OcppMessageType {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            2 => Some(Self::Call),
            3 => Some(Self::CallResult),
            4 => Some(Self::CallError),
            _ => None,
        }
    }

    fn to_u8(self) -> u8 {
        match self {
            Self::Call => 2,
            Self::CallResult => 3,
            Self::CallError => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OcppAction {
    BootNotification,
    StatusNotification,
    MeterValues,
    StartTransaction,
    StopTransaction,
    Heartbeat,
    Authorize,
    Other(String),
}

impl OcppAction {
    fn from_str(s: &str) -> Self {
        match s {
            "BootNotification" => Self::BootNotification,
            "StatusNotification" => Self::StatusNotification,
            "MeterValues" => Self::MeterValues,
            "StartTransaction" => Self::StartTransaction,
            "StopTransaction" => Self::StopTransaction,
            "Heartbeat" => Self::Heartbeat,
            "Authorize" => Self::Authorize,
            other => Self::Other(other.into()),
        }
    }

    fn as_str(&self) -> &str {
        match self {
            Self::BootNotification => "BootNotification",
            Self::StatusNotification => "StatusNotification",
            Self::MeterValues => "MeterValues",
            Self::StartTransaction => "StartTransaction",
            Self::StopTransaction => "StopTransaction",
            Self::Heartbeat => "Heartbeat",
            Self::Authorize => "Authorize",
            Self::Other(s) => s.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OcppFrame {
    pub message_type: OcppMessageType,
    pub unique_id: String,
    pub action: OcppAction,
    pub payload: String,
}

impl OcppFrame {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(self.message_type.to_u8());
        let id_bytes = self.unique_id.as_bytes();
        buf.extend(&(id_bytes.len() as u16).to_be_bytes());
        buf.extend_from_slice(id_bytes);
        if self.message_type == OcppMessageType::Call {
            let action_str = self.action.as_str();
            let act_bytes = action_str.as_bytes();
            buf.extend(&(act_bytes.len() as u16).to_be_bytes());
            buf.extend_from_slice(act_bytes);
        }
        let payload_bytes = self.payload.as_bytes();
        buf.extend(&(payload_bytes.len() as u32).to_be_bytes());
        buf.extend_from_slice(payload_bytes);
        buf
    }
}

pub struct OcppParser;

impl FrameParser for OcppParser {
    type Frame = OcppFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 5 {
            return Err(Error::Parse("ocpp frame too short".into()));
        }

        let message_type = OcppMessageType::from_u8(bytes[0])
            .ok_or_else(|| Error::Parse("invalid ocpp message type".into()))?;

        let mut offset = 1usize;

        if offset + 2 > bytes.len() {
            return Err(Error::Parse("truncated unique id length".into()));
        }
        let id_len = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
        offset += 2;
        if offset + id_len > bytes.len() {
            return Err(Error::Parse("truncated unique id".into()));
        }
        let unique_id = core::str::from_utf8(&bytes[offset..offset + id_len])
            .map_err(|_| Error::Parse("invalid utf-8 in unique id".into()))?
            .into();
        offset += id_len;

        let action = if message_type == OcppMessageType::Call {
            if bytes.len().saturating_sub(offset) < 2 {
                return Err(Error::Parse("truncated action length".into()));
            }
            let act_len = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
            offset += 2;
            if bytes.len().saturating_sub(offset) < act_len {
                return Err(Error::Parse("truncated action".into()));
            }
            let action_str = core::str::from_utf8(&bytes[offset..offset + act_len])
                .map_err(|_| Error::Parse("invalid utf-8 in action".into()))?;
            offset += act_len;
            OcppAction::from_str(action_str)
        } else {
            OcppAction::Other(alloc::string::String::new())
        };

        if bytes.len().saturating_sub(offset) < 4 {
            return Err(Error::Parse("truncated payload length".into()));
        }
        let payload_len = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;
        if bytes.len().saturating_sub(offset) < payload_len {
            return Err(Error::Parse("truncated payload".into()));
        }
        let payload = core::str::from_utf8(&bytes[offset..offset + payload_len])
            .map_err(|_| Error::Parse("invalid utf-8 in payload".into()))?
            .into();

        Ok(OcppFrame {
            message_type,
            unique_id,
            action,
            payload,
        })
    }
}

pub struct OcppValidator;

impl StateValidator for OcppValidator {
    type State = OcppConnectionState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("ocpp: not connected".into()));
        }
        if state.last_message_type == OcppMessageType::CallError && state.error_count > 5 {
            return Err(Error::State("ocpp: too many errors".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OcppConnectionState {
    pub connected: bool,
    pub last_message_type: OcppMessageType,
    pub error_count: u32,
}

pub struct OcppNormalizer;

impl DataNormalizer for OcppNormalizer {
    type Frame = OcppFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = alloc::format!("ocpp/{}", frame.action.as_str());
        let point_id = frame.unique_id.clone();

        let value = TupValue::Text(frame.payload.clone());

        let quality = match frame.message_type {
            OcppMessageType::CallError => Quality::Bad,
            _ => Quality::Good,
        };

        Ok(DataPoint::new(
            SourceProtocol::Ocpp,
            device_id,
            point_id,
            0,
            value,
            quality,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_call_frame() -> Vec<u8> {
        let frame = OcppFrame {
            message_type: OcppMessageType::Call,
            unique_id: "abc123".into(),
            action: OcppAction::BootNotification,
            payload: "{}".into(),
        };
        frame.to_bytes()
    }

    fn make_result_frame() -> Vec<u8> {
        let frame = OcppFrame {
            message_type: OcppMessageType::CallResult,
            unique_id: "abc123".into(),
            action: OcppAction::Other(String::new()),
            payload: "{\"status\":\"Accepted\"}".into(),
        };
        frame.to_bytes()
    }

    #[test]
    fn valid_parse_call() {
        let parser = OcppParser;
        let bytes = make_call_frame();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.message_type, OcppMessageType::Call);
        assert_eq!(frame.unique_id, "abc123");
        assert_eq!(frame.action, OcppAction::BootNotification);
        assert_eq!(frame.payload, "{}");
    }

    #[test]
    fn valid_parse_result() {
        let parser = OcppParser;
        let bytes = make_result_frame();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.message_type, OcppMessageType::CallResult);
        assert_eq!(frame.payload, "{\"status\":\"Accepted\"}");
    }

    #[test]
    fn invalid_message_type() {
        let parser = OcppParser;
        let result = parser.parse(&[5, 0, 1, 97, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(result.is_err());
    }

    #[test]
    fn truncation() {
        let parser = OcppParser;
        let result = parser.parse(&[2, 0, 3, 97, 98]);
        assert!(result.is_err());
    }

    #[test]
    fn garbage() {
        let parser = OcppParser;
        let result = parser.parse(&[0xff; 64]);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = OcppParser;
        let normalizer = OcppNormalizer;
        let bytes = make_call_frame();
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Ocpp);
        assert_eq!(dp.point_id, "abc123");
    }

    #[test]
    fn validator_ok() {
        let validator = OcppValidator;
        let state = OcppConnectionState {
            connected: true,
            last_message_type: OcppMessageType::CallResult,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        let validator = OcppValidator;
        let state = OcppConnectionState {
            connected: false,
            last_message_type: OcppMessageType::Call,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn huge_payload_length_no_overflow() {
        // Call frame with payload_length = u32::MAX. The previous `offset +
        // payload_len` could overflow on 32-bit targets and then index an
        // inverted range. Must return Err, never panic.
        let bytes = [
            2u8, // Call
            0, 0, // unique id length = 0
            0, 0, // action length = 0
            0xFF, 0xFF, 0xFF, 0xFF, // payload length = u32::MAX
        ];
        let parser = OcppParser;
        let result = parser.parse(&bytes);
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let parser = OcppParser;
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }
}
