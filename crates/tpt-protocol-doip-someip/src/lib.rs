//! DoIP / SOME/IP protocol implementation for automotive diagnostics and SOA.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

// ---------------------------------------------------------------------------
// DoIP types
// ---------------------------------------------------------------------------

/// DoIP payload type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DoipPayloadType {
    GenericDoipHeaderNegativeAck,
    VehicleIdentificationRequest,
    VehicleIdentificationResponse,
    RoutingActivationRequest,
    RoutingActivationResponse,
    AliveCheckRequest,
    AliveCheckResponse,
    DiagnosticMessage,
    DiagnosticMessagePositiveAck,
    DiagnosticMessageNegativeAck,
    Unknown(u16),
}

impl DoipPayloadType {
    fn from_u16(v: u16) -> Self {
        match v {
            0x0000 => Self::GenericDoipHeaderNegativeAck,
            0x0001 => Self::VehicleIdentificationRequest,
            0x0002 => Self::VehicleIdentificationResponse,
            0x0003 => Self::RoutingActivationRequest,
            0x0004 => Self::RoutingActivationResponse,
            0x0005 => Self::AliveCheckRequest,
            0x0006 => Self::AliveCheckResponse,
            0x0007 => Self::DiagnosticMessage,
            0x0008 => Self::DiagnosticMessagePositiveAck,
            0x0009 => Self::DiagnosticMessageNegativeAck,
            other => Self::Unknown(other),
        }
    }
}

impl fmt::Display for DoipPayloadType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GenericDoipHeaderNegativeAck => write!(f, "GenericNACK"),
            Self::VehicleIdentificationRequest => write!(f, "VehIdentReq"),
            Self::VehicleIdentificationResponse => write!(f, "VehIdentResp"),
            Self::RoutingActivationRequest => write!(f, "RoutingActivationReq"),
            Self::RoutingActivationResponse => write!(f, "RoutingActivationResp"),
            Self::AliveCheckRequest => write!(f, "AliveCheckReq"),
            Self::AliveCheckResponse => write!(f, "AliveCheckResp"),
            Self::DiagnosticMessage => write!(f, "DiagMsg"),
            Self::DiagnosticMessagePositiveAck => write!(f, "DiagPosAck"),
            Self::DiagnosticMessageNegativeAck => write!(f, "DiagNegAck"),
            Self::Unknown(v) => write!(f, "Unknown(0x{v:04X})"),
        }
    }
}

/// A parsed DoIP header + payload.
#[derive(Clone, Debug, PartialEq)]
pub struct DoipFrame {
    pub protocol_version: u8,
    pub inverse_version: u8,
    pub payload_type: DoipPayloadType,
    pub payload_length: u32,
    pub payload: Vec<u8>,
}

impl fmt::Display for DoipFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DoIP[ver=0x{:02X}, inv=0x{:02X}, type={}, len={}]",
            self.protocol_version, self.inverse_version, self.payload_type, self.payload_length,
        )
    }
}

/// SOME/IP message type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SomeipMessageType {
    Request,
    RequestNoReturn,
    Notification,
    Response,
    Error,
    Unknown(u8),
}

impl SomeipMessageType {
    fn from_u8(v: u8) -> Self {
        match v {
            0x00 => Self::Request,
            0x01 => Self::RequestNoReturn,
            0x02 => Self::Notification,
            0x80 => Self::Response,
            0x81 => Self::Error,
            other => Self::Unknown(other),
        }
    }
}

impl fmt::Display for SomeipMessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request => write!(f, "Request"),
            Self::RequestNoReturn => write!(f, "RequestNoReturn"),
            Self::Notification => write!(f, "Notification"),
            Self::Response => write!(f, "Response"),
            Self::Error => write!(f, "Error"),
            Self::Unknown(v) => write!(f, "Unknown(0x{v:02X})"),
        }
    }
}

/// A parsed SOME/IP header + payload.
#[derive(Clone, Debug, PartialEq)]
pub struct SomeipFrame {
    pub service_id: u16,
    pub method_id: u16,
    pub length: u32,
    pub client_id: u16,
    pub session_id: u16,
    pub protocol_version: u8,
    pub interface_version: u8,
    pub message_type: SomeipMessageType,
    pub return_code: u8,
    pub payload: Vec<u8>,
}

impl fmt::Display for SomeipFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SOME/IP[service=0x{:04X}, method=0x{:04X}, type={}, len={}]",
            self.service_id, self.method_id, self.message_type, self.length,
        )
    }
}

/// Combined DoIP/SOME-IP frame.
#[derive(Clone, Debug, PartialEq)]
pub enum DoipSomeipFrame {
    Doip(DoipFrame),
    Someip(SomeipFrame),
}

impl fmt::Display for DoipSomeipFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Doip(d) => write!(f, "{d}"),
            Self::Someip(s) => write!(f, "{s}"),
        }
    }
}

