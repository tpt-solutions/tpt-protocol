//! OPC UA protocol implementation for industrial interoperability.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// OPC UA Binary message type (OPC 10000-6, section 7.1.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OpcuaMessageType {
    Hello,
    Acknowledge,
    Error,
    Message,
    OpenSecureChannel,
    CloseSecureChannel,
}

impl OpcuaMessageType {
    fn from_bytes(b: &[u8; 3]) -> Option<Self> {
        match b {
            b"HEL" => Some(Self::Hello),
            b"ACK" => Some(Self::Acknowledge),
            b"ERR" => Some(Self::Error),
            b"MSG" => Some(Self::Message),
            b"OPN" => Some(Self::OpenSecureChannel),
            b"CLO" => Some(Self::CloseSecureChannel),
            _ => None,
        }
    }

    /// Messages that carry a secure channel header (channel id, token id,
    /// sequence number and request id) after the 8-byte message header.
    fn has_sequence_header(&self) -> bool {
        matches!(self, Self::Message | Self::CloseSecureChannel)
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Hello => "HEL",
            Self::Acknowledge => "ACK",
            Self::Error => "ERR",
            Self::Message => "MSG",
            Self::OpenSecureChannel => "OPN",
            Self::CloseSecureChannel => "CLO",
        }
    }
}

impl fmt::Display for OpcuaMessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A parsed OPC UA Binary message. All multi-byte fields are little-endian.
#[derive(Clone, Debug, PartialEq)]
pub struct OpcuaFrame {
    pub message_type: OpcuaMessageType,
    /// 'F' (final), 'C' (intermediate chunk) or 'A' (abort).
    pub chunk_type: u8,
    /// Total message length including the 8-byte header.
    pub message_size: u32,
    /// Secure channel id. Zero for HEL, ACK and ERR.
    pub channel_id: u32,
    pub token_id: u32,
    pub sequence_num: u32,
    pub request_id: u32,
    /// Numeric id of the service TypeId (MSG only), when it is numeric.
    pub service_id: Option<u32>,
    /// Namespace index of the service TypeId (MSG only).
    pub type_namespace: Option<u16>,
    /// The message body after the header fields above and the TypeId.
    pub payload: Vec<u8>,
}

impl fmt::Display for OpcuaFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OPCUA[type={}, chunk={}, size={}, chan=0x{:08X}, tok=0x{:08X}, seq={}, req={}]",
            self.message_type,
            self.chunk_type as char,
            self.message_size,
            self.channel_id,
            self.token_id,
            self.sequence_num,
            self.request_id,
        )
    }
}

/// Message header size: type (3) + chunk (1) + size (4).
const MESSAGE_HEADER: usize = 8;
/// Secure channel fields for MSG and CLO: channel (4) + token (4) + sequence (4) + request (4).
const SEQUENCE_HEADER: usize = 16;

/// Reads a little-endian u32 at `at`. The caller has checked the length.
fn le_u32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Decodes the TypeId NodeId at the start of `body` (OPC 10000-6, 5.2.2.9).
/// Returns `(namespace, numeric id, bytes used)`, or `None` for non-numeric ids.
fn decode_numeric_node_id(body: &[u8]) -> Result<Option<(u16, u32, usize)>, Error> {
    let encoding = *body
        .first()
        .ok_or_else(|| Error::Parse("OPC UA TypeId is missing".into()))?;
    match encoding & 0x0F {
        0x00 => {
            let id = *body
                .get(1)
                .ok_or_else(|| Error::Parse("OPC UA two-byte NodeId truncated".into()))?;
            Ok(Some((0, id as u32, 2)))
        }
        0x01 => {
            if body.len() < 4 {
                return Err(Error::Parse("OPC UA four-byte NodeId truncated".into()));
            }
            let id = u16::from_le_bytes([body[2], body[3]]) as u32;
            Ok(Some((body[1] as u16, id, 4)))
        }
        0x02 => {
            if body.len() < 7 {
                return Err(Error::Parse("OPC UA numeric NodeId truncated".into()));
            }
            let ns = u16::from_le_bytes([body[1], body[2]]);
            Ok(Some((ns, le_u32(body, 3), 7)))
        }
        _ => Ok(None),
    }
}

