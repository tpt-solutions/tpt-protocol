//! Sparkplug B protocol implementation for MQTT-based IIoT infrastructure.
//!
//! A Sparkplug B message is a Protobuf `Payload` published on an MQTT topic of
//! the form `spBv1.0/<group>/<message type>/<edge node>[/<device>]`. The topic
//! carries the group, message type and node. The payload carries the metrics.
//! This crate decodes both, without a Protobuf dependency.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// Sparkplug B topic namespace.
const NAMESPACE: &str = "spBv1.0";

/// Sparkplug B message type, from the topic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SparkplugMessageType {
    NBirth,
    NDeath,
    DBirth,
    DDeath,
    NData,
    DData,
    NCmd,
    DCmd,
}

impl SparkplugMessageType {
    fn from_topic_segment(s: &str) -> Option<Self> {
        match s {
            "NBIRTH" => Some(Self::NBirth),
            "NDEATH" => Some(Self::NDeath),
            "DBIRTH" => Some(Self::DBirth),
            "DDEATH" => Some(Self::DDeath),
            "NDATA" => Some(Self::NData),
            "DDATA" => Some(Self::DData),
            "NCMD" => Some(Self::NCmd),
            "DCMD" => Some(Self::DCmd),
            _ => None,
        }
    }

    /// Device-level messages carry a device id in the topic.
    fn is_device_level(&self) -> bool {
        matches!(self, Self::DBirth | Self::DDeath | Self::DData | Self::DCmd)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NBirth => "NBIRTH",
            Self::NDeath => "NDEATH",
            Self::DBirth => "DBIRTH",
            Self::DDeath => "DDEATH",
            Self::NData => "NDATA",
            Self::DData => "DDATA",
            Self::NCmd => "NCMD",
            Self::DCmd => "DCMD",
        }
    }
}

/// A parsed `spBv1.0` topic.
#[derive(Debug, Clone, PartialEq)]
pub struct SparkplugTopic {
    pub group_id: String,
    pub message_type: SparkplugMessageType,
    pub edge_node_id: String,
    pub device_id: Option<String>,
}

impl SparkplugTopic {
    /// Parses an MQTT topic. Wildcards are rejected: a topic must name one node.
    pub fn parse(topic: &str) -> Result<Self, Error> {
        let rest = topic
            .strip_prefix(NAMESPACE)
            .and_then(|r| r.strip_prefix('/'))
            .ok_or_else(|| Error::Parse(format!("Sparkplug topic must start with {NAMESPACE}/")))?;
        let segments: Vec<&str> = rest.split('/').collect();
        if segments
            .iter()
            .any(|s| s.is_empty() || s.contains(['+', '#']))
        {
            return Err(Error::Parse(
                "Sparkplug topic segments must be non-empty and literal".into(),
            ));
        }
        let message_type =
            SparkplugMessageType::from_topic_segment(segments[1]).ok_or_else(|| {
                Error::Parse(format!("unknown Sparkplug message type '{}'", segments[1]))
            })?;
        let expected = if message_type.is_device_level() { 4 } else { 3 };
        if segments.len() != expected {
            return Err(Error::Parse(format!(
                "{} topic needs {expected} segments after {NAMESPACE}, found {}",
                message_type.as_str(),
                segments.len()
            )));
        }
        Ok(Self {
            group_id: segments[0].to_string(),
            message_type,
            edge_node_id: segments[2].to_string(),
            device_id: segments.get(3).map(|d| d.to_string()),
        })
    }
}

/// One metric of a payload.
#[derive(Debug, Clone, PartialEq)]
pub struct Metric {
    pub name: String,
    pub alias: Option<u64>,
    pub timestamp: Option<u64>,
    /// Sparkplug datatype number (see the Sparkplug B specification).
    pub datatype: u32,
    /// The decoded value. `None` for null metrics and for structured types
    /// (DataSet, Template) that are not decoded here.
    pub value: Option<TupValue>,
}

