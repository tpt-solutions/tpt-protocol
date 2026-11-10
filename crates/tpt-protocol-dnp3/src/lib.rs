//! DNP3 protocol implementation for electric utility SCADA systems.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameEncoder, FrameParser, StateValidator};
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
    Confirm,
    Response,
    UnsolicitedResponse,
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
            0 => Self::Confirm,
            129 => Self::Response,
            130 => Self::UnsolicitedResponse,
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
            Self::Confirm => write!(f, "Confirm"),
            Self::Response => write!(f, "Response"),
            Self::UnsolicitedResponse => write!(f, "UnsolicitedResponse"),
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
    /// True when this is the first fragment (FIR bit), which carries the application header.
    pub transport_first: bool,
    /// Application control byte (FIR, FIN, CON, UNS, SEQ). Zero when absent.
    pub application_control: u8,
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

/// Writes a [`Dnp3Frame`] as an IEEE 1815 data link frame. The CRCs are
/// computed: one over the 8 header bytes, and one per 16-byte block of user
/// data. User data is the transport byte, then the application control and
/// function code (first fragment only), then the payload.
///
/// `length` decides the frame shape: 5 is a link-layer-only frame with no user
/// data (the parser reports exactly that), and anything else must equal 5 plus
/// the user data length. `data_link_crc` is ignored, since it is recomputed.
#[derive(Clone, Copy, Debug, Default)]
pub struct Dnp3Encoder;

impl Dnp3Encoder {
    pub fn new() -> Self {
        Self
    }
}

fn function_code_value(function: Dnp3FunctionCode) -> u8 {
    match function {
        Dnp3FunctionCode::Confirm => 0,
        Dnp3FunctionCode::Read => 1,
        Dnp3FunctionCode::Write => 2,
        Dnp3FunctionCode::Select => 3,
        Dnp3FunctionCode::Operate => 4,
        Dnp3FunctionCode::DirectOperate => 5,
        Dnp3FunctionCode::DirectOperateNoAck => 6,
        Dnp3FunctionCode::Freeze => 7,
        Dnp3FunctionCode::FreezeNoAck => 8,
        Dnp3FunctionCode::FreezeClear => 9,
        Dnp3FunctionCode::FreezeClearNoAck => 10,
        Dnp3FunctionCode::Response => 129,
        Dnp3FunctionCode::UnsolicitedResponse => 130,
        Dnp3FunctionCode::Unknown(value) => value,
    }
}

impl FrameEncoder for Dnp3Encoder {
    type Frame = Dnp3Frame;

    fn encode(&self, frame: &Self::Frame) -> Result<Vec<u8>, Error> {
        if frame.start != 0x0564 {
            return Err(Error::Parse(format!(
                "DNP3 start bytes 0x{:04X} cannot be encoded, expected 0x0564",
                frame.start
            )));
        }

        let mut user = Vec::new();
        if frame.length == 5 {
            // Link-layer-only frame: no user data, so no transport or application header.
            if !frame.payload.is_empty()
                || frame.application_control != 0
                || frame.app_function != Dnp3FunctionCode::Unknown(0)
            {
                return Err(Error::Parse(
                    "DNP3 length 5 is a link-layer-only frame, but it has application data".into(),
                ));
            }
        } else {
            let transport = (if frame.transport_final { 0x80 } else { 0 })
                | (if frame.transport_first { 0x40 } else { 0 })
                | (frame.transport_seq & 0x3F);
            user.push(transport);
            if frame.transport_first {
                user.push(frame.application_control);
                user.push(function_code_value(frame.app_function));
            } else if frame.application_control != 0
                || frame.app_function != Dnp3FunctionCode::Unknown(0)
            {
                return Err(Error::Parse(
                    "DNP3 continuation fragments carry no application header".into(),
                ));
            }
            user.extend_from_slice(&frame.payload);

            let expected = 5 + user.len();
            if usize::from(frame.length) != expected || expected > 255 {
                return Err(Error::Parse(format!(
                    "DNP3 length {} does not match {} bytes of user data (length must be 5 plus that)",
                    frame.length,
                    user.len()
                )));
            }
        }

        let mut out = Vec::with_capacity(10 + user.len() + 2 * user.len().div_ceil(16));
        out.extend_from_slice(&0x0564u16.to_be_bytes());
        out.push(frame.length);
        out.push(frame.control);
        out.extend_from_slice(&frame.destination.to_le_bytes());
        out.extend_from_slice(&frame.source.to_le_bytes());
        let header_crc = crc16_dnp(&out[0..8]);
        out.extend_from_slice(&header_crc.to_le_bytes());
        for block in user.chunks(16) {
            out.extend_from_slice(block);
            out.extend_from_slice(&crc16_dnp(block).to_le_bytes());
        }
        Ok(out)
    }
}

