//! CANopen protocol implementation for embedded motion control.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// CANopen function code (bits 10-7 of the 11-bit COB-ID).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CanOpenFunctionCode {
    Nmt,
    Sync,
    Pdo1Tx,
    Pdo1Rx,
    Pdo2Tx,
    Pdo2Rx,
    Pdo3Tx,
    Pdo3Rx,
    Pdo4Tx,
    Pdo4Rx,
    SdoTx,
    SdoRx,
    Heartbeat,
    Emergency,
    Timestamp,
    Unknown(u8),
}

impl CanOpenFunctionCode {
    fn from_u8(v: u8) -> Self {
        match v {
            0x00 => Self::Nmt,
            0x02 => Self::Timestamp,
            0x03 => Self::Pdo1Tx,
            0x04 => Self::Pdo1Rx,
            0x05 => Self::Pdo2Tx,
            0x06 => Self::Pdo2Rx,
            0x07 => Self::Pdo3Tx,
            0x08 => Self::Pdo3Rx,
            0x09 => Self::Pdo4Tx,
            0x0A => Self::Pdo4Rx,
            0x0B => Self::SdoTx,
            0x0C => Self::SdoRx,
            0x0E => Self::Heartbeat,
            other => Self::Unknown(other),
        }
    }
}

impl fmt::Display for CanOpenFunctionCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nmt => write!(f, "NMT"),
            Self::Sync => write!(f, "SYNC"),
            Self::Pdo1Tx => write!(f, "PDO1-TX"),
            Self::Pdo1Rx => write!(f, "PDO1-RX"),
            Self::Pdo2Tx => write!(f, "PDO2-TX"),
            Self::Pdo2Rx => write!(f, "PDO2-RX"),
            Self::Pdo3Tx => write!(f, "PDO3-TX"),
            Self::Pdo3Rx => write!(f, "PDO3-RX"),
            Self::Pdo4Tx => write!(f, "PDO4-TX"),
            Self::Pdo4Rx => write!(f, "PDO4-RX"),
            Self::SdoTx => write!(f, "SDO-TX"),
            Self::SdoRx => write!(f, "SDO-RX"),
            Self::Heartbeat => write!(f, "Heartbeat"),
            Self::Emergency => write!(f, "Emergency"),
            Self::Timestamp => write!(f, "Timestamp"),
            Self::Unknown(v) => write!(f, "Unknown(0x{v:02X})"),
        }
    }
}

/// Function code 1 is SYNC only for node 0 (COB-ID 0x080). For any other node it is
/// an Emergency message (COB-ID 0x081-0x0FF).
fn classify(fc_raw: u8, node_id: u8) -> CanOpenFunctionCode {
    match (fc_raw, node_id) {
        (0x01, 0) => CanOpenFunctionCode::Sync,
        (0x01, _) => CanOpenFunctionCode::Emergency,
        _ => CanOpenFunctionCode::from_u8(fc_raw),
    }
}

/// A parsed CANopen frame.
#[derive(Clone, Debug, PartialEq)]
pub struct CanOpenFrame {
    /// 11-bit COB-ID
    pub cob_id: u16,
    /// Function code
    pub function_code: CanOpenFunctionCode,
    /// Node ID (bits 6-0 of COB-ID)
    pub node_id: u8,
    /// Payload (max 8 bytes)
    pub payload: Vec<u8>,
}

impl fmt::Display for CanOpenFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "CANopen[COB=0x{:03X}, FC={}, node=0x{:02X}, {} bytes]",
            self.cob_id,
            self.function_code,
            self.node_id,
            self.payload.len(),
        )
    }
}

/// CANopen frame parser.
#[derive(Clone, Debug, Default)]
pub struct CanOpenParser;

impl CanOpenParser {
    pub fn new() -> Self {
        Self
    }
}

impl FrameParser for CanOpenParser {
    type Frame = CanOpenFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 2 {
            return Err(Error::Parse(format!(
                "CANopen frame too short: {} bytes, need at least 2",
                bytes.len(),
            )));
        }

        let cob_id_raw = u16::from_be_bytes([bytes[0], bytes[1]]);
        // 11-bit COB-ID: bits 10-0
        let cob_id = cob_id_raw & 0x07FF;
        let fc_raw = ((cob_id >> 7) & 0x0F) as u8;
        let node_id = (cob_id & 0x7F) as u8;

        let function_code = classify(fc_raw, node_id);

        let payload = if bytes.len() > 2 {
            bytes[2..].to_vec()
        } else {
            Vec::new()
        };

