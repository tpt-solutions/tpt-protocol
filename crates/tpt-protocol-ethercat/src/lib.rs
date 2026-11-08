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

/// EtherCAT datagram command (IEC 61158-4-12).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EcCmd {
    /// APRD (1): auto-increment physical read.
    Aprd,
    /// APWR (2): auto-increment physical write.
    Apwr,
    /// APRW (3): auto-increment physical read/write.
    Aprw,
    /// FPRD (4): configured-address physical read.
    Fprd,
    /// FPWR (5): configured-address physical write.
    Fpwr,
    /// FPRW (6): configured-address physical read/write.
    Fprw,
    /// BRD (7): broadcast read.
    Brd,
    /// BWR (8): broadcast write.
    Bwr,
    /// BRW (9): broadcast read/write.
    Brw,
    /// LRD (10): logical memory read.
    Lrd,
    /// LWR (11): logical memory write.
    Lwr,
    /// LRW (12): logical memory read/write.
    Lrw,
    /// ARMW (13): auto-increment read multiple write.
    Armw,
    /// FRMW (14): configured-address read multiple write.
    Frmw,
    /// Any other command code.
    Unknown(u8),
}

impl EcCmd {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Aprd,
            2 => Self::Apwr,
            3 => Self::Aprw,
            4 => Self::Fprd,
            5 => Self::Fpwr,
            6 => Self::Fprw,
            7 => Self::Brd,
            8 => Self::Bwr,
            9 => Self::Brw,
            10 => Self::Lrd,
            11 => Self::Lwr,
            12 => Self::Lrw,
            13 => Self::Armw,
            14 => Self::Frmw,
            other => Self::Unknown(other),
        }
    }
}

impl fmt::Display for EcCmd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Aprd => write!(f, "APRD"),
            Self::Apwr => write!(f, "APWR"),
            Self::Aprw => write!(f, "APRW"),
            Self::Fprd => write!(f, "FPRD"),
            Self::Fpwr => write!(f, "FPWR"),
            Self::Fprw => write!(f, "FPRW"),
            Self::Brd => write!(f, "BRD"),
            Self::Bwr => write!(f, "BWR"),
            Self::Brw => write!(f, "BRW"),
            Self::Lrd => write!(f, "LRD"),
            Self::Lwr => write!(f, "LWR"),
            Self::Lrw => write!(f, "LRW"),
            Self::Armw => write!(f, "ARMW"),
            Self::Frmw => write!(f, "FRMW"),
            Self::Unknown(v) => write!(f, "Unknown({v})"),
        }
    }
}

/// One datagram inside an EtherCAT frame.
#[derive(Clone, Debug, PartialEq)]
pub struct EcDatagram {
    pub cmd: EcCmd,
    pub index: u8,
    /// Auto-increment or configured station address (little-endian on the wire).
    pub adp: u16,
    /// Offset within the slave's memory, or the logical address for LRD/LWR (little-endian).
    pub ado: u16,
    /// Set when another datagram follows in this frame.
    pub more: bool,
    /// Set when the datagram is circulating (bit 14 of the length field).
    pub circulating: bool,
    pub irq: u16,
    pub data: Vec<u8>,
    pub working_counter: u16,
}

/// A parsed EtherCAT frame: the EtherCAT header and its datagram chain.
///
/// Expects the Ethernet payload, with the EtherType 0x88A4 already removed.
#[derive(Clone, Debug, PartialEq)]
pub struct EthercatFrame {
    /// Length in bytes of the datagram section (bits 0-10 of the header).
    pub eth_length: u16,
    /// Protocol type (bits 12-15 of the header). 1 means EtherCAT commands.
    pub eth_type: u8,
    pub datagrams: Vec<EcDatagram>,
}

impl fmt::Display for EthercatFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.datagrams.first() {
            Some(d) => write!(
                f,
                "EtherCAT[cmd={}, idx={}, datagrams={}, wkc={}]",
                d.cmd,
                d.index,
                self.datagrams.len(),
                d.working_counter,
            ),
            None => write!(f, "EtherCAT[no datagrams]"),
        }
    }
}

