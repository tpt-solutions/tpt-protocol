//! BACnet protocol implementation for building automation and control.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// BACnet BVLC function codes (for IP).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BvlcFunction {
    OriginalUnicastNpdu,
    OriginalBroadcastNpdu,
    AddressResolutionRequest,
    AddressResolutionResponse,
    VirtualLayerResolution,
    ForwardedNpdu,
    Unknown(u8),
}

impl BvlcFunction {
    fn from_u8(v: u8) -> Self {
        match v {
            0x04 => Self::OriginalUnicastNpdu,
            0x05 => Self::OriginalBroadcastNpdu,
            0x07 => Self::AddressResolutionRequest,
            0x08 => Self::AddressResolutionResponse,
            0x0A => Self::VirtualLayerResolution,
            0x09 => Self::ForwardedNpdu,
            other => Self::Unknown(other),
        }
    }
}

impl fmt::Display for BvlcFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OriginalUnicastNpdu => write!(f, "UnicastNPDU"),
            Self::OriginalBroadcastNpdu => write!(f, "BroadcastNPDU"),
            Self::AddressResolutionRequest => write!(f, "AddrResReq"),
            Self::AddressResolutionResponse => write!(f, "AddrResResp"),
            Self::VirtualLayerResolution => write!(f, "VirtualLayerRes"),
            Self::ForwardedNpdu => write!(f, "ForwardedNPDU"),
            Self::Unknown(v) => write!(f, "Unknown(0x{v:02X})"),
        }
    }
}

/// BACnet APDU type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ApduType {
    ConfirmedRequest,
    UnconfirmedRequest,
    SimpleAck,
    ComplexAck,
    Error,
    Reject,
    Abort,
    Unknown(u8),
}

impl ApduType {
    fn from_nibble(v: u8) -> Self {
        match v {
            0 => Self::ConfirmedRequest,
            1 => Self::UnconfirmedRequest,
            2 => Self::SimpleAck,
            3 => Self::ComplexAck,
            5 => Self::Error,
            6 => Self::Reject,
            7 => Self::Abort,
            other => Self::Unknown(other),
        }
    }
}

impl fmt::Display for ApduType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfirmedRequest => write!(f, "ConfirmedReq"),
            Self::UnconfirmedRequest => write!(f, "UnconfirmedReq"),
            Self::SimpleAck => write!(f, "SimpleAck"),
            Self::ComplexAck => write!(f, "ComplexAck"),
            Self::Error => write!(f, "Error"),
            Self::Reject => write!(f, "Reject"),
            Self::Abort => write!(f, "Abort"),
            Self::Unknown(v) => write!(f, "Unknown({v})"),
        }
    }
}

/// BACnet object identifier: type (2 bytes) + instance (4 bytes).
#[derive(Clone, Debug, PartialEq)]
pub struct BacnetObjectId {
    pub object_type: u16,
    pub instance: u32,
}

/// A parsed BACnet frame (BVLC + APDU).
#[derive(Clone, Debug, PartialEq)]
pub struct BacnetFrame {
    pub bvlc_type: u8,
    pub bvlc_function: BvlcFunction,
    pub bvlc_length: u16,
    pub apdu_type: ApduType,
    pub object: Option<BacnetObjectId>,
    pub property_id: Option<u16>,
    pub payload: Vec<u8>,
}

impl fmt::Display for BacnetFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BACnet[BVLC=0x{:02X}/{}, APDU={}]",
            self.bvlc_type, self.bvlc_function, self.apdu_type,
        )
    }
}

/// BACnet frame parser.
#[derive(Clone, Debug, Default)]
pub struct BacnetParser;

impl BacnetParser {
    pub fn new() -> Self {
        Self
    }
}

impl FrameParser for BacnetParser {
    type Frame = BacnetFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 4 {
            return Err(Error::Parse(format!(
                "BACnet frame too short: {} bytes, need at least 4",
                bytes.len(),
            )));
        }

        // BVLC header
        let bvlc_type = bytes[0];
        if bvlc_type != 0x81 {
            return Err(Error::Parse(format!(
                "BACnet invalid BVLC type: 0x{bvlc_type:02X}, expected 0x81",
            )));
        }

        let bvlc_function = BvlcFunction::from_u8(bytes[1]);
        let bvlc_length = u16::from_be_bytes([bytes[2], bytes[3]]);

        let apdu_start = 4usize;
        let payload_available = if bytes.len() > apdu_start {
            &bytes[apdu_start..]
        } else {
            &[]
        };

        // Parse APDU type from high nibble of first APDU byte
        let (apdu_type, object, property_id, data) = if payload_available.is_empty() {
            (ApduType::Unknown(0), None, None, Vec::new())
        } else {
            let apdu_nibble = (payload_available[0] >> 4) & 0x0F;
            let at = ApduType::from_nibble(apdu_nibble);

            let mut obj = None;
            let mut pid = None;
            let rest;

            // Try to decode object identifier: typically at offset 1-6
            if payload_available.len() >= 7 {
                let obj_type = u16::from_be_bytes([payload_available[1], payload_available[2]]);
                let instance = u32::from_be_bytes([
                    payload_available[3],
                    payload_available[4],
                    payload_available[5],
                    payload_available[6],
                ]);
                obj = Some(BacnetObjectId {
                    object_type: obj_type,
                    instance,
                });
                if payload_available.len() >= 9 {
                    pid = Some(u16::from_be_bytes([
                        payload_available[7],
                        payload_available[8],
                    ]));
                    rest = payload_available[9..].to_vec();
                } else {
                    rest = payload_available[7..].to_vec();
                }
            } else {
                rest = payload_available[1..].to_vec();
            }

            (at, obj, pid, rest)
        };