        if payload.len() > 8 {
            return Err(Error::Parse(format!(
                "CANopen payload too large: {} bytes, max 8",
                payload.len(),
            )));
        }

        Ok(CanOpenFrame {
            cob_id,
            function_code,
            node_id,
            payload,
        })
    }
}

/// CANopen connection state.
#[derive(Clone, Debug, PartialEq)]
pub struct CanOpenState {
    pub connected: bool,
    pub node_id: u8,
    pub heartbeat_interval_ms: u32,
    pub last_heartbeat: u64,
    pub error_count: u64,
}

impl Default for CanOpenState {
    fn default() -> Self {
        Self {
            connected: false,
            node_id: 0,
            heartbeat_interval_ms: 1000,
            last_heartbeat: 0,
            error_count: 0,
        }
    }
}

/// Validates CANopen connection health.
#[derive(Clone, Debug, Default)]
pub struct CanOpenValidator;

impl CanOpenValidator {
    pub fn new() -> Self {
        Self
    }
}

impl StateValidator for CanOpenValidator {
    type State = CanOpenState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("CANopen device not connected".into()));
        }
        if state.node_id == 0 || state.node_id > 127 {
            return Err(Error::State(format!(
                "CANopen invalid node ID: {}",
                state.node_id,
            )));
        }
        if state.error_count > 10 {
            return Err(Error::State(format!(
                "CANopen device has {} consecutive errors",
                state.error_count,
            )));
        }
        Ok(())
    }
}

/// Normalizes CANopen frames into TUP DataPoints.
#[derive(Clone, Debug, Default)]
pub struct CanOpenNormalizer;

impl CanOpenNormalizer {
    pub fn new() -> Self {
        Self
    }
}

impl DataNormalizer for CanOpenNormalizer {
    type Frame = CanOpenFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = format!("canopen:{}", frame.node_id);
        let point_id = format!("fc:{}", frame.function_code);
        let timestamp = 0;

        let value = if frame.payload.len() >= 4 {
            // Little-endian on the wire.
            let raw = u32::from_le_bytes([
                frame.payload[0],
                frame.payload[1],
                frame.payload[2],
                frame.payload[3],
            ]);
            TupValue::Int(raw as i64)
        } else if !frame.payload.is_empty() {
            TupValue::Bytes(frame.payload.clone())
        } else {
            TupValue::Int(frame.cob_id as i64)
        };

        Ok(DataPoint::new(
            SourceProtocol::Canopen,
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

    fn make_canopen_frame(fc: u8, node: u8, payload: &[u8]) -> Vec<u8> {
        let cob_id = ((fc as u16) << 7) | (node as u16);
        let mut buf = Vec::with_capacity(2 + payload.len());
        buf.extend_from_slice(&cob_id.to_be_bytes());
        buf.extend_from_slice(payload);
        buf
    }

    #[test]
    fn valid_parse_pdo1_tx() {
        // FC=0x03, node=0x01
        let bytes = make_canopen_frame(0x03, 0x01, &[0x00, 0x64, 0x00, 0x00]);
        let parser = CanOpenParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.function_code, CanOpenFunctionCode::Pdo1Tx);
        assert_eq!(frame.node_id, 0x01);
        assert_eq!(frame.payload.len(), 4);
    }

    #[test]
    fn function_codes_follow_cia_301() {
        let cases: [(u16, CanOpenFunctionCode); 15] = [
            (0x000, CanOpenFunctionCode::Nmt),
            (0x080, CanOpenFunctionCode::Sync),
            (0x081, CanOpenFunctionCode::Emergency),
            (0x100, CanOpenFunctionCode::Timestamp),
            (0x181, CanOpenFunctionCode::Pdo1Tx),
            (0x201, CanOpenFunctionCode::Pdo1Rx),
            (0x281, CanOpenFunctionCode::Pdo2Tx),
            (0x301, CanOpenFunctionCode::Pdo2Rx),
            (0x381, CanOpenFunctionCode::Pdo3Tx),
            (0x401, CanOpenFunctionCode::Pdo3Rx),
            (0x481, CanOpenFunctionCode::Pdo4Tx),
            (0x501, CanOpenFunctionCode::Pdo4Rx),
            (0x581, CanOpenFunctionCode::SdoTx),
            (0x601, CanOpenFunctionCode::SdoRx),
            (0x701, CanOpenFunctionCode::Heartbeat),
        ];
        for (cob_id, expected) in cases {
            let frame = CanOpenParser::new().parse(&cob_id.to_be_bytes()).unwrap();
            assert_eq!(frame.function_code, expected, "COB-ID 0x{cob_id:03X}");
        }
    }

    #[test]
    fn emergency_message_parses_its_eight_byte_payload() {
        // Error code 0x8110 (little-endian), error register 0x01, manufacturer data.
        let bytes = [0x00, 0x81, 0x10, 0x81, 0x01, 0x00, 0x00, 0x00, 0x00];
        let frame = CanOpenParser::new().parse(&bytes).unwrap();
        assert_eq!(frame.function_code, CanOpenFunctionCode::Emergency);
        assert_eq!(frame.node_id, 1);
        assert_eq!(frame.payload.len(), 7);
    }

    #[test]
    fn valid_parse_heartbeat() {
        let bytes = make_canopen_frame(0x0E, 0x05, &[0x00]);
        let parser = CanOpenParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.function_code, CanOpenFunctionCode::Heartbeat);
        assert_eq!(frame.node_id, 0x05);
    }

