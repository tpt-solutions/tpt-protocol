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
    /// Transport Protocol connection management, TP.CM (PGN 60416 / 0xEC00)
    TpCm,
    /// Transport Protocol data transfer, TP.DT (PGN 60160 / 0xEB00)
    TpDt,
    /// Request (PGN 59904 / 0xEA00)
    Request,
    /// Address Claimed (PGN 60928 / 0xEE00)
    AddressClaim,
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
            0xEC00 => Self::TpCm,
            0xEB00 => Self::TpDt,
            0xEA00 => Self::Request,
            0xEE00 => Self::AddressClaim,
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
            Self::TpCm => write!(f, "TP.CM"),
            Self::TpDt => write!(f, "TP.DT"),
            Self::Request => write!(f, "Request"),
            Self::AddressClaim => write!(f, "AddressClaim"),
            Self::Pto => write!(f, "PTO"),
            Self::WorkingSetMaster => write!(f, "WorkingSetMaster"),
            Self::TaskController => write!(f, "TaskController"),
            Self::Raw(v) => write!(f, "Raw(0x{v:06X})"),
        }
    }
}

/// The 64-bit NAME of an ECU, sent in an Address Claimed message (PGN 0xEE00).
/// Bit layout from ISO 11783-5, section 4.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IsobusName {
    pub raw: u64,
    /// Bits 0-20.
    pub identity_number: u32,
    /// Bits 21-31.
    pub manufacturer_code: u16,
    /// Bits 32-34.
    pub ecu_instance: u8,
    /// Bits 35-39.
    pub function_instance: u8,
    /// Bits 40-47.
    pub function: u8,
    /// Bits 49-55.
    pub device_class: u8,
    /// Bits 56-59.
    pub device_class_instance: u8,
    /// Bits 60-62.
    pub industry_group: u8,
    /// Bit 63.
    pub arbitrary_address_capable: bool,
}

impl IsobusName {
    /// Decodes an 8-byte NAME, little-endian.
    pub fn from_bytes(bytes: [u8; 8]) -> Self {
        let raw = u64::from_le_bytes(bytes);
        let field = |shift: u32, bits: u32| (raw >> shift) & ((1u64 << bits) - 1);
        Self {
            raw,
            identity_number: field(0, 21) as u32,
            manufacturer_code: field(21, 11) as u16,
            ecu_instance: field(32, 3) as u8,
            function_instance: field(35, 5) as u8,
            function: field(40, 8) as u8,
            device_class: field(49, 7) as u8,
            device_class_instance: field(56, 4) as u8,
            industry_group: field(60, 3) as u8,
            arbitrary_address_capable: field(63, 1) == 1,
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
    /// The ECU NAME, present only on Address Claimed frames (PGN 0xEE00).
    pub name: Option<IsobusName>,
}

impl fmt::Display for IsobusFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ISOBUS[can_id=0x{:08X}, PGN={}, SA=0x{:02X}, name={}]",
            self.can_id,
            self.pgn,
            self.sa,
            match self.name {
                Some(n) => format!("0x{:016X}", n.raw),
                None => "-".into(),
            },
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

        // PS is part of the PGN only for PDU2 (PF >= 240); for PDU1 it is the destination address.
        let ps_part = if pf >= 240 { ps as u32 } else { 0 };
        let pgn_val = ((dp as u32) << 16) | ((pf as u32) << 8) | ps_part;
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

        // The NAME is only defined for Address Claimed. Other PGNs carry no identity.
        let name = match (pgn, payload.as_slice()) {
            (IsobusPgn::AddressClaim, [b0, b1, b2, b3, b4, b5, b6, b7]) => {
                Some(IsobusName::from_bytes([
                    *b0, *b1, *b2, *b3, *b4, *b5, *b6, *b7,
                ]))
            }
            _ => None,
        };

        Ok(IsobusFrame {
            can_id: raw_id,
            priority,
            dp,
            pf,
            ps,
            sa,
            pgn,
            payload,
            name,
        })
    }
}

