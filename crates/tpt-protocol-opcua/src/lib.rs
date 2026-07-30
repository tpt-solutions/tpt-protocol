//! OPC UA protocol implementation for industrial interoperability.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// OPC UA message type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OpcuaMessageType {
    Hello,
    Acknowledge,
    Error,
    Message,
    OpenSecureChannel,
    CloseSecureChannel,
    Unknown([u8; 3]),
}

impl OpcuaMessageType {
    fn from_bytes(b: &[u8]) -> Self {
        if b.len() < 3 {
            return Self::Unknown([
                b[0],
                b.get(1).copied().unwrap_or(0),
                b.get(2).copied().unwrap_or(0),
            ]);
        }
        match (b[0], b[1], b[2]) {
            (b'H', b'E', b'L') => Self::Hello,
            (b'A', b'C', b'K') => Self::Acknowledge,
            (b'E', b'R', b'R') => Self::Error,
            (b'M', b'S', b'G') => Self::Message,
            (b'O', b'P', b'N') => Self::OpenSecureChannel,
            (b'C', b'L', b'O') => Self::CloseSecureChannel,
            (a, b, c) => Self::Unknown([a, b, c]),
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Hello => "HEL",
            Self::Acknowledge => "ACK",
            Self::Error => "ERR",
            Self::Message => "MSG",
            Self::OpenSecureChannel => "OPN",
            Self::CloseSecureChannel => "CLO",
            Self::Unknown(_) => "???",
        }
    }
}

impl fmt::Display for OpcuaMessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A parsed OPC UA frame.
#[derive(Clone, Debug, PartialEq)]
pub struct OpcuaFrame {
    pub message_type: OpcuaMessageType,
    pub chunk_type: u8,
    pub message_size: u32,
    pub channel_id: u32,
    pub token_id: u32,
    pub sequence_num: u32,
    pub request_id: u32,
    /// For MSG frames: service_id, node_id, value slice
    pub service_id: Option<u32>,
    pub node_id: Option<u32>,
    pub payload: Vec<u8>,
}

impl fmt::Display for OpcuaFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OPCUA[type={}, chunk=0x{:02X}, size={}, chan=0x{:08X}, tok=0x{:08X}, seq={}, req={}]",
            self.message_type,
            self.chunk_type,
            self.message_size,
            self.channel_id,
            self.token_id,
            self.sequence_num,
            self.request_id,
        )
    }
}

/// OPC UA frame parser.
#[derive(Clone, Debug, Default)]
pub struct OpcuaParser;

impl OpcuaParser {
    pub fn new() -> Self {
        Self
    }
}

impl FrameParser for OpcuaParser {
    type Frame = OpcuaFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 8 {
            return Err(Error::Parse(format!(
                "OPC UA frame too short: {} bytes, need at least 8",
                bytes.len(),
            )));
        }

        let message_type = OpcuaMessageType::from_bytes(&bytes[0..3]);
        let chunk_type = bytes[3];

        if bytes.len() < 8 {
            return Err(Error::Parse("OPC UA: missing message size".into()));
        }
        let message_size = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);

        if (message_size as usize) > bytes.len() {
            return Err(Error::Parse(format!(
                "OPC UA message_size {} > buffer length {}",
                message_size,
                bytes.len(),
            )));
        }

        // These fields are only present if we have enough bytes
        let (channel_id, token_id, sequence_num, request_id) = if bytes.len() >= 24 {
            let ch = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
            let tok = u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
            let seq = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
            let req = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
            (ch, tok, seq, req)
        } else {
            (0, 0, 0, 0)
        };

        // For MSG frames, try to decode service_id (4 bytes) + node_id (4 bytes)
        let (service_id, node_id, payload) =
            if message_type == OpcuaMessageType::Message && bytes.len() >= 32 {
                let svc = u32::from_be_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
                let nid = u32::from_be_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]);
                let rest = if bytes.len() > 32 {
                    bytes[32..].to_vec()
                } else {
                    Vec::new()
                };
                (Some(svc), Some(nid), rest)
            } else {
                let rest = if bytes.len() > 24 {
                    bytes[24..].to_vec()
                } else {
                    Vec::new()
                };
                (None, None, rest)
            };

        Ok(OpcuaFrame {
            message_type,
            chunk_type,
            message_size,
            channel_id,
            token_id,
            sequence_num,
            request_id,
            service_id,
            node_id,
            payload,
        })
    }
}

/// OPC UA connection state.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct OpcuaState {
    pub connected: bool,
    pub channel_id: u32,
    pub token_id: u32,
    pub last_ok_timestamp: u64,
    pub error_count: u64,
}

/// Validates OPC UA connection health.
#[derive(Clone, Debug, Default)]
pub struct OpcuaValidator;

impl OpcuaValidator {
    pub fn new() -> Self {
        Self
    }
}

