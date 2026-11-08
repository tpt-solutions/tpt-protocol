//! Matter (CSA Matter 1.x) for smart home devices, over Thread or IP.
//!
//! This crate decodes three layers:
//! - Matter TLV (Matter core spec, Appendix A): all element types, the tag
//!   forms, and nested containers with a depth limit.
//! - The message header and protocol header (Matter core spec, sections 4.4
//!   and 4.5), for unsecured messages (session 0). Secured payloads are
//!   encrypted. Their header is decoded, but the payload is not.
//! - Interaction Model ReportData messages, decoded into attribute reports
//!   (endpoint, cluster, attribute and value).
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// Deepest container nesting accepted (bounds recursion).
const MAX_DEPTH: usize = 8;

/// Protocol id of the Secure Channel protocol.
const PROTOCOL_SECURE_CHANNEL: u16 = 0x0000;
/// Protocol id of the Interaction Model.
const PROTOCOL_INTERACTION_MODEL: u16 = 0x0001;
/// Interaction Model ReportData opcode.
const OPCODE_REPORT_DATA: u8 = 0x05;

// ---------------------------------------------------------------------------
// Matter TLV
// ---------------------------------------------------------------------------

/// A TLV tag (Appendix A.7).
#[derive(Debug, Clone, PartialEq)]
pub enum TlvTag {
    Anonymous,
    /// Context-specific tag, one byte.
    Context(u8),
    /// Common or implicit profile tag.
    Profile(u32),
    /// Fully-qualified tag: vendor, profile and tag number.
    FullyQualified {
        vendor: u16,
        profile: u16,
        number: u32,
    },
}

/// A decoded TLV value.
#[derive(Debug, Clone, PartialEq)]
pub enum TlvValue {
    Signed(i64),
    Unsigned(u64),
    Bool(bool),
    Float32(f32),
    Float64(f64),
    Utf8(String),
    Bytes(Vec<u8>),
    Null,
    Structure(Vec<(TlvTag, TlvValue)>),
    Array(Vec<(TlvTag, TlvValue)>),
    List(Vec<(TlvTag, TlvValue)>),
}

impl TlvValue {
    fn fields(&self) -> Option<&[(TlvTag, TlvValue)]> {
        match self {
            Self::Structure(f) | Self::Array(f) | Self::List(f) => Some(f),
            _ => None,
        }
    }
}

/// Looks up a context-tagged field of a container.
fn context_field(fields: &[(TlvTag, TlvValue)], tag: u8) -> Option<&TlvValue> {
    fields
        .iter()
        .find(|(t, _)| *t == TlvTag::Context(tag))
        .map(|(_, v)| v)
}

fn take<'a>(buf: &'a [u8], pos: &mut usize, len: usize) -> Result<&'a [u8], Error> {
    let end = pos
        .checked_add(len)
        .filter(|&e| e <= buf.len())
        .ok_or_else(|| Error::Parse("Matter TLV value runs past the end of the message".into()))?;
    let slice = &buf[*pos..end];
    *pos = end;
    Ok(slice)
}

fn read_uint_le(buf: &[u8], pos: &mut usize, len: usize) -> Result<u64, Error> {
    let bytes = take(buf, pos, len)?;
    Ok(bytes
        .iter()
        .rev()
        .fold(0u64, |acc, &b| (acc << 8) | b as u64))
}

fn read_tag(buf: &[u8], pos: &mut usize, control: u8) -> Result<TlvTag, Error> {
    Ok(match control {
        0 => TlvTag::Anonymous,
        1 => TlvTag::Context(take(buf, pos, 1)?[0]),
        2 => TlvTag::Profile(read_uint_le(buf, pos, 2)? as u32),
        3 => TlvTag::Profile(read_uint_le(buf, pos, 4)? as u32),
        4 => TlvTag::Profile(read_uint_le(buf, pos, 2)? as u32),
        5 => TlvTag::Profile(read_uint_le(buf, pos, 4)? as u32),
        6 => {
            let vendor = read_uint_le(buf, pos, 2)? as u16;
            let profile = read_uint_le(buf, pos, 2)? as u16;
            let number = read_uint_le(buf, pos, 2)? as u32;
            TlvTag::FullyQualified {
                vendor,
                profile,
                number,
            }
        }
        _ => {
            let vendor = read_uint_le(buf, pos, 2)? as u16;
            let profile = read_uint_le(buf, pos, 2)? as u16;
            let number = read_uint_le(buf, pos, 4)? as u32;
            TlvTag::FullyQualified {
                vendor,
                profile,
                number,
            }
        }
    })
}