/// OPC UA Binary parser.
#[derive(Clone, Debug, Default)]
pub struct OpcuaParser;

impl OpcuaParser {
    pub fn new() -> Self {
        Self
    }
}

/// Decodes one message from the front of `bytes`. Returns `Ok(None)` when the
/// message is incomplete. Bytes after the message belong to the next one.
fn decode_message(bytes: &[u8]) -> Result<Option<(OpcuaFrame, usize)>, Error> {
    if bytes.len() < MESSAGE_HEADER {
        return Ok(None);
    }
    let type_bytes = [bytes[0], bytes[1], bytes[2]];
    let message_type = OpcuaMessageType::from_bytes(&type_bytes).ok_or_else(|| {
        Error::Parse(format!(
            "OPC UA unknown message type {:?}",
            core::str::from_utf8(&type_bytes).unwrap_or("<binary>")
        ))
    })?;
    let chunk_type = bytes[3];
    if !matches!(chunk_type, b'F' | b'C' | b'A') {
        return Err(Error::Parse(format!(
            "OPC UA invalid chunk type 0x{chunk_type:02X}"
        )));
    }
    let message_size = le_u32(bytes, 4);
    let size = message_size as usize;
    if size < MESSAGE_HEADER {
        return Err(Error::Parse(format!(
            "OPC UA message_size {message_size} is below the 8-byte header"
        )));
    }
    if bytes.len() < size {
        return Ok(None);
    }
    let message = &bytes[..size];

    let mut frame = OpcuaFrame {
        message_type,
        chunk_type,
        message_size,
        channel_id: 0,
        token_id: 0,
        sequence_num: 0,
        request_id: 0,
        service_id: None,
        type_namespace: None,
        payload: Vec::new(),
    };

    let mut body_start = MESSAGE_HEADER;
    if message_type.has_sequence_header() {
        if size < MESSAGE_HEADER + SEQUENCE_HEADER {
            return Err(Error::Parse(format!(
                "OPC UA {message_type} message is {size} bytes, needs at least {}",
                MESSAGE_HEADER + SEQUENCE_HEADER
            )));
        }
        frame.channel_id = le_u32(message, 8);
        frame.token_id = le_u32(message, 12);
        frame.sequence_num = le_u32(message, 16);
        frame.request_id = le_u32(message, 20);
        body_start += SEQUENCE_HEADER;
    }

    let body = &message[body_start..];
    if message_type == OpcuaMessageType::Message {
        decode_service_body(body, &mut frame)?;
    } else {
        frame.payload = body.to_vec();
    }
    Ok(Some((frame, size)))
}

/// A MSG body is an ExtensionObject: a TypeId NodeId, an encoding byte, and a
/// length-prefixed body. Numeric TypeIds are decoded; others are kept raw.
fn decode_service_body(body: &[u8], frame: &mut OpcuaFrame) -> Result<(), Error> {
    let Some((namespace, id, used)) = decode_numeric_node_id(body)? else {
        frame.payload = body.to_vec();
        return Ok(());
    };
    frame.service_id = Some(id);
    frame.type_namespace = Some(namespace);
    let rest = &body[used..];
    match rest {
        // Encoding 0x01: binary body with an int32 length.
        [0x01, l0, l1, l2, l3, data @ ..] => {
            let len = i32::from_le_bytes([*l0, *l1, *l2, *l3]);
            if len < 0 || len as usize != data.len() {
                return Err(Error::Parse(format!(
                    "OPC UA ExtensionObject body length {len} does not match {} bytes",
                    data.len()
                )));
            }
            frame.payload = data.to_vec();
        }
        _ => frame.payload = rest.to_vec(),
    }
    Ok(())
}

impl FrameParser for OpcuaParser {
    type Frame = OpcuaFrame;

