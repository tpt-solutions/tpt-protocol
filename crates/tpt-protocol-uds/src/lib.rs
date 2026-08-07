//! UDS (ISO 14229) protocol implementation for automotive diagnostic services.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// UDS Service Identifier.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UdsSid {
    /// DiagnosticSessionControl (0x10)
    DiagnosticSessionControl,
    /// ECUReset (0x11)
    EcuReset,
    /// ClearDiagnosticInformation (0x14)
    ClearDiagnosticInformation,
    /// SecurityAccess (0x27)
    SecurityAccess,
    /// CommunicationControl (0x28)
    CommunicationControl,
    /// TestPresenter (0x3E)
    TestPresenter,
    /// ReadDataByIdentifier (0x22)
    ReadDataByIdentifier,
    /// WriteDataByIdentifier (0x2E)
    WriteDataByIdentifier,
    /// ReadMemoryByAddress (0x23)
    ReadMemoryByAddress,
    /// Response to any of the above (SID | 0x40)
    Response(u8),
    /// Unknown SID
    Unknown(u8),
}

impl UdsSid {
    fn from_u8(v: u8) -> Self {
        if (0x40..=0x7E).contains(&v) {
            return Self::Response(v & 0x3F);
        }
        match v {
            0x10 => Self::DiagnosticSessionControl,
            0x11 => Self::EcuReset,
            0x14 => Self::ClearDiagnosticInformation,
            0x22 => Self::ReadDataByIdentifier,
            0x23 => Self::ReadMemoryByAddress,
            0x27 => Self::SecurityAccess,
            0x28 => Self::CommunicationControl,
            0x2E => Self::WriteDataByIdentifier,
            0x3E => Self::TestPresenter,
            other => Self::Unknown(other),
        }
    }

    fn is_response(&self) -> bool {
        matches!(self, Self::Response(_))
    }

    fn raw_value(&self) -> u8 {
        match self {
            Self::DiagnosticSessionControl => 0x10,
            Self::EcuReset => 0x11,
            Self::ClearDiagnosticInformation => 0x14,
            Self::ReadDataByIdentifier => 0x22,
            Self::ReadMemoryByAddress => 0x23,
            Self::SecurityAccess => 0x27,
            Self::CommunicationControl => 0x28,
            Self::WriteDataByIdentifier => 0x2E,
            Self::TestPresenter => 0x3E,
            Self::Response(sid) => *sid | 0x40,
            Self::Unknown(v) => *v,
        }
    }
}

impl fmt::Display for UdsSid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DiagnosticSessionControl => write!(f, "DiagnosticSessionControl"),
            Self::EcuReset => write!(f, "ECUReset"),
            Self::ClearDiagnosticInformation => write!(f, "ClearDiagnosticInfo"),
            Self::ReadDataByIdentifier => write!(f, "ReadDataByIdentifier"),
            Self::ReadMemoryByAddress => write!(f, "ReadMemoryByAddress"),
            Self::SecurityAccess => write!(f, "SecurityAccess"),
            Self::CommunicationControl => write!(f, "CommunicationControl"),
            Self::WriteDataByIdentifier => write!(f, "WriteDataByIdentifier"),
            Self::TestPresenter => write!(f, "TestPresenter"),
            Self::Response(sid) => write!(f, "Response(0x{sid:02X})"),
            Self::Unknown(v) => write!(f, "Unknown(0x{v:02X})"),
        }
    }
}

/// Sub-function parameter (for SIDs that support it: 0x10, 0x11, 0x14, 0x27, 0x28, 0x3E).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SubFunction {
    None,
    Request(u8),
    Response(u8),
}

/// A parsed UDS frame.
#[derive(Clone, Debug, PartialEq)]
pub struct UdsFrame {
    pub sid: UdsSid,
    pub sub_function: SubFunction,
    pub data: Vec<u8>,
    pub is_request: bool,
}

impl fmt::Display for UdsFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UDS[{} data={:02x?}]", self.sid, self.data,)
    }
}

/// UDS frame parser.
#[derive(Clone, Debug, Default)]
pub struct UdsParser;

impl UdsParser {
    pub fn new() -> Self {
        Self
    }
}

impl FrameParser for UdsParser {
    type Frame = UdsFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.is_empty() {
            return Err(Error::Parse("UDS frame empty".into()));
        }

        let sid_byte = bytes[0];
        let sid = UdsSid::from_u8(sid_byte);
        let is_request = !sid.is_response();

        // SIDs that have a sub-function parameter
        let has_subfunc = matches!(
            sid_byte,
            0x10 | 0x11 | 0x14 | 0x27 | 0x28 | 0x3E | 0x50 | 0x51 | 0x54 | 0x67 | 0x68 | 0x7E
        );

        let (sub_function, data_start) = if has_subfunc && bytes.len() > 1 {
            let sf = bytes[1];
            let is_sf_response = (sf & 0x80) != 0;
            (
                if is_sf_response {
                    SubFunction::Response(sf & 0x7F)
                } else {
                    SubFunction::Request(sf & 0x7F)
                },
                2,
            )
        } else {
            (SubFunction::None, 1)
        };

