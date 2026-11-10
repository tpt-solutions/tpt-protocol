//! UDS (ISO 14229) protocol implementation for automotive diagnostic services.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameEncoder, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// UDS Service Identifier.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UdsSid {
    /// DiagnosticSessionControl (0x10)
    DiagnosticSessionControl,
    /// ECUReset (0x11)
    EcuReset,
    /// ClearDiagnosticInformation (0x14). Takes a group of DTC, not a sub-function.
    ClearDiagnosticInformation,
    /// ReadDTCInformation (0x19)
    ReadDtcInformation,
    /// ReadDataByIdentifier (0x22)
    ReadDataByIdentifier,
    /// ReadMemoryByAddress (0x23)
    ReadMemoryByAddress,
    /// SecurityAccess (0x27)
    SecurityAccess,
    /// CommunicationControl (0x28)
    CommunicationControl,
    /// WriteDataByIdentifier (0x2E)
    WriteDataByIdentifier,
    /// RoutineControl (0x31)
    RoutineControl,
    /// RequestDownload (0x34)
    RequestDownload,
    /// RequestUpload (0x35)
    RequestUpload,
    /// TransferData (0x36)
    TransferData,
    /// RequestTransferExit (0x37)
    RequestTransferExit,
    /// TesterPresent (0x3E)
    TestPresenter,
    /// ControlDTCSetting (0x85)
    ControlDtcSetting,
    /// Positive response to a service: SID | 0x40. The field is the request SID.
    Response(u8),
    /// Negative response (0x7F). Its data is the requested SID and the NRC.
    NegativeResponse,
    /// Unknown SID
    Unknown(u8),
}

impl UdsSid {
    fn from_u8(v: u8) -> Self {
        match v {
            0x7F => Self::NegativeResponse,
            0x40..=0x7E => Self::Response(v & 0x3F),
            0xC5 => Self::Response(0x85),
            0x10 => Self::DiagnosticSessionControl,
            0x11 => Self::EcuReset,
            0x14 => Self::ClearDiagnosticInformation,
            0x19 => Self::ReadDtcInformation,
            0x22 => Self::ReadDataByIdentifier,
            0x23 => Self::ReadMemoryByAddress,
            0x27 => Self::SecurityAccess,
            0x28 => Self::CommunicationControl,
            0x2E => Self::WriteDataByIdentifier,
            0x31 => Self::RoutineControl,
            0x34 => Self::RequestDownload,
            0x35 => Self::RequestUpload,
            0x36 => Self::TransferData,
            0x37 => Self::RequestTransferExit,
            0x3E => Self::TestPresenter,
            0x85 => Self::ControlDtcSetting,
            other => Self::Unknown(other),
        }
    }

    fn is_response(&self) -> bool {
        matches!(self, Self::Response(_) | Self::NegativeResponse)
    }

    /// The SID byte as it appears on the wire (a response keeps its 0x40 bit).
    pub fn value(&self) -> u8 {
        self.raw_value()
    }

