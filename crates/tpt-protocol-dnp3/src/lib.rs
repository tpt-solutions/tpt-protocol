//! DNP3 protocol implementation for electric utility SCADA systems.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// DNP3 application layer function code.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dnp3FunctionCode {
    Read,
    Write,
    Select,
    Operate,
    DirectOperate,
    DirectOperateNoAck,
    Freeze,
    FreezeNoAck,
    FreezeClear,
    FreezeClearNoAck,
    Unknown(u8),
}

impl Dnp3FunctionCode {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Read,
            2 => Self::Write,
            3 => Self::Select,
            4 => Self::Operate,
            5 => Self::DirectOperate,
            6 => Self::DirectOperateNoAck,
            7 => Self::Freeze,
            8 => Self::FreezeNoAck,
            9 => Self::FreezeClear,
            10 => Self::FreezeClearNoAck,
            other => Self::Unknown(other),
        }
    }
}

impl fmt::Display for Dnp3FunctionCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read => write!(f, "Read"),
            Self::Write => write!(f, "Write"),
            Self::Select => write!(f, "Select"),
            Self::Operate => write!(f, "Operate"),
            Self::DirectOperate => write!(f, "DirectOperate"),
            Self::DirectOperateNoAck => write!(f, "DirectOperateNoAck"),
            Self::Freeze => write!(f, "Freeze"),
            Self::FreezeNoAck => write!(f, "FreezeNoAck"),
            Self::FreezeClear => write!(f, "FreezeClear"),
            Self::FreezeClearNoAck => write!(f, "FreezeClearNoAck"),
            Self::Unknown(v) => write!(f, "Unknown({v})"),
        }
    }
}

/// A parsed DNP3 data link layer frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Dnp3Frame {
    pub start: u16,
    pub length: u8,
    pub control: u8,
    pub destination: u16,
    pub source: u16,
    pub data_link_crc: u16,
    pub transport_final: bool,
    pub transport_seq: u8,
    pub app_function: Dnp3FunctionCode,
    pub payload: Vec<u8>,
}

impl fmt::Display for Dnp3Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "DNP3[src={}, dest={}, ctrl=0x{:02X}, len={}, func={}]",
            self.source, self.destination, self.control, self.length, self.app_function,
        )
    }
}

/// CRC-16/DNP.
fn crc16_dnp(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            if crc & 0x0001 != 0 {
                crc = (crc >> 1) ^ 0xA6BC;
            } else {
                crc >>= 1;
            }
        }
    }
    !crc
}

/// DNP3 frame parser.
#[derive(Clone, Debug, Default)]
pub struct Dnp3Parser;

impl Dnp3Parser {
    pub fn new() -> Self {
        Self
    }
}

impl FrameParser for Dnp3Parser {
    type Frame = Dnp3Frame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 8 {
            return Err(Error::Parse(format!(
                "DNP3 frame too short: {} bytes, need at least 8",
                bytes.len(),
            )));
        }

        // Data link header
        let start = u16::from_be_bytes([bytes[0], bytes[1]]);
        if start != 0x0564 {
            return Err(Error::Parse(format!(
                "DNP3 invalid start bytes: 0x{start:04X}, expected 0x0564",
            )));
        }

        let length = bytes[2];
        let control = bytes[3];
        let destination = u16::from_le_bytes([bytes[4], bytes[5]]);
        let source = u16::from_le_bytes([bytes[6], bytes[7]]);

        let expected_total = length as usize + 2; // +2 for start bytes
        // The data-link CRC lives at bytes[8..10], so we must have the full
        // 10-byte header (start + length + control + dest + src + CRC) before
        // indexing it, independent of the advertised `length` field.
        if bytes.len() < 10 || bytes.len() < expected_total {
            return Err(Error::Parse(format!(
                "DNP3 truncated: have {} bytes, need at least {}",
                bytes.len(),
                expected_total.max(10),
            )));
        }

        // Validate data link CRC over bytes 2-7
        let data_link_crc = u16::from_le_bytes([bytes[8], bytes[9]]);
        let computed_crc = crc16_dnp(&bytes[2..8]);
        if computed_crc != data_link_crc {
            return Err(Error::Parse(format!(
                "DNP3 data link CRC mismatch: computed 0x{computed_crc:04X}, got 0x{data_link_crc:04X}",
            )));
        }

        // Transport header (byte after CRC, if available)
        let transport_start = 10usize;
        let (transport_final, transport_seq, app_start) = if bytes.len() > transport_start {
            let th = bytes[transport_start];
            let fin = (th & 0x80) != 0;
            let seq = th & 0x3F;
            (fin, seq, transport_start + 1)
        } else {
            (true, 0, transport_start)
        };

        // Application layer function code
        let (app_function, payload) = if bytes.len() > app_start {
            let fc = Dnp3FunctionCode::from_u8(bytes[app_start]);
            let data = if app_start + 1 < bytes.len() {
                bytes[app_start + 1..].to_vec()
            } else {
                Vec::new()
            };
            (fc, data)
        } else {
            (Dnp3FunctionCode::Unknown(0), Vec::new())
        };

        Ok(Dnp3Frame {
            start,
            length,
            control,
            destination,
            source,
            data_link_crc,
            transport_final,
            transport_seq,
            app_function,
            payload,
        })
    }
}

/// DNP3 connection state.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Dnp3State {
    pub connected: bool,
    pub source_address: u16,
    pub destination_address: u16,
    pub last_ok_timestamp: u64,
    pub error_count: u64,
}

/// Validates DNP3 connection health.
#[derive(Clone, Debug, Default)]
pub struct Dnp3Validator;

impl Dnp3Validator {
    pub fn new() -> Self {
        Self
    }
}

