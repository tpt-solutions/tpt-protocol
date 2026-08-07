//! MAVLink protocol implementation for drone and unmanned vehicle telemetry.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

fn msg_id_value(id: &MavlinkMessageId) -> u32 {
    match id {
        MavlinkMessageId::Heartbeat => 0,
        MavlinkMessageId::SysStatus => 1,
        MavlinkMessageId::GpsRawInt => 24,
        MavlinkMessageId::Attitude => 30,
        MavlinkMessageId::Unknown(v) => *v,
    }
}

/// MAVLink protocol version.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MavlinkVersion {
    V1,
    V2,
}

/// Known MAVLink message IDs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MavlinkMessageId {
    Heartbeat,
    SysStatus,
    GpsRawInt,
    Attitude,
    Unknown(u32),
}

impl MavlinkMessageId {
    fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::Heartbeat,
            1 => Self::SysStatus,
            24 => Self::GpsRawInt,
            30 => Self::Attitude,
            other => Self::Unknown(other),
        }
    }
}

impl fmt::Display for MavlinkMessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Heartbeat => write!(f, "HEARTBEAT"),
            Self::SysStatus => write!(f, "SYS_STATUS"),
            Self::GpsRawInt => write!(f, "GPS_RAW_INT"),
            Self::Attitude => write!(f, "ATTITUDE"),
            Self::Unknown(v) => write!(f, "Unknown({v})"),
        }
    }
}

/// A parsed MAVLink frame (v1 or v2).
#[derive(Clone, Debug, PartialEq)]
pub struct MavlinkFrame {
    pub version: MavlinkVersion,
    pub sequence: u8,
    pub system_id: u8,
    pub component_id: u8,
    pub message_id: MavlinkMessageId,
    pub payload: Vec<u8>,
    pub checksum: u16,
}

impl fmt::Display for MavlinkFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MAVLink[ver={}, sys={}, comp={}, msg={}, seq={}, {} bytes]",
            if self.version == MavlinkVersion::V1 {
                "v1"
            } else {
                "v2"
            },
            self.system_id,
            self.component_id,
            self.message_id,
            self.sequence,
            self.payload.len(),
        )
    }
}

/// CRC-16-IBM (CCITT) used by MAVLink.
fn crc16_ibm(data: &[u8], initial: u16) -> u16 {
    let mut crc = initial;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            if crc & 0x8000 != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// MAVLink frame parser (auto-detects v1 vs v2 by magic byte).
#[derive(Clone, Debug, Default)]
pub struct MavlinkParser;

impl MavlinkParser {
    pub fn new() -> Self {
        Self
    }
}

impl FrameParser for MavlinkParser {
    type Frame = MavlinkFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.is_empty() {
            return Err(Error::Parse("MAVLink frame empty".into()));
        }

        match bytes[0] {
            0xFE => self.parse_v1(bytes),
            0xFD => self.parse_v2(bytes),
            magic => Err(Error::Parse(format!(
                "MAVLink invalid magic byte: 0x{magic:02X}",
            ))),
        }
    }
}

impl MavlinkParser {
    fn parse_v1(&self, bytes: &[u8]) -> Result<MavlinkFrame, Error> {
        // v1: magic(1) + length(1) + seq(1) + sys(1) + comp(1) + msg_id(1) + payload + checksum(2)
        if bytes.len() < 8 {
            return Err(Error::Parse(format!(
                "MAVLink v1 frame too short: {} bytes, need at least 8",
                bytes.len(),
            )));
        }

        let payload_len = bytes[1] as usize;
        let expected_total = 6 + payload_len + 2;

        if bytes.len() < expected_total {
            return Err(Error::Parse(format!(
                "MAVLink v1 truncated: have {} bytes, need {}",
                bytes.len(),
                expected_total,
            )));
        }

        let sequence = bytes[2];
        let system_id = bytes[3];
        let component_id = bytes[4];
        let msg_id = bytes[5] as u32;
        let payload = bytes[6..6 + payload_len].to_vec();

        let csum_pos = 6 + payload_len;
        let checksum = u16::from_le_bytes([bytes[csum_pos], bytes[csum_pos + 1]]);

        // Validate CRC over header (bytes 1..6) + payload
        let computed = crc16_ibm(&bytes[1..csum_pos], 0xFFFF);
        if computed != checksum {
            return Err(Error::Parse(format!(
                "MAVLink v1 CRC mismatch: computed 0x{computed:04X}, got 0x{checksum:04X}",
            )));
        }

        Ok(MavlinkFrame {
            version: MavlinkVersion::V1,
            sequence,
            system_id,
            component_id,
            message_id: MavlinkMessageId::from_u32(msg_id),
            payload,
            checksum,
        })
    }