        Ok(BacnetFrame {
            bvlc_type,
            bvlc_function,
            bvlc_length,
            apdu_type,
            object,
            property_id,
            payload: data,
        })
    }
}

/// BACnet connection state.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BacnetState {
    pub connected: bool,
    pub device_instance: u32,
    pub last_ok_timestamp: u64,
    pub error_count: u64,
}

/// Validates BACnet connection health.
#[derive(Clone, Debug, Default)]
pub struct BacnetValidator;

impl BacnetValidator {
    pub fn new() -> Self {
        Self
    }
}

impl StateValidator for BacnetValidator {
    type State = BacnetState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("BACnet device not connected".into()));
        }
        if state.device_instance > 4_194_303 {
            return Err(Error::State(format!(
                "BACnet invalid device instance: {}",
                state.device_instance,
            )));
        }
        if state.error_count > 10 {
            return Err(Error::State(format!(
                "BACnet device has {} consecutive errors",
                state.error_count,
            )));
        }
        Ok(())
    }
}

/// Normalizes BACnet frames into TUP DataPoints.
#[derive(Clone, Debug, Default)]
pub struct BacnetNormalizer;

impl BacnetNormalizer {
    pub fn new() -> Self {
        Self
    }
}

impl DataNormalizer for BacnetNormalizer {
    type Frame = BacnetFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = format!("bacnet:bvlc:{}", frame.bvlc_function);
        let point_id = if let Some(ref obj) = frame.object {
            format!("obj:{}:{}", obj.object_type, obj.instance)
        } else {
            format!("apdu:{}", frame.apdu_type)
        };
        let timestamp = 0;

        let value = if !frame.payload.is_empty() {
            TupValue::Bytes(frame.payload.clone())
        } else {
            TupValue::Int(0)
        };

        Ok(DataPoint::new(
            SourceProtocol::Bacnet,
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

    fn make_bacnet_frame(func: u8, apdu_nibble: u8, obj_type: u16, instance: u32) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(0x81); // BVLC type
        buf.push(func);
        // length placeholder
        buf.extend_from_slice(&0x0000u16.to_be_bytes());
        // APDU
        buf.push(apdu_nibble << 4);
        buf.extend_from_slice(&obj_type.to_be_bytes());
        buf.extend_from_slice(&instance.to_be_bytes());
        // Set actual length
        let len = buf.len() as u16;
        buf[2] = (len >> 8) as u8;
        buf[3] = (len & 0xFF) as u8;
        buf
    }

    #[test]
    fn valid_parse_unconfirmed_request() {
        let bytes = make_bacnet_frame(0x05, 1, 0, 12345);
        let parser = BacnetParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.bvlc_type, 0x81);
        assert_eq!(frame.bvlc_function, BvlcFunction::OriginalBroadcastNpdu);
        assert_eq!(frame.apdu_type, ApduType::UnconfirmedRequest);
        assert_eq!(frame.object.as_ref().unwrap().instance, 12345);
    }

    #[test]
    fn valid_parse_confirmed_request() {
        let bytes = make_bacnet_frame(0x04, 0, 0, 999);
        let parser = BacnetParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.bvlc_function, BvlcFunction::OriginalUnicastNpdu);
        assert_eq!(frame.apdu_type, ApduType::ConfirmedRequest);
        assert_eq!(frame.object.as_ref().unwrap().instance, 999);
    }

    #[test]
    fn invalid_bvlc_type() {
        let mut bytes = make_bacnet_frame(0x05, 1, 0, 0);
        bytes[0] = 0x82;
        let parser = BacnetParser::new();
        let result = parser.parse(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn truncated_input() {
        let parser = BacnetParser::new();
        let result = parser.parse(&[0x81, 0x04]);
        assert!(result.is_err());
    }

    #[test]
    fn garbage_input() {
        let parser = BacnetParser::new();
        let result = parser.parse(&[0x81, 0xFF, 0x00, 0x04, 0xFE]);
        assert!(result.is_ok());
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = BacnetParser::new();
        let normalizer = BacnetNormalizer::new();
        let bytes = make_bacnet_frame(0x04, 0, 0x01, 42);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Bacnet);
        assert_eq!(dp.point_id, "obj:1:42");
    }

    #[test]
    fn validator_ok() {
        let validator = BacnetValidator::new();
        let state = BacnetState {
            connected: true,
            device_instance: 100,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        let validator = BacnetValidator::new();
        let state = BacnetState::default();
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn validator_bad_instance() {
        let validator = BacnetValidator::new();
        let state = BacnetState {
            connected: true,
            device_instance: 5_000_000,
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
            let parser = BacnetParser::new();
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }
}