        let data = if data_start < bytes.len() {
            bytes[data_start..].to_vec()
        } else {
            Vec::new()
        };

        Ok(UdsFrame {
            sid,
            sub_function,
            data,
            is_request,
        })
    }
}

/// UDS connection state.
#[derive(Clone, Debug, PartialEq)]
pub struct UdsState {
    pub connected: bool,
    pub session_type: u8,
    pub security_level: u8,
    pub last_ok_timestamp: u64,
    pub error_count: u64,
}

impl Default for UdsState {
    fn default() -> Self {
        Self {
            connected: false,
            session_type: 1,
            security_level: 0,
            last_ok_timestamp: 0,
            error_count: 0,
        }
    }
}

/// Validates UDS connection health.
#[derive(Clone, Debug, Default)]
pub struct UdsValidator;

impl UdsValidator {
    pub fn new() -> Self {
        Self
    }
}

impl StateValidator for UdsValidator {
    type State = UdsState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("UDS device not connected".into()));
        }
        if state.session_type > 3 {
            return Err(Error::State(format!(
                "UDS invalid session type: {}",
                state.session_type,
            )));
        }
        if state.error_count > 10 {
            return Err(Error::State(format!(
                "UDS device has {} consecutive errors",
                state.error_count,
            )));
        }
        Ok(())
    }
}

/// Normalizes UDS frames into TUP DataPoints.
#[derive(Clone, Debug, Default)]
pub struct UdsNormalizer;

impl UdsNormalizer {
    pub fn new() -> Self {
        Self
    }
}

impl DataNormalizer for UdsNormalizer {
    type Frame = UdsFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = String::from("uds:ecu");
        let point_id = format!("sid:{}", frame.sid);
        let timestamp = 0;

        let value = if frame.data.is_empty() {
            TupValue::Int(frame.sid.raw_value() as i64)
        } else if frame.data.len() <= 4 {
            let mut buf = [0u8; 4];
            let data_len = frame.data.len();
            buf[4 - data_len..].copy_from_slice(&frame.data);
            TupValue::Int(u32::from_be_bytes(buf) as i64)
        } else {
            TupValue::Bytes(frame.data.clone())
        };

        Ok(DataPoint::new(
            SourceProtocol::Uds,
            device_id,
            point_id,
            timestamp,
            value,
            Quality::Good,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn valid_parse_diagnostic_session_control() {
        // SID 0x10 + sub-function 0x01 (default session)
        let bytes = &[0x10, 0x01];
        let parser = UdsParser::new();
        let frame = parser.parse(bytes).unwrap();
        assert_eq!(frame.sid, UdsSid::DiagnosticSessionControl);
        assert_eq!(frame.sub_function, SubFunction::Request(1));
        assert!(frame.is_request);
    }

    #[test]
    fn valid_parse_response() {
        // Response to ECUReset: 0x51 (0x11 | 0x40) + sub-function 0x01
        let bytes = &[0x51, 0x01];
        let parser = UdsParser::new();
        let frame = parser.parse(bytes).unwrap();
        assert_eq!(frame.sid, UdsSid::Response(0x11));
        assert!(!frame.is_request);
    }

    #[test]
    fn valid_parse_read_data_by_id() {
        // SID 0x22 + DID 0xF190
        let bytes = &[0x22, 0xF1, 0x90];
        let parser = UdsParser::new();
        let frame = parser.parse(bytes).unwrap();
        assert_eq!(frame.sid, UdsSid::ReadDataByIdentifier);
        assert_eq!(frame.sub_function, SubFunction::None);
        assert_eq!(frame.data, vec![0xF1, 0x90]);
    }

    #[test]
    fn empty_input() {
        let parser = UdsParser::new();
        let result = parser.parse(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn garbage_input() {
        let parser = UdsParser::new();
        let result = parser.parse(&[0xFF, 0xFE, 0xFD]);
        assert!(result.is_ok());
        let frame = result.unwrap();
        assert_eq!(frame.sid, UdsSid::Unknown(0xFF));
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = UdsParser::new();
        let normalizer = UdsNormalizer::new();
        let bytes = &[0x22, 0xF1, 0x90];
        let frame = parser.parse(bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Uds);
        assert_eq!(dp.point_id, "sid:ReadDataByIdentifier");
        assert_eq!(dp.value, TupValue::Int(0xF190));
    }

    #[test]
    fn validator_ok() {
        let validator = UdsValidator::new();
        let state = UdsState {
            connected: true,
            session_type: 1,
            security_level: 0,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        let validator = UdsValidator::new();
        let state = UdsState::default();
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn validator_bad_session() {
        let validator = UdsValidator::new();
        let state = UdsState {
            connected: true,
            session_type: 5,
            security_level: 0,
            last_ok_timestamp: 1000,
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
            let parser = UdsParser::new();
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }
}
