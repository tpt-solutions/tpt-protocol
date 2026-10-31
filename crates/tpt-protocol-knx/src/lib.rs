//! KNX protocol implementation for home and building control.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KnxServiceType {
    GroupValueRead,
    GroupValueResponse,
    GroupValueWrite,
}

impl KnxServiceType {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(Self::GroupValueRead),
            0x40 => Some(Self::GroupValueResponse),
            0x80 => Some(Self::GroupValueWrite),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GroupValueRead => "GroupValueRead",
            Self::GroupValueResponse => "GroupValueResponse",
            Self::GroupValueWrite => "GroupValueWrite",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupAddress {
    pub main: u8,
    pub middle: u8,
    pub sub: u8,
}

impl GroupAddress {
    fn from_u16(addr: u16) -> Self {
        let main = ((addr >> 11) & 0x1f) as u8;
        let middle = ((addr >> 8) & 0x07) as u8;
        let sub = (addr & 0xff) as u8;
        Self { main, middle, sub }
    }

    pub fn to_u16(self) -> u16 {
        ((self.main as u16) << 11) | ((self.middle as u16) << 8) | (self.sub as u16)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DptValue {
    Switch(bool),
    Scaling(u8),
    Temperature(f32),
    Percent(f32),
    Raw(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub struct KnxFrame {
    pub service_type: KnxServiceType,
    pub group_address: GroupAddress,
    pub source_address: u16,
    pub dpt: DptValue,
    pub priority: u8,
}

pub struct KnxParser;

impl FrameParser for KnxParser {
    type Frame = KnxFrame;

    /// KNX cEMI-like binary frame format:
    /// [0]: control byte (bits 6-7: priority)
    /// [1-2]: source address (big-endian u16)
    /// [3-4]: destination/group address (big-endian u16)
    /// [5]: counter + type (bit 6 = 1 for group, lower nibble for service type)
    /// [6]: service type (0x00=read, 0x40=response, 0x80=write)
    /// [7+]: DPT data
    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 7 {
            return Err(Error::Parse("knx frame too short".into()));
        }

        let control = bytes[0];
        let priority = (control >> 4) & 0x03;

        let source_address = u16::from_be_bytes([bytes[1], bytes[2]]);
        let group_addr_raw = u16::from_be_bytes([bytes[3], bytes[4]]);
        let group_address = GroupAddress::from_u16(group_addr_raw);

        if bytes[5] & 0x40 == 0 {
            return Err(Error::Parse("not a group address frame".into()));
        }

        let service_type = KnxServiceType::from_u8(bytes[6])
            .ok_or_else(|| Error::Parse("invalid knx service type".into()))?;

        let dpt = match service_type {
            KnxServiceType::GroupValueRead => {
                if bytes.len() < 8 {
                    return Err(Error::Parse("knx: read needs data byte".into()));
                }
                DptValue::Raw(u16::from(bytes[7]))
            }
            KnxServiceType::GroupValueResponse | KnxServiceType::GroupValueWrite => {
                if bytes.len() < 8 {
                    return Err(Error::Parse("knx: truncated dpt data".into()));
                }
                match bytes[7] {
                    0..=1 => DptValue::Switch(bytes[7] != 0),
                    2..=0xff => {
                        if bytes.len() >= 9 {
                            DptValue::Temperature(
                                i16::from_be_bytes([bytes[7], bytes[8]]) as f32 / 100.0,
                            )
                        } else if bytes[7] <= 100 {
                            DptValue::Scaling(bytes[7])
                        } else {
                            DptValue::Raw(u16::from(bytes[7]))
                        }
                    }
                }
            }
        };

        Ok(KnxFrame {
            service_type,
            group_address,
            source_address,
            dpt,
            priority,
        })
    }
}

pub struct KnxValidator;

impl StateValidator for KnxValidator {
    type State = KnxConnectionState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("knx: not connected".into()));
        }
        if state.group_address.main > 31 || state.group_address.middle > 7 {
            return Err(Error::State("knx: invalid group address range".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct KnxConnectionState {
    pub connected: bool,
    pub group_address: GroupAddress,
    pub sequence_counter: u32,
}

pub struct KnxNormalizer;

impl DataNormalizer for KnxNormalizer {
    type Frame = KnxFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = alloc::format!("knx/{:#04x}", frame.source_address);
        let point_id = alloc::format!(
            "{}/{}/{}",
            frame.group_address.main,
            frame.group_address.middle,
            frame.group_address.sub
        );

        let value = match frame.dpt {
            DptValue::Switch(b) => TupValue::Bool(b),
            DptValue::Scaling(v) => TupValue::Int(v as i64),
            DptValue::Temperature(v) => TupValue::Float(v as f64),
            DptValue::Percent(v) => TupValue::Float(v as f64),
            DptValue::Raw(v) => TupValue::Int(v as i64),
        };

        Ok(DataPoint::new(
            SourceProtocol::Knx,
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
    use alloc::vec::Vec;

    fn make_write_frame(ga: u16, data: u8) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(0b0011_0000); // control byte, priority=3
        buf.extend(&0x0101u16.to_be_bytes()); // source address
        buf.extend(&ga.to_be_bytes()); // group address
        buf.push(0b0100_0000); // counter + group flag
        buf.push(0x80); // GroupValueWrite
        buf.push(data); // DPT data
        buf
    }

    // Helper: 0x0801 = main=1, middle=0, sub=1
    const GA_1_0_1: u16 = 0x0801;
    // Helper: 0x1002 = main=2, middle=0, sub=2
    const GA_2_0_2: u16 = 0x1002;

    #[test]
    fn valid_parse_write_switch() {
        let parser = KnxParser;
        let bytes = make_write_frame(GA_1_0_1, 1);
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.service_type, KnxServiceType::GroupValueWrite);
        assert_eq!(frame.group_address.main, 1);
        assert_eq!(frame.group_address.middle, 0);
        assert_eq!(frame.group_address.sub, 1);
        assert_eq!(frame.dpt, DptValue::Switch(true));
    }

    #[test]
    fn valid_parse_write_scaling() {
        let parser = KnxParser;
        let bytes = make_write_frame(GA_2_0_2, 75);
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.group_address.sub, 2);
        assert_eq!(frame.dpt, DptValue::Scaling(75));
    }

    #[test]
    fn invalid_service_type() {
        let parser = KnxParser;
        let mut bytes = make_write_frame(0x1001, 0);
        bytes[6] = 0x20; // invalid service type
        let result = parser.parse(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn truncation() {
        let parser = KnxParser;
        let result = parser.parse(&[0x30, 0x01, 0x01]);
        assert!(result.is_err());
    }

    #[test]
    fn garbage() {
        let parser = KnxParser;
        let result = parser.parse(&[0xff; 16]);
        assert!(result.is_err());
    }

    #[test]
    fn no_group_flag() {
        let parser = KnxParser;
        let mut bytes = make_write_frame(0x1001, 1);
        bytes[5] = 0x00; // clear group flag
        let result = parser.parse(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = KnxParser;
        let normalizer = KnxNormalizer;
        let bytes = make_write_frame(GA_1_0_1, 1);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Knx);
        assert_eq!(dp.point_id, "1/0/1");
        assert_eq!(dp.value, TupValue::Bool(true));
    }

    #[test]
    fn validator_ok() {
        let v = KnxValidator;
        let s = KnxConnectionState {
            connected: true,
            group_address: GroupAddress {
                main: 1,
                middle: 0,
                sub: 1,
            },
            sequence_counter: 0,
        };
        assert!(v.validate(&s).is_ok());
    }

    #[test]
    fn validator_bad_address() {
        let v = KnxValidator;
        let s = KnxConnectionState {
            connected: true,
            group_address: GroupAddress {
                main: 32,
                middle: 8,
                sub: 255,
            },
            sequence_counter: 0,
        };
        assert!(v.validate(&s).is_err());
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let parser = KnxParser;
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }
}