    fn parse_v2(&self, bytes: &[u8]) -> Result<MavlinkFrame, Error> {
        // v2: magic(1) + length(1) + incompat(1) + compat(1) + seq(1) + sys(1) + comp(1) + msg_id(3) + payload + checksum(2)
        if bytes.len() < 12 {
            return Err(Error::Parse(format!(
                "MAVLink v2 frame too short: {} bytes, need at least 12",
                bytes.len(),
            )));
        }

        let payload_len = bytes[1] as usize;
        let _incompat_flags = bytes[2];
        let _compat_flags = bytes[3];
        let sequence = bytes[4];
        let system_id = bytes[5];
        let component_id = bytes[6];

        let msg_id = (bytes[7] as u32) | ((bytes[8] as u32) << 8) | ((bytes[9] as u32) << 16);

        let expected_total = 10 + payload_len + 2;
        if bytes.len() < expected_total {
            return Err(Error::Parse(format!(
                "MAVLink v2 truncated: have {} bytes, need {}",
                bytes.len(),
                expected_total,
            )));
        }

        let payload = bytes[10..10 + payload_len].to_vec();

        let csum_pos = 10 + payload_len;
        let checksum = u16::from_le_bytes([bytes[csum_pos], bytes[csum_pos + 1]]);

        // Validate CRC over header (bytes 1..10) + payload
        let computed = crc16_ibm(&bytes[1..csum_pos], 0xFFFF);
        if computed != checksum {
            return Err(Error::Parse(format!(
                "MAVLink v2 CRC mismatch: computed 0x{computed:04X}, got 0x{checksum:04X}",
            )));
        }

        Ok(MavlinkFrame {
            version: MavlinkVersion::V2,
            sequence,
            system_id,
            component_id,
            message_id: MavlinkMessageId::from_u32(msg_id),
            payload,
            checksum,
        })
    }
}

/// MAVLink connection state.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MavlinkState {
    pub connected: bool,
    pub system_id: u8,
    pub component_id: u8,
    pub heartbeat_received: bool,
    pub last_ok_timestamp: u64,
    pub error_count: u64,
}

/// Validates MAVLink connection health.
#[derive(Clone, Debug, Default)]
pub struct MavlinkValidator;

impl MavlinkValidator {
    pub fn new() -> Self {
        Self
    }
}

impl StateValidator for MavlinkValidator {
    type State = MavlinkState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("MAVLink device not connected".into()));
        }
        if !state.heartbeat_received {
            return Err(Error::State("MAVLink no HEARTBEAT received yet".into()));
        }
        if state.system_id == 0 {
            return Err(Error::State(format!(
                "MAVLink invalid system ID: {}",
                state.system_id,
            )));
        }
        if state.error_count > 15 {
            return Err(Error::State(format!(
                "MAVLink device has {} consecutive errors",
                state.error_count,
            )));
        }
        Ok(())
    }
}

/// Normalizes MAVLink frames into TUP DataPoints.
#[derive(Clone, Debug, Default)]
pub struct MavlinkNormalizer;

impl MavlinkNormalizer {
    pub fn new() -> Self {
        Self
    }
}

impl DataNormalizer for MavlinkNormalizer {
    type Frame = MavlinkFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = format!("mavlink:{}:{}", frame.system_id, frame.component_id);
        let point_id = format!("msg:{}", frame.message_id);
        let timestamp = 0;

