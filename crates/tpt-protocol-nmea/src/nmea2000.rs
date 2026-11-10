//! NMEA 2000: single-frame PGNs carried in CAN frames with 29-bit identifiers.
//!
//! The identifier is the same layout as J1939: priority (bits 26-28), data page
//! (bit 24), PDU format (bits 16-23), PDU specific (bits 8-15) and source address
//! (bits 0-7). PDU format below 240 is PDU1, where the PDU specific byte is the
//! destination address; 240 and up is PDU2, where it is part of the PGN.
//!
//! Supported: the single-frame PGNs listed in the private `decode_pgn` table. Fast-packet PGNs
//! (more than eight bytes, for example GNSS position 129029) are parsed as
//! frames but not reassembled, so the normalizer refuses them with an error
//! rather than producing a partial value.
//!
//! Each field keeps the resolution and sentinel rules from the NMEA 2000 field
//! definitions: a field set to its "not available" value is skipped, and a
//! frame with no decodable field is an error.

use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;

use tpt_protocol_core::{DataNormalizer, Error, FrameEncoder, FrameParser};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// Destination address of a PDU2 (broadcast) message.
pub const GLOBAL_ADDRESS: u8 = 0xFF;

/// One NMEA 2000 message, as one CAN frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Nmea2000Frame {
    /// Identifier priority, 0 (highest) to 7.
    pub priority: u8,
    /// Parameter group number, including the data page bit.
    pub pgn: u32,
    /// Source address of the sender.
    pub source: u8,
    /// Destination address for PDU1 messages, [`GLOBAL_ADDRESS`] for PDU2.
    pub destination: u8,
    /// The data bytes, one to eight.
    pub data: Vec<u8>,
}

/// Parses a CAN frame body: the 32-bit identifier (big-endian), then one to
/// eight data bytes.
#[derive(Clone, Copy, Debug, Default)]
pub struct Nmea2000Parser;

impl Nmea2000Parser {
    pub fn new() -> Self {
        Self
    }
}

impl FrameParser for Nmea2000Parser {
    type Frame = Nmea2000Frame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        let [a, b, c, d, data @ ..] = bytes else {
            return Err(Error::Parse(format!(
                "NMEA 2000 frame too short: {} bytes, need at least 4",
                bytes.len()
            )));
        };
        if data.is_empty() || data.len() > 8 {
            return Err(Error::Parse(format!(
                "NMEA 2000 data is {} bytes, must be 1 to 8",
                data.len()
            )));
        }
        let id = u32::from_be_bytes([*a, *b, *c, *d]);
        let priority = ((id >> 26) & 0x07) as u8;
        let data_page = (id >> 24) & 0x01;
        let pdu_format = ((id >> 16) & 0xFF) as u8;
        let pdu_specific = ((id >> 8) & 0xFF) as u8;
        let source = (id & 0xFF) as u8;

        let (pgn, destination) = if pdu_format < 240 {
            ((data_page << 16) | ((pdu_format as u32) << 8), pdu_specific)
        } else {
            (
                (data_page << 16) | ((pdu_format as u32) << 8) | pdu_specific as u32,
                GLOBAL_ADDRESS,
            )
        };
        Ok(Nmea2000Frame {
            priority,
            pgn,
            source,
            destination,
            data: data.to_vec(),
        })
    }
}

/// Writes a [`Nmea2000Frame`] as the CAN frame body the parser reads. The
/// destination is written only for PDU1 messages; for PDU2 the PGN supplies
/// the low byte of the identifier, and the destination must be
/// [`GLOBAL_ADDRESS`].
#[derive(Clone, Copy, Debug, Default)]
pub struct Nmea2000Encoder;

impl Nmea2000Encoder {
    pub fn new() -> Self {
        Self
    }
}

impl FrameEncoder for Nmea2000Encoder {
    type Frame = Nmea2000Frame;