/// Reads one element. Returns `None` for an end-of-container marker.
fn read_element(
    buf: &[u8],
    pos: &mut usize,
    depth: usize,
) -> Result<Option<(TlvTag, TlvValue)>, Error> {
    let control = take(buf, pos, 1)?[0];
    let element_type = control & 0x1F;
    let tag_control = control >> 5;
    if element_type == 0x18 {
        return Ok(None);
    }
    let tag = read_tag(buf, pos, tag_control)?;
    let value = match element_type {
        // Signed integers: 1, 2, 4 or 8 bytes, little-endian, sign-extended.
        0x00..=0x03 => {
            let size = 1usize << element_type;
            let raw = read_uint_le(buf, pos, size)?;
            let shift = 64 - 8 * size as u32;
            TlvValue::Signed(((raw << shift) as i64) >> shift)
        }
        0x04..=0x07 => TlvValue::Unsigned(read_uint_le(buf, pos, 1 << (element_type - 0x04))?),
        0x08 => TlvValue::Bool(false),
        0x09 => TlvValue::Bool(true),
        0x0A => TlvValue::Float32(f32::from_bits(read_uint_le(buf, pos, 4)? as u32)),
        0x0B => TlvValue::Float64(f64::from_bits(read_uint_le(buf, pos, 8)?)),
        // UTF-8 strings and octet strings: a length prefix of 1, 2, 4 or 8 bytes.
        0x0C..=0x13 => {
            let prefix = 1usize << (element_type & 0x03);
            let len = read_uint_le(buf, pos, prefix)?;
            let len = usize::try_from(len)
                .map_err(|_| Error::Parse("Matter TLV length too large".into()))?;
            let data = take(buf, pos, len)?;
            if element_type <= 0x0F {
                TlvValue::Utf8(
                    core::str::from_utf8(data)
                        .map_err(|_| Error::Parse("Matter TLV string is not UTF-8".into()))?
                        .to_string(),
                )
            } else {
                TlvValue::Bytes(data.to_vec())
            }
        }
        0x14 => TlvValue::Null,
        0x15 => TlvValue::Structure(read_container(buf, pos, depth)?),
        0x16 => TlvValue::Array(read_container(buf, pos, depth)?),
        0x17 => TlvValue::List(read_container(buf, pos, depth)?),
        other => {
            return Err(Error::Parse(format!(
                "Matter TLV reserved element type 0x{other:02X}"
            )))
        }
    };
    Ok(Some((tag, value)))
}

fn read_container(
    buf: &[u8],
    pos: &mut usize,
    depth: usize,
) -> Result<Vec<(TlvTag, TlvValue)>, Error> {
    if depth >= MAX_DEPTH {
        return Err(Error::Parse("Matter TLV nesting is too deep".into()));
    }
    let mut fields = Vec::new();
    while let Some(field) = read_element(buf, pos, depth + 1)? {
        fields.push(field);
    }
    Ok(fields)
}

/// Decodes one TLV element that spans all of `bytes`.
pub fn decode_tlv(bytes: &[u8]) -> Result<(TlvTag, TlvValue), Error> {
    let mut pos = 0usize;
    let element = read_element(bytes, &mut pos, 0)?
        .ok_or_else(|| Error::Parse("Matter TLV starts with an end-of-container".into()))?;
    if pos != bytes.len() {
        return Err(Error::Parse("Matter TLV has trailing bytes".into()));
    }
    Ok(element)
}

// ---------------------------------------------------------------------------
// Messages and Interaction Model
// ---------------------------------------------------------------------------

/// An attribute value reported by a ReportData message.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributeReport {
    pub endpoint: u16,
    pub cluster: u32,
    pub attribute: u32,
    pub data_version: Option<u32>,
    pub value: TlvValue,
}

/// A decoded Matter message. Only unsecured messages carry their protocol
/// header and Interaction Model content.
#[derive(Debug, Clone, PartialEq)]
pub struct MatterFrame {
    pub session_id: u16,
    pub message_counter: u32,
    pub source_node: Option<u64>,
    pub destination_node: Option<u64>,
    /// True when the payload is encrypted (session other than 0). Its
    /// header is decoded, but its content is not.
    pub encrypted: bool,
    pub exchange_id: u16,
    pub opcode: u8,
    pub protocol_id: u16,
    /// Attribute reports, for a ReportData message.
    pub attributes: Vec<AttributeReport>,
}

