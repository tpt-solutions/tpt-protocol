//! KNX protocol implementation for home and building control.
//!
//! Parses cEMI `L_Data` frames (KNX 3/3/3 and 3/6/3), the frame format carried
//! in KNXnet/IP tunnelling and on EMI interfaces. DPT decoding is explicit:
//! the caller names the datapoint type, because the bus frame does not.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;

use tpt_protocol_core::{DataNormalizer, Error, FrameEncoder, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// cEMI message code.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageCode {
    /// L_Data.req (0x11)
    LDataRequest,
    /// L_Data.con (0x2E)
    LDataConfirmation,
    /// L_Data.ind (0x29)
    LDataIndication,
}

impl MessageCode {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x11 => Some(Self::LDataRequest),
            0x2E => Some(Self::LDataConfirmation),
            0x29 => Some(Self::LDataIndication),
            _ => None,
        }
    }
}

/// Telegram priority (control field 1, bits 3-2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KnxPriority {
    System,
    Normal,
    Urgent,
    Low,
}

impl KnxPriority {
    fn from_bits(v: u8) -> Self {
        match v & 0x03 {
            0 => Self::System,
            1 => Self::Normal,
            2 => Self::Urgent,
            _ => Self::Low,
        }
    }
}

/// Application-layer service (APCI). Group communication uses the first three.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KnxServiceType {
    GroupValueRead,
    GroupValueResponse,
    GroupValueWrite,
    /// Any other APCI (for example device management services).
    Other(u8),
}

impl KnxServiceType {
    fn from_apci(v: u8) -> Self {
        match v {
            0x00 => Self::GroupValueRead,
            0x01 => Self::GroupValueResponse,
            0x02 => Self::GroupValueWrite,
            other => Self::Other(other),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GroupValueRead => "GroupValueRead",
            Self::GroupValueResponse => "GroupValueResponse",
            Self::GroupValueWrite => "GroupValueWrite",
            Self::Other(_) => "Other",
        }
    }
}

/// Three-level group address: main (5 bits), middle (3 bits), sub (8 bits).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupAddress {
    pub main: u8,
    pub middle: u8,
    pub sub: u8,
}

impl GroupAddress {
    fn from_u16(addr: u16) -> Self {
        let main = ((addr >> 11) & 0x1f) as u8;
        let middle = ((addr >> 8) & 0x07) as u8;
        let sub = (addr & 0xff) as u8;
        Self { main, middle, sub }
    }

    pub fn to_u16(self) -> u16 {
        ((self.main as u16) << 11) | ((self.middle as u16) << 8) | (self.sub as u16)
    }
}

/// Individual address: area (4 bits), line (4 bits), device (8 bits).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IndividualAddress(pub u16);

impl IndividualAddress {
    pub fn area(self) -> u8 {
        ((self.0 >> 12) & 0x0F) as u8
    }
    pub fn line(self) -> u8 {
        ((self.0 >> 8) & 0x0F) as u8
    }
    pub fn device(self) -> u8 {
        (self.0 & 0xFF) as u8
    }
}

/// Payload of a group telegram.
#[derive(Debug, Clone, PartialEq)]
pub enum DptValue {
    /// Six-bit value carried in the APCI field of a one-octet telegram.
    Short(u8),
    /// Data octets after the APCI field.
    Data(Vec<u8>),
}

/// A parsed cEMI `L_Data` frame.
#[derive(Debug, Clone, PartialEq)]
pub struct KnxFrame {
    pub message_code: MessageCode,
    pub priority: KnxPriority,
    /// Hop count (control field 2, bits 6-4).
    pub hop_count: u8,
    pub source: IndividualAddress,
    /// Group address when `destination_is_group`, otherwise an individual address.
    pub destination: u16,
    pub destination_is_group: bool,
    pub service_type: KnxServiceType,
    pub dpt: DptValue,
}

