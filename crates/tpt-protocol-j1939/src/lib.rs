//! SAE J1939 protocol implementation for heavy-duty vehicle networks.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// J1939 Parameter Group Number (PGN) field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pgn {
    /// Torque/Speed Control 1 (PGN 0; PDU1, so the destination address is not part of the PGN)
    Tc1,
    /// Electronic Engine Controller 1 (PGN 61444 / 0xF004)
    Eec1,
    /// Engine Temperature 1 (PGN 65262 / 0xFEEE)
    EngineTemp1,
    /// Fuel Economy (PGN 65257 / 0xFEE9)
    FuelEconomy,
    /// Cruise Control / Vehicle Speed (PGN 65265 / 0xFEF1)
    Ccvs,
    /// Ambient Conditions (PGN 65269 / 0xFEF5)
    AmbientConditions,
    /// Any unrecognized PGN
    Raw(u32),
}

impl Pgn {
    fn from_u32(v: u32) -> Self {
        match v {
            0x0000 => Self::Tc1,
            0xF004 => Self::Eec1,
            0xFEEE => Self::EngineTemp1,
            0xFEE9 => Self::FuelEconomy,
            0xFEF1 => Self::Ccvs,
            0xFEF5 => Self::AmbientConditions,
            other => Self::Raw(other),
        }
    }
}

impl fmt::Display for Pgn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tc1 => write!(f, "TC1"),
            Self::Eec1 => write!(f, "EEC1"),
            Self::EngineTemp1 => write!(f, "EngineTemp1"),
            Self::FuelEconomy => write!(f, "FuelEconomy"),
            Self::Ccvs => write!(f, "CCVS"),
            Self::AmbientConditions => write!(f, "AmbientConditions"),
            Self::Raw(v) => write!(f, "Raw(0x{v:06X})"),
        }
    }
}

/// A parsed J1939 CAN frame.
#[derive(Clone, Debug, PartialEq)]
pub struct J1939Frame {
    /// 29-bit CAN identifier
    pub can_id: u32,
    /// Priority (bits 26-28)
    pub priority: u8,
    /// Extended Data Page
    pub edp: u8,
    /// Data Page
    pub dp: u8,
    /// PDU Format (byte 2 of CAN ID)
    pub pf: u8,
    /// PDU Specific (byte 1 of CAN ID)
    pub ps: u8,
    /// Source Address (byte 0 of CAN ID)
    pub sa: u8,
    /// PGN
    pub pgn: Pgn,
    /// Payload (0-8 bytes)
    pub payload: Vec<u8>,
}

impl fmt::Display for J1939Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "J1939[can_id=0x{:08X}, PGN={}, SA=0x{:02X}, {} bytes]",
            self.can_id,
            self.pgn,
            self.sa,
            self.payload.len(),
        )
    }
}

/// J1939 frame parser.
#[derive(Clone, Debug, Default)]
pub struct J1939Parser;

impl J1939Parser {
    pub fn new() -> Self {
        Self
    }
}

impl FrameParser for J1939Parser {
    type Frame = J1939Frame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 4 {
            return Err(Error::Parse(format!(
                "J1939 frame too short: {} bytes, need at least 4",
                bytes.len()
            )));
        }