/// A decoded Sparkplug B message.
#[derive(Debug, Clone, PartialEq)]
pub struct SparkplugFrame {
    /// Present when the frame was decoded with its topic.
    pub topic: Option<SparkplugTopic>,
    pub timestamp: Option<u64>,
    /// Sequence number, 0-255.
    pub seq: Option<u64>,
    pub uuid: Option<String>,
    pub metrics: Vec<Metric>,
    pub body: Option<Vec<u8>>,
}

/// A Protobuf field value on the wire.
#[derive(Clone, Copy)]
enum Wire<'a> {
    Varint(u64),
    Fixed64(u64),
    Bytes(&'a [u8]),
    Fixed32(u32),
}

fn read_varint(buf: &[u8], pos: &mut usize) -> Result<u64, Error> {
    let mut value = 0u64;
    for i in 0..10 {
        let byte = *buf
            .get(*pos)
            .ok_or_else(|| Error::Parse("Protobuf varint truncated".into()))?;
        *pos += 1;
        value |= ((byte & 0x7F) as u64) << (7 * i);
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(Error::Parse(
        "Protobuf varint is longer than 10 bytes".into(),
    ))
}

/// Calls `f` for each field of a Protobuf message. Groups (wire types 3 and 4)
/// are rejected, as Sparkplug does not use them.
fn for_each_field<'a>(
    buf: &'a [u8],
    mut f: impl FnMut(u64, Wire<'a>) -> Result<(), Error>,
) -> Result<(), Error> {
    let mut pos = 0usize;
    while pos < buf.len() {
        let key = read_varint(buf, &mut pos)?;
        let field = key >> 3;
        if field == 0 {
            return Err(Error::Parse("Protobuf field number 0".into()));
        }
        let value = match key & 0x07 {
            0 => Wire::Varint(read_varint(buf, &mut pos)?),
            1 => {
                let bytes = take(buf, &mut pos, 8)?;
                Wire::Fixed64(u64::from_le_bytes(bytes.try_into().unwrap_or([0; 8])))
            }
            2 => {
                let len = read_varint(buf, &mut pos)?;
                let len = usize::try_from(len)
                    .map_err(|_| Error::Parse("Protobuf length too large".into()))?;
                Wire::Bytes(take(buf, &mut pos, len)?)
            }
            5 => {
                let bytes = take(buf, &mut pos, 4)?;
                Wire::Fixed32(u32::from_le_bytes(bytes.try_into().unwrap_or([0; 4])))
            }
            other => {
                return Err(Error::Parse(format!(
                    "Protobuf wire type {other} is not supported"
                )))
            }
        };
        f(field, value)?;
    }
    Ok(())
}

fn take<'a>(buf: &'a [u8], pos: &mut usize, len: usize) -> Result<&'a [u8], Error> {
    let end = pos
        .checked_add(len)
        .filter(|&e| e <= buf.len())
        .ok_or_else(|| Error::Parse("Protobuf field runs past the end of the message".into()))?;
    let slice = &buf[*pos..end];
    *pos = end;
    Ok(slice)
}

fn want_varint(field: u64, w: Wire<'_>) -> Result<u64, Error> {
    match w {
        Wire::Varint(v) => Ok(v),
        _ => Err(Error::Parse(format!(
            "Protobuf field {field} must be a varint"
        ))),
    }
}

fn want_bool(field: u64, w: Wire<'_>) -> Result<bool, Error> {
    Ok(want_varint(field, w)? != 0)
}

fn want_bytes<'a>(field: u64, w: Wire<'a>) -> Result<&'a [u8], Error> {
    match w {
        Wire::Bytes(b) => Ok(b),
        _ => Err(Error::Parse(format!(
            "Protobuf field {field} must be length-delimited"
        ))),
    }
}