/// DNP3 frame parser: IEEE 1815 data link layer, then transport and
/// application headers from the user data.
#[derive(Clone, Debug, Default)]
pub struct Dnp3Parser;

impl Dnp3Parser {
    pub fn new() -> Self {
        Self
    }
}

/// Decodes one frame from the front of `bytes`. Returns `Ok(None)` when more
/// bytes are needed, and `Ok(Some((frame, consumed)))` otherwise.
fn decode_frame(bytes: &[u8]) -> Result<Option<(Dnp3Frame, usize)>, Error> {
    if bytes.len() >= 2 {
        let start = u16::from_be_bytes([bytes[0], bytes[1]]);
        if start != 0x0564 {
            return Err(Error::Parse(format!(
                "DNP3 invalid start bytes: 0x{start:04X}, expected 0x0564",
            )));
        }
    }
    if bytes.len() < 10 {
        return Ok(None);
    }

    let length = bytes[2];
    if length < 5 {
        return Err(Error::Parse(format!(
            "DNP3 length {length} is below the minimum of 5 (control, destination, source)",
        )));
    }

    // The header CRC covers all eight bytes before it, start bytes included.
    let header_crc = u16::from_le_bytes([bytes[8], bytes[9]]);
    let computed = crc16_dnp(&bytes[0..8]);
    if computed != header_crc {
        return Err(Error::checksum(
            computed as u32,
            header_crc as u32,
            "DNP3 header CRC",
        ));
    }
    let control = bytes[3];
    let destination = u16::from_le_bytes([bytes[4], bytes[5]]);
    let source = u16::from_le_bytes([bytes[6], bytes[7]]);

    // User data follows in blocks of up to 16 bytes, each with its own CRC.
    let mut remaining = length as usize - 5;
    let mut user = Vec::with_capacity(remaining);
    let mut pos = 10usize;
    while remaining > 0 {
        let block = remaining.min(16);
        let data_end = pos + block;
        if bytes.len() < data_end + 2 {
            return Ok(None);
        }
        let got = u16::from_le_bytes([bytes[data_end], bytes[data_end + 1]]);
        let computed = crc16_dnp(&bytes[pos..data_end]);
        if computed != got {
            return Err(Error::checksum(
                computed as u32,
                got as u32,
                format!("DNP3 data block CRC at offset {pos}"),
            ));
        }
        user.extend_from_slice(&bytes[pos..data_end]);
        pos = data_end + 2;
        remaining -= block;
    }

    // Transport header: FIN (0x80), FIR (0x40), sequence (0x3F).
    let (transport_first, transport_final, transport_seq, rest) = match user.split_first() {
        Some((&th, rest)) => (th & 0x40 != 0, th & 0x80 != 0, th & 0x3F, rest),
        None => (true, true, 0, &user[..]),
    };

    // Application header (control, function code) is only present in the first
    // fragment. Continuation fragments are all data.
    let (application_control, app_function, payload) = if user.is_empty() {
        // Link-layer-only frame: no transport or application data.
        (0, Dnp3FunctionCode::Unknown(0), Vec::new())
    } else if transport_first {
        match rest {
            [control, function, data @ ..] => (
                *control,
                Dnp3FunctionCode::from_u8(*function),
                data.to_vec(),
            ),
            _ => {
                return Err(Error::Parse(
                    "DNP3 first fragment is too short for an application header".into(),
                ))
            }
        }
    } else {
        (0, Dnp3FunctionCode::Unknown(0), rest.to_vec())
    };

    Ok(Some((
        Dnp3Frame {
            start: 0x0564,
            length,
            control,
            destination,
            source,
            data_link_crc: header_crc,
            transport_final,
            transport_seq,
            transport_first,
            application_control,
            app_function,
            payload,
        },
        pos,
    )))
}

