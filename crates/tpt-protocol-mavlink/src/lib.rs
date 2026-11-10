//! MAVLink protocol implementation for drone and unmanned vehicle telemetry.
//!
//! Frame decoding (magic, header, CRC and CRC_EXTRA validation) is delegated
//! to the [`mavlink`] crate. This crate keeps the decoded message and the v2
//! signature, and maps the message onto the TUP data model.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use mavlink::common::MavMessage;
use mavlink::peek_reader::PeekReader;
use mavlink::{MavlinkVersion as WireVersion, Message as _};
use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// Largest MAVLink payload (v1 and v2 both cap at 255 bytes).
const MAX_PAYLOAD: usize = 255;
/// v2 incompatibility flag: the frame carries a 13-byte signature.
const IFLAG_SIGNED: u8 = 0x01;
/// v2 header length, including the magic byte.
const V2_HEADER: usize = 10;
/// Signature block: link id (1), timestamp (6), signature value (6).
const SIGNATURE_LEN: usize = 13;

/// MAVLink protocol version.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MavlinkVersion {
    V1,
    V2,
}

/// The 13-byte v2 signature block of a signed frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MavlinkSignature {
    pub link_id: u8,
    /// 48-bit timestamp (10 microsecond units since 2015-01-01 UTC).
    pub timestamp: u64,
    /// The 48-bit signature value.
    pub value: [u8; 6],
}

/// A decoded MAVLink frame (v1 or v2).
#[derive(Clone, Debug, PartialEq)]
pub struct MavlinkFrame {
    pub version: MavlinkVersion,
    pub sequence: u8,
    pub system_id: u8,
    pub component_id: u8,
    /// Numeric message id from the header.
    pub message_id: u32,
    /// Name from the MAVLink dialect (for example `HEARTBEAT`).
    pub message_name: &'static str,
    /// The decoded message with its typed fields.
    pub message: MavMessage,
    /// Message payload, re-serialized from the decoded message.
    pub payload: Vec<u8>,
    /// Present only on signed v2 frames.
    pub signature: Option<MavlinkSignature>,
}

impl fmt::Display for MavlinkFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "MAVLink[ver={}, sys={}, comp={}, msg={}({}), seq={}, {} bytes{}]",
            if self.version == MavlinkVersion::V1 {
                "v1"
            } else {
                "v2"
            },
            self.system_id,
            self.component_id,
            self.message_name,
            self.message_id,
            self.sequence,
            self.payload.len(),
            if self.signature.is_some() {
                ", signed"
            } else {
                ""
            },
        )
    }
}

/// MAVLink frame parser (auto-detects v1 vs v2 by magic byte).
#[derive(Clone, Debug, Default)]
pub struct MavlinkParser;

impl MavlinkParser {
    pub fn new() -> Self {
        Self
    }
}

impl FrameParser for MavlinkParser {
    type Frame = MavlinkFrame;

    /// Parses exactly one frame. Trailing bytes are an error.
    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        let version = match bytes.first() {
            None => return Err(Error::Parse("MAVLink frame empty".into())),
            Some(0xFE) => MavlinkVersion::V1,
            Some(0xFD) => MavlinkVersion::V2,
            Some(magic) => {
                return Err(Error::Parse(format!(
                    "MAVLink invalid magic byte: 0x{magic:02X}",
                )));
            }
        };