fn want_text(field: u64, w: Wire<'_>) -> Result<String, Error> {
    let bytes = want_bytes(field, w)?;
    core::str::from_utf8(bytes)
        .map(|s| s.to_string())
        .map_err(|_| Error::Parse(format!("Protobuf field {field} is not UTF-8")))
}

/// The value field of a metric, before conversion by datatype.
enum RawValue {
    Int(u32),
    Long(u64),
    Float(f32),
    Double(f64),
    Bool(bool),
    Text(String),
    Bytes(Vec<u8>),
}

/// Sparkplug datatype numbers.
mod datatype {
    pub const INT8: u32 = 1;
    pub const INT16: u32 = 2;
    pub const INT32: u32 = 3;
    pub const INT64: u32 = 4;
    pub const UINT8: u32 = 5;
    pub const UINT16: u32 = 6;
    pub const UINT32: u32 = 7;
    pub const UINT64: u32 = 8;
    pub const FLOAT: u32 = 9;
    pub const DOUBLE: u32 = 10;
    pub const BOOLEAN: u32 = 11;
    pub const STRING: u32 = 12;
    pub const DATETIME: u32 = 13;
    pub const TEXT: u32 = 14;
    pub const UUID: u32 = 15;
    pub const DATASET: u32 = 16;
    pub const BYTES: u32 = 17;
    pub const FILE: u32 = 18;
    pub const TEMPLATE: u32 = 19;
}

fn mismatch(datatype: u32) -> Error {
    Error::Parse(format!(
        "Sparkplug value does not match datatype {datatype}"
    ))
}

/// Converts a raw value by datatype. Integers that do not fit `i64` are errors,
/// not silent changes.
fn convert(datatype: u32, raw: RawValue) -> Result<Option<TupValue>, Error> {
    let int = |v: i64| Ok(Some(TupValue::Int(v)));
    match (datatype, raw) {
        (datatype::INT8, RawValue::Int(v)) => int((v as u8 as i8) as i64),
        (datatype::INT16, RawValue::Int(v)) => int((v as u16 as i16) as i64),
        (datatype::INT32, RawValue::Int(v)) => int((v as i32) as i64),
        (datatype::UINT8, RawValue::Int(v)) => int((v as u8) as i64),
        (datatype::UINT16, RawValue::Int(v)) => int((v as u16) as i64),
        (datatype::UINT32, RawValue::Int(v)) => int(v as i64),
        (datatype::INT64, RawValue::Long(v)) => int(v as i64),
        (datatype::DATETIME, RawValue::Long(v)) => int(v as i64),
        (datatype::UINT64, RawValue::Long(v)) => {
            if v > i64::MAX as u64 {
                return Err(Error::Parse(
                    "Sparkplug UInt64 value exceeds the i64 range".into(),
                ));
            }
            int(v as i64)
        }
        (datatype::FLOAT, RawValue::Float(v)) => Ok(Some(TupValue::Float(v as f64))),
        (datatype::DOUBLE, RawValue::Double(v)) => Ok(Some(TupValue::Float(v))),
        (datatype::BOOLEAN, RawValue::Bool(v)) => Ok(Some(TupValue::Bool(v))),
        (datatype::STRING | datatype::TEXT | datatype::UUID, RawValue::Text(v)) => {
            Ok(Some(TupValue::Text(v)))
        }
        (datatype::BYTES | datatype::FILE, RawValue::Bytes(v)) => Ok(Some(TupValue::Bytes(v))),
        // Structured types and unknown datatypes are not decoded.
        (datatype::DATASET | datatype::TEMPLATE | 0, _) => Ok(None),
        (other, _) => Err(mismatch(other)),
    }
}

