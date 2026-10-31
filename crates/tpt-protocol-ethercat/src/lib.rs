//! EtherCAT protocol implementation for high-speed industrial automation.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// EtherCAT datagram command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EcCmd {
    /// APRD – Auto-increment Physical Read
    Aprd,
    /// APWR – Auto-increment Physical Write
    Apwr,
    /// FPRD – Configured-address Physical Read
    Fprd,
    /// FPWR – Configured-address Physical Write
    Fpwr,
    /// BRD – Broadcast Read
    Brd,
    /// BWR – Broadcast Write
    Bwr,
    /// LRD – Logical Memory Read
    Lrd,
    /// LWR – Logical Memory Write
    Lwr,
    /// Unknown command
    Unknown(u8),
}

impl EcCmd {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Aprd,
            5 => Self::Apwr,
            4 => Self::Fprd,
            8 => Self::Fpwr,
            7 => Self::Brd,
            11 => Self::Bwr,
            12 => Self::Lrd,
            13 => Self::Lwr,
            other => Self::Unknown(other),
        }
    }
}

impl fmt::Display for EcCmd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Aprd => write!(f, "APRD"),
            Self::Apwr => write!(f, "APWR"),
            Self::Fprd => write!(f, "FPRD"),
            Self::Fpwr => write!(f, "FPWR"),
            Self::Brd => write!(f, "BRD"),
            Self::Bwr => write!(f, "BWR"),
            Self::Lrd => write!(f, "LRD"),
            Self::Lwr => write!(f, "LWR"),
            Self::Unknown(v) => write!(f, "Unknown({v})"),
        }
    }
}

/// A parsed EtherCAT frame (Ethernet frame with EtherType 0x88A4).
#[derive(Clone, Debug, PartialEq)]
pub struct EthercatFrame {
    pub eth_length: u16,
    pub eth_type: u8,
    pub datagram_cmd: EcCmd,
    pub datagram_index: u8,
    pub addressing: u32,
    pub datagram_length: u16,
    pub data: Vec<u8>,
    pub working_counter: u16,
}

impl fmt::Display for EthercatFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "EtherCAT[cmd={}, idx={}, addr=0x{:08X}, len={}, wkc={}]",
            self.datagram_cmd,
            self.datagram_index,
            self.addressing,
            self.datagram_length,
            self.working_counter,
        )
    }
}

/// EtherCAT frame parser.
///
/// Expects raw Ethernet payload starting at EtherCAT header
/// (EtherType 0x88A4 should already be stripped).
#[derive(Clone, Debug, Default)]
pub struct EthercatParser;

impl EthercatParser {
    pub fn new() -> Self {
        Self
    }
}

impl FrameParser for EthercatParser {
    type Frame = EthercatFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 3 {
            return Err(Error::Parse(format!(
                "EtherCAT frame too short: {} bytes, need at least 3",
                bytes.len(),
            )));
        }

        // EtherCAT header: length(2) + type(1)
        let eth_length = u16::from_be_bytes([bytes[0], bytes[1]]);
        let eth_type = bytes[2];

        // Datagram header (10 bytes: cmd + index + addressing + length + reserved)
        if bytes.len() < 13 {
            return Err(Error::Parse(format!(
                "EtherCAT frame too short for datagram header: {} bytes, need at least 13",
                bytes.len(),
            )));
        }

        let cmd_byte = bytes[3];
        let datagram_cmd = EcCmd::from_u8(cmd_byte);
        let datagram_index = bytes[4];
        let addressing = u32::from_be_bytes([bytes[5], bytes[6], bytes[7], bytes[8]]);
        let datagram_length = u16::from_be_bytes([bytes[9], bytes[10]]);

        // Reserved bytes 11-12
        // Data starts at offset 13
        let data_start = 13usize;
        let wkc_start = data_start + datagram_length as usize;

        if bytes.len() < wkc_start + 2 {
            return Err(Error::Parse(format!(
                "EtherCAT truncated: have {} bytes, need {} for data+WKC",
                bytes.len(),
                wkc_start + 2,
            )));
        }

        let data = if datagram_length > 0 {
            bytes[data_start..wkc_start].to_vec()
        } else {
            Vec::new()
        };

        let working_counter = u16::from_le_bytes([bytes[wkc_start], bytes[wkc_start + 1]]);

        Ok(EthercatFrame {
            eth_length,
            eth_type,
            datagram_cmd,
            datagram_index,
            addressing,
            datagram_length,
            data,
            working_counter,
        })
    }
}

/// EtherCAT connection state.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct EthercatState {
    pub connected: bool,
    pub slave_count: u16,
    pub operational: bool,
    pub last_ok_timestamp: u64,
    pub error_count: u64,
}