impl StateValidator for OpcuaValidator {
    type State = OpcuaState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("OPC UA device not connected".into()));
        }
        if state.channel_id == 0 {
            return Err(Error::State("OPC UA invalid channel ID (zero)".into()));
        }
        if state.token_id == 0 {
            return Err(Error::State("OPC UA invalid token ID (zero)".into()));
        }
        if state.error_count > 8 {
            return Err(Error::State(format!(
                "OPC UA device has {} consecutive errors",
                state.error_count,
            )));
        }
        Ok(())
    }
}

/// Normalizes OPC UA frames into TUP DataPoints.
#[derive(Clone, Debug, Default)]
pub struct OpcuaNormalizer;

impl OpcuaNormalizer {
    pub fn new() -> Self {
        Self
    }
}

impl DataNormalizer for OpcuaNormalizer {
    type Frame = OpcuaFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = format!("opcua:chan:0x{:08X}", frame.channel_id);
        let point_id = match frame.service_id {
            Some(svc) => format!("node:{}", svc),
            None => format!("msg:{}", frame.message_type),
        };
        let timestamp = 0;

        let value = if let (Some(svc), Some(nid)) = (frame.service_id, frame.node_id) {
            TupValue::Int(((svc as u64) << 32 | (nid as u64)) as i64)
        } else if !frame.payload.is_empty() {
            TupValue::Bytes(frame.payload.clone())
        } else {
            TupValue::Int(frame.message_size as i64)
        };

        Ok(DataPoint::new(
            SourceProtocol::Opcua,
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

    fn make_opcua_frame(
        msg_type: &[u8; 3],
        chunk: u8,
        channel: u32,
        token: u32,
        seq: u32,
        req: u32,
        extra: &[u8],
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(msg_type);
        buf.push(chunk);
        // placeholder for size
        buf.extend_from_slice(&0x00000000u32.to_be_bytes());
        buf.extend_from_slice(&channel.to_be_bytes());
        buf.extend_from_slice(&token.to_be_bytes());
        buf.extend_from_slice(&seq.to_be_bytes());
        buf.extend_from_slice(&req.to_be_bytes());
        buf.extend_from_slice(extra);
        // fix size
        let size = buf.len() as u32;
        buf[4..8].copy_from_slice(&size.to_be_bytes());
        buf
    }

    #[test]
    fn valid_parse_hello() {
        let bytes = make_opcua_frame(b"HEL", b'F', 0, 0, 0, 0, &[]);
        let parser = OpcuaParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.message_type, OpcuaMessageType::Hello);
        assert_eq!(frame.chunk_type, b'F');
    }

    #[test]
    fn valid_parse_msg() {
        let extra = &[0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0xAA]; // service=1, node=2, payload
        let bytes = make_opcua_frame(b"MSG", b'F', 0x1234, 0x5678, 0x0001, 0x0002, extra);
        let parser = OpcuaParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.message_type, OpcuaMessageType::Message);
        assert_eq!(frame.channel_id, 0x1234);
        assert_eq!(frame.token_id, 0x5678);
        assert_eq!(frame.sequence_num, 0x0001);
        assert_eq!(frame.request_id, 0x0002);
        assert_eq!(frame.service_id, Some(1));
        assert_eq!(frame.node_id, Some(2));
        assert_eq!(frame.payload, vec![0xAA]);
    }

    #[test]
    fn invalid_too_short() {
        let parser = OpcuaParser::new();
        let result = parser.parse(&[0x48, 0x45, 0x4C]);
        assert!(result.is_err());
    }

    #[test]
    fn truncated_header() {
        let parser = OpcuaParser::new();
        let result = parser.parse(&[0x48, 0x45, 0x4C, 0x46, 0x00]);
        assert!(result.is_err());
    }

    #[test]
    fn garbage_input() {
        let parser = OpcuaParser::new();
        let result = parser.parse(&[0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x08]);
        assert!(result.is_ok());
        let frame = result.unwrap();
        assert_eq!(
            frame.message_type,
            OpcuaMessageType::Unknown([0xFF, 0xFF, 0xFF])
        );
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = OpcuaParser::new();
        let normalizer = OpcuaNormalizer::new();
        let extra = &[0x00, 0x00, 0x00, 0x0A, 0x00, 0x00, 0x00, 0x14];
        let bytes = make_opcua_frame(b"MSG", b'F', 0x1234, 0x5678, 1, 1, extra);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Opcua);
        assert_eq!(dp.device_id, "opcua:chan:0x00001234");
        assert_eq!(dp.point_id, "node:10");
    }

    #[test]
    fn validator_ok() {
        let validator = OpcuaValidator::new();
        let state = OpcuaState {
            connected: true,
            channel_id: 0x1234,
            token_id: 0x5678,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        let validator = OpcuaValidator::new();
        let state = OpcuaState::default();
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn validator_zero_channel() {
        let validator = OpcuaValidator::new();
        let state = OpcuaState {
            connected: true,
            channel_id: 0,
            token_id: 0x5678,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_err());
    }
}