fn decode_metric(bytes: &[u8]) -> Result<Metric, Error> {
    let mut name = None;
    let mut alias = None;
    let mut timestamp = None;
    let mut datatype_value = 0u32;
    let mut is_null = false;
    let mut raw: Option<RawValue> = None;

    for_each_field(bytes, |field, w| {
        match field {
            1 => name = Some(want_text(field, w)?),
            2 => alias = Some(want_varint(field, w)?),
            3 => timestamp = Some(want_varint(field, w)?),
            4 => {
                let v = want_varint(field, w)?;
                datatype_value = u32::try_from(v)
                    .map_err(|_| Error::Parse("Sparkplug datatype out of range".into()))?;
            }
            5 | 6 => {
                // is_historical, is_transient: informational only.
                want_bool(field, w)?;
            }
            7 => is_null = want_bool(field, w)?,
            10 => raw = Some(RawValue::Int(want_varint(field, w)? as u32)),
            11 => raw = Some(RawValue::Long(want_varint(field, w)?)),
            12 => match w {
                Wire::Fixed32(v) => raw = Some(RawValue::Float(f32::from_bits(v))),
                _ => return Err(Error::Parse("Sparkplug float must be fixed32".into())),
            },
            13 => match w {
                Wire::Fixed64(v) => raw = Some(RawValue::Double(f64::from_bits(v))),
                _ => return Err(Error::Parse("Sparkplug double must be fixed64".into())),
            },
            14 => raw = Some(RawValue::Bool(want_bool(field, w)?)),
            15 => raw = Some(RawValue::Text(want_text(field, w)?)),
            16 => raw = Some(RawValue::Bytes(want_bytes(field, w)?.to_vec())),
            // Metadata, properties, dataset, template and extension: not decoded.
            _ => {}
        }
        Ok(())
    })?;

    let name = name.ok_or_else(|| Error::Parse("Sparkplug metric has no name".into()))?;
    let value = match (is_null, raw) {
        (true, _) | (_, None) => None,
        (false, Some(v)) => convert(datatype_value, v)?,
    };
    Ok(Metric {
        name,
        alias,
        timestamp,
        datatype: datatype_value,
        value,
    })
}

/// Decodes a Sparkplug B payload (the Protobuf `Payload` message).
fn decode_payload(bytes: &[u8], topic: Option<SparkplugTopic>) -> Result<SparkplugFrame, Error> {
    let mut frame = SparkplugFrame {
        topic,
        timestamp: None,
        seq: None,
        uuid: None,
        metrics: Vec::new(),
        body: None,
    };
    for_each_field(bytes, |field, w| {
        match field {
            1 => frame.timestamp = Some(want_varint(field, w)?),
            2 => frame.metrics.push(decode_metric(want_bytes(field, w)?)?),
            3 => {
                let seq = want_varint(field, w)?;
                if seq > 255 {
                    return Err(Error::Parse("Sparkplug seq must be 0-255".into()));
                }
                frame.seq = Some(seq);
            }
            4 => frame.uuid = Some(want_text(field, w)?),
            5 => frame.body = Some(want_bytes(field, w)?.to_vec()),
            // Unknown fields are skipped, as Protobuf requires.
            _ => {}
        }
        Ok(())
    })?;
    Ok(frame)
}

/// Decodes Sparkplug B payloads. The payload bytes alone are enough; use
/// [`decode`] to attach the topic.
pub struct SparkplugParser;

impl FrameParser for SparkplugParser {
    type Frame = SparkplugFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        decode_payload(bytes, None)
    }
}

/// Decodes a payload together with its MQTT topic.
pub fn decode(topic: &str, payload: &[u8]) -> Result<SparkplugFrame, Error> {
    let topic = SparkplugTopic::parse(topic)?;
    decode_payload(payload, Some(topic))
}

/// Sparkplug B connection state.
#[derive(Debug, Clone, PartialEq)]
pub struct SparkplugConnectionState {
    pub connected: bool,
    pub last_seq: u16,
    pub birth_received: bool,
}

pub struct SparkplugValidator;

impl StateValidator for SparkplugValidator {
    type State = SparkplugConnectionState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("sparkplug: not connected".into()));
        }
        if state.last_seq > 255 {
            return Err(Error::State("sparkplug: seq out of range".into()));
        }
        Ok(())
    }
}