/// Validates EtherCAT connection health.
#[derive(Clone, Debug, Default)]
pub struct EthercatValidator;

impl EthercatValidator {
    pub fn new() -> Self {
        Self
    }
}

impl StateValidator for EthercatValidator {
    type State = EthercatState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("EtherCAT device not connected".into()));
        }
        if !state.operational {
            return Err(Error::State("EtherCAT not in OPERATIONAL state".into()));
        }
        if state.slave_count == 0 {
            return Err(Error::State(format!(
                "EtherCAT invalid slave count: {}",
                state.slave_count,
            )));
        }
        if state.error_count > 10 {
            return Err(Error::State(format!(
                "EtherCAT device has {} consecutive errors",
                state.error_count,
            )));
        }
        Ok(())
    }
}

/// Normalizes EtherCAT frames into TUP DataPoints.
#[derive(Clone, Debug, Default)]
pub struct EthercatNormalizer;

impl EthercatNormalizer {
    pub fn new() -> Self {
        Self
    }
}

impl DataNormalizer for EthercatNormalizer {
    type Frame = EthercatFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = String::from("ethercat:master");
        let point_id = format!("cmd:{}", frame.datagram_cmd);
        let timestamp = 0;

        let value = if !frame.data.is_empty() {
            TupValue::Bytes(frame.data.clone())
        } else {
            TupValue::Int(frame.working_counter as i64)
        };

        Ok(DataPoint::new(
            SourceProtocol::Ethercat,
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

    fn make_ethercat_frame(cmd: u8, idx: u8, addr: u32, data: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        // EtherCAT header
        let eth_length = (10 + data.len() + 2) as u16; // datagram header + data + WKC
        buf.extend_from_slice(&eth_length.to_be_bytes());
        buf.push(0x01); // type
                        // Datagram header
        buf.push(cmd);
        buf.push(idx);
        buf.extend_from_slice(&addr.to_be_bytes());
        buf.extend_from_slice(&(data.len() as u16).to_be_bytes());
        buf.extend_from_slice(&[0x00, 0x00]); // reserved
        buf.extend_from_slice(data);
        buf.extend_from_slice(&0x0000u16.to_le_bytes()); // WKC placeholder
        buf
    }

    #[test]
    fn valid_parse_fprd() {
        // FPRD (cmd=4) with some data
        let bytes = make_ethercat_frame(4, 0x01, 0x1000, &[0xAA, 0xBB]);
        let parser = EthercatParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.datagram_cmd, EcCmd::Fprd);
        assert_eq!(frame.datagram_index, 0x01);
        assert_eq!(frame.addressing, 0x1000);
        assert_eq!(frame.data, vec![0xAA, 0xBB]);
        assert_eq!(frame.working_counter, 0);
    }

    #[test]
    fn valid_parse_apwr() {
        let bytes = make_ethercat_frame(5, 0x02, 0x2000, &[0x01, 0x02, 0x03, 0x04]);
        let parser = EthercatParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.datagram_cmd, EcCmd::Apwr);
        assert_eq!(frame.datagram_index, 0x02);
        assert_eq!(frame.data.len(), 4);
    }

    #[test]
    fn invalid_too_short() {
        let parser = EthercatParser::new();
        let result = parser.parse(&[0x00, 0x01]);
        assert!(result.is_err());
    }

    #[test]
    fn truncated_datagram() {
        let parser = EthercatParser::new();
        // Only EtherCAT header + partial datagram
        let result = parser.parse(&[
            0x00, 0x0A, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn garbage_input() {
        let parser = EthercatParser::new();
        // Frames with datagram_length = 0 avoid truncation
        let mut buf = vec![0xFFu8; 15];
        buf[9] = 0x00;
        buf[10] = 0x00;
        let result = parser.parse(&buf);
        assert!(result.is_ok());
        let frame = result.unwrap();
        assert_eq!(frame.datagram_cmd, EcCmd::Unknown(0xFF));
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = EthercatParser::new();
        let normalizer = EthercatNormalizer::new();
        let bytes = make_ethercat_frame(1, 0x00, 0x0000, &[]);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Ethercat);
        assert_eq!(dp.device_id, "ethercat:master");
        assert_eq!(dp.point_id, "cmd:APRD");
    }

    #[test]
    fn validator_ok() {
        let validator = EthercatValidator::new();
        let state = EthercatState {
            connected: true,
            slave_count: 5,
            operational: true,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        let validator = EthercatValidator::new();
        let state = EthercatState::default();
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn validator_not_operational() {
        let validator = EthercatValidator::new();
        let state = EthercatState {
            connected: true,
            slave_count: 5,
            operational: false,
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
            let parser = EthercatParser::new();
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }
}