        let raw_id = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);

        let priority = ((raw_id >> 26) & 0x07) as u8;
        let edp = ((raw_id >> 25) & 0x01) as u8;
        let dp = ((raw_id >> 24) & 0x01) as u8;
        let pf = ((raw_id >> 16) & 0xFF) as u8;
        let ps = ((raw_id >> 8) & 0xFF) as u8;
        let sa = (raw_id & 0xFF) as u8;

        // PGN = EDP, DP, PF, and PS only for PDU2 (PF >= 240). For PDU1, PS is the
        // destination address and is not part of the PGN.
        let ps_part = if pf >= 240 { ps as u32 } else { 0 };
        let pgn_val = ((edp as u32) << 17) | ((dp as u32) << 16) | ((pf as u32) << 8) | ps_part;
        let pgn = Pgn::from_u32(pgn_val);

        let payload = if bytes.len() > 4 {
            bytes[4..].to_vec()
        } else {
            Vec::new()
        };

        if payload.len() > 8 {
            return Err(Error::Parse(format!(
                "J1939 payload too large: {} bytes, max 8",
                payload.len()
            )));
        }

        Ok(J1939Frame {
            can_id: raw_id,
            priority,
            edp,
            dp,
            pf,
            ps,
            sa,
            pgn,
            payload,
        })
    }

    fn try_parse(&self, bytes: &[u8]) -> Result<Option<(Self::Frame, usize)>, Error> {
        if bytes.len() < 4 {
            return Ok(None);
        }
        self.parse(bytes).map(|f| Some((f, bytes.len())))
    }
}

/// J1939 connection state.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct J1939State {
    pub connected: bool,
    pub source_address: u8,
    pub last_ok_timestamp: u64,
    pub error_count: u64,
}

/// Validates J1939 connection health.
#[derive(Clone, Debug, Default)]
pub struct J1939Validator;

impl J1939Validator {
    pub fn new() -> Self {
        Self
    }
}

impl StateValidator for J1939Validator {
    type State = J1939State;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("J1939 device not connected".into()));
        }
        if state.source_address == 0xFF {
            return Err(Error::State(
                "J1939 invalid source address 0xFF (global)".into(),
            ));
        }
        if state.error_count > 20 {
            return Err(Error::State(format!(
                "J1939 device has {} consecutive errors",
                state.error_count,
            )));
        }
        Ok(())
    }
}

/// Normalizes J1939 frames into TUP DataPoints.
#[derive(Clone, Debug, Default)]
pub struct J1939Normalizer;

impl J1939Normalizer {
    pub fn new() -> Self {
        Self
    }
}