    fn raw_value(&self) -> u8 {
        match self {
            Self::DiagnosticSessionControl => 0x10,
            Self::EcuReset => 0x11,
            Self::ClearDiagnosticInformation => 0x14,
            Self::ReadDtcInformation => 0x19,
            Self::ReadDataByIdentifier => 0x22,
            Self::ReadMemoryByAddress => 0x23,
            Self::SecurityAccess => 0x27,
            Self::CommunicationControl => 0x28,
            Self::WriteDataByIdentifier => 0x2E,
            Self::RoutineControl => 0x31,
            Self::RequestDownload => 0x34,
            Self::RequestUpload => 0x35,
            Self::TransferData => 0x36,
            Self::RequestTransferExit => 0x37,
            Self::TestPresenter => 0x3E,
            Self::ControlDtcSetting => 0x85,
            Self::Response(sid) => *sid | 0x40,
            Self::NegativeResponse => 0x7F,
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
            Self::ReadDtcInformation => write!(f, "ReadDTCInformation"),
            Self::ReadDataByIdentifier => write!(f, "ReadDataByIdentifier"),
            Self::ReadMemoryByAddress => write!(f, "ReadMemoryByAddress"),
            Self::SecurityAccess => write!(f, "SecurityAccess"),
            Self::CommunicationControl => write!(f, "CommunicationControl"),
            Self::WriteDataByIdentifier => write!(f, "WriteDataByIdentifier"),
            Self::RoutineControl => write!(f, "RoutineControl"),
            Self::RequestDownload => write!(f, "RequestDownload"),
            Self::RequestUpload => write!(f, "RequestUpload"),
            Self::TransferData => write!(f, "TransferData"),
            Self::RequestTransferExit => write!(f, "RequestTransferExit"),
            Self::TestPresenter => write!(f, "TesterPresent"),
            Self::ControlDtcSetting => write!(f, "ControlDTCSetting"),
            Self::Response(sid) => write!(f, "Response(0x{sid:02X})"),
            Self::NegativeResponse => write!(f, "NegativeResponse"),
            Self::Unknown(v) => write!(f, "Unknown(0x{v:02X})"),
        }
    }
}

/// Sub-function byte. For requests, bit 7 is `suppressPosRsp` and is stored in
/// [`UdsFrame::suppress_positive_response`], not here.
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
    /// True when a request sets `suppressPosRsp` (bit 7 of the sub-function).
    pub suppress_positive_response: bool,
}

impl fmt::Display for UdsFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UDS[{} data={:02x?}]", self.sid, self.data,)
    }
}

/// Services whose first data byte after the SID is a sub-function, in both
/// directions. Per ISO 14229, 0x14 (ClearDiagnosticInformation) is not one of
/// them: its parameter is a group of DTC.
fn has_sub_function(sid_byte: u8) -> bool {
    matches!(
        sid_byte,
        0x10 | 0x11
            | 0x19
            | 0x27
            | 0x28
            | 0x31
            | 0x3E
            | 0x85
            | 0x50
            | 0x51
            | 0x59
            | 0x67
            | 0x68
            | 0x71
            | 0x7E
            | 0xC5
    )
}

/// Writes a [`UdsFrame`] as ISO 14229 wire bytes: the SID, then the
/// sub-function (with `suppressPosRsp` as bit 7 on requests), then the data.
/// The inverse of [`UdsParser`] for every frame the parser produces.
#[derive(Clone, Copy, Debug, Default)]
pub struct UdsEncoder;

impl UdsEncoder {
    pub fn new() -> Self {
        Self
    }
}

impl FrameEncoder for UdsEncoder {
    type Frame = UdsFrame;