/// Datagram header: cmd, index, ADP, ADO, length/flags, IRQ. Then data and WKC.
const DATAGRAM_HEADER: usize = 10;
const WKC: usize = 2;
/// EtherCAT header (2 bytes) plus the datagram section length.
const HEADER: usize = 2;

/// Parses EtherCAT frames.
#[derive(Clone, Debug, Default)]
pub struct EthercatParser;

impl EthercatParser {
    pub fn new() -> Self {
        Self
    }
}

/// Decodes one frame from the front of `bytes`. Returns `Ok(None)` when more
/// bytes are needed. Trailing bytes after the EtherCAT section are Ethernet
/// padding and are left unconsumed.
fn decode_frame(bytes: &[u8]) -> Result<Option<(EthercatFrame, usize)>, Error> {
    if bytes.len() < HEADER {
        return Ok(None);
    }
    // EtherCAT header is little-endian: length (bits 0-10), reserved (bit 11), type (bits 12-15).
    let header = u16::from_le_bytes([bytes[0], bytes[1]]);
    let eth_length = header & 0x07FF;
    let eth_type = ((header >> 12) & 0x0F) as u8;
    if eth_type != 1 {
        return Err(Error::Parse(format!(
            "EtherCAT type {eth_type} is not an EtherCAT command frame (expected 1)"
        )));
    }
    let section_end = HEADER + eth_length as usize;
    if eth_length == 0 {
        return Err(Error::Parse("EtherCAT length field is zero".into()));
    }
    if bytes.len() < section_end {
        return Ok(None);
    }

    let mut datagrams = Vec::new();
    let mut pos = HEADER;
    loop {
        if pos + DATAGRAM_HEADER > section_end {
            return Err(Error::Parse(format!(
                "EtherCAT datagram header needs {} bytes but only {} remain",
                DATAGRAM_HEADER,
                section_end - pos,
            )));
        }
        let h = &bytes[pos..pos + DATAGRAM_HEADER];
        let cmd = EcCmd::from_u8(h[0]);
        let index = h[1];
        let adp = u16::from_le_bytes([h[2], h[3]]);
        let ado = u16::from_le_bytes([h[4], h[5]]);
        let flags = u16::from_le_bytes([h[6], h[7]]);
        let data_len = (flags & 0x07FF) as usize;
        let circulating = flags & 0x4000 != 0;
        let more = flags & 0x8000 != 0;
        let irq = u16::from_le_bytes([h[8], h[9]]);

        let data_start = pos + DATAGRAM_HEADER;
        let wkc_start = data_start + data_len;
        let datagram_end = wkc_start + WKC;
        if datagram_end > section_end {
            return Err(Error::Parse(format!(
                "EtherCAT datagram of {data_len} data bytes overruns the {eth_length}-byte section"
            )));
        }
        let working_counter = u16::from_le_bytes([bytes[wkc_start], bytes[wkc_start + 1]]);
        datagrams.push(EcDatagram {
            cmd,
            index,
            adp,
            ado,
            more,
            circulating,
            irq,
            data: bytes[data_start..wkc_start].to_vec(),
            working_counter,
        });
        pos = datagram_end;
        if !more {
            break;
        }
        if pos == section_end {
            return Err(Error::Parse(
                "EtherCAT datagram sets 'more follows' but no datagram follows".into(),
            ));
        }
    }

    if pos != section_end {
        return Err(Error::Parse(format!(
            "EtherCAT section is {eth_length} bytes but its datagrams end at {}",
            pos - HEADER,
        )));
    }

    Ok(Some((
        EthercatFrame {
            eth_length,
            eth_type,
            datagrams,
        },
        section_end,
    )))
}