    fn encode(&self, frame: &Self::Frame) -> Result<Vec<u8>, Error> {
        if frame.priority > 7 {
            return Err(Error::Parse(format!(
                "NMEA 2000 priority {} is above 7",
                frame.priority
            )));
        }
        if frame.pgn > 0x1FFFF {
            return Err(Error::Parse(format!(
                "NMEA 2000 PGN 0x{:X} is wider than 17 bits",
                frame.pgn
            )));
        }
        if frame.data.is_empty() || frame.data.len() > 8 {
            return Err(Error::Parse(format!(
                "NMEA 2000 data is {} bytes, must be 1 to 8",
                frame.data.len()
            )));
        }
        let data_page = (frame.pgn >> 16) & 0x01;
        let pdu_format = ((frame.pgn >> 8) & 0xFF) as u8;
        let low = if pdu_format < 240 {
            if frame.destination == GLOBAL_ADDRESS {
                return Err(Error::Parse(
                    "NMEA 2000 PDU1 message needs a destination address, not the global address"
                        .into(),
                ));
            }
            frame.destination
        } else {
            if frame.destination != GLOBAL_ADDRESS {
                return Err(Error::Parse(
                    "NMEA 2000 PDU2 message is broadcast, so its destination must be the global address"
                        .into(),
                ));
            }
            (frame.pgn & 0xFF) as u8
        };
        let id = (u32::from(frame.priority) << 26)
            | (data_page << 24)
            | (u32::from(pdu_format) << 16)
            | (u32::from(low) << 8)
            | u32::from(frame.source);
        let mut out = Vec::with_capacity(4 + frame.data.len());
        out.extend_from_slice(&id.to_be_bytes());
        out.extend_from_slice(&frame.data);
        Ok(out)
    }
}

/// Largest message a fast-packet sequence can carry (NMEA 2000 limit).
const FAST_PACKET_MAX: usize = 223;
/// Open fast-packet sequences kept at once. The oldest is dropped when a new one
/// starts beyond this, so a device that never finishes cannot grow memory.
const FAST_PACKET_SESSIONS: usize = 16;

/// PGNs sent as fast-packet sequences (more than eight bytes). The list is the
/// set this crate decodes or is likely to meet; other PGNs are single frames.
pub fn is_fast_packet(pgn: u32) -> bool {
    matches!(pgn, 126_996 | 129_029 | 129_540)
}

struct Partial {
    pgn: u32,
    source: u8,
    sequence: u8,
    next_index: u8,
    total: usize,
    priority: u8,
    destination: u8,
    data: Vec<u8>,
}

/// Joins fast-packet frames into whole messages.
///
/// The first frame of a sequence carries the total length and six data bytes;
/// the following frames carry seven each. A frame out of order, or from a
/// sequence that was never started, is dropped. Single-frame PGNs pass through
/// unchanged.
#[derive(Default)]
pub struct FastPacketReassembler {
    partials: Vec<Partial>,
}

impl FastPacketReassembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Offers one frame. Returns the complete message when this frame finishes
    /// one, `None` while a sequence is still open or the frame was dropped.
    pub fn push(&mut self, frame: &Nmea2000Frame) -> Result<Option<Nmea2000Frame>, Error> {
        if !is_fast_packet(frame.pgn) {
            return Ok(Some(frame.clone()));
        }
        let Some(&control) = frame.data.first() else {
            return Err(Error::Parse("NMEA 2000 fast packet has no data".into()));
        };
        let sequence = control >> 5;
        let index = control & 0x1F;

        if index == 0 {
            let total = *frame
                .data
                .get(1)
                .ok_or_else(|| Error::Parse("NMEA 2000 fast packet has no length".into()))?
                as usize;
            if total == 0 || total > FAST_PACKET_MAX {
                return Err(Error::Parse(format!(
                    "NMEA 2000 fast packet length {total} is outside 1..={FAST_PACKET_MAX}"
                )));
            }
            self.partials
                .retain(|p| !(p.pgn == frame.pgn && p.source == frame.source));
            if self.partials.len() >= FAST_PACKET_SESSIONS {
                self.partials.remove(0);
            }
            self.partials.push(Partial {
                pgn: frame.pgn,
                source: frame.source,
                sequence,
                next_index: 1,
                total,
                priority: frame.priority,
                destination: frame.destination,
                data: frame.data[2..].to_vec(),
            });
        } else {
            let Some(at) = self.partials.iter().position(|p| {
                p.pgn == frame.pgn
                    && p.source == frame.source
                    && p.sequence == sequence
                    && p.next_index == index
            }) else {
                return Ok(None);
            };
            let partial = &mut self.partials[at];
            partial.data.extend_from_slice(&frame.data[1..]);
            partial.next_index += 1;
            partial.priority = frame.priority;
            partial.destination = frame.destination;
        }

        let Some(at) = self
            .partials
            .iter()
            .position(|p| p.pgn == frame.pgn && p.source == frame.source && p.sequence == sequence)
        else {
            return Ok(None);
        };
        if self.partials[at].data.len() < self.partials[at].total {
            return Ok(None);
        }
        let mut done = self.partials.remove(at);
        done.data.truncate(done.total);
        Ok(Some(Nmea2000Frame {
            priority: done.priority,
            pgn: done.pgn,
            source: done.source,
            destination: done.destination,
            data: done.data,
        }))
    }
}