/// Human-readable names for common clusters (Matter application clusters).
fn cluster_name(cluster: u32) -> Option<&'static str> {
    match cluster {
        0x0003 => Some("Identify"),
        0x0006 => Some("OnOff"),
        0x0008 => Some("LevelControl"),
        0x0028 => Some("BasicInformation"),
        0x0402 => Some("TemperatureMeasurement"),
        0x0405 => Some("RelativeHumidityMeasurement"),
        0x0406 => Some("OccupancySensing"),
        _ => None,
    }
}

/// Reads an AttributePathIB (a structure or list with endpoint, cluster and attribute).
fn attribute_path(value: &TlvValue) -> Result<(u16, u32, u32), Error> {
    let fields = value
        .fields()
        .ok_or_else(|| Error::Parse("Matter attribute path is not a container".into()))?;
    let number = |tag: u8| -> Result<u64, Error> {
        match context_field(fields, tag) {
            Some(TlvValue::Unsigned(v)) => Ok(*v),
            _ => Err(Error::Parse(format!(
                "Matter attribute path needs context tag {tag}"
            ))),
        }
    };
    let endpoint = u16::try_from(number(2)?)
        .map_err(|_| Error::Parse("Matter endpoint id out of range".into()))?;
    let cluster = u32::try_from(number(3)?)
        .map_err(|_| Error::Parse("Matter cluster id out of range".into()))?;
    let attribute = u32::try_from(number(4)?)
        .map_err(|_| Error::Parse("Matter attribute id out of range".into()))?;
    Ok((endpoint, cluster, attribute))
}

/// Decodes a ReportData payload into attribute reports. Attribute statuses
/// (without data) and event reports are skipped.
fn report_data_attributes(payload: &[u8]) -> Result<Vec<AttributeReport>, Error> {
    let (_, top) = decode_tlv(payload)?;
    let fields = top
        .fields()
        .ok_or_else(|| Error::Parse("ReportData payload is not a structure".into()))?;
    let reports = match context_field(fields, 1) {
        Some(TlvValue::Array(items)) => items,
        None => return Ok(Vec::new()),
        Some(_) => {
            return Err(Error::Parse(
                "ReportData attribute reports are not an array".into(),
            ))
        }
    };

    let mut out = Vec::new();
    for (_, report) in reports {
        let Some(report_fields) = report.fields() else {
            continue;
        };
        // AttributeDataIB is context tag 1 inside AttributeReportIB. A report
        // with only an AttributeStatus has no data and is skipped.
        let Some(data) = context_field(report_fields, 1).and_then(|v| v.fields()) else {
            continue;
        };
        let path = context_field(data, 1)
            .ok_or_else(|| Error::Parse("AttributeDataIB has no path".into()))?;
        let (endpoint, cluster, attribute) = attribute_path(path)?;
        let data_version = match context_field(data, 0) {
            Some(TlvValue::Unsigned(v)) => u32::try_from(*v).ok(),
            _ => None,
        };
        let value = context_field(data, 2)
            .cloned()
            .ok_or_else(|| Error::Parse("AttributeDataIB has no data".into()))?;
        out.push(AttributeReport {
            endpoint,
            cluster,
            attribute,
            data_version,
            value,
        });
    }
    Ok(out)
}

/// Parses one Matter message (the bytes of one UDP datagram or Thread frame).
pub struct MatterParser;

impl FrameParser for MatterParser {
    type Frame = MatterFrame;

    /// Message header (section 4.4): flags, security flags, session id,
    /// message counter, then optional source and destination node ids.
    /// Protocol header (section 4.5): exchange flags, opcode, exchange id,
    /// optional vendor id, protocol id, optional acknowledged counter.
    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        let mut pos = 0usize;
        let flags = take(bytes, &mut pos, 1)?[0];
        if flags >> 4 != 0 {
            return Err(Error::Parse(format!(
                "unsupported Matter message version {}",
                flags >> 4
            )));
        }
        let security = take(bytes, &mut pos, 1)?[0];
        let session_id = read_uint_le(bytes, &mut pos, 2)? as u16;
        let message_counter = read_uint_le(bytes, &mut pos, 4)? as u32;