impl KnxFrame {
    /// The destination as a group address, if it is one.
    pub fn group_address(&self) -> Option<GroupAddress> {
        self.destination_is_group
            .then(|| GroupAddress::from_u16(self.destination))
    }
}

/// cEMI frame parser. The frame must be exactly one `L_Data` telegram.
pub struct KnxParser;

/// Decodes an unsigned DPT 5.001 (scaling) octet as a percentage (0..=100).
pub fn decode_dpt5_percent(raw: u8) -> f32 {
    raw as f32 * 100.0 / 255.0
}

/// Decodes a DPT 1.x (boolean) value from a six-bit short telegram.
pub fn decode_dpt1(short: u8) -> bool {
    short & 0x01 != 0
}

/// Decodes a DPT 9.xxx (KNX 2-octet float) value. Returns `None` for the
/// "invalid" pattern 0x7FFF.
pub fn decode_dpt9(bytes: [u8; 2]) -> Option<f32> {
    let raw = u16::from_be_bytes(bytes);
    if raw == 0x7FFF {
        return None;
    }
    let sign = raw & 0x8000 != 0;
    let exponent = ((raw >> 11) & 0x0F) as i32;
    let mut mantissa = (raw & 0x07FF) as i32;
    if sign {
        // Mantissa is 11-bit two's complement.
        mantissa -= 2048;
    }
    Some(0.01 * mantissa as f32 * (1i32 << exponent) as f32)
}

impl FrameParser for KnxParser {
    type Frame = KnxFrame;

    /// cEMI `L_Data` layout (KNX 3/6/3 section 4.1.5):
    /// \[0\] message code, \[1\] additional-info length N, \[2..2+N\] additional info,
    /// then control 1, control 2, source (2), destination (2), NPDU length,
    /// TPCI/APCI (2 octets), and data.
    /// The NPDU length counts the octets after it, minus one. For a one-octet
    /// NPDU, the six-bit value is in the APCI field instead of a data octet.
    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        let (&message, rest) = bytes
            .split_first()
            .ok_or_else(|| Error::Parse("KNX cEMI frame is empty".into()))?;
        let message_code = MessageCode::from_u8(message)
            .ok_or_else(|| Error::Parse(format!("KNX unknown message code 0x{message:02X}")))?;
        let (&info_len, rest) = rest
            .split_first()
            .ok_or_else(|| Error::Parse("KNX cEMI frame has no additional-info length".into()))?;
        let rest = rest
            .get(info_len as usize..)
            .ok_or_else(|| Error::Parse("KNX additional info is truncated".into()))?;

        // Fixed part: control 1, control 2, source (2), destination (2), NPDU length, TPCI/APCI (2).
        let [ctrl1, ctrl2, s0, s1, d0, d1, npdu_len, t0, t1, data @ ..] = rest else {
            return Err(Error::Parse("KNX cEMI frame is too short".into()));
        };
        if *npdu_len == 0 {
            return Err(Error::Parse("KNX NPDU length 0 is invalid".into()));
        }

        // The NPDU is npdu_len + 1 octets: TPCI/APCI (2) and npdu_len - 1 data octets.
        let data_len = *npdu_len as usize - 1;
        if data.len() != data_len {
            return Err(Error::Parse(format!(
                "KNX NPDU length {} does not match {} data octets",
                npdu_len,
                data.len()
            )));
        }

        let destination_is_group = ctrl2 & 0x80 != 0;
        let apci = ((t0 & 0x03) << 2) | (t1 >> 6);
        let dpt = if *npdu_len == 1 {
            DptValue::Short(t1 & 0x3F)
        } else {
            DptValue::Data(data.to_vec())
        };

        Ok(KnxFrame {
            message_code,
            priority: KnxPriority::from_bits(ctrl1 >> 2),
            hop_count: (ctrl2 >> 4) & 0x07,
            source: IndividualAddress(u16::from_be_bytes([*s0, *s1])),
            destination: u16::from_be_bytes([*d0, *d1]),
            destination_is_group,
            service_type: KnxServiceType::from_apci(apci),
            dpt,
        })
    }
}

