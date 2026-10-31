//! Sparkplug B protocol implementation for MQTT-based IIoT infrastructure.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// Upper bound on pre-allocated metrics from a wire-controlled count field.
const MAX_SPARKPLUG_METRICS: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SparkplugMessageType {
    NBIRTH,
    DBIRTH,
    NCMD,
    DCMD,
    NDATA,
    DDATA,
    NDEATH,
    DDEATH,
}

impl SparkplugMessageType {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::NBIRTH),
            1 => Some(Self::DBIRTH),
            2 => Some(Self::NCMD),
            3 => Some(Self::DCMD),
            4 => Some(Self::NDATA),
            5 => Some(Self::DDATA),
            6 => Some(Self::NDEATH),
            7 => Some(Self::DDEATH),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::NBIRTH => "NBIRTH",
            Self::DBIRTH => "DBIRTH",
            Self::NCMD => "NCMD",
            Self::DCMD => "DCMD",
            Self::NDATA => "NDATA",
            Self::DDATA => "DDATA",
            Self::NDEATH => "NDEATH",
            Self::DDEATH => "DDEATH",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Metric {
    pub name: String,
    pub data_type: u8,
    pub value: TupValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SparkplugFrame {
    pub version: u8,
    pub message_type: SparkplugMessageType,
    pub seq: u16,
    pub timestamp: u64,
    pub metrics: Vec<Metric>,
}

pub struct SparkplugParser;

impl FrameParser for SparkplugParser {
    type Frame = SparkplugFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 14 {
            return Err(Error::Parse("frame too short".into()));
        }

        let version = bytes[0];
        let message_type = SparkplugMessageType::from_u8(bytes[1])
            .ok_or_else(|| Error::Parse("invalid message type".into()))?;
        let seq = u16::from_be_bytes([bytes[2], bytes[3]]);
        if seq > 255 {
            return Err(Error::Parse("seq number out of range (0-255)".into()));
        }
        let timestamp = u64::from_be_bytes([
            bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9], bytes[10], bytes[11],
        ]);
        let metric_count = u16::from_be_bytes([bytes[12], bytes[13]]);
        let mut offset = 14usize;
        let mut metrics = Vec::with_capacity((metric_count as usize).min(MAX_SPARKPLUG_METRICS));

        for _ in 0..metric_count {
            if offset >= bytes.len() {
                return Err(Error::Parse("truncated metric header".into()));
            }
            let name_len = bytes[offset] as usize;
            offset += 1;
            if offset + name_len > bytes.len() {
                return Err(Error::Parse("truncated metric name".into()));
            }
            let name = core::str::from_utf8(&bytes[offset..offset + name_len])
                .map_err(|_| Error::Parse("invalid metric name utf-8".into()))?
                .into();
            offset += name_len;

            if offset >= bytes.len() {
                return Err(Error::Parse("truncated metric data type".into()));
            }
            let data_type = bytes[offset];
            offset += 1;

            let value = match data_type {
                0 => {
                    if offset + 4 > bytes.len() {
                        return Err(Error::Parse("truncated int32 value".into()));
                    }
                    let v = i32::from_be_bytes([
                        bytes[offset],
                        bytes[offset + 1],
                        bytes[offset + 2],
                        bytes[offset + 3],
                    ]);
                    offset += 4;
                    TupValue::Int(v as i64)
                }
                1 => {
                    if offset + 8 > bytes.len() {
                        return Err(Error::Parse("truncated int64 value".into()));
                    }
                    let v = i64::from_be_bytes([
                        bytes[offset],
                        bytes[offset + 1],
                        bytes[offset + 2],
                        bytes[offset + 3],
                        bytes[offset + 4],
                        bytes[offset + 5],
                        bytes[offset + 6],
                        bytes[offset + 7],
                    ]);
                    offset += 8;
                    TupValue::Int(v)
                }
                2 => {
                    if offset + 4 > bytes.len() {
                        return Err(Error::Parse("truncated float value".into()));
                    }
                    let bits = u32::from_be_bytes([
                        bytes[offset],
                        bytes[offset + 1],
                        bytes[offset + 2],
                        bytes[offset + 3],
                    ]);
                    offset += 4;
                    TupValue::Float(f32::from_bits(bits) as f64)
                }
                3 => {
                    if offset + 8 > bytes.len() {
                        return Err(Error::Parse("truncated double value".into()));
                    }
                    let bits = u64::from_be_bytes([
                        bytes[offset],
                        bytes[offset + 1],
                        bytes[offset + 2],
                        bytes[offset + 3],
                        bytes[offset + 4],
                        bytes[offset + 5],
                        bytes[offset + 6],
                        bytes[offset + 7],
                    ]);
                    offset += 8;
                    TupValue::Float(f64::from_bits(bits))
                }
                4 => {
                    if offset >= bytes.len() {
                        return Err(Error::Parse("truncated bool value".into()));
                    }
                    let v = bytes[offset] != 0;
                    offset += 1;
                    TupValue::Bool(v)
                }
                5 => {
                    if offset + 2 > bytes.len() {
                        return Err(Error::Parse("truncated string length".into()));
                    }
                    let slen = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
                    offset += 2;
                    if offset + slen > bytes.len() {
                        return Err(Error::Parse("truncated string value".into()));
                    }
                    let s = core::str::from_utf8(&bytes[offset..offset + slen])
                        .map_err(|_| Error::Parse("invalid string utf-8".into()))?
                        .into();
                    offset += slen;
                    TupValue::Text(s)
                }
                _ => return Err(Error::Parse("unknown data type".into())),
            };

            metrics.push(Metric {
                name,
                data_type,
                value,
            });
        }

        Ok(SparkplugFrame {
            version,
            message_type,
            seq,
            timestamp,
            metrics,
        })
    }
}

