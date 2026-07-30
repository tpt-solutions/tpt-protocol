//! IEEE 11073 SDC protocol implementation for medical device interoperability.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SdcMessageType {
    Mdib,
    Waveform,
    Alert,
    Context,
}

impl SdcMessageType {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Mdib),
            1 => Some(Self::Waveform),
            2 => Some(Self::Alert),
            3 => Some(Self::Context),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Mdib => "mdib",
            Self::Waveform => "waveform",
            Self::Alert => "alert",
            Self::Context => "context",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SdcMetricValue {
    pub handle: String,
    pub descriptor_handle: String,
    pub value: f64,
    pub unit: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WaveformSample {
    pub handle: String,
    pub samples: Vec<f32>,
    pub sample_rate: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlertState {
    pub handle: String,
    pub descriptor_handle: String,
    pub active: bool,
    pub priority: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SdcBody {
    NumericMetrics(Vec<SdcMetricValue>),
    Waveforms(Vec<WaveformSample>),
    Alerts(Vec<AlertState>),
    ContextInfo(Vec<(String, String)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SdcFrame {
    pub message_type: SdcMessageType,
    pub timestamp: u64,
    pub body: SdcBody,
}

pub struct SdcParser;

impl FrameParser for SdcParser {
    type Frame = SdcFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 9 {
            return Err(Error::Parse("sdc frame too short".into()));
        }

        let message_type = SdcMessageType::from_u8(bytes[0])
            .ok_or_else(|| Error::Parse("invalid sdc message type".into()))?;

        let timestamp = u64::from_be_bytes([
            bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8],
        ]);

        let body = match message_type {
            SdcMessageType::Mdib | SdcMessageType::Context => {
                if bytes.len() < 11 {
                    return Err(Error::Parse("truncated metric count".into()));
                }
                let count = u16::from_be_bytes([bytes[9], bytes[10]]) as usize;
                let mut offset = 11usize;
                let mut metrics = Vec::with_capacity(count);
                for _ in 0..count {
                    if offset + 2 > bytes.len() {
                        return Err(Error::Parse("truncated handle length".into()));
                    }
                    let hlen = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
                    offset += 2;
                    if offset + hlen > bytes.len() {
                        return Err(Error::Parse("truncated handle".into()));
                    }
                    let handle = core::str::from_utf8(&bytes[offset..offset + hlen])
                        .map_err(|_| Error::Parse("invalid handle utf-8".into()))?;
                    offset += hlen;

                    if offset + 2 > bytes.len() {
                        return Err(Error::Parse("truncated descriptor handle len".into()));
                    }
                    let dlen = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
                    offset += 2;
                    if offset + dlen > bytes.len() {
                        return Err(Error::Parse("truncated descriptor handle".into()));
                    }
                    let descriptor_handle = core::str::from_utf8(&bytes[offset..offset + dlen])
                        .map_err(|_| Error::Parse("invalid descriptor utf-8".into()))?;
                    offset += dlen;

                    if offset + 8 > bytes.len() {
                        return Err(Error::Parse("truncated metric value".into()));
                    }
                    let value_bits = u64::from_be_bytes([
                        bytes[offset],
                        bytes[offset + 1],
                        bytes[offset + 2],
                        bytes[offset + 3],
                        bytes[offset + 4],
                        bytes[offset + 5],
                        bytes[offset + 6],
                        bytes[offset + 7],
                    ]);
                    offset += 8;
                    let value = f64::from_bits(value_bits);

                    if offset + 2 > bytes.len() {
                        return Err(Error::Parse("truncated unit length".into()));
                    }
                    let ulen = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
                    offset += 2;
                    if offset + ulen > bytes.len() {
                        return Err(Error::Parse("truncated unit".into()));
                    }
                    let unit = core::str::from_utf8(&bytes[offset..offset + ulen])
                        .map_err(|_| Error::Parse("invalid unit utf-8".into()))?
                        .into();
                    offset += ulen;

                    metrics.push(SdcMetricValue {
                        handle: handle.into(),
                        descriptor_handle: descriptor_handle.into(),
                        value,
                        unit,
                    });
                }
                if message_type == SdcMessageType::Context {
                    let pairs = metrics.into_iter().map(|m| (m.handle, m.unit)).collect();
                    SdcBody::ContextInfo(pairs)
                } else {
                    SdcBody::NumericMetrics(metrics)
                }
            }
            SdcMessageType::Waveform => {
                if bytes.len() < 11 {
                    return Err(Error::Parse("truncated waveform count".into()));
                }
                let count = u16::from_be_bytes([bytes[9], bytes[10]]) as usize;
                let mut offset = 11usize;
                let mut waveforms = Vec::with_capacity(count);
                for _ in 0..count {
                    if offset + 2 > bytes.len() {
                        return Err(Error::Parse("truncated waveform handle".into()));
                    }
                    let hlen = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
                    offset += 2;
                    if offset + hlen > bytes.len() {
                        return Err(Error::Parse("truncated waveform handle str".into()));
                    }
                    let handle = core::str::from_utf8(&bytes[offset..offset + hlen])
                        .map_err(|_| Error::Parse("invalid waveform handle utf-8".into()))?;
                    offset += hlen;

                    if offset + 4 > bytes.len() {
                        return Err(Error::Parse("truncated sample rate".into()));
                    }
                    let rate_bits = u32::from_be_bytes([
                        bytes[offset],
                        bytes[offset + 1],
                        bytes[offset + 2],
                        bytes[offset + 3],
                    ]);
                    offset += 4;
                    let sample_rate = f32::from_bits(rate_bits);

                    if offset + 2 > bytes.len() {
                        return Err(Error::Parse("truncated sample count".into()));
                    }
                    let sample_count =
                        u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
                    offset += 2;
                    if offset + sample_count * 4 > bytes.len() {
                        return Err(Error::Parse("truncated samples".into()));
                    }
                    let mut samples = Vec::with_capacity(sample_count);
                    for _ in 0..sample_count {
                        let bits = u32::from_be_bytes([
                            bytes[offset],
                            bytes[offset + 1],
                            bytes[offset + 2],
                            bytes[offset + 3],
                        ]);
                        offset += 4;
                        samples.push(f32::from_bits(bits));
                    }

                    waveforms.push(WaveformSample {
                        handle: handle.into(),
                        samples,
                        sample_rate,
                    });
                }
                SdcBody::Waveforms(waveforms)
            }
            SdcMessageType::Alert => {
                if bytes.len() < 11 {
                    return Err(Error::Parse("truncated alert count".into()));
                }
                let count = u16::from_be_bytes([bytes[9], bytes[10]]) as usize;
                let mut offset = 11usize;
                let mut alerts = Vec::with_capacity(count);
                for _ in 0..count {
                    if offset + 2 > bytes.len() {
                        return Err(Error::Parse("truncated alert handle".into()));
                    }
                    let hlen = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
                    offset += 2;
                    if offset + hlen > bytes.len() {
                        return Err(Error::Parse("truncated alert handle str".into()));
                    }
                    let handle = core::str::from_utf8(&bytes[offset..offset + hlen])
                        .map_err(|_| Error::Parse("invalid alert handle utf-8".into()))?;
                    offset += hlen;

                    if offset + 2 > bytes.len() {
                        return Err(Error::Parse("truncated descriptor handle len".into()));
                    }
                    let dlen = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) as usize;
                    offset += 2;
                    if offset + dlen > bytes.len() {
                        return Err(Error::Parse("truncated alert desc handle".into()));
                    }
                    let descriptor_handle = core::str::from_utf8(&bytes[offset..offset + dlen])
                        .map_err(|_| Error::Parse("invalid alert desc utf-8".into()))?;
                    offset += dlen;

                    if offset >= bytes.len() {
                        return Err(Error::Parse("truncated alert active".into()));
                    }
                    let active = bytes[offset] != 0;
                    offset += 1;

                    if offset >= bytes.len() {
                        return Err(Error::Parse("truncated alert priority".into()));
                    }
                    let priority = bytes[offset];
                    offset += 1;

                    alerts.push(AlertState {
                        handle: handle.into(),
                        descriptor_handle: descriptor_handle.into(),
                        active,
                        priority,
                    });
                }
                SdcBody::Alerts(alerts)
            }
        };

        Ok(SdcFrame {
            message_type,
            timestamp,
            body,
        })
    }
}

pub struct SdcValidator;

impl StateValidator for SdcValidator {
    type State = SdcConnectionState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("sdc: not connected".into()));
        }
        if state.metric_count > 1000 {
            return Err(Error::State("sdc: too many metrics".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SdcConnectionState {
    pub connected: bool,
    pub metric_count: usize,
    pub last_sequence_id: u64,
}

pub struct SdcNormalizer;

impl DataNormalizer for SdcNormalizer {
    type Frame = SdcFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = alloc::format!("sdc/{}", frame.message_type.as_str());

        let (point_id, value, unit) = match &frame.body {
            SdcBody::NumericMetrics(metrics) => {
                if metrics.is_empty() {
                    return Err(Error::Normalize("no metrics to normalize".into()));
                }
                let m = &metrics[0];
                (
                    m.handle.clone(),
                    TupValue::Float(m.value),
                    Some(m.unit.clone()),
                )
            }
            SdcBody::Waveforms(waveforms) => {
                if waveforms.is_empty() {
                    return Err(Error::Normalize("no waveforms to normalize".into()));
                }
                let w = &waveforms[0];
                let sum: f32 = w.samples.iter().sum();
                let avg = if w.samples.is_empty() {
                    0.0
                } else {
                    sum / w.samples.len() as f32
                };
                (
                    w.handle.clone(),
                    TupValue::Float(avg as f64),
                    Some("mV".into()),
                )
            }
            SdcBody::Alerts(alerts) => {
                if alerts.is_empty() {
                    return Err(Error::Normalize("no alerts to normalize".into()));
                }
                let a = &alerts[0];
                (a.handle.clone(), TupValue::Bool(a.active), None)
            }
            SdcBody::ContextInfo(pairs) => {
                if pairs.is_empty() {
                    return Err(Error::Normalize("no context to normalize".into()));
                }
                let (k, v) = &pairs[0];
                (k.clone(), TupValue::Text(v.clone()), None)
            }
        };

        let mut dp = DataPoint::new(
            SourceProtocol::Ieee11073Sdc,
            device_id,
            point_id,
            frame.timestamp,
            value,
            Quality::Good,
        );
        if let Some(u) = unit {
            dp = dp.with_unit(u);
        }
        Ok(dp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_numeric_frame() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(0); // Mdib
        buf.extend(&1000000u64.to_be_bytes()); // timestamp
        buf.extend(&1u16.to_be_bytes()); // 1 metric
        buf.extend(&4u16.to_be_bytes()); // handle len
        buf.extend(b"temp");
        buf.extend(&7u16.to_be_bytes()); // descriptor handle len
        buf.extend(b"descr001");
        buf.extend(&37.5f64.to_bits().to_be_bytes()); // value
        buf.extend(&2u16.to_be_bytes()); // unit len
        buf.extend(b"C"); // unit
        buf
    }

    fn make_waveform_frame() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(1); // Waveform
        buf.extend(&2000000u64.to_be_bytes());
        buf.extend(&1u16.to_be_bytes()); // 1 waveform
        buf.extend(&3u16.to_be_bytes()); // handle len
        buf.extend(b"ecg");
        buf.extend(&250.0f32.to_bits().to_be_bytes()); // sample rate
        buf.extend(&3u16.to_be_bytes()); // 3 samples
        buf.extend(&1.0f32.to_bits().to_be_bytes());
        buf.extend(&2.0f32.to_bits().to_be_bytes());
        buf.extend(&3.0f32.to_bits().to_be_bytes());
        buf
    }

    fn make_alert_frame() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(2); // Alert
        buf.extend(&3000000u64.to_be_bytes());
        buf.extend(&1u16.to_be_bytes()); // 1 alert
        buf.extend(&3u16.to_be_bytes());
        buf.extend(b"a01");
        buf.extend(&3u16.to_be_bytes());
        buf.extend(b"d01");
        buf.push(1); // active
        buf.push(2); // priority
        buf
    }

    #[test]
    fn valid_parse_numeric() {
        let parser = SdcParser;
        let bytes = make_numeric_frame();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.message_type, SdcMessageType::Mdib);
        assert_eq!(frame.timestamp, 1000000);
    }

    #[test]
    fn valid_parse_waveform() {
        let parser = SdcParser;
        let bytes = make_waveform_frame();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.message_type, SdcMessageType::Waveform);
        match &frame.body {
            SdcBody::Waveforms(w) => {
                assert_eq!(w[0].handle, "ecg");
                assert_eq!(w[0].samples.len(), 3);
            }
            _ => panic!("expected waveforms"),
        }
    }

    #[test]
    fn invalid_frame_too_short() {
        let parser = SdcParser;
        let result = parser.parse(&[0, 0]);
        assert!(result.is_err());
    }

    #[test]
    fn truncation() {
        let mut bytes = make_numeric_frame();
        bytes.truncate(12);
        let parser = SdcParser;
        let result = parser.parse(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn garbage() {
        let parser = SdcParser;
        let result = parser.parse(&[0xff; 64]);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_roundtrip_numeric() {
        let parser = SdcParser;
        let normalizer = SdcNormalizer;
        let bytes = make_numeric_frame();
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Ieee11073Sdc);
        assert_eq!(dp.point_id, "temp");
    }

    #[test]
    fn normalize_roundtrip_alert() {
        let parser = SdcParser;
        let normalizer = SdcNormalizer;
        let bytes = make_alert_frame();
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.point_id, "a01");
        assert_eq!(dp.value, TupValue::Bool(true));
    }

    #[test]
    fn validator_ok() {
        let v = SdcValidator;
        let s = SdcConnectionState {
            connected: true,
            metric_count: 10,
            last_sequence_id: 0,
        };
        assert!(v.validate(&s).is_ok());
    }

    #[test]
    fn validator_fail() {
        let v = SdcValidator;
        let s = SdcConnectionState {
            connected: false,
            metric_count: 0,
            last_sequence_id: 0,
        };
        assert!(v.validate(&s).is_err());
    }
}