impl StateValidator for Dnp3Validator {
    type State = Dnp3State;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("DNP3 device not connected".into()));
        }
        if state.source_address == 0 || state.source_address == 0xFFFF {
            return Err(Error::State(format!(
                "DNP3 invalid source address: {}",
                state.source_address,
            )));
        }
        if state.error_count > 10 {
            return Err(Error::State(format!(
                "DNP3 device has {} consecutive errors",
                state.error_count,
            )));
        }
        Ok(())
    }
}

/// Normalizes DNP3 frames into TUP DataPoints.
#[derive(Clone, Debug, Default)]
pub struct Dnp3Normalizer;

impl Dnp3Normalizer {
    pub fn new() -> Self {
        Self
    }
}

impl DataNormalizer for Dnp3Normalizer {
    type Frame = Dnp3Frame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = format!("dnp3:{}", frame.source);
        let point_id = format!("func:{}", frame.app_function);
        let timestamp = 0;

        let value = if !frame.payload.is_empty() {
            TupValue::Bytes(frame.payload.clone())
        } else {
            TupValue::Int(frame.length as i64)
        };

        Ok(DataPoint::new(
            SourceProtocol::Dnp3,
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

    fn make_dnp3_frame(src: u16, dest: u16, ctrl: u8, func: u8, payload: &[u8]) -> Vec<u8> {
        let mut header = Vec::new();
        header.push(ctrl);
        header.extend_from_slice(&dest.to_le_bytes());
        header.extend_from_slice(&src.to_le_bytes());

        let transport_only = 0xC0u8; // final + seq 0
                                     // Frame: start(2) + length(1) + header(5) + CRC(2) + transport(1) + func(1) + payload(N)
        let total_len = 2 + 1 + 5 + 2 + 1 + 1 + payload.len();
        // Length field = everything after start bytes
        let length = (total_len - 2) as u8;

        let mut buf = Vec::with_capacity(total_len);
        buf.extend_from_slice(&0x0564u16.to_be_bytes());
        buf.push(length);
        buf.extend_from_slice(&header);
        // CRC placeholder
        buf.push(0x00);
        buf.push(0x00);
        buf.push(transport_only);
        buf.push(func);
        buf.extend_from_slice(payload);

        // Compute CRC over bytes 2..8 (length + header)
        let crc = crc16_dnp(&buf[2..8]);
        buf[8] = (crc & 0xFF) as u8;
        buf[9] = ((crc >> 8) & 0xFF) as u8;
        buf
    }

    #[test]
    fn valid_parse_read() {
        let bytes = make_dnp3_frame(0x0001, 0x0A, 0x44, 1, &[]);
        let parser = Dnp3Parser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.start, 0x0564);
        assert_eq!(frame.source, 1);
        assert_eq!(frame.destination, 0x0A);
        assert_eq!(frame.app_function, Dnp3FunctionCode::Read);
        assert!(frame.transport_final);
    }

    #[test]
    fn valid_parse_direct_operate() {
        let bytes = make_dnp3_frame(0x0002, 0x0B, 0x44, 5, &[0x01, 0x02]);
        let parser = Dnp3Parser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.source, 2);
        assert_eq!(frame.app_function, Dnp3FunctionCode::DirectOperate);
        assert_eq!(frame.payload, vec![0x01, 0x02]);
    }

    #[test]
    fn invalid_start_bytes() {
        let parser = Dnp3Parser::new();
        let result = parser.parse(&[0x00, 0x00, 0x08, 0x44, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00]);
        assert!(result.is_err());
    }

    #[test]
    fn truncated_input() {
        let parser = Dnp3Parser::new();
        let result = parser.parse(&[0x05, 0x64]);
        assert!(result.is_err());
    }

    #[test]
    fn garbage_input() {
        let parser = Dnp3Parser::new();
        let result = parser.parse(&[0x05, 0x64, 0x08, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00]);
        // Should fail CRC
        assert!(result.is_err());
    }

    #[test]
    fn corrupt_crc() {
        let mut bytes = make_dnp3_frame(0x0001, 0x0A, 0x44, 1, &[]);
        bytes[8] ^= 0xFF; // corrupt CRC low byte
        let parser = Dnp3Parser::new();
        let result = parser.parse(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = Dnp3Parser::new();
        let normalizer = Dnp3Normalizer::new();
        let bytes = make_dnp3_frame(0x0123, 0x0A, 0x44, 1, &[]);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Dnp3);
        assert_eq!(dp.device_id, "dnp3:291");
        assert_eq!(dp.point_id, "func:Read");
    }

    #[test]
    fn validator_ok() {
        let validator = Dnp3Validator::new();
        let state = Dnp3State {
            connected: true,
            source_address: 0x0001,
            destination_address: 0x0A,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        let validator = Dnp3Validator::new();
        let state = Dnp3State::default();
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn validator_zero_source() {
        let validator = Dnp3Validator::new();
        let state = Dnp3State {
            connected: true,
            source_address: 0,
            destination_address: 0,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn truncated_short_header_no_panic() {
        // length field = 0 claims a 2-byte frame, but the parser indexes the
        // data-link CRC at bytes[8..10]. A 9-byte buffer must not panic.
        let parser = Dnp3Parser::new();
        let result = parser.parse(&[0x05, 0x64, 0x00, 0x44, 0x01, 0x00, 0x01, 0x00, 0x00]);
        assert!(result.is_err());
    }

    #[test]
    fn truncated_length_seven_no_panic() {
        // length field = 7 -> expected_total = 9, leaving bytes[9] unguarded.
        let parser = Dnp3Parser::new();
        let result = parser.parse(&[0x05, 0x64, 0x07, 0x44, 0x01, 0x00, 0x01, 0x00, 0x00]);
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
            let parser = Dnp3Parser::new();
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }
}