fn le_u16(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*data.get(at)?, *data.get(at + 1)?]))
}

fn le_i16(data: &[u8], at: usize) -> Option<i16> {
    le_u16(data, at).map(|v| v as i16)
}

fn le_i64(data: &[u8], at: usize) -> Option<i64> {
    let bytes: [u8; 8] = data.get(at..at + 8)?.try_into().ok()?;
    Some(i64::from_le_bytes(bytes))
}

fn le_u32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *data.get(at)?,
        *data.get(at + 1)?,
        *data.get(at + 2)?,
        *data.get(at + 3)?,
    ]))
}

fn le_i32(data: &[u8], at: usize) -> Option<i32> {
    le_u32(data, at).map(|v| v as i32)
}

/// One decoded field: its name, unit and value.
struct Field {
    name: &'static str,
    unit: &'static str,
    value: f64,
}

/// Decodes the single-frame PGNs this crate knows. Returns `None` for other PGNs.
/// A field whose bytes hold its "not available" value is left out.
fn decode_pgn(frame: &Nmea2000Frame) -> Option<Vec<Field>> {
    let d = frame.data.as_slice();
    let mut out = Vec::new();
    match frame.pgn {
        // 129025 Position, Rapid Update: latitude and longitude, 1e-7 degrees each.
        129025 => {
            if let Some(lat) = le_i32(d, 0).filter(|&v| v != i32::MAX) {
                out.push(Field {
                    name: "latitude",
                    unit: "deg",
                    value: f64::from(lat) * 1e-7,
                });
            }
            if let Some(lon) = le_i32(d, 4).filter(|&v| v != i32::MAX) {
                out.push(Field {
                    name: "longitude",
                    unit: "deg",
                    value: f64::from(lon) * 1e-7,
                });
            }
        }
        // 127250 Vessel Heading: heading in 0.0001 rad, from SID byte 0.
        127250 => {
            if let Some(h) = le_u16(d, 1).filter(|&v| v != u16::MAX) {
                out.push(Field {
                    name: "heading",
                    unit: "rad",
                    value: f64::from(h) * 0.0001,
                });
            }
        }
        // 130306 Wind Data: speed in 0.01 m/s, angle in 0.0001 rad.
        130306 => {
            if let Some(s) = le_u16(d, 1).filter(|&v| v != u16::MAX) {
                out.push(Field {
                    name: "wind_speed",
                    unit: "m/s",
                    value: f64::from(s) * 0.01,
                });
            }
            if let Some(a) = le_u16(d, 3).filter(|&v| v != u16::MAX) {
                out.push(Field {
                    name: "wind_angle",
                    unit: "rad",
                    value: f64::from(a) * 0.0001,
                });
            }
        }
        // 128267 Water Depth: depth in 0.01 m, offset in 0.001 m.
        128267 => {
            if let Some(depth) = le_u32(d, 1).filter(|&v| v != u32::MAX) {
                out.push(Field {
                    name: "depth",
                    unit: "m",
                    value: f64::from(depth) * 0.01,
                });
            }
            if let Some(offset) = le_i16(d, 5).filter(|&v| v != i16::MAX) {
                out.push(Field {
                    name: "depth_offset",
                    unit: "m",
                    value: f64::from(offset) * 0.001,
                });
            }
        }
        // 129029 GNSS Position Data (fast packet): latitude and longitude in
        // 1e-16 degrees, altitude in 1e-6 metres. The date and time are not decoded.
        129029 => {
            if let Some(lat) = le_i64(d, 7).filter(|&v| v != i64::MAX) {
                out.push(Field {
                    name: "latitude",
                    unit: "deg",
                    value: lat as f64 * 1e-16,
                });
            }
            if let Some(lon) = le_i64(d, 15).filter(|&v| v != i64::MAX) {
                out.push(Field {
                    name: "longitude",
                    unit: "deg",
                    value: lon as f64 * 1e-16,
                });
            }
            if let Some(alt) = le_i64(d, 23).filter(|&v| v != i64::MAX) {
                out.push(Field {
                    name: "altitude",
                    unit: "m",
                    value: alt as f64 * 1e-6,
                });
            }
        }
        _ => return None,
    }
    Some(out)
}

