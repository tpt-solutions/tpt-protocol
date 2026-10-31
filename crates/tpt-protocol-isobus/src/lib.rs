//! ISO 11783 (ISOBUS) protocol implementation for agricultural vehicle electronics.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// ISOBUS (ISO 11783) Parameter Group Number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IsobusPgn {
    /// Request for Rep (PGN 60416 / 0xEC00)
    RequestForRep,
    /// Power Take-Off (PGN 65202 / 0xFEBA)
    Pto,
    /// Working Set Master (PGN 65057 / 0xFE12)
    WorkingSetMaster,
    /// Task Controller (PGN 65195 / 0xFEAB)
    TaskController,
    /// Any unrecognized PGN
    Raw(u32),
}

impl IsobusPgn {
    fn from_u32(v: u32) -> Self {
        match v {
            0xEC00 => Self::RequestForRep,
            0xFEBA => Self::Pto,
            0xFE12 => Self::WorkingSetMaster,
            0xFEAB => Self::TaskController,
            other => Self::Raw(other),
        }
    }
}

impl fmt::Display for IsobusPgn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RequestForRep => write!(f, "RequestForRep"),
            Self::Pto => write!(f, "PTO"),
            Self::WorkingSetMaster => write!(f, "WorkingSetMaster"),
            Self::TaskController => write!(f, "TaskController"),
            Self::Raw(v) => write!(f, "Raw(0x{v:06X})"),
        }
    }
}

/// A parsed ISOBUS CAN frame (same 29-bit CAN ID structure as J1939).
#[derive(Clone, Debug, PartialEq)]
pub struct IsobusFrame {
    pub can_id: u32,
    pub priority: u8,
    pub dp: u8,
    pub pf: u8,
    pub ps: u8,
    pub sa: u8,
    pub pgn: IsobusPgn,
    pub payload: Vec<u8>,
    pub implement_id: u16,
    pub device_class: u8,
}

impl fmt::Display for IsobusFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ISOBUS[can_id=0x{:08X}, PGN={}, SA=0x{:02X}, impl_id=0x{:04X}, dev_class={}]",
            self.can_id, self.pgn, self.sa, self.implement_id, self.device_class,
        )
    }
}

/// ISOBUS frame parser.
#[derive(Clone, Debug, Default)]
pub struct IsobusParser;

impl IsobusParser {
    pub fn new() -> Self {
        Self
    }
}

impl FrameParser for IsobusParser {
    type Frame = IsobusFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 4 {
            return Err(Error::Parse(format!(
                "ISOBUS frame too short: {} bytes, need at least 4",
                bytes.len(),
            )));
        }

        let raw_id = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);

        let priority = ((raw_id >> 26) & 0x07) as u8;
        let dp = ((raw_id >> 24) & 0x01) as u8;
        let pf = ((raw_id >> 16) & 0xFF) as u8;
        let ps = ((raw_id >> 8) & 0xFF) as u8;
        let sa = (raw_id & 0xFF) as u8;

        let pgn_val = if pf >= 240 {
            ((dp as u32) << 16) | ((pf as u32) << 8) | (ps as u32)
        } else {
            ((dp as u32) << 16) | ((pf as u32) << 8)
        };
        let pgn = IsobusPgn::from_u32(pgn_val);

        let payload = if bytes.len() > 4 {
            bytes[4..].to_vec()
        } else {
            Vec::new()
        };

        if payload.len() > 8 {
            return Err(Error::Parse(format!(
                "ISOBUS payload too large: {} bytes, max 8",
                payload.len(),
            )));
        }

        // Extract implement_id from payload bytes 0-1, device_class from byte 2 (if available)
        let implement_id = if payload.len() >= 2 {
            u16::from_be_bytes([payload[0], payload[1]])
        } else {
            0
        };
        let device_class = if payload.len() >= 3 { payload[2] } else { 0 };

        Ok(IsobusFrame {
            can_id: raw_id,
            priority,
            dp,
            pf,
            ps,
            sa,
            pgn,
            payload,
            implement_id,
            device_class,
        })
    }
}

/// ISOBUS connection state.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct IsobusState {
    pub connected: bool,
    pub source_address: u8,
    pub implement_id: u16,
    pub last_ok_timestamp: u64,
    pub error_count: u64,
}

/// Validates ISOBUS connection health.
#[derive(Clone, Debug, Default)]
pub struct IsobusValidator;