/// Normalizes Sparkplug B metrics into TUP DataPoints. The device is the
/// topic's group and edge node (and device, when present). The point is the
/// metric name.
pub struct SparkplugNormalizer;

impl SparkplugNormalizer {
    fn device_id(frame: &SparkplugFrame) -> String {
        match &frame.topic {
            Some(t) => match &t.device_id {
                Some(d) => format!("sparkplug:{}/{}/{}", t.group_id, t.edge_node_id, d),
                None => format!("sparkplug:{}/{}", t.group_id, t.edge_node_id),
            },
            None => "sparkplug:unknown".to_string(),
        }
    }

    /// One data point per metric that has a value. Null metrics and
    /// structured types are skipped.
    pub fn points(&self, frame: &SparkplugFrame) -> Vec<DataPoint> {
        let device_id = Self::device_id(frame);
        frame
            .metrics
            .iter()
            .filter_map(|m| {
                let value = m.value.clone()?;
                Some(DataPoint::new(
                    SourceProtocol::Sparkplugb,
                    device_id.clone(),
                    m.name.clone(),
                    m.timestamp.or(frame.timestamp).unwrap_or(0),
                    value,
                    Quality::Good,
                ))
            })
            .collect()
    }
}

impl DataNormalizer for SparkplugNormalizer {
    type Frame = SparkplugFrame;

    /// Every metric with a value. A payload with none (such as a death
    /// certificate) yields the sequence point, as `normalize` does.
    fn normalize_all(&self, frame: &Self::Frame) -> Result<Vec<DataPoint>, Error> {
        let points = self.points(frame);
        if points.is_empty() {
            return Ok(alloc::vec![self.normalize(frame)?]);
        }
        Ok(points)
    }