    fn encode(&self, frame: &Self::Frame) -> Result<Vec<u8>, Error> {
        let mut out = Vec::with_capacity(2 + frame.data.len());
        out.push(frame.sid.raw_value());
        match frame.sub_function {
            SubFunction::None => {}
            SubFunction::Request(sf) => {
                if sf & 0x80 != 0 {
                    return Err(Error::Parse(format!(
                        "UDS sub-function 0x{sf:02X} has bit 7 set; use suppress_positive_response"
                    )));
                }
                let suppress = if frame.suppress_positive_response {
                    0x80
                } else {
                    0
                };
                out.push(sf | suppress);
            }
            SubFunction::Response(sf) => out.push(sf),
        }
        out.extend_from_slice(&frame.data);
        Ok(out)
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
        let (&sid_byte, rest) = bytes
            .split_first()
            .ok_or_else(|| Error::Parse("UDS frame empty".into()))?;
        let sid = UdsSid::from_u8(sid_byte);
        let is_request = !sid.is_response();

        if sid == UdsSid::NegativeResponse {
            // 0x7F, the requested SID, and the negative response code.
            return match rest {
                [_, _] => Ok(UdsFrame {
                    sid,
                    sub_function: SubFunction::None,
                    data: rest.to_vec(),
                    is_request: false,
                    suppress_positive_response: false,
                }),
                _ => Err(Error::Parse(format!(
                    "UDS negative response needs 2 bytes after 0x7F, got {}",
                    rest.len()
                ))),
            };
        }

        let mut suppress_positive_response = false;
        let (sub_function, data) = if has_sub_function(sid_byte) {
            match rest.split_first() {
                Some((&sf, data)) if is_request => {
                    suppress_positive_response = sf & 0x80 != 0;
                    (SubFunction::Request(sf & 0x7F), data.to_vec())
                }
                Some((&sf, data)) => (SubFunction::Response(sf), data.to_vec()),
                None => (SubFunction::None, Vec::new()),
            }
        } else {
            (SubFunction::None, rest.to_vec())
        };

        Ok(UdsFrame {
            sid,
            sub_function,
            data,
            is_request,
            suppress_positive_response,
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

        if frame.sid == UdsSid::NegativeResponse {
            let (requested, nrc) = match frame.data.as_slice() {
                [requested, nrc] => (*requested, *nrc),
                _ => {
                    return Err(Error::Normalize(
                        "negative response needs the requested SID and NRC".into(),
                    ))
                }
            };
            return Ok(DataPoint::new(
                SourceProtocol::Uds,
                device_id,
                format!("negative:0x{requested:02X}"),
                timestamp,
                TupValue::Int(nrc as i64),
                Quality::Bad,
            ));
        }

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
    fn negative_response_parses_requested_sid_and_nrc() {
        // 0x7F, requested SID 0x22, NRC 0x31 (requestOutOfRange).
        let frame = UdsParser::new().parse(&[0x7F, 0x22, 0x31]).unwrap();
        assert_eq!(frame.sid, UdsSid::NegativeResponse);
        assert!(!frame.is_request);
        assert_eq!(frame.data, vec![0x22, 0x31]);
    }

    #[test]
    fn negative_response_needs_exactly_two_bytes() {
        assert!(UdsParser::new().parse(&[0x7F]).is_err());
        assert!(UdsParser::new().parse(&[0x7F, 0x22]).is_err());
        assert!(UdsParser::new().parse(&[0x7F, 0x22, 0x31, 0x00]).is_err());
    }

    #[test]
    fn clear_dtc_has_no_sub_function() {
        // 0x14 takes a 3-byte group of DTC, so 0xFF is data, not a sub-function.
        let frame = UdsParser::new().parse(&[0x14, 0xFF, 0xFF, 0xFF]).unwrap();
        assert_eq!(frame.sub_function, SubFunction::None);
        assert_eq!(frame.data, vec![0xFF, 0xFF, 0xFF]);
    }

    #[test]
    fn suppress_positive_response_bit_is_parsed_for_requests() {
        let suppressed = UdsParser::new().parse(&[0x10, 0x82]).unwrap();
        assert_eq!(suppressed.sub_function, SubFunction::Request(0x02));
        assert!(suppressed.suppress_positive_response);

        let normal = UdsParser::new().parse(&[0x10, 0x02]).unwrap();
        assert_eq!(normal.sub_function, SubFunction::Request(0x02));
        assert!(!normal.suppress_positive_response);
    }

    #[test]
    fn response_sub_function_is_not_masked() {
        // Responses echo the sub-function unchanged; bit 7 is not suppressPosRsp.
        let frame = UdsParser::new().parse(&[0x50, 0x82]).unwrap();
        assert!(!frame.is_request);
        assert_eq!(frame.sub_function, SubFunction::Response(0x82));
        assert!(!frame.suppress_positive_response);
    }

    #[test]
    fn missing_services_are_named() {
        let cases: [(&[u8], UdsSid); 6] = [
            (&[0x19, 0x02, 0xFF], UdsSid::ReadDtcInformation),
            (&[0x31, 0x01, 0xFF, 0x00], UdsSid::RoutineControl),
            (&[0x34, 0x00], UdsSid::RequestDownload),
            (&[0x35, 0x00], UdsSid::RequestUpload),
            (&[0x36, 0x01], UdsSid::TransferData),
            (&[0x85, 0x01], UdsSid::ControlDtcSetting),
        ];
        for (bytes, expected) in cases {
            assert_eq!(UdsParser::new().parse(bytes).unwrap().sid, expected);
        }
        assert_eq!(
            UdsParser::new().parse(&[0x37]).unwrap().sid,
            UdsSid::RequestTransferExit
        );
    }

    #[test]
    fn positive_response_to_read_dtc_echoes_sub_function() {
        let frame = UdsParser::new().parse(&[0x59, 0x02]).unwrap();
        assert_eq!(frame.sid, UdsSid::Response(0x19));
        assert_eq!(frame.sub_function, SubFunction::Response(0x02));
    }

    #[test]
    fn normalize_negative_response_reports_nrc() {
        let frame = UdsParser::new().parse(&[0x7F, 0x22, 0x31]).unwrap();
        let dp = UdsNormalizer::new().normalize(&frame).unwrap();
        assert_eq!(dp.point_id, "negative:0x22");
        assert_eq!(dp.value, TupValue::Int(0x31));
        assert_eq!(dp.quality, Quality::Bad);
    }

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

    /// A request or response built from known service IDs (with and without a
    /// sub-function), plus a negative response (0x7F, SID, NRC) in some cases.
    fn uds_frame() -> impl Strategy<Value = Vec<u8>> {
        const SIDS: &[u8] = &[
            0x10, 0x11, 0x14, 0x19, 0x22, 0x27, 0x2E, 0x31, 0x34, 0x36, 0x37, 0x3E, 0x85,
        ];
        (
            0usize..SIDS.len(),
            any::<u8>(),
            proptest::collection::vec(any::<u8>(), 0..32),
            any::<u8>(),
            0u8..4,
        )
            .prop_map(|(idx, sf, data, nrc, shape)| {
                let sid = SIDS[idx];
                match shape {
                    // Negative response: 0x7F, requested SID, NRC.
                    0 => alloc::vec![0x7F, sid, nrc],
                    // Request or response with a sub-function byte.
                    1 => {
                        let mut out = alloc::vec![sid, sf];
                        out.extend_from_slice(&data);
                        out
                    }
                    // Request or response without a sub-function.
                    2 => {
                        let mut out = alloc::vec![sid];
                        out.extend_from_slice(&data);
                        out
                    }
                    // Response to a service that carries a sub-function.
                    _ => {
                        let mut out = alloc::vec![sid | 0x40, sf];
                        out.extend_from_slice(&data);
                        out
                    }
                }
            })
    }

    /// One random byte overwrite or one truncation of a well-formed frame.
    fn mutated_uds() -> impl Strategy<Value = Vec<u8>> {
        (uds_frame(), any::<usize>(), any::<u8>(), any::<bool>()).prop_map(
            |(mut bytes, idx, val, truncate)| {
                if bytes.is_empty() {
                    return bytes;
                }
                let i = idx % bytes.len();
                if truncate {
                    bytes.truncate(i);
                } else {
                    bytes[i] = val;
                }
                bytes
            },
        )
    }

    proptest! {
        #[test]
        fn structured_uds_never_panics(bytes in mutated_uds()) {
            let _ = UdsParser::new().parse(&bytes);
        }

        #[test]
        fn parsed_uds_frames_encode_back_to_the_same_bytes(
            bytes in proptest::collection::vec(any::<u8>(), 0..64)
        ) {
            if let Ok(frame) = UdsParser::new().parse(&bytes) {
                prop_assert_eq!(UdsEncoder::new().encode(&frame).unwrap(), bytes);
            }
        }

        #[test]
        fn well_formed_uds_never_panics_in_the_normalizer(bytes in uds_frame()) {
            if let Ok(frame) = UdsParser::new().parse(&bytes) {
                let _ = UdsNormalizer::new().normalize(&frame);
            }
        }
    }
}