        let source_node = if flags & 0x04 != 0 {
            Some(read_uint_le(bytes, &mut pos, 8)?)
        } else {
            None
        };
        let destination_node = match flags & 0x03 {
            0 => None,
            1 => Some(read_uint_le(bytes, &mut pos, 8)?),
            2 => Some(read_uint_le(bytes, &mut pos, 2)?),
            _ => {
                return Err(Error::Parse(
                    "Matter destination node id type 3 is reserved".into(),
                ))
            }
        };

        // Session 0 is the unsecured session; its payload is plaintext. Any
        // other session (or a group session) is encrypted.
        let encrypted = session_id != 0 || security & 0x03 != 0;
        if encrypted {
            return Ok(MatterFrame {
                session_id,
                message_counter,
                source_node,
                destination_node,
                encrypted: true,
                exchange_id: 0,
                opcode: 0,
                protocol_id: 0,
                attributes: Vec::new(),
            });
        }

        let exchange_flags = take(bytes, &mut pos, 1)?[0];
        let opcode = take(bytes, &mut pos, 1)?[0];
        let exchange_id = read_uint_le(bytes, &mut pos, 2)? as u16;
        if exchange_flags & 0x10 != 0 {
            take(bytes, &mut pos, 2)?; // vendor id
        }
        let protocol_id = read_uint_le(bytes, &mut pos, 2)? as u16;
        if exchange_flags & 0x02 != 0 {
            take(bytes, &mut pos, 4)?; // acknowledged message counter
        }
        if exchange_flags & 0x08 != 0 {
            // Secured extensions: a 2-byte length, then the extension data.
            let len = read_uint_le(bytes, &mut pos, 2)? as usize;
            take(bytes, &mut pos, len)?;
        }

        let payload = &bytes[pos..];
        let attributes = match (protocol_id, opcode) {
            (PROTOCOL_INTERACTION_MODEL, OPCODE_REPORT_DATA) => report_data_attributes(payload)?,
            // Secure Channel messages (session setup, acks) carry no attributes.
            (PROTOCOL_SECURE_CHANNEL, _) => Vec::new(),
            _ => Vec::new(),
        };

        Ok(MatterFrame {
            session_id,
            message_counter,
            source_node,
            destination_node,
            encrypted: false,
            exchange_id,
            opcode,
            protocol_id,
            attributes,
        })
    }
}

/// Matter connection state.
#[derive(Debug, Clone, PartialEq)]
pub struct MatterConnectionState {
    pub connected: bool,
    pub fabric_id: u64,
    pub node_id: u64,
}

pub struct MatterValidator;

impl StateValidator for MatterValidator {
    type State = MatterConnectionState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("matter: not connected".into()));
        }
        Ok(())
    }
}

/// Normalizes Matter attribute reports into TUP DataPoints. The device is
/// the source node and endpoint. The point is the cluster and attribute.
pub struct MatterNormalizer;

impl MatterNormalizer {
    /// One data point per scalar attribute value. Containers and nulls are
    /// skipped. An unsigned value above i64::MAX is an error.
    pub fn points(&self, frame: &MatterFrame) -> Result<Vec<DataPoint>, Error> {
        if frame.encrypted {
            return Err(Error::Normalize(
                "encrypted Matter payload cannot be normalized without keys".into(),
            ));
        }
        let node = match frame.source_node {
            Some(n) => format!("{n:016x}"),
            None => "unknown".to_string(),
        };
        let mut points = Vec::new();
        for report in &frame.attributes {
            let value = match &report.value {
                TlvValue::Signed(v) => TupValue::Int(*v),
                TlvValue::Unsigned(v) => {
                    if *v > i64::MAX as u64 {
                        return Err(Error::Normalize(
                            "Matter unsigned value exceeds the i64 range".into(),
                        ));
                    }
                    TupValue::Int(*v as i64)
                }
                TlvValue::Bool(b) => TupValue::Bool(*b),
                TlvValue::Float32(f) => TupValue::Float(*f as f64),
                TlvValue::Float64(f) => TupValue::Float(*f),
                TlvValue::Utf8(s) => TupValue::Text(s.clone()),
                TlvValue::Bytes(b) => TupValue::Bytes(b.clone()),
                TlvValue::Null
                | TlvValue::Structure(_)
                | TlvValue::Array(_)
                | TlvValue::List(_) => {
                    continue;
                }
            };
            let cluster = match cluster_name(report.cluster) {
                Some(name) => name.to_string(),
                None => format!("0x{:04X}", report.cluster),
            };
            points.push(DataPoint::new(
                SourceProtocol::MatterThread,
                format!("matter:{node}/ep{}", report.endpoint),
                format!("{cluster}/0x{:04X}", report.attribute),
                0,
                value,
                Quality::Good,
            ));
        }
        Ok(points)
    }
}