    /// Parses exactly one message. The message_size field must equal the length.
    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        match decode_message(bytes)? {
            None => Err(Error::Parse(format!(
                "OPC UA message incomplete: have {} bytes",
                bytes.len()
            ))),
            Some((frame, consumed)) if consumed == bytes.len() => Ok(frame),
            Some((_, consumed)) => Err(Error::Parse(format!(
                "OPC UA has {} trailing bytes after the message",
                bytes.len() - consumed
            ))),
        }
    }

    fn try_parse(&self, bytes: &[u8]) -> Result<Option<(Self::Frame, usize)>, Error> {
        decode_message(bytes)
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
            Some(id) => format!("node:{id}"),
            None => format!("msg:{}", frame.message_type),
        };
        let timestamp = 0;

        let value = if !frame.payload.is_empty() {
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

    /// Builds a message with the header size filled in, all fields little-endian.
    fn message(
        msg_type: &[u8; 3],
        chunk: u8,
        sequence: Option<(u32, u32, u32, u32)>,
        body: &[u8],
    ) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(msg_type);
        out.push(chunk);
        out.extend_from_slice(&[0, 0, 0, 0]); // size, filled in below
        if let Some((channel, token, seq, req)) = sequence {
            out.extend_from_slice(&channel.to_le_bytes());
            out.extend_from_slice(&token.to_le_bytes());
            out.extend_from_slice(&seq.to_le_bytes());
            out.extend_from_slice(&req.to_le_bytes());
        }
        out.extend_from_slice(body);
        let size = out.len() as u32;
        out[4..8].copy_from_slice(&size.to_le_bytes());
        out
    }

    /// A MSG body: numeric TypeId (namespace 0, id 0x2A) and a binary ExtensionObject.
    fn service_body(data: &[u8]) -> Vec<u8> {
        let mut body = vec![0x00, 0x2A]; // two-byte NodeId, id 42
        body.push(0x01); // binary body
        body.extend_from_slice(&(data.len() as i32).to_le_bytes());
        body.extend_from_slice(data);
        body
    }

    #[test]
    fn header_fields_are_little_endian() {
        let bytes = message(
            b"MSG",
            b'F',
            Some((0x1234, 0x5678, 1, 2)),
            &service_body(&[0xAA]),
        );
        assert_eq!(&bytes[4..8], &(bytes.len() as u32).to_le_bytes());
        let frame = OpcuaParser::new().parse(&bytes).unwrap();
        assert_eq!(frame.channel_id, 0x1234);
        assert_eq!(frame.token_id, 0x5678);
        assert_eq!(frame.sequence_num, 1);
        assert_eq!(frame.request_id, 2);
    }

    #[test]
    fn hello_payload_is_the_body() {
        let body = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00];
        let frame = OpcuaParser::new()
            .parse(&message(b"HEL", b'F', None, &body))
            .unwrap();
        assert_eq!(frame.message_type, OpcuaMessageType::Hello);
        assert_eq!(frame.channel_id, 0);
        assert_eq!(frame.payload, body.to_vec());
    }

    #[test]
    fn msg_service_nodeid_and_extension_object_are_decoded() {
        let frame = OpcuaParser::new()
            .parse(&message(
                b"MSG",
                b'F',
                Some((1, 2, 3, 4)),
                &service_body(&[0xAA, 0xBB]),
            ))
            .unwrap();
        assert_eq!(frame.service_id, Some(0x2A));
        assert_eq!(frame.type_namespace, Some(0));
        assert_eq!(frame.payload, vec![0xAA, 0xBB]);
    }

    #[test]
    fn four_byte_and_numeric_node_ids_are_decoded() {
        // Four-byte form: namespace 2, id 0x1234 (little-endian).
        let mut four = vec![0x01, 0x02, 0x34, 0x12, 0x01];
        four.extend_from_slice(&0i32.to_le_bytes());
        let frame = OpcuaParser::new()
            .parse(&message(b"MSG", b'F', Some((1, 2, 3, 4)), &four))
            .unwrap();
        assert_eq!(
            (frame.type_namespace, frame.service_id),
            (Some(2), Some(0x1234))
        );

        // Numeric form: namespace 0x0102, id 0x0001_0000 (little-endian).
        let mut numeric = vec![0x02, 0x02, 0x01, 0x00, 0x00, 0x01, 0x00];
        numeric.extend_from_slice(&0i32.to_le_bytes());
        let frame = OpcuaParser::new()
            .parse(&message(b"MSG", b'F', Some((1, 2, 3, 4)), &numeric))
            .unwrap();
        assert_eq!(
            (frame.type_namespace, frame.service_id),
            (Some(0x0102), Some(0x0001_0000))
        );
    }

    #[test]
    fn extension_object_length_must_match() {
        let mut body = vec![0x00, 0x2A, 0x01];
        body.extend_from_slice(&10i32.to_le_bytes()); // claims 10 bytes
        body.push(0xAA); // only 1 present
        assert!(OpcuaParser::new()
            .parse(&message(b"MSG", b'F', Some((1, 2, 3, 4)), &body))
            .is_err());
    }

    #[test]
    fn message_size_must_match_the_frame() {
        let mut bytes = message(b"HEL", b'F', None, &[0, 0, 0, 0]);
        // Claim 4 more bytes than present.
        let claimed = (bytes.len() as u32 + 4).to_le_bytes();
        bytes[4..8].copy_from_slice(&claimed);
        assert!(OpcuaParser::new().parse(&bytes).is_err());
        assert_eq!(OpcuaParser::new().try_parse(&bytes).unwrap(), None);
    }

    #[test]
    fn trailing_bytes_are_left_for_the_next_message() {
        let bytes = message(b"HEL", b'F', None, &[0, 0, 0, 0]);
        let mut stream = bytes.clone();
        stream.extend_from_slice(&message(b"ACK", b'F', None, &[0, 0, 0, 0]));
        let (frame, consumed) = OpcuaParser::new().try_parse(&stream).unwrap().unwrap();
        assert_eq!(frame.message_type, OpcuaMessageType::Hello);
        assert_eq!(consumed, bytes.len());
        assert!(OpcuaParser::new().parse(&stream).is_err());
    }

    #[test]
    fn message_size_below_header_is_rejected() {
        let mut bytes = message(b"HEL", b'F', None, &[]);
        bytes[4..8].copy_from_slice(&4u32.to_le_bytes());
        assert!(OpcuaParser::new().parse(&bytes).is_err());
    }

    #[test]
    fn unknown_message_type_is_rejected() {
        let bytes = message(b"XYZ", b'F', None, &[0, 0, 0, 0]);
        assert!(OpcuaParser::new().parse(&bytes).is_err());
    }

    #[test]
    fn chunk_type_is_validated() {
        let bytes = message(b"HEL", b'X', None, &[0, 0, 0, 0]);
        assert!(OpcuaParser::new().parse(&bytes).is_err());
    }

    #[test]
    fn msg_needs_the_sequence_header() {
        // A MSG of only the 8-byte header cannot carry channel, token, sequence and request.
        let bytes = message(b"MSG", b'F', None, &[]);
        assert!(OpcuaParser::new().parse(&bytes).is_err());
    }

    #[test]
    fn garbage_input_is_rejected() {
        assert!(OpcuaParser::new()
            .parse(&[0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x08])
            .is_err());
    }

    #[test]
    fn truncated_header_needs_more_data() {
        assert_eq!(
            OpcuaParser::new()
                .try_parse(&[0x48, 0x45, 0x4C, 0x46, 0x00])
                .unwrap(),
            None
        );
    }

    #[test]
    fn normalize_msg_uses_service_id() {
        let bytes = message(
            b"MSG",
            b'F',
            Some((0x1234, 0x5678, 1, 1)),
            &service_body(&[0x01, 0x02]),
        );
        let frame = OpcuaParser::new().parse(&bytes).unwrap();
        let dp = OpcuaNormalizer::new().normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Opcua);
        assert_eq!(dp.device_id, "opcua:chan:0x00001234");
        assert_eq!(dp.point_id, "node:42");
        assert_eq!(dp.value, TupValue::Bytes(vec![0x01, 0x02]));
    }

    #[test]
    fn validator_ok() {
        let state = OpcuaState {
            connected: true,
            channel_id: 0x1234,
            token_id: 0x5678,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(OpcuaValidator::new().validate(&state).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        assert!(OpcuaValidator::new()
            .validate(&OpcuaState::default())
            .is_err());
    }

    #[test]
    fn validator_zero_channel() {
        let state = OpcuaState {
            connected: true,
            channel_id: 0,
            token_id: 0x5678,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(OpcuaValidator::new().validate(&state).is_err());
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let parser = OpcuaParser::new();
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }
}