        let (header, msg, signature, consumed) = match version {
            MavlinkVersion::V1 => {
                let mut reader = PeekReader::new(bytes);
                let (header, msg) = mavlink::read_v1_msg::<MavMessage, _>(&mut reader)
                    .map_err(|e| Error::Parse(format!("MAVLink v1 decode failed: {e:?}")))?;
                (header, msg, None, frame_len_v1(bytes))
            }
            MavlinkVersion::V2 => {
                // read_v2_msg checks the CRC and decodes the message. The
                // signature block is not decoded there (that needs std), so it
                // is read from its fixed position: after the header, payload
                // and CRC.
                let mut reader = PeekReader::new(bytes);
                let (header, msg) = mavlink::read_v2_msg::<MavMessage, _>(&mut reader)
                    .map_err(|e| Error::Parse(format!("MAVLink v2 decode failed: {e:?}")))?;
                let base = V2_HEADER + bytes[1] as usize + 2;
                let (signature, consumed) = if bytes[2] & IFLAG_SIGNED != 0 {
                    let block = bytes
                        .get(base..base + SIGNATURE_LEN)
                        .ok_or_else(|| Error::Parse("MAVLink signature is truncated".into()))?;
                    let mut ts = [0u8; 8];
                    ts[..6].copy_from_slice(&block[1..7]);
                    let mut value = [0u8; 6];
                    value.copy_from_slice(&block[7..13]);
                    (
                        Some(MavlinkSignature {
                            link_id: block[0],
                            timestamp: u64::from_le_bytes(ts),
                            value,
                        }),
                        base + SIGNATURE_LEN,
                    )
                } else {
                    (None, base)
                };
                (header, msg, signature, consumed)
            }
        };
        if consumed != bytes.len() {
            return Err(Error::Parse(format!(
                "MAVLink has {} trailing bytes after the frame",
                bytes.len().saturating_sub(consumed),
            )));
        }

        let wire_version = match version {
            MavlinkVersion::V1 => WireVersion::V1,
            MavlinkVersion::V2 => WireVersion::V2,
        };
        let mut payload = [0u8; MAX_PAYLOAD];
        let len = msg.ser(wire_version, &mut payload);

        Ok(MavlinkFrame {
            version,
            sequence: header.sequence,
            system_id: header.system_id,
            component_id: header.component_id,
            message_id: msg.message_id(),
            message_name: msg.message_name(),
            message: msg,
            payload: payload[..len].to_vec(),
            signature,
        })
    }
}

/// Length of a v1 frame: magic, length, sequence, system, component, message id, payload, CRC.
fn frame_len_v1(bytes: &[u8]) -> usize {
    match bytes.get(1) {
        Some(&len) => 8 + len as usize,
        None => 0,
    }
}

/// MAVLink connection state.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MavlinkState {
    pub connected: bool,
    pub system_id: u8,
    pub component_id: u8,
    pub heartbeat_received: bool,
    pub last_ok_timestamp: u64,
    pub error_count: u64,
}

/// Validates MAVLink connection health.
#[derive(Clone, Debug, Default)]
pub struct MavlinkValidator;

impl MavlinkValidator {
    pub fn new() -> Self {
        Self
    }
}

impl StateValidator for MavlinkValidator {
    type State = MavlinkState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("MAVLink device not connected".into()));
        }
        if !state.heartbeat_received {
            return Err(Error::State("MAVLink no HEARTBEAT received yet".into()));
        }
        if state.system_id == 0 {
            return Err(Error::State(format!(
                "MAVLink invalid system ID: {}",
                state.system_id,
            )));
        }
        if state.error_count > 15 {
            return Err(Error::State(format!(
                "MAVLink device has {} consecutive errors",
                state.error_count,
            )));
        }
        Ok(())
    }
}

/// Normalizes MAVLink frames into TUP DataPoints. A few common messages map
/// one typed field to a point. Other messages carry their payload bytes.
#[derive(Clone, Debug, Default)]
pub struct MavlinkNormalizer;

impl MavlinkNormalizer {
    pub fn new() -> Self {
        Self
    }
}