impl FrameParser for EthercatParser {
    type Frame = EthercatFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        match decode_frame(bytes)? {
            Some((frame, _consumed)) => Ok(frame),
            None => Err(Error::Parse(format!(
                "EtherCAT frame truncated: have {} bytes",
                bytes.len()
            ))),
        }
    }

    fn try_parse(&self, bytes: &[u8]) -> Result<Option<(Self::Frame, usize)>, Error> {
        decode_frame(bytes)
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

/// Normalizes EtherCAT frames into TUP DataPoints. Uses the first datagram.
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
        let datagram = frame
            .datagrams
            .first()
            .ok_or_else(|| Error::Normalize("EtherCAT frame has no datagrams".into()))?;
        let device_id = String::from("ethercat:master");
        let point_id = format!("cmd:{}", datagram.cmd);
        let timestamp = 0;

        let value = if !datagram.data.is_empty() {
            TupValue::Bytes(datagram.data.clone())
        } else {
            TupValue::Int(datagram.working_counter as i64)
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

    /// One datagram: header, data, and a working counter, all little-endian.
    fn datagram(cmd: u8, idx: u8, adp: u16, ado: u16, data: &[u8], more: bool) -> Vec<u8> {
        let mut out = vec![cmd, idx];
        out.extend_from_slice(&adp.to_le_bytes());
        out.extend_from_slice(&ado.to_le_bytes());
        let flags = data.len() as u16 | if more { 0x8000 } else { 0 };
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&[0x00, 0x00]); // IRQ
        out.extend_from_slice(data);
        out.extend_from_slice(&0x0003u16.to_le_bytes()); // WKC = 3
        out
    }

    /// EtherCAT frame: little-endian header with length and type 1, then datagrams.
    fn frame(datagrams: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = datagrams.concat();
        let header = (body.len() as u16 & 0x07FF) | (1 << 12);
        let mut out = header.to_le_bytes().to_vec();
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn command_table_matches_iec_61158() {
        assert_eq!(EcCmd::from_u8(1), EcCmd::Aprd);
        assert_eq!(EcCmd::from_u8(2), EcCmd::Apwr);
        assert_eq!(EcCmd::from_u8(4), EcCmd::Fprd);
        assert_eq!(EcCmd::from_u8(5), EcCmd::Fpwr);
        assert_eq!(EcCmd::from_u8(7), EcCmd::Brd);
        assert_eq!(EcCmd::from_u8(8), EcCmd::Bwr);
        assert_eq!(EcCmd::from_u8(10), EcCmd::Lrd);
        assert_eq!(EcCmd::from_u8(11), EcCmd::Lwr);
        assert_eq!(EcCmd::from_u8(12), EcCmd::Lrw);
    }

    #[test]
    fn header_is_little_endian_with_eleven_bit_length() {
        // Length 12 and type 1: header value 0x100C, sent low byte first.
        let bytes = frame(&[datagram(1, 0, 0, 0, &[], false)]);
        assert_eq!(&bytes[..2], &[0x0C, 0x10]);
        let parsed = EthercatParser::new().parse(&bytes).unwrap();
        assert_eq!(parsed.eth_length, 12);
        assert_eq!(parsed.eth_type, 1);
    }

    #[test]
    fn single_datagram_fields_are_little_endian() {
        let bytes = frame(&[datagram(4, 0x01, 0x1000, 0x0A0B, &[0xAA, 0xBB], false)]);
        let parsed = EthercatParser::new().parse(&bytes).unwrap();
        assert_eq!(parsed.datagrams.len(), 1);
        let d = &parsed.datagrams[0];
        assert_eq!(d.cmd, EcCmd::Fprd);
        assert_eq!(d.index, 0x01);
        assert_eq!(d.adp, 0x1000);
        assert_eq!(d.ado, 0x0A0B);
        assert_eq!(d.data, vec![0xAA, 0xBB]);
        assert_eq!(d.working_counter, 3);
        assert!(!d.more);
    }

    #[test]
    fn multiple_datagrams_are_chained() {
        let bytes = frame(&[
            datagram(10, 0x01, 0, 0x0000, &[1, 2, 3, 4], true),
            datagram(2, 0x02, 0x0001, 0x0130, &[0xFF], false),
        ]);
        let parsed = EthercatParser::new().parse(&bytes).unwrap();
        assert_eq!(parsed.datagrams.len(), 2);
        assert_eq!(parsed.datagrams[0].cmd, EcCmd::Lrd);
        assert!(parsed.datagrams[0].more);
        assert_eq!(parsed.datagrams[1].cmd, EcCmd::Apwr);
        assert_eq!(parsed.datagrams[1].data, vec![0xFF]);
    }

    #[test]
    fn more_flag_without_following_datagram_is_rejected() {
        let bytes = frame(&[datagram(1, 0, 0, 0, &[], true)]);
        assert!(EthercatParser::new().parse(&bytes).is_err());
    }

    #[test]
    fn section_length_must_match_the_datagram_chain() {
        // Header claims 4 extra bytes that no datagram accounts for.
        let mut bytes = frame(&[datagram(1, 0, 0, 0, &[], false)]);
        bytes.extend_from_slice(&[0x00; 4]);
        let header = (12u16 + 4) | (1 << 12);
        bytes[..2].copy_from_slice(&header.to_le_bytes());
        assert!(EthercatParser::new().parse(&bytes).is_err());
    }

    #[test]
    fn trailing_ethernet_padding_is_ignored() {
        let mut bytes = frame(&[datagram(1, 0, 0, 0, &[], false)]);
        bytes.extend_from_slice(&[0x00; 20]);
        let (parsed, consumed) = EthercatParser::new().try_parse(&bytes).unwrap().unwrap();
        assert_eq!(parsed.datagrams.len(), 1);
        assert_eq!(consumed, 14);
    }

    #[test]
    fn wrong_type_is_rejected() {
        let mut bytes = frame(&[datagram(1, 0, 0, 0, &[], false)]);
        bytes[1] = 0x20; // type 2
        assert!(EthercatParser::new().parse(&bytes).is_err());
    }

    #[test]
    fn incomplete_frame_needs_more_data() {
        let bytes = frame(&[datagram(1, 0, 0, 0, &[1, 2, 3], false)]);
        let short = &bytes[..bytes.len() - 1];
        assert_eq!(EthercatParser::new().try_parse(short).unwrap(), None);
        assert!(EthercatParser::new().parse(short).is_err());
    }

    #[test]
    fn datagram_overrunning_section_is_rejected() {
        // Data length 50 in a 12-byte section.
        let mut bytes = frame(&[datagram(1, 0, 0, 0, &[], false)]);
        bytes[2 + 6] = 50; // low byte of the first datagram's length field
        assert!(EthercatParser::new().parse(&bytes).is_err());
    }

    #[test]
    fn invalid_too_short() {
        assert!(EthercatParser::new().parse(&[0x00, 0x01]).is_err());
    }

    #[test]
    fn garbage_input_is_rejected() {
        assert!(EthercatParser::new().parse(&[0xFFu8; 15]).is_err());
    }

    #[test]
    fn normalize_uses_first_datagram() {
        let bytes = frame(&[datagram(2, 0x00, 0x0000, 0x0000, &[0x34, 0x12], false)]);
        let parsed = EthercatParser::new().parse(&bytes).unwrap();
        let dp = EthercatNormalizer::new().normalize(&parsed).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Ethercat);
        assert_eq!(dp.device_id, "ethercat:master");
        assert_eq!(dp.point_id, "cmd:APWR");
        assert_eq!(dp.value, TupValue::Bytes(vec![0x34, 0x12]));
    }

    #[test]
    fn validator_ok() {
        let state = EthercatState {
            connected: true,
            slave_count: 5,
            operational: true,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(EthercatValidator::new().validate(&state).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        assert!(EthercatValidator::new()
            .validate(&EthercatState::default())
            .is_err());
    }

    #[test]
    fn validator_not_operational() {
        let state = EthercatState {
            connected: true,
            slave_count: 5,
            operational: false,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(EthercatValidator::new().validate(&state).is_err());
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