    /// The first metric with a value. A payload with no such metric (for
    /// example a death certificate) yields a point for its sequence number, if
    /// it has one. A payload with neither has nothing to report.
    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        if let Some(point) = self.points(frame).into_iter().next() {
            return Ok(point);
        }
        let Some(seq) = frame.seq else {
            return Err(Error::Normalize(
                "sparkplugb payload has no metric and no sequence number".into(),
            ));
        };
        Ok(DataPoint::new(
            SourceProtocol::Sparkplugb,
            Self::device_id(frame),
            format!("seq:{seq}"),
            frame.timestamp.unwrap_or(0),
            TupValue::Int(seq as i64),
            Quality::Good,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put_varint(out: &mut Vec<u8>, mut v: u64) {
        loop {
            let byte = (v & 0x7F) as u8;
            v >>= 7;
            if v == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }

    fn put_key(out: &mut Vec<u8>, field: u64, wire: u64) {
        put_varint(out, (field << 3) | wire);
    }

    fn put_varint_field(out: &mut Vec<u8>, field: u64, v: u64) {
        put_key(out, field, 0);
        put_varint(out, v);
    }

    fn put_bytes_field(out: &mut Vec<u8>, field: u64, data: &[u8]) {
        put_key(out, field, 2);
        put_varint(out, data.len() as u64);
        out.extend_from_slice(data);
    }

    /// A Metric with a name, datatype and value written by `value`.
    fn metric(name: &str, datatype: u64, value: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
        let mut m = Vec::new();
        put_bytes_field(&mut m, 1, name.as_bytes());
        put_varint_field(&mut m, 4, datatype);
        value(&mut m);
        m
    }

    fn payload(metrics: &[Vec<u8>], seq: u64) -> Vec<u8> {
        let mut p = Vec::new();
        put_varint_field(&mut p, 1, 1_700_000_000_000);
        for m in metrics {
            put_bytes_field(&mut p, 2, m);
        }
        put_varint_field(&mut p, 3, seq);
        p
    }

    fn double_metric(name: &str, v: f64) -> Vec<u8> {
        metric(name, 10, |m| {
            put_key(m, 13, 1);
            m.extend_from_slice(&v.to_bits().to_le_bytes());
        })
    }

    #[test]
    fn varint_decodes_multi_byte_values() {
        let mut pos = 0;
        assert_eq!(read_varint(&[0xAC, 0x02], &mut pos).unwrap(), 300);
        assert_eq!(pos, 2);
    }

    #[test]
    fn truncated_varint_is_rejected() {
        assert!(read_varint(&[0x80], &mut 0).is_err());
    }

    #[test]
    fn topic_parses_node_and_device_forms() {
        let node = SparkplugTopic::parse("spBv1.0/Plant1/NDATA/Edge7").unwrap();
        assert_eq!(node.group_id, "Plant1");
        assert_eq!(node.message_type, SparkplugMessageType::NData);
        assert_eq!(node.edge_node_id, "Edge7");
        assert_eq!(node.device_id, None);

        let device = SparkplugTopic::parse("spBv1.0/Plant1/DDATA/Edge7/Pump3").unwrap();
        assert_eq!(device.device_id.as_deref(), Some("Pump3"));
    }

    #[test]
    fn topic_rules_are_enforced() {
        assert!(SparkplugTopic::parse("mqtt/Plant1/NDATA/Edge7").is_err());
        assert!(SparkplugTopic::parse("spBv1.0/Plant1/NDATA/+").is_err());
        assert!(SparkplugTopic::parse("spBv1.0/Plant1/NDATA/Edge7/Pump3").is_err());
        assert!(SparkplugTopic::parse("spBv1.0/Plant1/DDATA/Edge7").is_err());
        assert!(SparkplugTopic::parse("spBv1.0/Plant1/STATE/Edge7").is_err());
    }

    #[test]
    fn birth_payload_decodes_metrics_by_datatype() {
        let bytes = payload(
            &[
                double_metric("Temp", 21.5),
                metric("Count", 6, |m| put_varint_field(m, 10, 500)),
                metric("Flag", 11, |m| put_varint_field(m, 14, 1)),
                metric("Label", 12, |m| put_bytes_field(m, 15, b"ok")),
            ],
            7,
        );
        let frame = decode("spBv1.0/Plant1/NBIRTH/Edge7", &bytes).unwrap();
        assert_eq!(frame.seq, Some(7));
        assert_eq!(frame.timestamp, Some(1_700_000_000_000));
        assert_eq!(frame.metrics[0].value, Some(TupValue::Float(21.5)));
        assert_eq!(frame.metrics[1].value, Some(TupValue::Int(500)));
        assert_eq!(frame.metrics[2].value, Some(TupValue::Bool(true)));
        assert_eq!(frame.metrics[3].value, Some(TupValue::Text("ok".into())));
    }

    #[test]
    fn signed_integer_datatypes_are_sign_extended() {
        // Int8 -1 is carried as the uint32 0xFFFFFFFF.
        let bytes = payload(
            &[metric("I8", 1, |m| put_varint_field(m, 10, 0xFFFF_FFFF))],
            0,
        );
        let frame = SparkplugParser.parse(&bytes).unwrap();
        assert_eq!(frame.metrics[0].value, Some(TupValue::Int(-1)));
    }

    #[test]
    fn uint64_above_i64_max_is_rejected() {
        let bytes = payload(&[metric("U", 8, |m| put_varint_field(m, 11, u64::MAX))], 0);
        assert!(SparkplugParser.parse(&bytes).is_err());
    }

    #[test]
    fn null_metric_has_no_value() {
        let bytes = payload(
            &[metric("Gone", 3, |m| {
                put_varint_field(m, 7, 1); // is_null
            })],
            0,
        );
        let frame = SparkplugParser.parse(&bytes).unwrap();
        assert_eq!(frame.metrics[0].value, None);
        assert_eq!(frame.metrics[0].name, "Gone");
    }

    #[test]
    fn unknown_fields_are_skipped() {
        let mut bytes = payload(&[double_metric("T", 1.0)], 0);
        put_varint_field(&mut bytes, 99, 12345); // not a Sparkplug field
        assert!(SparkplugParser.parse(&bytes).is_ok());
    }

    #[test]
    fn wrong_wire_type_for_a_known_field_is_rejected() {
        let mut bytes = Vec::new();
        put_bytes_field(&mut bytes, 1, b"not a timestamp");
        assert!(SparkplugParser.parse(&bytes).is_err());
    }

    #[test]
    fn length_past_the_end_is_rejected() {
        let mut bytes = Vec::new();
        put_key(&mut bytes, 4, 2);
        put_varint(&mut bytes, 50);
        bytes.extend_from_slice(b"short");
        assert!(SparkplugParser.parse(&bytes).is_err());
    }

    #[test]
    fn seq_above_255_is_rejected() {
        assert!(SparkplugParser.parse(&payload(&[], 256)).is_err());
    }

    #[test]
    fn datatype_mismatch_is_rejected() {
        // Double datatype carrying an integer value.
        let bytes = payload(&[metric("X", 10, |m| put_varint_field(m, 10, 1))], 0);
        assert!(SparkplugParser.parse(&bytes).is_err());
    }

    #[test]
    fn normalizer_emits_one_point_per_metric_with_device_from_topic() {
        let bytes = payload(
            &[
                double_metric("Temp", 21.5),
                double_metric("Pressure", 101.3),
            ],
            3,
        );
        let frame = decode("spBv1.0/Plant1/DDATA/Edge7/Pump3", &bytes).unwrap();
        let points = SparkplugNormalizer.points(&frame);
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].device_id, "sparkplug:Plant1/Edge7/Pump3");
        assert_eq!(points[0].point_id, "Temp");
        assert_eq!(points[0].timestamp, 1_700_000_000_000);
        assert_eq!(points[1].point_id, "Pressure");
    }

    #[test]
    fn normalize_batch_fans_out_every_metric() {
        let bytes = payload(&[double_metric("A", 1.0), double_metric("B", 2.0)], 1);
        let frame = decode("spBv1.0/G/NDATA/E", &bytes).unwrap();
        let points = SparkplugNormalizer.normalize_batch(&[frame]).unwrap();
        assert_eq!(points.len(), 2);
    }

    #[test]
    fn normalize_without_metrics_reports_the_sequence() {
        let frame = decode("spBv1.0/Plant1/NDEATH/Edge7", &payload(&[], 9)).unwrap();
        let dp = SparkplugNormalizer.normalize(&frame).unwrap();
        assert_eq!(dp.point_id, "seq:9");
        assert_eq!(dp.value, TupValue::Int(9));
    }

    #[test]
    fn empty_payload_with_no_sequence_is_an_error_not_a_zero() {
        // No metric and no sequence number: nothing to report, not "seq:0 = 0".
        let frame = decode("spBv1.0/Plant1/NDEATH/Edge7", &[]).unwrap();
        assert!(SparkplugNormalizer.normalize(&frame).is_err());
    }

    #[test]
    fn validator_connected_and_seq_range() {
        let ok = SparkplugConnectionState {
            connected: true,
            last_seq: 255,
            birth_received: true,
        };
        assert!(SparkplugValidator.validate(&ok).is_ok());
        let down = SparkplugConnectionState {
            connected: false,
            ..ok.clone()
        };
        assert!(SparkplugValidator.validate(&down).is_err());
        let bad = SparkplugConnectionState {
            last_seq: 256,
            ..ok
        };
        assert!(SparkplugValidator.validate(&bad).is_err());
    }

    #[test]
    fn garbage_is_rejected_without_panic() {
        assert!(SparkplugParser.parse(&[0xFF; 32]).is_err());
        assert!(SparkplugParser.parse(&[0x0B, 0x80]).is_err());
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
            let _ = SparkplugParser.parse(&bytes);
        }
    }
}