impl DataNormalizer for MatterNormalizer {
    type Frame = MatterFrame;

    /// Every scalar attribute of the message. Encrypted messages are an error.
    fn normalize_all(&self, frame: &Self::Frame) -> Result<Vec<DataPoint>, Error> {
        self.points(frame)
    }

    /// The first scalar attribute of the message.
    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        self.points(frame)?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Normalize("Matter message has no scalar attribute value".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    // Small encoders, so test vectors read as structure rather than hex.

    fn put_control(out: &mut Vec<u8>, tag: Option<u8>, element_type: u8) {
        match tag {
            Some(t) => {
                out.push(0x20 | element_type);
                out.push(t);
            }
            None => out.push(element_type),
        }
    }

    fn put_uint(out: &mut Vec<u8>, tag: Option<u8>, v: u64) {
        let (etype, size) = if v <= 0xFF {
            (0x04, 1)
        } else if v <= 0xFFFF {
            (0x05, 2)
        } else if v <= 0xFFFF_FFFF {
            (0x06, 4)
        } else {
            (0x07, 8)
        };
        put_control(out, tag, etype);
        out.extend_from_slice(&v.to_le_bytes()[..size]);
    }

    fn put_bool(out: &mut Vec<u8>, tag: Option<u8>, v: bool) {
        put_control(out, tag, if v { 0x09 } else { 0x08 });
    }

    fn start(out: &mut Vec<u8>, tag: Option<u8>, element_type: u8) {
        put_control(out, tag, element_type);
    }

    fn end(out: &mut Vec<u8>) {
        out.push(0x18);
    }

    /// A ReportData payload with one attribute report.
    fn report_data(
        endpoint: u64,
        cluster: u64,
        attribute: u64,
        value: impl FnOnce(&mut Vec<u8>),
    ) -> Vec<u8> {
        let mut out = Vec::new();
        start(&mut out, None, 0x15); // ReportData structure
        start(&mut out, Some(1), 0x16); // AttributeReportIBs array
        start(&mut out, None, 0x15); // AttributeReportIB
        start(&mut out, Some(1), 0x15); // AttributeData
        put_uint(&mut out, Some(0), 7); // DataVersion
        start(&mut out, Some(1), 0x17); // AttributePath list
        put_uint(&mut out, Some(2), endpoint);
        put_uint(&mut out, Some(3), cluster);
        put_uint(&mut out, Some(4), attribute);
        end(&mut out);
        value(&mut out); // tag 2 is added by the closure
        end(&mut out); // AttributeData
        end(&mut out); // AttributeReportIB
        end(&mut out); // array
        end(&mut out); // ReportData
        out
    }

    /// Wraps a payload in an unsecured message: header, protocol header, payload.
    fn unsecured_message(
        source: Option<u64>,
        protocol_id: u16,
        opcode: u8,
        payload: &[u8],
    ) -> Vec<u8> {
        let mut out = vec![if source.is_some() { 0x04 } else { 0x00 }, 0x00];
        out.extend_from_slice(&0u16.to_le_bytes()); // session 0
        out.extend_from_slice(&42u32.to_le_bytes()); // message counter
        if let Some(n) = source {
            out.extend_from_slice(&n.to_le_bytes());
        }
        out.push(0x05); // exchange flags: initiator, reliability
        out.push(opcode);
        out.extend_from_slice(&0x1234u16.to_le_bytes()); // exchange id
        out.extend_from_slice(&protocol_id.to_le_bytes());
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn unsigned_one_byte_anonymous() {
        assert_eq!(
            decode_tlv(&[0x04, 0x20]).unwrap(),
            (TlvTag::Anonymous, TlvValue::Unsigned(0x20))
        );
    }

    #[test]
    fn signed_negative_is_sign_extended() {
        assert_eq!(decode_tlv(&[0x00, 0xFF]).unwrap().1, TlvValue::Signed(-1));
        assert_eq!(
            decode_tlv(&[0x01, 0x00, 0x80]).unwrap().1,
            TlvValue::Signed(-32768)
        );
    }

    #[test]
    fn bool_and_null() {
        assert_eq!(decode_tlv(&[0x09]).unwrap().1, TlvValue::Bool(true));
        assert_eq!(decode_tlv(&[0x08]).unwrap().1, TlvValue::Bool(false));
        assert_eq!(decode_tlv(&[0x14]).unwrap().1, TlvValue::Null);
    }

    #[test]
    fn utf8_string_with_one_byte_length() {
        assert_eq!(
            decode_tlv(&[0x0C, 0x02, b'o', b'k']).unwrap().1,
            TlvValue::Utf8("ok".into())
        );
    }

    #[test]
    fn octet_string_with_two_byte_length() {
        assert_eq!(
            decode_tlv(&[0x11, 0x02, 0x00, 0xAA, 0xBB]).unwrap().1,
            TlvValue::Bytes(vec![0xAA, 0xBB])
        );
    }

    #[test]
    fn float32_is_little_endian() {
        assert_eq!(
            decode_tlv(&[0x0A, 0x00, 0x00, 0x80, 0x3F]).unwrap().1,
            TlvValue::Float32(1.0)
        );
    }

    #[test]
    fn context_tagged_structure() {
        // Structure with context tag 1 holding unsigned 0x20 with context tag 1.
        let bytes = [0x35, 0x01, 0x24, 0x01, 0x20, 0x18];
        let (tag, value) = decode_tlv(&bytes).unwrap();
        assert_eq!(tag, TlvTag::Context(1));
        assert_eq!(
            value,
            TlvValue::Structure(vec![(TlvTag::Context(1), TlvValue::Unsigned(0x20))])
        );
    }

    #[test]
    fn fully_qualified_tag_is_read() {
        // Unsigned 1 byte with a fully-qualified 6-byte tag: vendor 0xFFF1, profile 0x0000, tag 0x0007.
        let bytes = [0xC4, 0xF1, 0xFF, 0x00, 0x00, 0x07, 0x00, 0x20];
        let (tag, value) = decode_tlv(&bytes).unwrap();
        assert_eq!(
            tag,
            TlvTag::FullyQualified {
                vendor: 0xFFF1,
                profile: 0,
                number: 7
            }
        );
        assert_eq!(value, TlvValue::Unsigned(0x20));
    }

    #[test]
    fn reserved_element_type_is_rejected() {
        assert!(decode_tlv(&[0x19]).is_err());
    }

    #[test]
    fn truncated_value_is_rejected() {
        assert!(decode_tlv(&[0x06, 0x01, 0x02]).is_err());
        assert!(decode_tlv(&[0x0C, 0x05, b'a']).is_err());
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        assert!(decode_tlv(&[0x09, 0x09]).is_err());
    }

    #[test]
    fn nesting_is_bounded() {
        let mut deep = Vec::new();
        for _ in 0..40 {
            start(&mut deep, None, 0x15);
        }
        for _ in 0..40 {
            end(&mut deep);
        }
        assert!(decode_tlv(&deep).is_err());
    }

    #[test]
    fn report_data_becomes_attribute_reports() {
        // OnOff (0x0006) attribute 0x0000 on endpoint 1 is true.
        let payload = report_data(1, 0x0006, 0x0000, |out| put_bool(out, Some(2), true));
        let bytes = unsecured_message(
            Some(0x1122),
            PROTOCOL_INTERACTION_MODEL,
            OPCODE_REPORT_DATA,
            &payload,
        );
        let frame = MatterParser.parse(&bytes).unwrap();
        assert!(!frame.encrypted);
        assert_eq!(frame.source_node, Some(0x1122));
        assert_eq!(frame.message_counter, 42);
        assert_eq!(frame.exchange_id, 0x1234);
        assert_eq!(frame.attributes.len(), 1);
        let a = &frame.attributes[0];
        assert_eq!((a.endpoint, a.cluster, a.attribute), (1, 0x0006, 0x0000));
        assert_eq!(a.data_version, Some(7));
        assert_eq!(a.value, TlvValue::Bool(true));
    }

    #[test]
    fn normalizer_names_the_cluster_and_device() {
        let payload = report_data(2, 0x0402, 0x0000, |out| put_uint(out, Some(2), 2150));
        let bytes = unsecured_message(
            Some(0xABCD),
            PROTOCOL_INTERACTION_MODEL,
            OPCODE_REPORT_DATA,
            &payload,
        );
        let frame = MatterParser.parse(&bytes).unwrap();
        let points = MatterNormalizer.points(&frame).unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].device_id, "matter:000000000000abcd/ep2");
        assert_eq!(points[0].point_id, "TemperatureMeasurement/0x0000");
        assert_eq!(points[0].value, TupValue::Int(2150));
        assert_eq!(points[0].source_protocol, SourceProtocol::MatterThread);
    }

