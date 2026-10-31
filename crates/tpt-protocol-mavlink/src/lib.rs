//! MAVLink protocol implementation for drone and unmanned vehicle telemetry.
//!
//! Frame decoding (magic, header, CRC and CRC_EXTRA validation) is delegated
//! to the [`mavlink`] crate; this crate maps its decoded messages onto the
//! TUP data model.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use core::fmt;

use mavlink::common::MavMessage;
use mavlink::peek_reader::PeekReader;
use mavlink::{MavlinkVersion as WireVersion, Message as _};
use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// Largest MAVLink payload (v1 and v2 both cap at 255 bytes).
const MAX_PAYLOAD: usize = 255;

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

/// A decoded MAVLink frame (v1 or v2).
#[derive(Clone, Debug, PartialEq)]
pub struct MavlinkFrame {
    pub version: MavlinkVersion,
    pub sequence: u8,
    pub system_id: u8,
    pub component_id: u8,
    pub message_id: MavlinkMessageId,
    /// Message payload, re-serialized from the decoded message.
    pub payload: Vec<u8>,
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
        let version = match bytes.first() {
            None => return Err(Error::Parse("MAVLink frame empty".into())),
            Some(0xFE) => MavlinkVersion::V1,
            Some(0xFD) => MavlinkVersion::V2,
            Some(magic) => {
                return Err(Error::Parse(format!(
                    "MAVLink invalid magic byte: 0x{magic:02X}",
                )));
            }
        };

        let mut reader = PeekReader::new(bytes);
        let decoded = match version {
            MavlinkVersion::V1 => mavlink::read_v1_msg::<MavMessage, _>(&mut reader),
            MavlinkVersion::V2 => mavlink::read_v2_msg::<MavMessage, _>(&mut reader),
        };
        let (header, msg) = decoded
            .map_err(|e| Error::Parse(format!("MAVLink {version:?} decode failed: {e:?}")))?;

        let wire_version = match version {
            MavlinkVersion::V1 => WireVersion::V1,
            MavlinkVersion::V2 => WireVersion::V2,
        };
        let mut payload = [0u8; MAX_PAYLOAD];
        let len = msg.ser(wire_version, &mut payload);

        Ok(MavlinkFrame {
            version,
            sequence: header.sequence,
            system_id: header.system_id,
            component_id: header.component_id,
            message_id: MavlinkMessageId::from_u32(msg.message_id()),
            payload: payload[..len].to_vec(),
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
    use mavlink::common::{MavAutopilot, MavModeFlag, MavState, MavType, HEARTBEAT_DATA};
    use mavlink::MavHeader;

    /// Encodes a HEARTBEAT with the `mavlink` crate's writer, so the CRC and
    /// CRC_EXTRA bytes are correct for the wire version under test.
    fn make_frame(version: WireVersion, sys: u8, comp: u8) -> Vec<u8> {
        let header = MavHeader {
            system_id: sys,
            component_id: comp,
            sequence: 1,
        };
        let msg = MavMessage::HEARTBEAT(HEARTBEAT_DATA {
            custom_mode: 0,
            mavtype: MavType::MAV_TYPE_QUADROTOR,
            autopilot: MavAutopilot::MAV_AUTOPILOT_ARDUPILOTMEGA,
            base_mode: MavModeFlag::empty(),
            system_status: MavState::MAV_STATE_STANDBY,
            mavlink_version: 3,
        });
        let mut out = [0u8; 280];
        let mut buf = &mut out[..];
        let written = match version {
            WireVersion::V1 => mavlink::write_v1_msg(&mut buf, header, &msg).unwrap(),
            WireVersion::V2 => mavlink::write_v2_msg(&mut buf, header, &msg).unwrap(),
        };
        out[..written].to_vec()
    }

    #[test]
    fn valid_v1_heartbeat() {
        let bytes = make_frame(WireVersion::V1, 1, 1);
        let parser = MavlinkParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.version, MavlinkVersion::V1);
        assert_eq!(frame.system_id, 1);
        assert_eq!(frame.component_id, 1);
        assert_eq!(frame.message_id, MavlinkMessageId::Heartbeat);
    }

    #[test]
    fn valid_v2_heartbeat() {
        let bytes = make_frame(WireVersion::V2, 1, 1);
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
        let mut bytes = make_frame(WireVersion::V1, 1, 1);
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
        let bytes = make_frame(WireVersion::V2, 10, 5);
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
