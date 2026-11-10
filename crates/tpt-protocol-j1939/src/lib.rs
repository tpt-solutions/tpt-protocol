//! SAE J1939 protocol implementation for heavy-duty vehicle networks.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameEncoder, FrameParser, StateValidator};
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

/// Writes a [`J1939Frame`] as the CAN frame body the parser reads: the 32-bit
/// identifier (big-endian, written as stored in `can_id`), then up to eight
/// data bytes. The inverse of [`J1939Parser`].
#[derive(Clone, Copy, Debug, Default)]
pub struct J1939Encoder;

impl J1939Encoder {
    pub fn new() -> Self {
        Self
    }
}

impl FrameEncoder for J1939Encoder {
    type Frame = J1939Frame;

    fn encode(&self, frame: &Self::Frame) -> Result<Vec<u8>, Error> {
        if frame.payload.len() > 8 {
            return Err(Error::Parse(format!(
                "J1939 payload too large: {} bytes, max 8",
                frame.payload.len()
            )));
        }
        let mut out = Vec::with_capacity(4 + frame.payload.len());
        out.extend_from_slice(&frame.can_id.to_be_bytes());
        out.extend_from_slice(&frame.payload);
        Ok(out)
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

impl J1939Normalizer {
    /// The points a frame carries. Known PGNs give their named signals in their
    /// units, with the "not available" value of each signal left out. Other PGNs
    /// give their raw payload as bytes; a number is not guessed for them.
    pub fn points(&self, frame: &J1939Frame) -> Result<Vec<DataPoint>, Error> {
        if frame.payload.is_empty() {
            // No payload means no measurement; zero would read as a real value.
            return Err(Error::Normalize(format!(
                "j1939 pgn {:?} carries no data",
                frame.pgn
            )));
        }
        let device_id = format!("j1939:0x{:02X}", frame.sa);
        let d = frame.payload.as_slice();
        let mut out = Vec::new();
        let mut push = |name: &str, value: TupValue, unit: Option<&str>| {
            let mut dp = DataPoint::new(
                SourceProtocol::J1939,
                device_id.clone(),
                format!("pgn:{}.{name}", frame.pgn),
                0,
                value,
                Quality::Good,
            );
            dp.unit = unit.map(alloc::string::String::from);
            out.push(dp);
        };
        match frame.pgn {
            // EEC1 (PGN 61444): engine speed, SPN 190, bytes 4-5 (index 3-4), 0.125 rpm
            // per bit, little-endian, 0xFFFF not available.
            Pgn::Eec1 => {
                if let Some(raw) = le_u16(d, 3).filter(|&v| v != 0xFFFF) {
                    push(
                        "engine_speed",
                        TupValue::Float(f64::from(raw) * 0.125),
                        Some("rpm"),
                    );
                }
            }
            // ET1 (PGN 65262): engine coolant temperature, SPN 110, byte 1, 1 degC per bit
            // with a -40 offset, 0xFF not available.
            Pgn::EngineTemp1 => {
                if let Some(raw) = d.first().copied().filter(|&v| v != 0xFF) {
                    push(
                        "coolant_temp",
                        TupValue::Float(f64::from(raw) - 40.0),
                        Some("Cel"),
                    );
                }
            }
            // CCVS (PGN 65265): wheel-based vehicle speed, SPN 84, bytes 2-3 (index 1-2),
            // 1/256 km/h per bit, 0xFFFF not available.
            Pgn::Ccvs => {
                if let Some(raw) = le_u16(d, 1).filter(|&v| v != 0xFFFF) {
                    push(
                        "wheel_speed",
                        TupValue::Float(f64::from(raw) / 256.0),
                        Some("km/h"),
                    );
                }
            }
            // Fuel economy (PGN 65257): fuel rate, SPN 183, bytes 1-2 (index 0-1), 0.05 L/h
            // per bit, 0xFFFF not available.
            Pgn::FuelEconomy => {
                if let Some(raw) = le_u16(d, 0).filter(|&v| v != 0xFFFF) {
                    push(
                        "fuel_rate",
                        TupValue::Float(f64::from(raw) * 0.05),
                        Some("L/h"),
                    );
                }
            }
            _ => {
                push("payload", TupValue::Bytes(frame.payload.clone()), None);
            }
        }
        if out.is_empty() {
            return Err(Error::Normalize(format!(
                "j1939 pgn {:?} has no available signal",
                frame.pgn
            )));
        }
        Ok(out)
    }
}

/// Reads a little-endian u16 at `at`, or `None` when the payload is too short.
fn le_u16(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*data.get(at)?, *data.get(at + 1)?]))
}