/// Writes a [`KnxFrame`] as a cEMI L_Data frame with no additional info. A
/// `DptValue::Short` value sits in the low six bits of the last octet; a
/// `DptValue::Data` value needs at least one octet. Fields the parser does
/// not keep (repeat and frame-type bits) are written with fixed values.
#[derive(Clone, Copy, Debug, Default)]
pub struct KnxEncoder;

impl KnxEncoder {
    pub fn new() -> Self {
        Self
    }
}

impl FrameEncoder for KnxEncoder {
    type Frame = KnxFrame;

    fn encode(&self, frame: &Self::Frame) -> Result<Vec<u8>, Error> {
        let message_code = match frame.message_code {
            MessageCode::LDataRequest => 0x11,
            MessageCode::LDataConfirmation => 0x2E,
            MessageCode::LDataIndication => 0x29,
        };
        let priority = match frame.priority {
            KnxPriority::System => 0,
            KnxPriority::Normal => 1,
            KnxPriority::Urgent => 2,
            KnxPriority::Low => 3,
        };
        if frame.hop_count > 7 {
            return Err(Error::Parse(format!(
                "KNX hop count {} does not fit in 3 bits",
                frame.hop_count
            )));
        }
        let apci = match frame.service_type {
            KnxServiceType::GroupValueRead => 0,
            KnxServiceType::GroupValueResponse => 1,
            KnxServiceType::GroupValueWrite => 2,
            KnxServiceType::Other(v) => v,
        };
        if apci > 0x0F {
            return Err(Error::Parse(format!(
                "KNX APCI 0x{apci:02X} is wider than 4 bits"
            )));
        }

        let (data, short): (&[u8], u8) = match &frame.dpt {
            DptValue::Short(v) => {
                if *v > 0x3F {
                    return Err(Error::Parse(format!(
                        "KNX short value {v} does not fit in 6 bits"
                    )));
                }
                (&[], *v)
            }
            DptValue::Data(data) => {
                if data.is_empty() || data.len() > 254 {
                    return Err(Error::Parse(format!(
                        "KNX data of {} octets is outside 1..=254",
                        data.len()
                    )));
                }
                (data.as_slice(), 0)
            }
        };

        let group_bit = if frame.destination_is_group { 0x80 } else { 0 };
        let mut out = Vec::with_capacity(9 + data.len());
        out.push(message_code);
        out.push(0); // additional info length
                     // Control 1: standard frame, not a repeat, broadcast, priority in bits 2-3.
        out.push(0xB0 | (priority << 2));
        // Control 2: destination type in bit 7, hop count in bits 4-6.
        out.push(group_bit | (frame.hop_count << 4));
        out.extend_from_slice(&frame.source.0.to_be_bytes());
        out.extend_from_slice(&frame.destination.to_be_bytes());
        out.push((data.len() + 1) as u8);
        out.push((apci >> 2) & 0x03);
        out.push(((apci & 0x03) << 6) | short);
        out.extend_from_slice(data);
        Ok(out)
    }
}

/// Validates KNX connection state.
pub struct KnxValidator;