    #[test]
    fn unknown_cluster_is_named_by_number() {
        let payload = report_data(1, 0xFC01, 0x0002, |out| put_bool(out, Some(2), false));
        let frame = MatterParser
            .parse(&unsecured_message(
                None,
                PROTOCOL_INTERACTION_MODEL,
                OPCODE_REPORT_DATA,
                &payload,
            ))
            .unwrap();
        let dp = MatterNormalizer.normalize(&frame).unwrap();
        assert_eq!(dp.point_id, "0xFC01/0x0002");
        assert_eq!(dp.device_id, "matter:unknown/ep1");
    }

    #[test]
    fn normalize_batch_yields_the_scalar_points_of_each_frame() {
        let payload = report_data(1, 0x0006, 0x0000, |out| put_bool(out, Some(2), true));
        let frame = MatterParser
            .parse(&unsecured_message(
                None,
                PROTOCOL_INTERACTION_MODEL,
                OPCODE_REPORT_DATA,
                &payload,
            ))
            .unwrap();
        // Two frames, one scalar attribute each.
        let points = MatterNormalizer
            .normalize_batch(&[frame.clone(), frame])
            .unwrap();
        assert_eq!(points.len(), 2);
    }

    #[test]
    fn encrypted_session_is_reported_not_decoded() {
        let mut bytes = unsecured_message(
            None,
            PROTOCOL_INTERACTION_MODEL,
            OPCODE_REPORT_DATA,
            &[0x00; 4],
        );
        bytes[2] = 0x05; // session id 5
        let frame = MatterParser.parse(&bytes).unwrap();
        assert!(frame.encrypted);
        assert!(frame.attributes.is_empty());
        assert!(MatterNormalizer.normalize(&frame).is_err());
    }