/// DoIP/SOME-IP frame parser.
///
/// Detection: if the first byte is 0x02 (DoIP protocol version), parse as DoIP.
/// Otherwise attempt SOME-IP (minimum 16 bytes).
#[derive(Clone, Debug, Default)]
pub struct DoipSomeipParser;

impl DoipSomeipParser {
    pub fn new() -> Self {
        Self
    }
}

impl FrameParser for DoipSomeipParser {
    type Frame = DoipSomeipFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.is_empty() {
            return Err(Error::Parse("DoIP/SOME-IP frame empty".into()));
        }

        if bytes[0] == 0x02 {
            self.parse_doip(bytes)
        } else if bytes.len() >= 16 {
            self.parse_someip(bytes)
        } else {
            Err(Error::Parse(format!(
                "not DoIP (ver=0x{:02X}) and too short for SOME-IP: {} bytes",
                bytes[0],
                bytes.len(),
            )))
        }
    }
}

impl DoipSomeipParser {
    fn parse_doip(&self, bytes: &[u8]) -> Result<DoipSomeipFrame, Error> {
        if bytes.len() < 8 {
            return Err(Error::Parse(format!(
                "DoIP frame too short: {} bytes, need at least 8",
                bytes.len(),
            )));
        }

        let protocol_version = bytes[0];
        let inverse_version = bytes[1];

        if inverse_version != !protocol_version {
            return Err(Error::Parse(format!(
                "DoIP inverse version mismatch: 0x{inverse_version:02X} != !0x{protocol_version:02X}",
            )));
        }

        let payload_type = DoipPayloadType::from_u16(u16::from_be_bytes([bytes[2], bytes[3]]));
        let payload_length = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);

        let payload_end = 8usize.checked_add(payload_length as usize);
        let payload = match payload_end {
            Some(end) if bytes.len() >= end => bytes[8..end].to_vec(),
            _ => Vec::new(),
        };

        Ok(DoipSomeipFrame::Doip(DoipFrame {
            protocol_version,
            inverse_version,
            payload_type,
            payload_length,
            payload,
        }))
    }

    fn parse_someip(&self, bytes: &[u8]) -> Result<DoipSomeipFrame, Error> {
        if bytes.len() < 16 {
            return Err(Error::Parse(format!(
                "SOME/IP frame too short: {} bytes, need at least 16",
                bytes.len(),
            )));
        }

        let service_id = u16::from_be_bytes([bytes[0], bytes[1]]);
        let method_id = u16::from_be_bytes([bytes[2], bytes[3]]);
        let length = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let client_id = u16::from_be_bytes([bytes[8], bytes[9]]);
        let session_id = u16::from_be_bytes([bytes[10], bytes[11]]);
        let protocol_version = bytes[12];
        let interface_version = bytes[13];
        let message_type = SomeipMessageType::from_u8(bytes[14]);
        let return_code = bytes[15];

        let payload_start = 16usize;
        let payload = if bytes.len() > payload_start {
            bytes[payload_start..].to_vec()
        } else {
            Vec::new()
        };

        Ok(DoipSomeipFrame::Someip(SomeipFrame {
            service_id,
            method_id,
            length,
            client_id,
            session_id,
            protocol_version,
            interface_version,
            message_type,
            return_code,
            payload,
        }))
    }
}

/// DoIP/SOME-IP connection state.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct DoipSomeipState {
    pub connected: bool,
    pub tcp_connected: bool,
    pub routing_active: bool,
    pub last_ok_timestamp: u64,
    pub error_count: u64,
}

/// Validates DoIP/SOME-IP connection health.
#[derive(Clone, Debug, Default)]
pub struct DoipSomeipValidator;

impl DoipSomeipValidator {
    pub fn new() -> Self {
        Self
    }
}

impl StateValidator for DoipSomeipValidator {
    type State = DoipSomeipState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("DoIP/SOME-IP device not connected".into()));
        }
        if !state.tcp_connected {
            return Err(Error::State("DoIP TCP connection not established".into()));
        }
        if !state.routing_active {
            return Err(Error::State("DoIP routing not activated".into()));
        }
        if state.error_count > 10 {
            return Err(Error::State(format!(
                "DoIP/SOME-IP device has {} consecutive errors",
                state.error_count,
            )));
        }
        Ok(())
    }
}

/// Normalizes DoIP/SOME-IP frames into TUP DataPoints.
#[derive(Clone, Debug, Default)]
pub struct DoipSomeipNormalizer;

impl DoipSomeipNormalizer {
    pub fn new() -> Self {
        Self
    }
}

impl DataNormalizer for DoipSomeipNormalizer {
    type Frame = DoipSomeipFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let timestamp = 0;