/// Normalizes NMEA 2000 frames into TUP DataPoints, one per decoded field.
#[derive(Clone, Copy, Debug, Default)]
pub struct Nmea2000Normalizer;

impl Nmea2000Normalizer {
    pub fn new() -> Self {
        Self
    }

    /// Every point the frame carries. Errors for a PGN that is not decoded, and
    /// for a decodable PGN whose fields are all "not available".
    pub fn points(&self, frame: &Nmea2000Frame) -> Result<Vec<DataPoint>, Error> {
        if is_fast_packet(frame.pgn) && frame.data.len() <= 8 {
            return Err(Error::Normalize(format!(
                "NMEA 2000 PGN {} is a fast-packet message; reassemble it before normalizing",
                frame.pgn
            )));
        }
        let Some(fields) = decode_pgn(frame) else {
            return Err(Error::Normalize(format!(
                "NMEA 2000 PGN {} is not decoded (single-frame PGNs only)",
                frame.pgn
            )));
        };
        if fields.is_empty() {
            return Err(Error::Normalize(format!(
                "NMEA 2000 PGN {} carries no available field",
                frame.pgn
            )));
        }
        let device_id = format!("nmea2000:{:02X}", frame.source);
        Ok(fields
            .into_iter()
            .map(|field| {
                let mut dp = DataPoint::new(
                    SourceProtocol::Nmea,
                    device_id.clone(),
                    format!("pgn{}.{}", frame.pgn, field.name),
                    0,
                    TupValue::Float(field.value),
                    Quality::Good,
                );
                dp.unit = Some(field.unit.to_string());
                dp
            })
            .collect())
    }
}