impl DataNormalizer for J1939Normalizer {
    type Frame = J1939Frame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = format!("j1939:0x{:02X}", frame.sa);
        let point_id = format!("pgn:{}", frame.pgn);
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
            // No payload means no measurement; zero would read as a real value.
            return Err(Error::Normalize(format!(
                "j1939 pgn {:?} carries no data",
                frame.pgn
            )));
        };

        Ok(DataPoint::new(
            SourceProtocol::J1939,
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

    fn make_j1939_frame(pf: u8, ps: u8, sa: u8, payload: &[u8]) -> Vec<u8> {
        let can_id = (0x03u32 << 26) | ((pf as u32) << 16) | ((ps as u32) << 8) | (sa as u32);
        let mut buf = Vec::with_capacity(4 + payload.len());
        buf.extend_from_slice(&can_id.to_be_bytes());
        buf.extend_from_slice(payload);
        buf
    }

    #[test]
    fn valid_parse_eec1() {
        // EEC1: PF=0xF0, PS=0x04 (for PGN 61444), SA=0
        let payload = &[0x00, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let bytes = make_j1939_frame(0xF0, 0x04, 0x00, payload);
        let parser = J1939Parser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.pgn, Pgn::Eec1);
        assert_eq!(frame.sa, 0x00);
        assert_eq!(frame.priority, 3);
        assert_eq!(frame.payload.len(), 8);
    }

    #[test]
    fn valid_parse_tc1() {
        // TC1 is PGN 0 (PDU1). PS = 0x17 is the destination address, not part of the PGN.
        let payload = &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let bytes = make_j1939_frame(0x00, 0x17, 0x11, payload);
        let parser = J1939Parser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.pgn, Pgn::Tc1);
        assert_eq!(frame.sa, 0x11);
    }

    #[test]
    fn pdu1_destination_address_is_not_part_of_the_pgn() {
        // Request PGN 0xEA00 (PDU1): different destination addresses, same PGN.
        for ps in [0x00u8, 0x25, 0xFF] {
            let frame = J1939Parser::new()
                .parse(&make_j1939_frame(0xEA, ps, 0x10, &[0; 3]))
                .unwrap();
            assert_eq!(frame.pgn, Pgn::Raw(0xEA00), "PS 0x{ps:02X}");
        }
    }

    #[test]
    fn edp_bit_is_part_of_the_pgn() {
        // Same PF/PS as CCVS, but with EDP set: PGN 0x2FEF1 (EDP is PGN bit 17), not CCVS.
        let mut can_id = (0x03u32 << 26) | (1 << 25) | (0xFE << 16) | (0xF1 << 8) | 0x10;
        let bytes = can_id.to_be_bytes();
        assert_eq!(
            J1939Parser::new().parse(&bytes).unwrap().pgn,
            Pgn::Raw(0x2FEF1)
        );
        can_id &= !(1 << 25); // EDP clear: CCVS
        let frame = J1939Parser::new().parse(&can_id.to_be_bytes()).unwrap();
        assert_eq!(frame.pgn, Pgn::Ccvs);
        assert_eq!(frame.edp, 0);
    }

    #[test]
    fn ambient_conditions_pgn_is_named() {
        let frame = J1939Parser::new()
            .parse(&make_j1939_frame(0xFE, 0xF5, 0x10, &[0; 8]))
            .unwrap();
        assert_eq!(frame.pgn, Pgn::AmbientConditions);
    }

    #[test]
    fn invalid_too_short() {
        let parser = J1939Parser::new();
        let result = parser.parse(&[0x00, 0x01]);
        assert!(result.is_err());
    }

    #[test]
    fn truncated_can_id() {
        let parser = J1939Parser::new();
        let result = parser.parse(&[0x00, 0x01, 0x02]);
        assert!(result.is_err());
    }

    #[test]
    fn garbage_input() {
        let parser = J1939Parser::new();
        let result = parser.parse(&[0xFF, 0xFF, 0xFF, 0xFF, 0xDE, 0xAD, 0xBE, 0xEF]);
        assert!(result.is_ok());
        let frame = result.unwrap();
        assert_eq!(frame.sa, 0xFF);
        assert_eq!(frame.payload.len(), 4);
    }

    #[test]
    fn payload_too_large() {
        let parser = J1939Parser::new();
        let result = parser.parse(&[
            0x00, 0x01, 0x02, 0x03, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09,
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_reads_little_endian_payload() {
        // SAE J1939 multi-byte fields are little-endian: [0x01, 0x02] means 0x0201.
        let parser = J1939Parser::new();
        let normalizer = J1939Normalizer::new();
        let bytes = make_j1939_frame(0xF0, 0x04, 0x12, &[0x01, 0x02, 0x00, 0x00, 0, 0, 0, 0]);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.value, TupValue::Int(0x0201));
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = J1939Parser::new();
        let normalizer = J1939Normalizer::new();
        let payload = &[0x00, 0x00, 0x00, 0x64, 0x00, 0x00, 0x00, 0x00];
        let bytes = make_j1939_frame(0xF0, 0x04, 0x12, payload);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::J1939);
        assert_eq!(dp.device_id, "j1939:0x12");
        assert_eq!(dp.point_id, "pgn:EEC1");
    }

    #[test]
    fn validator_ok() {
        let validator = J1939Validator::new();
        let state = J1939State {
            connected: true,
            source_address: 0x12,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        let validator = J1939Validator::new();
        let state = J1939State::default();
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn validator_global_address() {
        let validator = J1939Validator::new();
        let state = J1939State {
            connected: true,
            source_address: 0xFF,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn validator_too_many_errors() {
        let validator = J1939Validator::new();
        let state = J1939State {
            connected: true,
            source_address: 0x01,
            last_ok_timestamp: 1000,
            error_count: 21,
        };
        assert!(validator.validate(&state).is_err());
    }

    #[test]
    fn try_parse_empty() {
        let parser = J1939Parser::new();
        let result = parser.try_parse(&[]).unwrap();
        assert!(result.is_none());
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let parser = J1939Parser::new();
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }
}