        match frame {
            DoipSomeipFrame::Doip(d) => {
                let device_id = String::from("doip:gateway");
                let point_id = format!("doip:{}", d.payload_type);
                let value = TupValue::Int(d.payload_length as i64);
                Ok(DataPoint::new(
                    SourceProtocol::DoipSomeip,
                    device_id,
                    point_id,
                    timestamp,
                    value,
                    Quality::Good,
                ))
            }
            DoipSomeipFrame::Someip(s) => {
                let device_id = format!("someip:0x{:04X}", s.service_id);
                let point_id = format!("method:0x{:04X}", s.method_id);
                let value = if !s.payload.is_empty() {
                    TupValue::Bytes(s.payload.clone())
                } else {
                    TupValue::Int(s.return_code as i64)
                };
                Ok(DataPoint::new(
                    SourceProtocol::DoipSomeip,
                    device_id,
                    point_id,
                    timestamp,
                    value,
                    Quality::Good,
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn valid_doip_vehicle_id_request() {
        // protocol_ver=0x02, inverse=0xFD, type=0x0001, length=0x00000000
        let bytes = &[0x02, 0xFD, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00];
        let parser = DoipSomeipParser::new();
        let frame = parser.parse(bytes).unwrap();
        match frame {
            DoipSomeipFrame::Doip(d) => {
                assert_eq!(d.protocol_version, 0x02);
                assert_eq!(d.inverse_version, 0xFD);
                assert_eq!(
                    d.payload_type,
                    DoipPayloadType::VehicleIdentificationRequest
                );
            }
            _ => panic!("expected DoIP frame"),
        }
    }

    #[test]
    fn valid_doip_diag_message() {
        let payload = &[0x00, 0x01, 0x02, 0x03];
        let mut bytes = vec![0x02, 0xFD, 0x00, 0x07];
        let len = payload.len() as u32;
        bytes.extend_from_slice(&len.to_be_bytes());
        bytes.extend_from_slice(payload);
        let parser = DoipSomeipParser::new();
        let frame = parser.parse(&bytes).unwrap();
        match frame {
            DoipSomeipFrame::Doip(d) => {
                assert_eq!(d.payload_type, DoipPayloadType::DiagnosticMessage);
                assert_eq!(d.payload_length, 4);
                assert_eq!(d.payload, payload);
            }
            _ => panic!("expected DoIP frame"),
        }
    }

    #[test]
    fn invalid_inverse_version() {
        let bytes = &[0x02, 0x02, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00];
        let parser = DoipSomeipParser::new();
        let result = parser.parse(bytes);
        assert!(result.is_err());
    }

    #[test]
    fn valid_someip_request() {
        let mut bytes = vec![0x00, 0x01, 0x00, 0x01]; // service=1, method=1
        bytes.extend_from_slice(&0x00000010u32.to_be_bytes()); // length=16
        bytes.extend_from_slice(&0x0001u16.to_be_bytes()); // client=1
        bytes.extend_from_slice(&0x0001u16.to_be_bytes()); // session=1
        bytes.push(0x01); // protocol ver
        bytes.push(0x01); // interface ver
        bytes.push(0x00); // msg type = Request
        bytes.push(0x00); // return code
        bytes.extend_from_slice(&[0xAA, 0xBB]); // payload
        let parser = DoipSomeipParser::new();
        let frame = parser.parse(&bytes).unwrap();
        match frame {
            DoipSomeipFrame::Someip(s) => {
                assert_eq!(s.service_id, 1);
                assert_eq!(s.method_id, 1);
                assert_eq!(s.client_id, 1);
                assert_eq!(s.session_id, 1);
                assert_eq!(s.message_type, SomeipMessageType::Request);
                assert_eq!(s.payload, vec![0xAA, 0xBB]);
            }
            _ => panic!("expected SOME/IP frame"),
        }
    }

    #[test]
    fn short_input() {
        let parser = DoipSomeipParser::new();
        let result = parser.parse(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn garbage_input() {
        let parser = DoipSomeipParser::new();
        // Not DoIP (first byte != 0x02) and too short for SOME-IP
        let result = parser.parse(&[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_doip_roundtrip() {
        let parser = DoipSomeipParser::new();
        let normalizer = DoipSomeipNormalizer::new();
        let bytes = &[0x02, 0xFD, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00];
        let frame = parser.parse(bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::DoipSomeip);
        assert_eq!(dp.point_id, "doip:VehIdentReq");
    }

    #[test]
    fn validator_ok() {
        let validator = DoipSomeipValidator::new();
        let state = DoipSomeipState {
            connected: true,
            tcp_connected: true,
            routing_active: true,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        let validator = DoipSomeipValidator::new();
        let state = DoipSomeipState::default();
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn validator_routing_not_active() {
        let validator = DoipSomeipValidator::new();
        let state = DoipSomeipState {
            connected: true,
            tcp_connected: true,
            routing_active: false,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn doip_huge_payload_length_no_overflow() {
        // payload_length = u32::MAX would make `8 + len` overflow on 32-bit
        // targets and produce an inverted slice range on 64-bit.
        let mut bytes = vec![0x02, 0xFD, 0x00, 0x07];
        bytes.extend_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
        let parser = DoipSomeipParser::new();
        let frame = parser.parse(&bytes).unwrap();
        match frame {
            DoipSomeipFrame::Doip(d) => assert!(d.payload.is_empty()),
            _ => panic!("expected DoIP frame"),
        }
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let parser = DoipSomeipParser::new();
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }
}