impl StateValidator for KnxValidator {
    type State = KnxConnectionState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("knx: not connected".into()));
        }
        if state.group_address.main > 31 || state.group_address.middle > 7 {
            return Err(Error::State("knx: invalid group address range".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct KnxConnectionState {
    pub connected: bool,
    pub group_address: GroupAddress,
    pub sequence_counter: u32,
}

/// Normalizes group telegrams into TUP DataPoints. The device is the source
/// individual address, and the point is the destination group address.
pub struct KnxNormalizer;

impl DataNormalizer for KnxNormalizer {
    type Frame = KnxFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let group = frame
            .group_address()
            .ok_or_else(|| Error::Normalize("KNX telegram is not addressed to a group".into()))?;
        let device_id = format!(
            "knx:{}.{}.{}",
            frame.source.area(),
            frame.source.line(),
            frame.source.device()
        );
        let point_id = format!("{}/{}/{}", group.main, group.middle, group.sub);

        let value = match &frame.dpt {
            DptValue::Short(v) => TupValue::Int(*v as i64),
            // A telegram with no data octets carries no value; zero would read as one.
            DptValue::Data(d) if d.is_empty() => {
                return Err(Error::Normalize(format!(
                    "knx telegram to {point_id} carries no data"
                )))
            }
            DptValue::Data(d) => TupValue::Bytes(d.clone()),
        };

        Ok(DataPoint::new(
            SourceProtocol::Knx,
            device_id,
            point_id,
            0,
            value,
            Quality::Good,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    /// A group telegram: `npdu_octets` is the NPDU after the length octet
    /// (TPCI/APCI plus any data).
    fn group_telegram(ctrl2: u8, dest: u16, npdu_octets: &[u8]) -> Vec<u8> {
        let mut out = vec![0x29, 0x00, 0xBC, ctrl2, 0x11, 0x01, 0x00, 0x00];
        out[6..8].copy_from_slice(&dest.to_be_bytes());
        out.push((npdu_octets.len() - 1) as u8);
        out.extend_from_slice(npdu_octets);
        out
    }

    const GROUP: u8 = 0xE0; // group destination, hop count 6

    /// DPT 9 decoding is floating-point, so compare within a small tolerance.
    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.001
    }

    #[test]
    fn short_write_switch_is_decoded() {
        // GroupValueWrite (APCI 0x02) with value 1 in the APCI field.
        let frame = KnxParser
            .parse(&group_telegram(GROUP, 0x0801, &[0x00, 0x81]))
            .unwrap();
        assert_eq!(frame.message_code, MessageCode::LDataIndication);
        assert_eq!(frame.priority, KnxPriority::Low); // control 1 = 0xBC, bits 3-2 = 11
        assert_eq!(frame.hop_count, 6);
        assert_eq!(frame.source, IndividualAddress(0x1101));
        assert_eq!(frame.service_type, KnxServiceType::GroupValueWrite);
        assert_eq!(frame.dpt, DptValue::Short(1));
        assert_eq!(
            frame.group_address(),
            Some(GroupAddress {
                main: 1,
                middle: 0,
                sub: 1
            })
        );
    }

    #[test]
    fn read_request_has_no_data() {
        let frame = KnxParser
            .parse(&group_telegram(GROUP, 0x1002, &[0x00, 0x00]))
            .unwrap();
        assert_eq!(frame.service_type, KnxServiceType::GroupValueRead);
        assert_eq!(frame.dpt, DptValue::Short(0));
    }

    #[test]
    fn response_with_two_octets_of_data() {
        // DPT 9.001 temperature 21.0 °C is 0x0C 0x1A.
        let frame = KnxParser
            .parse(&group_telegram(GROUP, 0x2001, &[0x00, 0x40, 0x0C, 0x1A]))
            .unwrap();
        assert_eq!(frame.service_type, KnxServiceType::GroupValueResponse);
        assert_eq!(frame.dpt, DptValue::Data(vec![0x0C, 0x1A]));
        let DptValue::Data(d) = &frame.dpt else {
            panic!()
        };
        assert!(approx(decode_dpt9([d[0], d[1]]).unwrap(), 21.0));
    }

    #[test]
    fn individual_destination_is_not_a_group() {
        let frame = KnxParser
            .parse(&group_telegram(0x60, 0x1101, &[0x00, 0x00]))
            .unwrap();
        assert!(!frame.destination_is_group);
        assert_eq!(frame.group_address(), None);
        assert_eq!(frame.destination, 0x1101);
    }

    #[test]
    fn zero_npdu_length_is_rejected() {
        let mut bytes = group_telegram(GROUP, 0x0801, &[0x00, 0x81]);
        bytes[8] = 0; // NPDU length
        assert!(KnxParser.parse(&bytes).is_err());
    }

    #[test]
    fn npdu_length_must_match_the_data() {
        // NPDU length 4 means 3 data octets, but only 2 follow the APCI.
        let mut bytes = group_telegram(GROUP, 0x0801, &[0x00, 0x40, 0x0C, 0x1A]);
        bytes[8] = 4;
        assert!(KnxParser.parse(&bytes).is_err());
    }

    #[test]
    fn unknown_message_code_is_rejected() {
        let mut bytes = group_telegram(GROUP, 0x0801, &[0x00, 0x81]);
        bytes[0] = 0xFF;
        assert!(KnxParser.parse(&bytes).is_err());
    }

    #[test]
    fn additional_info_is_skipped() {
        let mut bytes = vec![0x29, 0x02, 0xAA, 0xBB];
        bytes.extend_from_slice(&group_telegram(GROUP, 0x0801, &[0x00, 0x81])[2..]);
        let frame = KnxParser.parse(&bytes).unwrap();
        assert_eq!(frame.dpt, DptValue::Short(1));
    }

    #[test]
    fn truncated_and_garbage_inputs_are_rejected() {
        assert!(KnxParser.parse(&[0x29, 0x00, 0xBC]).is_err());
        assert!(KnxParser.parse(&[0xFF; 16]).is_err());
        assert!(KnxParser.parse(&[]).is_err());
    }

    #[test]
    fn dpt5_percent_endpoints() {
        assert_eq!(decode_dpt5_percent(0), 0.0);
        assert_eq!(decode_dpt5_percent(255), 100.0);
    }

    #[test]
    fn dpt9_negative_value_uses_twos_complement_mantissa() {
        // Sign bit set, exponent 0, mantissa -2048: -20.48.
        assert!(approx(decode_dpt9([0x80, 0x00]).unwrap(), -20.48));
        assert_eq!(decode_dpt9([0x7F, 0xFF]), None);
    }

    #[test]
    fn normalize_group_write_uses_source_and_group_address() {
        let frame = KnxParser
            .parse(&group_telegram(GROUP, 0x0801, &[0x00, 0x81]))
            .unwrap();
        let dp = KnxNormalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Knx);
        assert_eq!(dp.device_id, "knx:1.1.1");
        assert_eq!(dp.point_id, "1/0/1");
        assert_eq!(dp.value, TupValue::Int(1));
    }

    #[test]
    fn normalize_rejects_individual_destination() {
        let frame = KnxParser
            .parse(&group_telegram(0x60, 0x1101, &[0x00, 0x00]))
            .unwrap();
        assert!(KnxNormalizer.normalize(&frame).is_err());
    }

    #[test]
    fn validator_ok() {
        let s = KnxConnectionState {
            connected: true,
            group_address: GroupAddress {
                main: 1,
                middle: 0,
                sub: 1,
            },
            sequence_counter: 0,
        };
        assert!(KnxValidator.validate(&s).is_ok());
    }

    #[test]
    fn validator_bad_address() {
        let s = KnxConnectionState {
            connected: true,
            group_address: GroupAddress {
                main: 32,
                middle: 8,
                sub: 255,
            },
            sequence_counter: 0,
        };
        assert!(KnxValidator.validate(&s).is_err());
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    /// A frame built from the model types, with every field the parser keeps.
    fn knx_model_frame() -> impl Strategy<Value = KnxFrame> {
        (
            0usize..3,
            0usize..4,
            0u8..8,
            any::<u16>(),
            any::<u16>(),
            any::<bool>(),
            0u8..16,
            (
                any::<bool>(),
                0u8..64,
                proptest::collection::vec(any::<u8>(), 1..14),
            ),
        )
            .prop_map(
                |(code, priority, hop, source, destination, group, apci, (short, value, data))| {
                    KnxFrame {
                        message_code: match code {
                            0 => MessageCode::LDataRequest,
                            1 => MessageCode::LDataConfirmation,
                            _ => MessageCode::LDataIndication,
                        },
                        priority: match priority {
                            0 => KnxPriority::System,
                            1 => KnxPriority::Normal,
                            2 => KnxPriority::Urgent,
                            _ => KnxPriority::Low,
                        },
                        hop_count: hop,
                        source: IndividualAddress(source),
                        destination,
                        destination_is_group: group,
                        service_type: match apci {
                            0 => KnxServiceType::GroupValueRead,
                            1 => KnxServiceType::GroupValueResponse,
                            2 => KnxServiceType::GroupValueWrite,
                            other => KnxServiceType::Other(other),
                        },
                        dpt: if short {
                            DptValue::Short(value)
                        } else {
                            DptValue::Data(data)
                        },
                    }
                },
            )
    }

    proptest! {
        /// Encoding a frame and parsing the bytes gives back the same frame.
        #[test]
        fn encoded_frames_parse_back_to_the_same_model(frame in knx_model_frame()) {
            let bytes = KnxEncoder::new().encode(&frame).unwrap();
            prop_assert_eq!(KnxParser.parse(&bytes).unwrap(), frame);
        }

        #[test]
        fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let parser = KnxParser;
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }

    /// A cEMI L_Data frame: message code (request, confirmation or indication),
    /// additional info, control fields, source and group or individual
    /// destination, NPDU length, TPCI/APCI and data. The NPDU length always
    /// matches the data octets.
    fn knx_frame() -> impl Strategy<Value = Vec<u8>> {
        const CODES: [u8; 3] = [0x11, 0x2E, 0x29];
        (
            0usize..CODES.len(),
            proptest::collection::vec(any::<u8>(), 0..4),
            any::<u8>(),
            any::<u8>(),
            any::<[u8; 4]>(),
            any::<[u8; 2]>(),
            proptest::collection::vec(any::<u8>(), 0..14),
        )
            .prop_map(|(ci, info, ctrl1, ctrl2, addrs, tpci, data)| {
                let mut out = alloc::vec![CODES[ci], info.len() as u8];
                out.extend_from_slice(&info);
                out.extend_from_slice(&[ctrl1, ctrl2]);
                out.extend_from_slice(&addrs);
                out.push(data.len() as u8 + 1);
                out.extend_from_slice(&tpci);
                out.extend_from_slice(&data);
                out
            })
    }

    /// One random byte overwrite or one truncation of a well-formed frame.
    fn mutated_knx() -> impl Strategy<Value = Vec<u8>> {
        (knx_frame(), any::<usize>(), any::<u8>(), any::<bool>()).prop_map(
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
        fn structured_knx_never_panics(bytes in mutated_knx()) {
            let _ = KnxParser.parse(&bytes);
        }

        #[test]
        fn well_formed_knx_never_panics_in_the_normalizer(bytes in knx_frame()) {
            if let Ok(frame) = KnxParser.parse(&bytes) {
                let _ = KnxNormalizer.normalize(&frame);
            }
        }
    }
}

impl KnxParser {
    /// Creates the value. The type has no state.
    pub fn new() -> Self {
        Self
    }
}

impl Default for KnxParser {
    fn default() -> Self {
        Self::new()
    }
}

impl KnxValidator {
    /// Creates the value. The type has no state.
    pub fn new() -> Self {
        Self
    }
}

impl Default for KnxValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl KnxNormalizer {
    /// Creates the value. The type has no state.
    pub fn new() -> Self {
        Self
    }
}

impl Default for KnxNormalizer {
    fn default() -> Self {
        Self::new()
    }
}