    #[test]
    fn secure_channel_message_has_no_attributes() {
        let bytes = unsecured_message(None, PROTOCOL_SECURE_CHANNEL, 0x20, &[]);
        let frame = MatterParser.parse(&bytes).unwrap();
        assert_eq!(frame.protocol_id, PROTOCOL_SECURE_CHANNEL);
        assert!(frame.attributes.is_empty());
    }

    #[test]
    fn unsigned_value_above_i64_is_an_error() {
        let payload = report_data(1, 0x0006, 0x0000, |out| put_uint(out, Some(2), u64::MAX));
        let frame = MatterParser
            .parse(&unsecured_message(
                None,
                PROTOCOL_INTERACTION_MODEL,
                OPCODE_REPORT_DATA,
                &payload,
            ))
            .unwrap();
        assert!(MatterNormalizer.points(&frame).is_err());
    }

    #[test]
    fn reserved_message_version_and_destination_type_are_rejected() {
        let mut bytes = unsecured_message(
            None,
            PROTOCOL_INTERACTION_MODEL,
            OPCODE_REPORT_DATA,
            &[0x15, 0x18],
        );
        bytes[0] = 0x10; // version 1
        assert!(MatterParser.parse(&bytes).is_err());
        let mut bytes = unsecured_message(
            None,
            PROTOCOL_INTERACTION_MODEL,
            OPCODE_REPORT_DATA,
            &[0x15, 0x18],
        );
        bytes[0] = 0x03; // destination id type 3
        assert!(MatterParser.parse(&bytes).is_err());
    }

    #[test]
    fn garbage_is_rejected_without_panic() {
        assert!(MatterParser.parse(&[0xFF; 32]).is_err());
        assert!(MatterParser.parse(&[]).is_err());
    }

    #[test]
    fn validator_connected() {
        let ok = MatterConnectionState {
            connected: true,
            fabric_id: 1,
            node_id: 2,
        };
        assert!(MatterValidator.validate(&ok).is_ok());
        let down = MatterConnectionState {
            connected: false,
            ..ok
        };
        assert!(MatterValidator.validate(&down).is_err());
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = MatterParser.parse(&bytes);
            let _ = decode_tlv(&bytes);
        }
    }
}