        let value = if frame.payload.len() >= 4 {
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
            TupValue::Int(msg_id_value(&frame.message_id) as i64)
        };

        Ok(DataPoint::new(
            SourceProtocol::Mavlink,
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

    #[allow(clippy::vec_init_then_push)]
    fn make_v1_frame(sys: u8, comp: u8, msg_id: u8, payload: &[u8]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(6 + payload.len() + 2);
        buf.push(0xFE);
        buf.push(payload.len() as u8);
        buf.push(0x01);
        buf.push(sys);
        buf.push(comp);
        buf.push(msg_id);
        buf.extend_from_slice(payload);
        let crc = crc16_ibm(&buf[1..], 0xFFFF);
        buf.extend_from_slice(&crc.to_le_bytes());
        buf
    }

    #[allow(clippy::vec_init_then_push)]
    fn make_v2_frame(sys: u8, comp: u8, msg_id: u32, payload: &[u8]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(10 + payload.len() + 2);
        buf.push(0xFD);
        buf.push(payload.len() as u8);
        buf.push(0x00);
        buf.push(0x00);
        buf.push(0x01);
        buf.push(sys);
        buf.push(comp);
        buf.push((msg_id & 0xFF) as u8);
        buf.push(((msg_id >> 8) & 0xFF) as u8);
        buf.push(((msg_id >> 16) & 0xFF) as u8);
        buf.extend_from_slice(payload);
        let crc = crc16_ibm(&buf[1..], 0xFFFF);
        buf.extend_from_slice(&crc.to_le_bytes());
        buf
    }

    #[test]
    fn valid_v1_heartbeat() {
        let bytes = make_v1_frame(1, 1, 0, &[0x00, 0x00, 0x00, 0x00]);
        let parser = MavlinkParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.version, MavlinkVersion::V1);
        assert_eq!(frame.system_id, 1);
        assert_eq!(frame.component_id, 1);
        assert_eq!(frame.message_id, MavlinkMessageId::Heartbeat);
    }

    #[test]
    fn valid_v2_heartbeat() {
        let bytes = make_v2_frame(1, 1, 0, &[0x00, 0x00, 0x00, 0x00]);
        let parser = MavlinkParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.version, MavlinkVersion::V2);
        assert_eq!(frame.system_id, 1);
        assert_eq!(frame.message_id, MavlinkMessageId::Heartbeat);
    }

    #[test]
    fn invalid_magic_byte() {
        let parser = MavlinkParser::new();
        let result = parser.parse(&[0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
        assert!(result.is_err());
    }

    #[test]
    fn truncated_v1() {
        let parser = MavlinkParser::new();
        let result = parser.parse(&[0xFE, 0x05]);
        assert!(result.is_err());
    }

    #[test]
    fn corrupt_crc() {
        let mut bytes = make_v1_frame(1, 1, 0, &[0x00; 4]);
        let len = bytes.len();
        bytes[len - 1] ^= 0xFF;
        let parser = MavlinkParser::new();
        let result = parser.parse(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn garbage_input() {
        let parser = MavlinkParser::new();
        let result = parser.parse(&[0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        // CRC should fail
        assert!(result.is_err());
    }

    #[test]
    fn normalize_roundtrip_v2() {
        let parser = MavlinkParser::new();
        let normalizer = MavlinkNormalizer::new();
        let bytes = make_v2_frame(10, 5, 0, &[0x00, 0x00, 0x00, 0x00]);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Mavlink);
        assert_eq!(dp.device_id, "mavlink:10:5");
        assert_eq!(dp.point_id, "msg:HEARTBEAT");
    }

    #[test]
    fn validator_ok() {
        let validator = MavlinkValidator::new();
        let state = MavlinkState {
            connected: true,
            system_id: 1,
            component_id: 1,
            heartbeat_received: true,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        let validator = MavlinkValidator::new();
        let state = MavlinkState::default();
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn validator_no_heartbeat() {
        let validator = MavlinkValidator::new();
        let state = MavlinkState {
            connected: true,
            system_id: 1,
            component_id: 1,
            heartbeat_received: false,
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
            let parser = MavlinkParser::new();
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }
}