    #[test]
    fn invalid_too_short() {
        let parser = CanOpenParser::new();
        let result = parser.parse(&[0x01]);
        assert!(result.is_err());
    }

    #[test]
    fn payload_too_large() {
        let parser = CanOpenParser::new();
        let result = parser.parse(&[
            0x01, 0x02, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09,
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn garbage_input() {
        let parser = CanOpenParser::new();
        let result = parser.parse(&[0xFF, 0xFF]);
        assert!(result.is_ok());
        let frame = result.unwrap();
        assert_eq!(frame.node_id, 0x7F);
    }

    #[test]
    fn normalize_reads_little_endian_payload() {
        // CAN/CANopen data is little-endian: bytes [0x01, 0x02] mean 0x0201.
        let parser = CanOpenParser::new();
        let normalizer = CanOpenNormalizer::new();
        let bytes = make_canopen_frame(0x03, 0x10, &[0x01, 0x02, 0x00, 0x00]);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.value, TupValue::Int(0x0201));
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = CanOpenParser::new();
        let normalizer = CanOpenNormalizer::new();
        let bytes = make_canopen_frame(0x03, 0x10, &[0x2A, 0x00, 0x00, 0x00]);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Canopen);
        assert_eq!(dp.device_id, "canopen:16");
        assert_eq!(dp.value, TupValue::Int(0x2A));
    }

    #[test]
    fn validator_ok() {
        let validator = CanOpenValidator::new();
        let state = CanOpenState {
            connected: true,
            node_id: 0x10,
            heartbeat_interval_ms: 1000,
            last_heartbeat: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        let validator = CanOpenValidator::new();
        let state = CanOpenState::default();
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn validator_bad_node_id() {
        let validator = CanOpenValidator::new();
        let state = CanOpenState {
            connected: true,
            node_id: 0,
            heartbeat_interval_ms: 1000,
            last_heartbeat: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_err());
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    /// A CANopen frame: a two-byte big-endian COB-ID, then up to 8 payload bytes. The
    /// COB-ID is a real function-code base (NMT, SYNC, EMCY, PDO, SDO, heartbeat) plus
    /// a node id. Some payloads run past 8 bytes, so the length check is exercised.
    fn cob_frame() -> impl Strategy<Value = Vec<u8>> {
        let base = proptest::sample::select(alloc::vec![
            0x000u16, 0x080, 0x180, 0x200, 0x280, 0x300, 0x380, 0x400, 0x480, 0x500, 0x580, 0x600,
            0x700,
        ]);
        (
            base,
            0u16..=127,
            proptest::collection::vec(any::<u8>(), 0..12),
        )
            .prop_map(|(base, node, payload)| {
                let cob_id = base | node;
                let mut out = cob_id.to_be_bytes().to_vec();
                out.extend_from_slice(&payload);
                out
            })
    }

    /// One random byte overwrite, one truncation, or one extra byte appended.
    fn mutated_cob_frame() -> impl Strategy<Value = Vec<u8>> {
        (cob_frame(), any::<usize>(), any::<u8>(), 0u8..3).prop_map(
            |(mut bytes, idx, val, kind)| {
                if bytes.is_empty() {
                    return bytes;
                }
                let i = idx % bytes.len();
                match kind {
                    0 => bytes.truncate(i),
                    1 => bytes[i] = val,
                    _ => bytes.push(val),
                }
                bytes
            },
        )
    }

    proptest! {
        #[test]
        fn structured_cob_frames_never_panic(bytes in mutated_cob_frame()) {
            let parser = CanOpenParser::new();
            if let Ok(frame) = parser.parse(&bytes) {
                let _ = CanOpenNormalizer::new().normalize_all(&frame);
            }
        }
    }

    proptest! {
        #[test]
        fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let parser = CanOpenParser::new();
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }
}