/// ISOBUS connection state.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct IsobusState {
    pub connected: bool,
    pub source_address: u8,
    /// The raw 64-bit NAME of the ECU. Zero means not yet claimed.
    pub ecu_name: u64,
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
        if state.ecu_name == 0 {
            return Err(Error::State(
                "ISOBUS ECU NAME is not set (device has not claimed an address)".into(),
            ));
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
        let device_id = match frame.name {
            Some(n) => format!(
                "isobus:0x{:02X}:{:03X}:{:06X}",
                frame.sa, n.manufacturer_code, n.identity_number
            ),
            None => format!("isobus:0x{:02X}", frame.sa),
        };
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
                "isobus pgn {:?} carries no data",
                frame.pgn
            )));
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
    fn valid_parse_tp_cm() {
        // PGN 60416 = 0xEC00 (TP.CM) → PF=0xEC, PS=0x00
        let bytes = make_isobus_frame(
            0xEC,
            0x00,
            0x20,
            &[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07],
        );
        let parser = IsobusParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.pgn, IsobusPgn::TpCm);
        assert_eq!(frame.sa, 0x20);
        assert_eq!(frame.name, None);
    }

    #[test]
    fn address_claimed_name_fields_follow_iso_11783() {
        // Build a NAME with each field at its bit position, then decode it.
        let raw: u64 = 0x12345 // identity number, bits 0-20
            | (0x0AB << 21) // manufacturer code, bits 21-31
            | (1 << 32) // ECU instance, bits 32-34
            | (2 << 35) // function instance, bits 35-39
            | (0x1F << 40) // function, bits 40-47
            | (0x02 << 49) // device class, bits 49-55
            | (3 << 56) // device class instance, bits 56-59
            | (2 << 60) // industry group, bits 60-62
            | (1 << 63); // arbitrary address capable, bit 63
        let name = IsobusName::from_bytes(raw.to_le_bytes());
        assert_eq!(name.identity_number, 0x12345);
        assert_eq!(name.manufacturer_code, 0x0AB);
        assert_eq!(name.ecu_instance, 1);
        assert_eq!(name.function_instance, 2);
        assert_eq!(name.function, 0x1F);
        assert_eq!(name.device_class, 0x02);
        assert_eq!(name.device_class_instance, 3);
        assert_eq!(name.industry_group, 2);
        assert!(name.arbitrary_address_capable);

        // Address Claimed (PGN 0xEE00): PF 0xEE, destination 0xFF, source 0x30.
        let bytes = make_isobus_frame(0xEE, 0xFF, 0x30, &raw.to_le_bytes());
        let frame = IsobusParser::new().parse(&bytes).unwrap();
        assert_eq!(frame.pgn, IsobusPgn::AddressClaim);
        assert_eq!(frame.name, Some(name));
        let dp = IsobusNormalizer::new().normalize(&frame).unwrap();
        assert_eq!(dp.device_id, "isobus:0x30:0AB:012345");
    }

    #[test]
    fn valid_parse_pto() {
        let bytes = make_isobus_frame(0xFE, 0xBA, 0x15, &[0x10, 0x20]);
        let parser = IsobusParser::new();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.pgn, IsobusPgn::Pto);
        assert_eq!(frame.sa, 0x15);
        assert_eq!(frame.name, None);
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
    fn normalize_reads_little_endian_payload() {
        // ISO 11783 multi-byte fields are little-endian: [0x01, 0x02] means 0x0201.
        let parser = IsobusParser::new();
        let normalizer = IsobusNormalizer::new();
        let bytes = make_isobus_frame(0xF0, 0x04, 0x30, &[0x01, 0x02, 0x00, 0x00]);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.value, TupValue::Int(0x0201));
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = IsobusParser::new();
        let normalizer = IsobusNormalizer::new();
        let bytes = make_isobus_frame(0xEC, 0x00, 0x30, &[0x00, 0x0A, 0x03]);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Isobus);
        assert_eq!(dp.device_id, "isobus:0x30");
        assert_eq!(dp.point_id, "pgn:TP.CM");
    }

    #[test]
    fn validator_ok() {
        let validator = IsobusValidator::new();
        let state = IsobusState {
            connected: true,
            source_address: 0x20,
            ecu_name: 0x1000,
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
            ecu_name: 0,
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