impl MavlinkNormalizer {
    /// Every point the frame carries. Heartbeat, system status, attitude and GPS
    /// messages give one point per field, with its unit. Other messages give the
    /// single point they always gave.
    pub fn points(&self, frame: &MavlinkFrame) -> Vec<DataPoint> {
        let device_id = format!("mavlink:{}:{}", frame.system_id, frame.component_id);
        let mut out = Vec::new();
        let mut push = |name: &str, value: TupValue, unit: Option<&str>| {
            let mut dp = DataPoint::new(
                SourceProtocol::Mavlink,
                device_id.clone(),
                name,
                0, // caller should set a real timestamp
                value,
                Quality::Good,
            );
            dp.unit = unit.map(String::from);
            out.push(dp);
        };

        match &frame.message {
            MavMessage::HEARTBEAT(d) => {
                push(
                    "HEARTBEAT.custom_mode",
                    TupValue::Int(i64::from(d.custom_mode)),
                    None,
                );
                push(
                    "HEARTBEAT.type",
                    TupValue::Int(i64::from(d.mavtype as u8)),
                    None,
                );
                push(
                    "HEARTBEAT.autopilot",
                    TupValue::Int(i64::from(d.autopilot as u8)),
                    None,
                );
                push(
                    "HEARTBEAT.base_mode",
                    TupValue::Int(i64::from(d.base_mode.bits())),
                    None,
                );
                push(
                    "HEARTBEAT.system_status",
                    TupValue::Int(i64::from(d.system_status as u8)),
                    None,
                );
                push(
                    "HEARTBEAT.mavlink_version",
                    TupValue::Int(i64::from(d.mavlink_version)),
                    None,
                );
            }
            MavMessage::SYS_STATUS(d) => {
                push(
                    "SYS_STATUS.voltage_battery",
                    TupValue::Int(i64::from(d.voltage_battery)),
                    Some("mV"),
                );
                push(
                    "SYS_STATUS.current_battery",
                    TupValue::Int(i64::from(d.current_battery)),
                    Some("cA"),
                );
                push(
                    "SYS_STATUS.battery_remaining",
                    TupValue::Int(i64::from(d.battery_remaining)),
                    Some("%"),
                );
                push(
                    "SYS_STATUS.load",
                    TupValue::Int(i64::from(d.load)),
                    Some("0.1%"),
                );
            }
            MavMessage::ATTITUDE(d) => {
                push(
                    "ATTITUDE.roll",
                    TupValue::Float(f64::from(d.roll)),
                    Some("rad"),
                );
                push(
                    "ATTITUDE.pitch",
                    TupValue::Float(f64::from(d.pitch)),
                    Some("rad"),
                );
                push(
                    "ATTITUDE.yaw",
                    TupValue::Float(f64::from(d.yaw)),
                    Some("rad"),
                );
            }
            MavMessage::GPS_RAW_INT(d) => {
                // Latitude and longitude are 1e-7 degrees, altitude is millimetres.
                push(
                    "GPS_RAW_INT.lat",
                    TupValue::Float(f64::from(d.lat) * 1e-7),
                    Some("deg"),
                );
                push(
                    "GPS_RAW_INT.lon",
                    TupValue::Float(f64::from(d.lon) * 1e-7),
                    Some("deg"),
                );
                push(
                    "GPS_RAW_INT.alt",
                    TupValue::Float(f64::from(d.alt) * 1e-3),
                    Some("m"),
                );
                push(
                    "GPS_RAW_INT.satellites_visible",
                    TupValue::Int(i64::from(d.satellites_visible)),
                    None,
                );
                push(
                    "GPS_RAW_INT.fix_type",
                    TupValue::Int(i64::from(d.fix_type as u8)),
                    None,
                );
            }
            _ => {
                let value = if frame.payload.is_empty() {
                    TupValue::Int(i64::from(frame.message_id))
                } else {
                    TupValue::Bytes(frame.payload.clone())
                };
                push(&format!("msg:{}", frame.message_name), value, None);
            }
        }
        out
    }
}

impl DataNormalizer for MavlinkNormalizer {
    type Frame = MavlinkFrame;

