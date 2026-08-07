//! Matter / Thread protocol implementation for smart home connectivity.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProtocolLayer {
    Matter,
    Thread,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InteractionType {
    Read,
    Write,
    Invoke,
    Subscribe,
}

impl InteractionType {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Read),
            1 => Some(Self::Write),
            2 => Some(Self::Invoke),
            3 => Some(Self::Subscribe),
            _ => None,
        }
    }

}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u16)]
pub enum ClusterId {
    OnOff = 0x0006,
    LevelControl = 0x0008,
    TemperatureMeasurement = 0x0402,
    OccupancySensing = 0x0406,
    Other(u16),
}

impl ClusterId {
    fn from_u16(v: u16) -> Self {
        match v {
            0x0006 => Self::OnOff,
            0x0008 => Self::LevelControl,
            0x0402 => Self::TemperatureMeasurement,
            0x0406 => Self::OccupancySensing,
            other => Self::Other(other),
        }
    }

    fn as_str(&self) -> &str {
        match self {
            Self::OnOff => "OnOff",
            Self::LevelControl => "LevelControl",
            Self::TemperatureMeasurement => "TemperatureMeasurement",
            Self::OccupancySensing => "OccupancySensing",
            Self::Other(_) => "Other",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatterData {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatterFrame {
    pub protocol: ProtocolLayer,
    pub cluster_id: ClusterId,
    pub command_id: u16,
    pub endpoint: u16,
    pub data_version: u32,
    pub interaction_type: InteractionType,
    pub data: MatterData,
}

pub struct MatterParser;

impl FrameParser for MatterParser {
    type Frame = MatterFrame;

    /// Matter/Thread binary frame format:
    /// [0]: protocol layer (0=Matter, 1=Thread)
    /// [1-2]: cluster ID (big-endian u16)
    /// [3-4]: command ID (big-endian u16)
    /// [5-6]: endpoint (big-endian u16)
    /// [7-10]: data version (big-endian u32)
    /// [11]: interaction type
    /// [12+]: TLV-like data
    ///   TLV tag: [1] type (0=Bool, 1=Int, 2=Float, 3=Text, 4=Bytes, 5=Null)
    ///   TLV value: depends on type
    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 13 {
            return Err(Error::Parse("matter frame too short".into()));
        }

        let protocol = match bytes[0] {
            0 => ProtocolLayer::Matter,
            1 => ProtocolLayer::Thread,
            _ => return Err(Error::Parse("invalid protocol layer".into())),
        };

        let cluster_id = ClusterId::from_u16(u16::from_be_bytes([bytes[1], bytes[2]]));
        let command_id = u16::from_be_bytes([bytes[3], bytes[4]]);
        let endpoint = u16::from_be_bytes([bytes[5], bytes[6]]);
        let data_version = u32::from_be_bytes([bytes[7], bytes[8], bytes[9], bytes[10]]);

        let interaction_type = InteractionType::from_u8(bytes[11])
            .ok_or_else(|| Error::Parse("invalid interaction type".into()))?;

        if bytes.len() < 13 {
            return Err(Error::Parse("truncated tlv tag".into()));
        }

        let tlv_type = bytes[12];

        let data = match tlv_type {
            0 => {
                if bytes.len() < 14 {
                    return Err(Error::Parse("truncated tlv bool".into()));
                }
                MatterData::Bool(bytes[13] != 0)
            }
            1 => {
                if bytes.len() < 21 {
                    return Err(Error::Parse("truncated tlv int".into()));
                }
                let v = i64::from_be_bytes([
                    bytes[13], bytes[14], bytes[15], bytes[16], bytes[17], bytes[18], bytes[19],
                    bytes[20],
                ]);
                MatterData::Int(v)
            }
            2 => {
                if bytes.len() < 20 {
                    return Err(Error::Parse("truncated tlv float".into()));
                }
                let v = f64::from_be_bytes([
                    bytes[13], bytes[14], bytes[15], bytes[16], bytes[17], bytes[18], bytes[19],
                    bytes[20],
                ]);
                MatterData::Float(v)
            }
            3 => {
                if bytes.len() < 15 {
                    return Err(Error::Parse("truncated tlv text length".into()));
                }
                let slen = u16::from_be_bytes([bytes[13], bytes[14]]) as usize;
                if bytes.len() < 15 + slen {
                    return Err(Error::Parse("truncated tlv text".into()));
                }
                let s = core::str::from_utf8(&bytes[15..15 + slen])
                    .map_err(|_| Error::Parse("invalid tlv text utf-8".into()))?;
                MatterData::Text(s.into())
            }
            4 => {
                if bytes.len() < 15 {
                    return Err(Error::Parse("truncated tlv bytes length".into()));
                }
                let blen = u16::from_be_bytes([bytes[13], bytes[14]]) as usize;
                if bytes.len() < 15 + blen {
                    return Err(Error::Parse("truncated tlv bytes".into()));
                }
                MatterData::Bytes(bytes[15..15 + blen].to_vec())
            }
            5 => MatterData::Null,
            _ => return Err(Error::Parse("unknown tlv type".into())),
        };

        Ok(MatterFrame {
            protocol,
            cluster_id,
            command_id,
            endpoint,
            data_version,
            interaction_type,
            data,
        })
    }
}

pub struct MatterValidator;

impl StateValidator for MatterValidator {
    type State = MatterConnectionState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("matter: not connected".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatterConnectionState {
    pub connected: bool,
    pub fabric_id: u64,
    pub node_id: u64,
}

pub struct MatterNormalizer;

impl DataNormalizer for MatterNormalizer {
    type Frame = MatterFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = alloc::format!("matter/{}/ep{}", frame.cluster_id.as_str(), frame.endpoint);
        let point_id = alloc::format!("cmd:{:04x}", frame.command_id);

        let value = match &frame.data {
            MatterData::Bool(b) => TupValue::Bool(*b),
            MatterData::Int(i) => TupValue::Int(*i),
            MatterData::Float(f) => TupValue::Float(*f),
            MatterData::Text(s) => TupValue::Text(s.clone()),
            MatterData::Bytes(b) => TupValue::Bytes(b.clone()),
            MatterData::Null => TupValue::Int(0),
        };

        Ok(DataPoint::new(
            SourceProtocol::MatterThread,
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

    fn make_onoff_frame(on: bool) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(0); // Matter
        buf.extend(&0x0006u16.to_be_bytes()); // OnOff cluster
        buf.extend(&0x0000u16.to_be_bytes()); // command ID
        buf.extend(&0x0001u16.to_be_bytes()); // endpoint 1
        buf.extend(&1u32.to_be_bytes()); // data version
        buf.push(0); // Read interaction
        buf.push(0); // TLV type Bool
        buf.push(if on { 1 } else { 0 }); // value
        buf
    }

    fn make_temperature_frame(temp: f64) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(0); // Matter
        buf.extend(&0x0402u16.to_be_bytes()); // TemperatureMeasurement
        buf.extend(&0x0000u16.to_be_bytes());
        buf.extend(&0x0001u16.to_be_bytes());
        buf.extend(&2u32.to_be_bytes());
        buf.push(1); // Write interaction
        buf.push(2); // TLV type Float
        buf.extend(&temp.to_bits().to_be_bytes());
        buf
    }

    fn make_thread_frame() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(1); // Thread
        buf.extend(&0x0006u16.to_be_bytes());
        buf.extend(&0x0001u16.to_be_bytes());
        buf.extend(&0x0002u16.to_be_bytes());
        buf.extend(&0u32.to_be_bytes());
        buf.push(3); // Subscribe
        buf.push(5); // Null TLV
        buf
    }

    #[test]
    fn valid_parse_onoff() {
        let parser = MatterParser;
        let bytes = make_onoff_frame(true);
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.protocol, ProtocolLayer::Matter);
        assert_eq!(frame.cluster_id, ClusterId::OnOff);
        assert_eq!(frame.endpoint, 1);
        assert_eq!(frame.data, MatterData::Bool(true));
    }

    #[test]
    fn valid_parse_temperature() {
        let parser = MatterParser;
        let bytes = make_temperature_frame(23.5);
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.cluster_id, ClusterId::TemperatureMeasurement);
        assert_eq!(frame.data, MatterData::Float(23.5));
    }

    #[test]
    fn valid_parse_thread() {
        let parser = MatterParser;
        let bytes = make_thread_frame();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.protocol, ProtocolLayer::Thread);
        assert_eq!(frame.interaction_type, InteractionType::Subscribe);
    }

    #[test]
    fn invalid_frame_too_short() {
        let parser = MatterParser;
        let result = parser.parse(&[0; 5]);
        assert!(result.is_err());
    }

    #[test]
    fn truncation() {
        let mut bytes = make_onoff_frame(true);
        bytes.truncate(12);
        let parser = MatterParser;
        let result = parser.parse(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn garbage() {
        let parser = MatterParser;
        let result = parser.parse(&[0xff; 32]);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = MatterParser;
        let normalizer = MatterNormalizer;
        let bytes = make_onoff_frame(true);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::MatterThread);
        assert_eq!(dp.value, TupValue::Bool(true));
    }

    #[test]
    fn validator_ok() {
        let v = MatterValidator;
        let s = MatterConnectionState {
            connected: true,
            fabric_id: 1,
            node_id: 2,
        };
        assert!(v.validate(&s).is_ok());
    }

    #[test]
    fn validator_fail() {
        let v = MatterValidator;
        let s = MatterConnectionState {
            connected: false,
            fabric_id: 0,
            node_id: 0,
        };
        assert!(v.validate(&s).is_err());
    }
}