impl IsobusValidator {
    pub fn new() -> Self {
        Self
    }
}

impl StateValidator for IsobusValidator {
    type State = IsobusState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("ISOBUS device not connected".into()));
        }
        if state.source_address == 0xFE || state.source_address == 0xFF {
            return Err(Error::State(format!(
                "ISOBUS invalid source address 0x{:02X}",
                state.source_address,
            )));
        }
        if state.implement_id == 0 {
            return Err(Error::State("ISOBUS implement_id must be non-zero".into()));
        }
        if state.error_count > 15 {
            return Err(Error::State(format!(
                "ISOBUS device has {} consecutive errors",
                state.error_count,
            )));
        }
        Ok(())
    }
}

/// Normalizes ISOBUS frames into TUP DataPoints.
#[derive(Clone, Debug, Default)]
pub struct IsobusNormalizer;

impl IsobusNormalizer {
    pub fn new() -> Self {
        Self
    }
}

impl DataNormalizer for IsobusNormalizer {
    type Frame = IsobusFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = format!("isobus:0x{:02X}:0x{:04X}", frame.sa, frame.implement_id);
        let point_id = format!("pgn:{}", frame.pgn);
        let timestamp = 0;

        let value = if frame.payload.len() >= 4 {
            let raw = u32::from_be_bytes([
                frame.payload[0],
                frame.payload[1],
                frame.payload[2],
                frame.payload[3],
            ]);
            TupValue::Int(raw as i64)
        } else if !frame.payload.is_empty() {
            TupValue::Bytes(frame.payload.clone())
        } else {
            TupValue::Int(0)
        };

        Ok(DataPoint::new(
            SourceProtocol::Isobus,
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

    fn make_isobus_frame(pf: u8, ps: u8, sa: u8, payload: &[u8]) -> Vec<u8> {
        let can_id = (0x03u32 << 26) | ((pf as u32) << 16) | ((ps as u32) << 8) | (sa as u32);
        let mut buf = Vec::with_capacity(4 + payload.len());
        buf.extend_from_slice(&can_id.to_be_bytes());
        buf.extend_from_slice(payload);
        buf
    }

    #[test]
    fn valid_parse_request_for_rep() {
        // PGN 60416 = 0xEC00 → PF=0xEC, PS=0x00
        let bytes = make_isobus_frame(
            0xEC,
            0x00,
            0x20,
            &[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07],
        );
        let parser = IsobusParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.pgn, IsobusPgn::RequestForRep);
        assert_eq!(frame.sa, 0x20);
        assert_eq!(frame.implement_id, 0x0001);
        assert_eq!(frame.device_class, 0x02);
    }

    #[test]
    fn valid_parse_pto() {
        let bytes = make_isobus_frame(0xFE, 0xBA, 0x15, &[0x10, 0x20]);
        let parser = IsobusParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.pgn, IsobusPgn::Pto);
        assert_eq!(frame.sa, 0x15);
        assert_eq!(frame.implement_id, 0x1020);
    }

    #[test]
    fn invalid_too_short() {
        let parser = IsobusParser::new();
        let result = parser.parse(&[0x00, 0x01]);
        assert!(result.is_err());
    }

    #[test]
    fn payload_too_large() {
        let parser = IsobusParser::new();
        let result = parser.parse(&[
            0x00, 0x01, 0x02, 0x03, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09,
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn garbage_input() {
        let parser = IsobusParser::new();
        let result = parser.parse(&[0xFF, 0xFF, 0xFF, 0xFF]);
        assert!(result.is_ok());
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = IsobusParser::new();
        let normalizer = IsobusNormalizer::new();
        let bytes = make_isobus_frame(0xEC, 0x00, 0x30, &[0x00, 0x0A, 0x03]);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Isobus);
        assert_eq!(dp.device_id, "isobus:0x30:0x000A");
        assert_eq!(dp.point_id, "pgn:RequestForRep");
    }

    #[test]
    fn validator_ok() {
        let validator = IsobusValidator::new();
        let state = IsobusState {
            connected: true,
            source_address: 0x20,
            implement_id: 0x1000,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        let validator = IsobusValidator::new();
        let state = IsobusState::default();
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn validator_zero_implement() {
        let validator = IsobusValidator::new();
        let state = IsobusState {
            connected: true,
            source_address: 0x20,
            implement_id: 0,
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
            let parser = IsobusParser::new();
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }
}