    /// The first point of the frame (the heartbeat's `custom_mode`, for example).
    /// Use [`DataNormalizer::normalize_all`] for every field.
    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        self.points(frame)
            .into_iter()
            .next()
            .ok_or_else(|| Error::Normalize("MAVLink message has no point".into()))
    }

    fn normalize_all(&self, frame: &Self::Frame) -> Result<Vec<DataPoint>, Error> {
        Ok(self.points(frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;
    use mavlink::common::{
        GpsFixType, MavAutopilot, MavModeFlag, MavState, MavType, ATTITUDE_DATA, GPS_RAW_INT_DATA,
        HEARTBEAT_DATA, SYS_STATUS_DATA,
    };
    use mavlink::MavHeader;

    fn heartbeat(custom_mode: u32) -> MavMessage {
        MavMessage::HEARTBEAT(HEARTBEAT_DATA {
            custom_mode,
            mavtype: MavType::MAV_TYPE_QUADROTOR,
            autopilot: MavAutopilot::MAV_AUTOPILOT_ARDUPILOTMEGA,
            base_mode: MavModeFlag::empty(),
            system_status: MavState::MAV_STATE_STANDBY,
            mavlink_version: 3,
        })
    }

    /// Encodes `msg` with the `mavlink` crate's writer, so CRC and CRC_EXTRA are correct.
    fn encode(version: WireVersion, sys: u8, comp: u8, msg: &MavMessage) -> Vec<u8> {
        let header = MavHeader {
            system_id: sys,
            component_id: comp,
            sequence: 1,
        };
        let mut out = [0u8; 280];
        let mut buf = &mut out[..];
        let written = match version {
            WireVersion::V1 => mavlink::write_v1_msg(&mut buf, header, msg).unwrap(),
            WireVersion::V2 => mavlink::write_v2_msg(&mut buf, header, msg).unwrap(),
        };
        out[..written].to_vec()
    }

    fn make_frame(version: WireVersion, sys: u8, comp: u8) -> Vec<u8> {
        encode(version, sys, comp, &heartbeat(0))
    }

    fn attitude(roll: f32) -> MavMessage {
        MavMessage::ATTITUDE(ATTITUDE_DATA {
            time_boot_ms: 1000,
            roll,
            pitch: 0.0,
            yaw: 0.0,
            rollspeed: 0.0,
            pitchspeed: 0.0,
            yawspeed: 0.0,
        })
    }

    /// Rewrites the CRC of an unsigned frame so it is computed with `extra`.
    /// v1 and v2 use the same CRC span: everything after the magic byte, up to the CRC.
    fn resign(bytes: &mut [u8], extra: u8) {
        let len = bytes.len();
        let crc = mavlink::calculate_crc(&bytes[1..len - 2], extra);
        bytes[len - 2..].copy_from_slice(&crc.to_le_bytes());
    }

    #[test]
    fn valid_v1_heartbeat() {
        let frame = MavlinkParser::new()
            .parse(&make_frame(WireVersion::V1, 1, 1))
            .unwrap();
        assert_eq!(frame.version, MavlinkVersion::V1);
        assert_eq!(frame.system_id, 1);
        assert_eq!(frame.component_id, 1);
        assert_eq!(frame.message_id, 0);
        assert_eq!(frame.message_name, "HEARTBEAT");
        assert_eq!(frame.signature, None);
    }

    #[test]
    fn valid_v2_heartbeat() {
        let frame = MavlinkParser::new()
            .parse(&make_frame(WireVersion::V2, 1, 1))
            .unwrap();
        assert_eq!(frame.version, MavlinkVersion::V2);
        assert_eq!(frame.message_name, "HEARTBEAT");
    }

    #[test]
    fn messages_beyond_heartbeat_are_named() {
        let cases = [
            (attitude(0.5), 30, "ATTITUDE"),
            (
                MavMessage::SYS_STATUS(SYS_STATUS_DATA {
                    voltage_battery: 11_800,
                    battery_remaining: 90,
                    ..Default::default()
                }),
                1,
                "SYS_STATUS",
            ),
        ];
        for (msg, id, name) in cases {
            let frame = MavlinkParser::new()
                .parse(&encode(WireVersion::V2, 1, 1, &msg))
                .unwrap();
            assert_eq!(frame.message_id, id, "{name}");
            assert_eq!(frame.message_name, name);
        }
    }

    #[test]
    fn decoded_fields_reach_the_normalizer() {
        let frame = MavlinkParser::new()
            .parse(&encode(WireVersion::V2, 1, 1, &attitude(0.5)))
            .unwrap();
        let dp = MavlinkNormalizer::new().normalize(&frame).unwrap();
        assert_eq!(dp.point_id, "ATTITUDE.roll");
        assert_eq!(dp.value, TupValue::Float(0.5));

        let frame = MavlinkParser::new()
            .parse(&encode(WireVersion::V1, 1, 1, &heartbeat(7)))
            .unwrap();
        let dp = MavlinkNormalizer::new().normalize(&frame).unwrap();
        assert_eq!(dp.point_id, "HEARTBEAT.custom_mode");
        assert_eq!(dp.value, TupValue::Int(7));
    }

    #[test]
    fn gps_raw_int_latitude_is_decoded() {
        // GPS_RAW_INT latitude is 1e-7 degrees, carried as a signed 32-bit integer.
        let msg = MavMessage::GPS_RAW_INT(GPS_RAW_INT_DATA {
            fix_type: GpsFixType::GPS_FIX_TYPE_3D_FIX,
            lat: -331_234_567,
            lon: 1_512_345_678,
            satellites_visible: 9,
            ..Default::default()
        });
        let frame = MavlinkParser::new()
            .parse(&encode(WireVersion::V2, 1, 1, &msg))
            .unwrap();
        let dp = MavlinkNormalizer::new().normalize(&frame).unwrap();
        assert_eq!(dp.point_id, "GPS_RAW_INT.lat");
        assert_eq!(dp.unit.as_deref(), Some("deg"));
        let TupValue::Float(lat) = dp.value else {
            panic!("latitude is a float in degrees")
        };
        assert!((lat + 33.123_456_7).abs() < 1e-9, "{lat}");
    }

    #[test]
    fn heartbeat_gives_every_field_as_its_own_point() {
        let frame = MavlinkParser::new()
            .parse(&encode(WireVersion::V2, 1, 1, &heartbeat(1234)))
            .unwrap();
        let names: Vec<String> = MavlinkNormalizer::new()
            .normalize_all(&frame)
            .unwrap()
            .into_iter()
            .map(|dp| dp.point_id)
            .collect();
        assert_eq!(
            names,
            [
                "HEARTBEAT.custom_mode",
                "HEARTBEAT.type",
                "HEARTBEAT.autopilot",
                "HEARTBEAT.base_mode",
                "HEARTBEAT.system_status",
                "HEARTBEAT.mavlink_version",
            ]
        );
    }

    #[test]
    fn invalid_magic_byte() {
        assert!(MavlinkParser::new().parse(&[0x00, 0x09, 0x00]).is_err());
    }

    #[test]
    fn truncated_v1() {
        let bytes = make_frame(WireVersion::V1, 1, 1);
        assert!(MavlinkParser::new()
            .parse(&bytes[..bytes.len() - 1])
            .is_err());
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        let mut bytes = make_frame(WireVersion::V2, 1, 1);
        bytes.push(0x00);
        assert!(MavlinkParser::new().parse(&bytes).is_err());
    }

    #[test]
    fn corrupt_crc() {
        let mut bytes = make_frame(WireVersion::V1, 1, 1);
        let n = bytes.len();
        bytes[n - 1] ^= 0xFF;
        assert!(MavlinkParser::new().parse(&bytes).is_err());
    }

    #[test]
    fn crc_extra_seed_is_enforced_for_every_tested_message() {
        // For each message and version, a frame with a self-consistent CRC under
        // the wrong CRC_EXTRA seed must be rejected.
        let cases: [(WireVersion, MavMessage); 4] = [
            (WireVersion::V1, heartbeat(0)),
            (WireVersion::V2, heartbeat(0)),
            (WireVersion::V1, attitude(0.0)),
            (WireVersion::V2, attitude(0.0)),
        ];
        for (version, msg) in cases {
            let id = msg.message_id();
            let correct = <MavMessage as mavlink::Message>::extra_crc(id);
            let mut good = encode(version, 1, 1, &msg);
            resign(&mut good, correct);
            assert!(
                MavlinkParser::new().parse(&good).is_ok(),
                "{version:?} id {id}"
            );
            let mut bad = encode(version, 1, 1, &msg);
            resign(&mut bad, correct.wrapping_add(1));
            assert!(
                MavlinkParser::new().parse(&bad).is_err(),
                "{version:?} id {id}"
            );
        }
    }

    /// Builds a signed v2 frame: the incompatibility flag is set, the CRC covers
    /// the new header, and the 13-byte signature block follows the CRC.
    fn signed_v2(extra: u8) -> Vec<u8> {
        let mut bytes = make_frame(WireVersion::V2, 1, 1);
        bytes[2] |= IFLAG_SIGNED;
        let n = bytes.len();
        resign(&mut bytes, extra);
        bytes.push(1); // link id
        bytes.extend_from_slice(&[1, 2, 3, 4, 5, 6]); // timestamp, 48-bit little-endian
        bytes.extend_from_slice(&[0xAA; 6]); // signature value
        assert_eq!(bytes.len(), n + 13);
        bytes
    }

    #[test]
    fn signed_v2_frame_keeps_its_signature() {
        let extra = <MavMessage as mavlink::Message>::extra_crc(0);
        let frame = MavlinkParser::new().parse(&signed_v2(extra)).unwrap();
        let sig = frame.signature.expect("signature is kept");
        assert_eq!(sig.link_id, 1);
        assert_eq!(sig.timestamp, 0x0000_0605_0403_0201);
        assert_eq!(sig.value, [0xAA; 6]);
        assert!(frame.to_string().contains("signed"));
    }

    #[test]
    fn unsigned_v2_frame_has_no_signature() {
        let frame = MavlinkParser::new()
            .parse(&make_frame(WireVersion::V2, 1, 1))
            .unwrap();
        assert_eq!(frame.signature, None);
    }

    #[test]
    fn garbage_input() {
        let result = MavlinkParser::new().parse(&[0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_roundtrip_v2() {
        let frame = MavlinkParser::new()
            .parse(&make_frame(WireVersion::V2, 10, 5))
            .unwrap();
        let dp = MavlinkNormalizer::new().normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Mavlink);
        assert_eq!(dp.device_id, "mavlink:10:5");
        assert_eq!(dp.point_id, "HEARTBEAT.custom_mode");
    }

    #[test]
    fn validator_ok() {
        let state = MavlinkState {
            connected: true,
            system_id: 1,
            component_id: 1,
            heartbeat_received: true,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(MavlinkValidator::new().validate(&state).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        assert!(MavlinkValidator::new()
            .validate(&MavlinkState::default())
            .is_err());
    }

    #[test]
    fn validator_no_heartbeat() {
        let state = MavlinkState {
            connected: true,
            system_id: 1,
            component_id: 1,
            heartbeat_received: false,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(MavlinkValidator::new().validate(&state).is_err());
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let parser = MavlinkParser::new();
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }

    /// A MAVLink frame with a plausible header, v1 or v2. The length field is
    /// skewed by -2..=2 so the length checks run; the CRC is random, so the checks
    /// that follow the header are exercised on frames that rarely pass them.
    fn mavlink_frame() -> impl Strategy<Value = Vec<u8>> {
        (
            any::<bool>(),
            any::<u8>(),
            any::<u8>(),
            any::<u8>(),
            any::<u32>(),
            proptest::collection::vec(any::<u8>(), 0..64),
            -2i32..=2,
            any::<u16>(),
        )
            .prop_map(|(v2, seq, sys, comp, msgid, payload, skew, crc)| {
                let len = (payload.len() as i32 + skew).clamp(0, 255) as u8;
                let mut out = if v2 {
                    alloc::vec![0xFD, len, 0, 0, seq, sys, comp]
                } else {
                    alloc::vec![0xFE, len, seq, sys, comp]
                };
                if v2 {
                    out.extend_from_slice(&msgid.to_le_bytes()[..3]);
                } else {
                    out.push(msgid as u8);
                }
                out.extend_from_slice(&payload);
                out.extend_from_slice(&crc.to_le_bytes());
                out
            })
    }

    /// One random byte overwrite or one truncation of a MAVLink-shaped frame.
    fn mutated_mavlink() -> impl Strategy<Value = Vec<u8>> {
        (mavlink_frame(), any::<usize>(), any::<u8>(), any::<bool>()).prop_map(
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
        #[test]
        fn structured_mavlink_never_panics(bytes in mutated_mavlink()) {
            let _ = MavlinkParser::new().parse(&bytes);
            let _ = MavlinkParser::new().try_parse(&bytes);
        }
    }
}