impl DataNormalizer for Nmea2000Normalizer {
    type Frame = Nmea2000Frame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        self.points(frame)?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Normalize("NMEA 2000 frame has no point".into()))
    }

    fn normalize_all(&self, frame: &Self::Frame) -> Result<Vec<DataPoint>, Error> {
        self.points(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parser_and_normalizer_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..16)) {
            if let Ok(frame) = Nmea2000Parser::new().parse(&bytes) {
                let _ = Nmea2000Normalizer::new().points(&frame);
            }
        }
    }

    /// A PDU2 identifier (PGN 129025, priority 2, source 0x23) with the given data.
    fn pdu2_frame(pgn: u32, data: &[u8]) -> Vec<u8> {
        let id = (2u32 << 26)
            | ((pgn >> 16) << 24)
            | (((pgn >> 8) & 0xFF) << 16)
            | ((pgn & 0xFF) << 8)
            | 0x23;
        let mut out = id.to_be_bytes().to_vec();
        out.extend_from_slice(data);
        out
    }

    #[test]
    fn position_rapid_update_is_decoded() {
        // 47.6062 and -122.3321 degrees, as 1e-7 degree integers, little-endian.
        let lat: i32 = 476_062_000;
        let lon: i32 = -1_223_321_000;
        let mut data = lat.to_le_bytes().to_vec();
        data.extend_from_slice(&lon.to_le_bytes());
        let frame = Nmea2000Parser::new()
            .parse(&pdu2_frame(129025, &data))
            .unwrap();
        assert_eq!(frame.pgn, 129025);
        assert_eq!(frame.source, 0x23);
        assert_eq!(frame.destination, GLOBAL_ADDRESS);
        assert_eq!(frame.priority, 2);

        let points = Nmea2000Normalizer::new().points(&frame).unwrap();
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].point_id, "pgn129025.latitude");
        assert_eq!(points[0].device_id, "nmea2000:23");
        assert_eq!(points[0].unit.as_deref(), Some("deg"));
        let TupValue::Float(v) = points[0].value else {
            panic!("latitude is not a float")
        };
        assert!((v - 47.6062).abs() < 1e-9, "{v}");
        let TupValue::Float(v) = points[1].value else {
            panic!("longitude is not a float")
        };
        assert!((v + 122.3321).abs() < 1e-9, "{v}");
    }

    #[test]
    fn unavailable_fields_are_skipped_and_an_empty_frame_is_an_error() {
        // Latitude not available, longitude available.
        let mut data = i32::MAX.to_le_bytes().to_vec();
        data.extend_from_slice(&10_000_000i32.to_le_bytes());
        let frame = Nmea2000Parser::new()
            .parse(&pdu2_frame(129025, &data))
            .unwrap();
        let points = Nmea2000Normalizer::new().points(&frame).unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].point_id, "pgn129025.longitude");

        // Both fields at their "not available" value (0x7FFFFFFF). All 0xFF bytes
        // would be -1, which is a real coordinate, not a sentinel.
        let mut all_unavailable = i32::MAX.to_le_bytes().to_vec();
        all_unavailable.extend_from_slice(&i32::MAX.to_le_bytes());
        let frame = Nmea2000Parser::new()
            .parse(&pdu2_frame(129025, &all_unavailable))
            .unwrap();
        assert!(Nmea2000Normalizer::new().points(&frame).is_err());
    }

    #[test]
    fn vessel_heading_wind_and_depth_are_decoded() {
        // Heading 1.2345 rad (12345 in 0.0001 rad), SID 1, reference true.
        let mut heading = alloc::vec![0x01];
        heading.extend_from_slice(&12345u16.to_le_bytes());
        heading.extend_from_slice(&[0xFF; 5]);
        let frame = Nmea2000Parser::new()
            .parse(&pdu2_frame(127250, &heading))
            .unwrap();
        let point = &Nmea2000Normalizer::new().points(&frame).unwrap()[0];
        assert_eq!(point.point_id, "pgn127250.heading");
        assert!(matches!(point.value, TupValue::Float(v) if (v - 1.2345).abs() < 1e-9));

        // Wind 5.25 m/s, 0.7854 rad.
        let mut wind = alloc::vec![0x01];
        wind.extend_from_slice(&525u16.to_le_bytes());
        wind.extend_from_slice(&7854u16.to_le_bytes());
        wind.extend_from_slice(&[0x00, 0xFF, 0xFF]);
        let frame = Nmea2000Parser::new()
            .parse(&pdu2_frame(130306, &wind))
            .unwrap();
        let points = Nmea2000Normalizer::new().points(&frame).unwrap();
        assert_eq!(points[0].point_id, "pgn130306.wind_speed");
        assert_eq!(points[1].point_id, "pgn130306.wind_angle");
        assert!(matches!(points[0].value, TupValue::Float(v) if (v - 5.25).abs() < 1e-9));

        // Depth 12.34 m, offset 0.5 m.
        let mut depth = alloc::vec![0x01];
        depth.extend_from_slice(&1234u32.to_le_bytes());
        depth.extend_from_slice(&500i16.to_le_bytes());
        depth.push(0xFF);
        let frame = Nmea2000Parser::new()
            .parse(&pdu2_frame(128267, &depth))
            .unwrap();
        let points = Nmea2000Normalizer::new().points(&frame).unwrap();
        assert_eq!(points[0].point_id, "pgn128267.depth");
        assert!(matches!(points[0].value, TupValue::Float(v) if (v - 12.34).abs() < 1e-9));
        assert!(matches!(points[1].value, TupValue::Float(v) if (v - 0.5).abs() < 1e-9));
    }

    #[test]
    fn pgns_without_a_decoder_are_refused_not_guessed() {
        let frame = Nmea2000Parser::new()
            .parse(&pdu2_frame(129029, &[0u8; 8]))
            .unwrap();
        let err = Nmea2000Normalizer::new().points(&frame).unwrap_err();
        assert!(err.to_string().contains("129029"), "{err}");
    }

    #[test]
    fn pdu1_destination_is_read_from_the_identifier() {
        // PDU1 (PF 0xEE, address claim style): PS is the destination, not part of the PGN.
        let id = (6u32 << 26) | (0xEE << 16) | (0x80 << 8) | 0x17;
        let mut bytes = id.to_be_bytes().to_vec();
        bytes.extend_from_slice(&[0u8; 8]);
        let frame = Nmea2000Parser::new().parse(&bytes).unwrap();
        assert_eq!(frame.pgn, 0xEE00);
        assert_eq!(frame.destination, 0x80);
        assert_eq!(frame.source, 0x17);
    }

    #[test]
    fn encoded_frames_parse_back_to_the_same_frame() {
        let frames = [
            Nmea2000Frame {
                priority: 3,
                pgn: 129025,
                source: 0x42,
                destination: GLOBAL_ADDRESS,
                data: alloc::vec![1, 2, 3, 4, 5, 6, 7, 8],
            },
            Nmea2000Frame {
                priority: 6,
                pgn: 0xEE00,
                source: 0x17,
                destination: 0x80,
                data: alloc::vec![0xAA],
            },
        ];
        for frame in frames {
            let bytes = Nmea2000Encoder::new().encode(&frame).unwrap();
            assert_eq!(Nmea2000Parser::new().parse(&bytes).unwrap(), frame);
        }
    }

    /// Splits a fast-packet message into frames: the first carries the length and
    /// six data bytes, the rest seven each, and the last frame is padded to eight.
    fn fast_packet_frames(
        pgn: u32,
        source: u8,
        sequence: u8,
        message: &[u8],
    ) -> Vec<Nmea2000Frame> {
        let mut frames = Vec::new();
        let mut first = alloc::vec![sequence << 5, message.len() as u8];
        first.extend_from_slice(&message[..6.min(message.len())]);
        first.resize(8, 0xFF);
        frames.push(Nmea2000Frame {
            priority: 3,
            pgn,
            source,
            destination: GLOBAL_ADDRESS,
            data: first,
        });
        let mut index = 1u8;
        let mut pos = 6;
        while pos < message.len() {
            let end = (pos + 7).min(message.len());
            let mut data = alloc::vec![(sequence << 5) | index];
            data.extend_from_slice(&message[pos..end]);
            data.resize(8, 0xFF);
            frames.push(Nmea2000Frame {
                priority: 3,
                pgn,
                source,
                destination: GLOBAL_ADDRESS,
                data,
            });
            index += 1;
            pos = end;
        }
        frames
    }

    /// A 129029 GNSS Position Data message (43 bytes): 47.6062 N, 122.3321 W, 12.5 m.
    fn gnss_position_message() -> Vec<u8> {
        let mut m = alloc::vec![0u8; 43];
        m[0] = 7; // SID
        m[7..15].copy_from_slice(&476_062_000_000_000_000i64.to_le_bytes());
        m[15..23].copy_from_slice(&(-1_223_321_000_000_000_000i64).to_le_bytes());
        m[23..31].copy_from_slice(&12_500_000i64.to_le_bytes());
        m
    }

    #[test]
    fn fast_packet_message_is_reassembled_and_decoded() {
        let message = gnss_position_message();
        let frames = fast_packet_frames(129029, 0x23, 3, &message);
        assert_eq!(frames.len(), 7);

        let mut reassembler = FastPacketReassembler::new();
        let mut complete = None;
        for frame in &frames {
            if let Some(done) = reassembler.push(frame).unwrap() {
                complete = Some(done);
            }
        }
        let complete = complete.expect("the last frame completes the message");
        assert_eq!(complete.data, message);
        assert_eq!(complete.source, 0x23);

        let points = Nmea2000Normalizer::new().points(&complete).unwrap();
        let names: Vec<&str> = points.iter().map(|p| p.point_id.as_str()).collect();
        assert_eq!(
            names,
            [
                "pgn129029.latitude",
                "pgn129029.longitude",
                "pgn129029.altitude"
            ]
        );
        assert!(matches!(points[0].value, TupValue::Float(v) if (v - 47.6062).abs() < 1e-9));
        assert!(matches!(points[1].value, TupValue::Float(v) if (v + 122.3321).abs() < 1e-9));
        assert!(matches!(points[2].value, TupValue::Float(v) if (v - 12.5).abs() < 1e-9));
    }

    #[test]
    fn out_of_order_frames_are_dropped_and_a_restart_replaces_the_sequence() {
        let message = gnss_position_message();
        let frames = fast_packet_frames(129029, 0x23, 1, &message);
        let mut reassembler = FastPacketReassembler::new();

        // Frame 2 with no frame 1: dropped.
        assert!(reassembler.push(&frames[2]).unwrap().is_none());
        // Start over with the first frame, then skip frame 2 by mistake, then recover.
        assert!(reassembler.push(&frames[0]).unwrap().is_none());
        assert!(reassembler.push(&frames[1]).unwrap().is_none());
        assert!(reassembler.push(&frames[3]).unwrap().is_none());
        // A new first frame restarts the sequence cleanly.
        let mut done = None;
        for frame in &frames {
            if let Some(m) = reassembler.push(frame).unwrap() {
                done = Some(m);
            }
        }
        assert_eq!(done.unwrap().data, message);
    }

    #[test]
    fn single_frame_pgns_pass_through_the_reassembler() {
        let frame = Nmea2000Parser::new()
            .parse(&pdu2_frame(129025, &[0u8; 8]))
            .unwrap();
        let out = FastPacketReassembler::new().push(&frame).unwrap();
        assert_eq!(out, Some(frame));
    }

    #[test]
    fn a_lone_fast_packet_frame_is_refused_by_the_normalizer() {
        let frame = fast_packet_frames(129029, 0x23, 0, &gnss_position_message())
            .into_iter()
            .next()
            .unwrap();
        let err = Nmea2000Normalizer::new().points(&frame).unwrap_err();
        assert!(err.to_string().contains("reassemble"), "{err}");
    }

    #[test]
    fn encoder_refuses_inconsistent_destinations_and_sizes() {
        let broadcast_pdu1 = Nmea2000Frame {
            priority: 6,
            pgn: 0xEE00,
            source: 1,
            destination: GLOBAL_ADDRESS,
            data: alloc::vec![0],
        };
        assert!(Nmea2000Encoder::new().encode(&broadcast_pdu1).is_err());

        let addressed_pdu2 = Nmea2000Frame {
            priority: 2,
            pgn: 129025,
            source: 1,
            destination: 5,
            data: alloc::vec![0],
        };
        assert!(Nmea2000Encoder::new().encode(&addressed_pdu2).is_err());

        let too_long = Nmea2000Frame {
            priority: 2,
            pgn: 129025,
            source: 1,
            destination: GLOBAL_ADDRESS,
            data: alloc::vec![0; 9],
        };
        assert!(Nmea2000Encoder::new().encode(&too_long).is_err());
    }
}