impl DataNormalizer for J1939Normalizer {
    type Frame = J1939Frame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        self.points(frame)?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Normalize("j1939 frame has no point".into()))
    }

    fn normalize_all(&self, frame: &Self::Frame) -> Result<Vec<DataPoint>, Error> {
        self.points(frame)
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

    fn normalize_points(pf: u8, ps: u8, payload: &[u8]) -> Vec<DataPoint> {
        let bytes = make_j1939_frame(pf, ps, 0x12, payload);
        let frame = J1939Parser::new().parse(&bytes).unwrap();
        J1939Normalizer::new().normalize_all(&frame).unwrap()
    }

    #[test]
    fn eec1_engine_speed_is_spn_190_in_bytes_4_and_5() {
        // 0x0C80 = 3200 raw, 0.125 rpm per bit = 400 rpm. The first bytes are not speed.
        let points = normalize_points(
            0xF0,
            0x04,
            &[0xFF, 0xFF, 0xFF, 0x80, 0x0C, 0xFF, 0xFF, 0xFF],
        );
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].point_id, "pgn:EEC1.engine_speed");
        assert_eq!(points[0].unit.as_deref(), Some("rpm"));
        assert_eq!(points[0].value, TupValue::Float(400.0));
        assert_eq!(points[0].device_id, "j1939:0x12");
    }

    #[test]
    fn eec1_engine_speed_not_available_is_skipped() {
        // Everything is "not available", so the frame has no point and is an error.
        let bytes = make_j1939_frame(0xF0, 0x04, 0x12, &[0xFF; 8]);
        let frame = J1939Parser::new().parse(&bytes).unwrap();
        assert!(J1939Normalizer::new().normalize(&frame).is_err());
    }

    #[test]
    fn coolant_temperature_has_a_minus_40_offset() {
        // PGN 65262 (ET1), 0xFEEE: byte 1 is the coolant temperature, 1 degC per bit.
        let points = normalize_points(0xFE, 0xEE, &[0x6E, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(points[0].point_id, "pgn:EngineTemp1.coolant_temp");
        assert_eq!(points[0].value, TupValue::Float(70.0));
    }

    #[test]
    fn wheel_speed_is_in_one_256th_km_per_hour() {
        // PGN 65265 (CCVS), 0xFEF1: bytes 2-3 are the wheel speed.
        let points = normalize_points(0xFE, 0xF1, &[0x00, 0x0A, 0x00, 0, 0, 0, 0, 0]);
        assert_eq!(points[0].point_id, "pgn:CCVS.wheel_speed");
        assert_eq!(points[0].value, TupValue::Float(10.0 / 256.0));
    }

    #[test]
    fn fuel_rate_is_in_twentieths_of_a_litre() {
        // PGN 65257 (fuel economy), 0xFEE9: bytes 1-2 are the fuel rate, 0.05 L/h per bit.
        let points = normalize_points(0xFE, 0xE9, &[0x64, 0x00, 0, 0, 0, 0, 0, 0]);
        assert_eq!(points[0].point_id, "pgn:FuelEconomy.fuel_rate");
        assert_eq!(points[0].value, TupValue::Float(5.0));
    }

    #[test]
    fn unknown_pgns_give_their_payload_as_bytes() {
        let points = normalize_points(0xEA, 0x12, &[0x01, 0x02, 0x03]);
        assert_eq!(
            points[0].value,
            TupValue::Bytes(alloc::vec![0x01, 0x02, 0x03])
        );
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
        assert_eq!(dp.point_id, "pgn:EEC1.engine_speed");
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

    /// A J1939 CAN frame: the 29-bit identifier (priority, EDP, DP, PF, PS, source
    /// address) as four big-endian bytes, then up to 10 payload bytes. PF is drawn
    /// across the PDU1 (below 240) and PDU2 (240 and up) ranges, where the PGN rules
    /// differ, and some payloads run past 8 bytes to reach the length check.
    fn j1939_frame() -> impl Strategy<Value = Vec<u8>> {
        (
            0u32..8,
            0u32..2,
            0u32..2,
            prop_oneof![0u32..240, 240u32..=255],
            any::<u8>(),
            any::<u8>(),
            proptest::collection::vec(any::<u8>(), 0..11),
        )
            .prop_map(|(priority, edp, dp, pf, ps, sa, payload)| {
                let id = (priority << 26)
                    | (edp << 25)
                    | (dp << 24)
                    | (pf << 16)
                    | ((ps as u32) << 8)
                    | sa as u32;
                let mut out = id.to_be_bytes().to_vec();
                out.extend_from_slice(&payload);
                out
            })
    }

    /// One random byte overwrite, one truncation, or one extra byte appended.
    fn mutated_j1939_frame() -> impl Strategy<Value = Vec<u8>> {
        (j1939_frame(), any::<usize>(), any::<u8>(), 0u8..3).prop_map(
            |(mut bytes, idx, val, kind)| {
                if bytes.is_empty() {
                    return bytes;
                }
                let i = idx % bytes.len();
                match kind {
                    0 => bytes.truncate(i),
                    1 => bytes[i] = val,
                    _ => bytes.push(val),
                }
                bytes
            },
        )
    }

    proptest! {
        #[test]
        fn structured_j1939_frames_never_panic(bytes in mutated_j1939_frame()) {
            if let Ok(frame) = J1939Parser::new().parse(&bytes) {
                let _ = J1939Normalizer::new().normalize_all(&frame);
            }
        }
    }

    proptest! {
        /// The identifier and payload are kept exactly, so parse then encode
        /// gives back the input bytes.
        #[test]
        fn parsed_frames_encode_back_to_the_same_bytes(
            bytes in proptest::collection::vec(any::<u8>(), 4..16)
        ) {
            if let Ok(frame) = J1939Parser::new().parse(&bytes) {
                prop_assert_eq!(J1939Encoder::new().encode(&frame).unwrap(), bytes);
            }
        }

        #[test]
        fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let parser = J1939Parser::new();
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }
}