pub struct SparkplugValidator;

impl StateValidator for SparkplugValidator {
    type State = SparkplugConnectionState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("sparkplug: not connected".into()));
        }
        if state.last_seq > 255 {
            return Err(Error::State("sparkplug: seq out of range".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SparkplugConnectionState {
    pub connected: bool,
    pub last_seq: u16,
    pub birth_received: bool,
}

pub struct SparkplugNormalizer;

impl DataNormalizer for SparkplugNormalizer {
    type Frame = SparkplugFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = alloc::format!("sparkplug/{}", frame.message_type.as_str());
        let point_id = if frame.metrics.is_empty() {
            alloc::format!("seq:{}", frame.seq)
        } else {
            frame.metrics[0].name.clone()
        };

        let value = if frame.metrics.is_empty() {
            TupValue::Int(frame.seq as i64)
        } else {
            frame.metrics[0].value.clone()
        };

        let quality = Quality::Good;

        Ok(DataPoint::new(
            SourceProtocol::Sparkplugb,
            device_id,
            point_id,
            frame.timestamp,
            value,
            quality,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use tpt_protocol_core::FrameParser;

    fn make_valid_frame_bytes() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(1); // version
        buf.push(0); // NBIRTH
        buf.extend(&0u16.to_be_bytes()); // seq = 0
        buf.extend(&1000000u64.to_be_bytes()); // timestamp
        buf.extend(&1u16.to_be_bytes()); // 1 metric
        buf.push(4); // name_len
        buf.extend(b"temp"); // name
        buf.push(3); // data_type = Double
        buf.extend(&2.5f64.to_bits().to_be_bytes()); // value
        buf
    }

    #[test]
    fn valid_parse() {
        let parser = SparkplugParser;
        let bytes = make_valid_frame_bytes();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.version, 1);
        assert_eq!(frame.message_type, SparkplugMessageType::NBIRTH);
        assert_eq!(frame.seq, 0);
        assert_eq!(frame.timestamp, 1000000);
        assert_eq!(frame.metrics.len(), 1);
        assert_eq!(frame.metrics[0].name, "temp");
    }

    #[test]
    fn invalid_frame_too_short() {
        let parser = SparkplugParser;
        let result = parser.parse(&[1, 0]);
        assert!(result.is_err());
    }

    #[test]
    fn truncation() {
        let mut bytes = make_valid_frame_bytes();
        bytes.truncate(15);
        let parser = SparkplugParser;
        let result = parser.parse(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn garbage_data() {
        let parser = SparkplugParser;
        let result = parser.parse(&[0xff; 32]);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_seq() {
        let mut bytes = make_valid_frame_bytes();
        bytes[2] = 0x01;
        bytes[3] = 0x00;
        let parser = SparkplugParser;
        let result = parser.parse(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = SparkplugParser;
        let normalizer = SparkplugNormalizer;
        let bytes = make_valid_frame_bytes();
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Sparkplugb);
        assert_eq!(dp.point_id, "temp");
    }

    #[test]
    fn validator_connected() {
        let validator = SparkplugValidator;
        let state = SparkplugConnectionState {
            connected: true,
            last_seq: 0,
            birth_received: true,
        };
        assert!(validator.validate(&state).is_ok());
    }

    #[test]
    fn validator_disconnected() {
        let validator = SparkplugValidator;
        let state = SparkplugConnectionState {
            connected: false,
            last_seq: 0,
            birth_received: false,
        };
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn huge_metric_count_no_unbounded_alloc() {
        // 14-byte header with a 0xFFFF metric count; must Err without allocating
        // 65k Metric structs.
        let parser = SparkplugParser;
        let mut bytes = vec![1u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF];
        let result = parser.parse(&bytes);
        assert!(result.is_err());
        // Also when the count is non-zero but data is missing.
        bytes[12] = 0x00;
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
            let parser = SparkplugParser;
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }
}