impl FrameParser for Dnp3Parser {
    type Frame = Dnp3Frame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        match decode_frame(bytes)? {
            None => Err(Error::Parse(format!(
                "DNP3 frame incomplete: have {} bytes",
                bytes.len(),
            ))),
            Some((frame, consumed)) if consumed == bytes.len() => Ok(frame),
            Some((_, consumed)) => Err(Error::Parse(format!(
                "DNP3 has {} trailing bytes after the frame",
                bytes.len() - consumed,
            ))),
        }
    }

    fn try_parse(&self, bytes: &[u8]) -> Result<Option<(Self::Frame, usize)>, Error> {
        decode_frame(bytes)
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
        // A request without data (such as a READ with no object headers) carries no
        // measured value, so there is no point to report. The data-link length is not
        // a measurement and is not reported as one.
        if frame.payload.is_empty() {
            return Err(Error::Normalize(format!(
                "dnp3 {} carries no data",
                frame.app_function
            )));
        }
        let device_id = format!("dnp3:{}", frame.source);
        let point_id = format!("func:{}", frame.app_function);
        Ok(DataPoint::new(
            SourceProtocol::Dnp3,
            device_id,
            point_id,
            0,
            TupValue::Bytes(frame.payload.clone()),
            Quality::Good,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn make_dnp3_frame(src: u16, dest: u16, ctrl: u8, func: u8, payload: &[u8]) -> Vec<u8> {
        // Transport byte 0xC0 (FIN + FIR, seq 0), then application control 0xC0.
        let mut user = vec![0xC0, 0xC0, func];
        user.extend_from_slice(payload);
        build_dnp3(dest, src, ctrl, &user)
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
        // A bare READ has no data, so it normalizes to an error, not a length.
        assert!(normalizer.normalize(&frame).is_err());

        // A response with data reports the data as bytes.
        let bytes = make_dnp3_frame(0x0123, 0x0A, 0x44, 0x81, &[0x00, 0x00, 0x1E, 0x02]);
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Dnp3);
        assert_eq!(dp.device_id, "dnp3:291");
        assert_eq!(dp.point_id, "func:Response");
        assert_eq!(dp.value, TupValue::Bytes(vec![0x00, 0x00, 0x1E, 0x02]));
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

    /// Builds a DNP3 frame from the IEEE 1815 layout: a 10-byte header with its
    /// CRC over bytes 0..8 (start bytes included), then user data in blocks of
    /// up to 16 bytes, each followed by its own CRC.
    fn build_dnp3(dest: u16, src: u16, ctrl: u8, user: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&[0x05, 0x64]);
        out.push((user.len() + 5) as u8);
        out.push(ctrl);
        out.extend_from_slice(&dest.to_le_bytes());
        out.extend_from_slice(&src.to_le_bytes());
        let header_crc = crc16_dnp(&out[0..8]);
        out.extend_from_slice(&header_crc.to_le_bytes());
        for chunk in user.chunks(16) {
            out.extend_from_slice(chunk);
            out.extend_from_slice(&crc16_dnp(chunk).to_le_bytes());
        }
        out
    }

    #[test]
    fn accepts_published_link_layer_vector() {
        // Link-layer-only frame from the DNP3 documentation examples: start,
        // length 5, control 0xC0, destination 1, source 0x0400, header CRC 0x21E9.
        let bytes = [0x05, 0x64, 0x05, 0xC0, 0x01, 0x00, 0x00, 0x04, 0xE9, 0x21];
        let frame = Dnp3Parser::new().parse(&bytes).unwrap();
        assert_eq!(frame.destination, 1);
        assert_eq!(frame.source, 0x0400); // bytes 00 04, little-endian
        assert!(frame.payload.is_empty());
    }

    #[test]
    fn crc_matches_standard_check_value() {
        // CRC-16/DNP check value for the ASCII string "123456789".
        assert_eq!(crc16_dnp(b"123456789"), 0xEA82);
    }

    #[test]
    fn header_crc_covers_start_bytes() {
        // A header CRC computed over bytes 2..8 (the old, wrong convention)
        // must be rejected.
        let mut bytes = build_dnp3(0x0A, 0x01, 0x44, &[0xC0, 0xC0, 0x01]);
        let wrong = crc16_dnp(&bytes[2..8]);
        bytes[8..10].copy_from_slice(&wrong.to_le_bytes());
        assert!(Dnp3Parser::new().parse(&bytes).is_err());
    }

    #[test]
    fn multi_block_user_data_is_reassembled() {
        // 35 bytes of user data = blocks of 16, 16 and 3, each with a CRC.
        let mut user = vec![0xC0, 0xC0, 0x81];
        user.extend((1u8..=32).collect::<Vec<u8>>());
        let bytes = build_dnp3(0x0A, 0x01, 0x44, &user);
        assert_eq!(bytes.len(), 10 + 35 + 2 * 3);
        let frame = Dnp3Parser::new().parse(&bytes).unwrap();
        assert_eq!(frame.app_function, Dnp3FunctionCode::Response);
        assert_eq!(frame.payload, (1u8..=32).collect::<Vec<u8>>());
    }

    #[test]
    fn corrupt_block_crc_is_rejected() {
        let mut user = vec![0xC0, 0xC0, 0x01];
        user.extend((0u8..20).collect::<Vec<u8>>());
        let mut bytes = build_dnp3(0x0A, 0x01, 0x44, &user);
        // Flip one data byte in the second block (offset 10 + 16 + 2).
        bytes[28] ^= 0x01;
        assert!(Dnp3Parser::new().parse(&bytes).is_err());
    }

    #[test]
    fn truncated_block_needs_more_data() {
        let user = vec![0xC0, 0xC0, 0x01, 0xAA, 0xBB];
        let bytes = build_dnp3(0x0A, 0x01, 0x44, &user);
        let short = &bytes[..bytes.len() - 1];
        assert_eq!(Dnp3Parser::new().try_parse(short).unwrap(), None);
        assert!(Dnp3Parser::new().parse(short).is_err());
    }

    #[test]
    fn try_parse_reports_exact_consumed_length() {
        let bytes = build_dnp3(0x0A, 0x01, 0x44, &[0xC0, 0xC0, 0x01]);
        let mut with_trailing = bytes.clone();
        with_trailing.push(0xFF);
        let (_, consumed) = Dnp3Parser::new()
            .try_parse(&with_trailing)
            .unwrap()
            .unwrap();
        assert_eq!(consumed, bytes.len());
        assert!(Dnp3Parser::new().parse(&with_trailing).is_err());
    }

    #[test]
    fn link_only_frame_has_no_user_data() {
        let bytes = build_dnp3(0x0A, 0x01, 0x44, &[]);
        assert_eq!(bytes.len(), 10);
        let frame = Dnp3Parser::new().parse(&bytes).unwrap();
        assert!(frame.payload.is_empty());
    }

    #[test]
    fn length_below_minimum_is_rejected() {
        // length 4 cannot hold control + destination + source.
        let mut out = vec![0x05, 0x64, 0x04, 0x44, 0x0A, 0x00, 0x01, 0x00];
        let crc = crc16_dnp(&out[0..8]);
        out.extend_from_slice(&crc.to_le_bytes());
        assert!(Dnp3Parser::new().parse(&out).is_err());
    }

    #[test]
    fn application_control_byte_is_parsed() {
        let bytes = build_dnp3(0x0A, 0x01, 0x44, &[0xC0, 0x5A, 0x01]);
        let frame = Dnp3Parser::new().parse(&bytes).unwrap();
        assert_eq!(frame.application_control, 0x5A);
        assert_eq!(frame.app_function, Dnp3FunctionCode::Read);
    }

    #[test]
    fn continuation_fragment_has_no_application_header() {
        // Transport byte 0x00: not the first fragment, so 0x5A is data, not app control.
        let bytes = build_dnp3(0x0A, 0x01, 0x44, &[0x00, 0x5A, 0x01]);
        let frame = Dnp3Parser::new().parse(&bytes).unwrap();
        assert!(!frame.transport_first);
        assert_eq!(frame.application_control, 0);
        assert_eq!(frame.payload, vec![0x5A, 0x01]);
    }

    #[test]
    fn response_function_code_is_recognized() {
        assert_eq!(Dnp3FunctionCode::from_u8(0x81), Dnp3FunctionCode::Response);
        assert_eq!(
            Dnp3FunctionCode::from_u8(0x82),
            Dnp3FunctionCode::UnsolicitedResponse
        );
        assert_eq!(Dnp3FunctionCode::from_u8(0x00), Dnp3FunctionCode::Confirm);
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

    /// A DNP3 data-link frame with correct CRCs (the header CRC, and one CRC per
    /// 16-byte block of user data). The length field is skewed by -2..=2 so the
    /// length checks run as well as the happy path.
    fn dnp3_frame() -> impl Strategy<Value = Vec<u8>> {
        (
            any::<u16>(),
            any::<u16>(),
            any::<u8>(),
            proptest::collection::vec(any::<u8>(), 0..48),
            -2i32..=2,
        )
            .prop_map(|(dest, src, ctrl, user, skew)| {
                let mut out = alloc::vec![0x05, 0x64];
                let len = (user.len() as i32 + 5 + skew).clamp(0, 255) as u8;
                out.push(len);
                out.push(ctrl);
                out.extend_from_slice(&dest.to_le_bytes());
                out.extend_from_slice(&src.to_le_bytes());
                let header_crc = crc16_dnp(&out[0..8]);
                out.extend_from_slice(&header_crc.to_le_bytes());
                for block in user.chunks(16) {
                    out.extend_from_slice(block);
                    out.extend_from_slice(&crc16_dnp(block).to_le_bytes());
                }
                out
            })
    }

    /// One random byte overwrite or one truncation of a well-formed frame.
    fn mutated_dnp3() -> impl Strategy<Value = Vec<u8>> {
        (dnp3_frame(), any::<usize>(), any::<u8>(), any::<bool>()).prop_map(
            |(mut bytes, idx, val, truncate)| {
                if bytes.is_empty() {
                    return bytes;
                }
                let i = idx % bytes.len();
                if truncate {
                    bytes.truncate(i);
                } else {
                    bytes[i] = val;
                }
                bytes
            },
        )
    }

    proptest! {
        /// A parsed frame encodes back to the same bytes: the CRCs are
        /// recomputed and match, and every header field is kept.
        #[test]
        fn parsed_frames_encode_back_to_the_same_bytes(bytes in dnp3_frame()) {
            if let Ok(frame) = Dnp3Parser::new().parse(&bytes) {
                prop_assert_eq!(Dnp3Encoder::new().encode(&frame).unwrap(), bytes);
            }
        }

        #[test]
        fn structured_dnp3_never_panics(bytes in mutated_dnp3()) {
            let _ = Dnp3Parser::new().parse(&bytes);
            let _ = Dnp3Parser::new().try_parse(&bytes);
        }

        #[test]
        fn well_formed_dnp3_never_panics_in_the_normalizer(bytes in dnp3_frame()) {
            if let Ok(frame) = Dnp3Parser::new().parse(&bytes) {
                let _ = Dnp3Normalizer::new().normalize(&frame);
            }
        }
    }
}
