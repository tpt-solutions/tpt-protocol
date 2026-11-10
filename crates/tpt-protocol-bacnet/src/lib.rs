//! BACnet protocol implementation for building automation and control.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::vec::Vec;
use core::fmt;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// BACnet/IP BVLC function codes (ASHRAE 135 Annex J).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BvlcFunction {
    BvlcResult,
    WriteBroadcastDistributionTable,
    ReadBroadcastDistributionTable,
    ReadBroadcastDistributionTableAck,
    ForwardedNpdu,
    RegisterForeignDevice,
    ReadForeignDeviceTable,
    ReadForeignDeviceTableAck,
    DeleteForeignDeviceTableEntry,
    DistributeBroadcastToNetwork,
    OriginalUnicastNpdu,
    OriginalBroadcastNpdu,
    Unknown(u8),
}

impl BvlcFunction {
    fn from_u8(v: u8) -> Self {
        match v {
            0x00 => Self::BvlcResult,
            0x01 => Self::WriteBroadcastDistributionTable,
            0x02 => Self::ReadBroadcastDistributionTable,
            0x03 => Self::ReadBroadcastDistributionTableAck,
            0x04 => Self::ForwardedNpdu,
            0x05 => Self::RegisterForeignDevice,
            0x06 => Self::ReadForeignDeviceTable,
            0x07 => Self::ReadForeignDeviceTableAck,
            0x08 => Self::DeleteForeignDeviceTableEntry,
            0x09 => Self::DistributeBroadcastToNetwork,
            0x0A => Self::OriginalUnicastNpdu,
            0x0B => Self::OriginalBroadcastNpdu,
            other => Self::Unknown(other),
        }
    }

    /// Functions whose body is an NPDU (network layer PDU) and possibly an APDU.
    fn carries_npdu(&self) -> bool {
        matches!(
            self,
            Self::ForwardedNpdu
                | Self::DistributeBroadcastToNetwork
                | Self::OriginalUnicastNpdu
                | Self::OriginalBroadcastNpdu
        )
    }
}

impl fmt::Display for BvlcFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BvlcResult => write!(f, "BVLC-Result"),
            Self::WriteBroadcastDistributionTable => write!(f, "WriteBDT"),
            Self::ReadBroadcastDistributionTable => write!(f, "ReadBDT"),
            Self::ReadBroadcastDistributionTableAck => write!(f, "ReadBDTAck"),
            Self::ForwardedNpdu => write!(f, "ForwardedNPDU"),
            Self::RegisterForeignDevice => write!(f, "RegisterForeignDevice"),
            Self::ReadForeignDeviceTable => write!(f, "ReadFDT"),
            Self::ReadForeignDeviceTableAck => write!(f, "ReadFDTAck"),
            Self::DeleteForeignDeviceTableEntry => write!(f, "DeleteFDTEntry"),
            Self::DistributeBroadcastToNetwork => write!(f, "DistributeBroadcastToNetwork"),
            Self::OriginalUnicastNpdu => write!(f, "UnicastNPDU"),
            Self::OriginalBroadcastNpdu => write!(f, "BroadcastNPDU"),
            Self::Unknown(v) => write!(f, "Unknown(0x{v:02X})"),
        }
    }
}

/// BACnet APDU type.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ApduType {
    ConfirmedRequest,
    UnconfirmedRequest,
    SimpleAck,
    ComplexAck,
    Error,
    Reject,
    Abort,
    Unknown(u8),
}

impl ApduType {
    fn from_nibble(v: u8) -> Self {
        match v {
            0 => Self::ConfirmedRequest,
            1 => Self::UnconfirmedRequest,
            2 => Self::SimpleAck,
            3 => Self::ComplexAck,
            5 => Self::Error,
            6 => Self::Reject,
            7 => Self::Abort,
            other => Self::Unknown(other),
        }
    }
}

impl fmt::Display for ApduType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfirmedRequest => write!(f, "ConfirmedReq"),
            Self::UnconfirmedRequest => write!(f, "UnconfirmedReq"),
            Self::SimpleAck => write!(f, "SimpleAck"),
            Self::ComplexAck => write!(f, "ComplexAck"),
            Self::Error => write!(f, "Error"),
            Self::Reject => write!(f, "Reject"),
            Self::Abort => write!(f, "Abort"),
            Self::Unknown(v) => write!(f, "Unknown({v})"),
        }
    }
}

/// BACnet object identifier: type (10 bits) and instance (22 bits), packed into 32 bits.
#[derive(Clone, Debug, PartialEq)]
pub struct BacnetObjectId {
    pub object_type: u16,
    pub instance: u32,
}

impl BacnetObjectId {
    /// Decodes the packed 32-bit form used by BACnet application data.
    pub fn from_packed(v: u32) -> Self {
        Self {
            object_type: ((v >> 22) & 0x03FF) as u16,
            instance: v & 0x003F_FFFF,
        }
    }
}

/// ReadProperty service choice (ASHRAE 135 clause 15.5).
const SERVICE_READ_PROPERTY: u8 = 12;
/// Who-Is unconfirmed service choice (clause 16.10).
#[allow(dead_code)]
const SERVICE_WHO_IS: u8 = 8;

/// A parsed BACnet frame (BVLC, NPDU and APDU).
#[derive(Clone, Debug, PartialEq)]
pub struct BacnetFrame {
    pub bvlc_type: u8,
    pub bvlc_function: BvlcFunction,
    pub bvlc_length: u16,
    /// APDU type, or `Unknown(0)` for network-layer messages and link-only frames.
    pub apdu_type: ApduType,
    /// Invoke ID for confirmed and acknowledgement APDUs.
    pub invoke_id: Option<u8>,
    /// Service choice for APDUs that carry one.
    pub service_choice: Option<u8>,
    pub object: Option<BacnetObjectId>,
    pub property_id: Option<u16>,
    /// The property value of a ReadProperty Complex-ACK, when it is a type this
    /// crate decodes. Requests carry no value.
    pub value: Option<BacnetValue>,
    /// Remaining service data after the fields above.
    pub payload: Vec<u8>,
}

/// An application-tagged value from a ReadProperty answer.
#[derive(Clone, Debug, PartialEq)]
pub enum BacnetValue {
    Null,
    Boolean(bool),
    Unsigned(u32),
    Signed(i32),
    Real(f32),
    Double(f64),
    Enumerated(u32),
    /// Any other application type, kept as its content octets.
    Other(Vec<u8>),
}

/// Decodes one application-tagged value at the front of `data`. Returns the
/// value and the number of bytes it used. Context-tagged data and extended
/// lengths are not decoded here.
fn read_application_value(data: &[u8]) -> Option<(BacnetValue, usize)> {
    let tag = *data.first()?;
    if tag & 0x08 != 0 {
        return None; // context-specific tag, not an application value
    }
    let tag_number = tag >> 4;
    let lvt = usize::from(tag & 0x07);
    // Booleans keep their value in the length field and have no content octets.
    if tag_number == 1 {
        return Some((BacnetValue::Boolean(lvt != 0), 1));
    }
    if lvt > 4 && tag_number != 5 {
        return None; // extended lengths (5, 6, 7) are not decoded
    }
    let content = data.get(1..1 + lvt)?;
    let used = 1 + lvt;
    let unsigned = || {
        content
            .iter()
            .fold(0u32, |acc, &b| (acc << 8) | u32::from(b))
    };
    let value = match tag_number {
        0 if lvt == 0 => BacnetValue::Null,
        2 if lvt <= 4 => BacnetValue::Unsigned(unsigned()),
        3 if (1..=4).contains(&lvt) => {
            // Sign-extend from the top bit of the first content octet.
            let shift = 32 - 8 * lvt as u32;
            BacnetValue::Signed(((unsigned() << shift) as i32) >> shift)
        }
        4 if lvt == 4 => BacnetValue::Real(f32::from_be_bytes(content.try_into().ok()?)),
        5 if lvt == 8 => BacnetValue::Double(f64::from_be_bytes(content.try_into().ok()?)),
        9 if lvt <= 4 => BacnetValue::Enumerated(unsigned()),
        _ => BacnetValue::Other(content.to_vec()),
    };
    Some((value, used))
}

impl fmt::Display for BacnetFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BACnet[BVLC=0x{:02X}/{}, APDU={}]",
            self.bvlc_type, self.bvlc_function, self.apdu_type,
        )
    }
}

/// BACnet frame parser.
#[derive(Clone, Debug, Default)]
pub struct BacnetParser;

impl BacnetParser {
    pub fn new() -> Self {
        Self
    }
}

/// Reads one application-tagged or context-tagged value length, returning
/// `(length, header_bytes)`. Only the context tags used here are supported.
fn read_context_u32(bytes: &[u8], tag: u8) -> Option<(u32, usize)> {
    match bytes {
        [h, a, b, c, d, ..] if *h == (tag << 4) | 0x0C => {
            Some((u32::from_be_bytes([*a, *b, *c, *d]), 5))
        }
        _ => None,
    }
}

fn read_context_property(bytes: &[u8], tag: u8) -> Option<(u16, usize)> {
    match bytes {
        [h, v, ..] if *h == (tag << 4) | 0x09 => Some((*v as u16, 2)),
        [h, a, b, ..] if *h == (tag << 4) | 0x0A => Some((u16::from_be_bytes([*a, *b]), 3)),
        _ => None,
    }
}

/// Parses the NPDU at the start of `body`. Returns the offset of the APDU and
/// whether the NPDU is a network-layer message (which carries no APDU).
fn parse_npdu(body: &[u8]) -> Result<(usize, bool), Error> {
    let [version, control, ..] = body else {
        return Err(Error::Parse("BACnet NPDU too short".into()));
    };
    if *version != 0x01 {
        return Err(Error::Parse(format!(
            "BACnet NPDU version 0x{version:02X}, expected 0x01"
        )));
    }
    let mut pos = 2usize;
    let has_dnet = control & 0x20 != 0;
    let has_snet = control & 0x08 != 0;
    let network_message = control & 0x80 != 0;

    let skip_address = |present: bool, pos: &mut usize| -> Result<(), Error> {
        if !present {
            return Ok(());
        }
        // Network number (2) + address length (1) + address.
        let len = *body
            .get(*pos + 2)
            .ok_or_else(|| Error::Parse("BACnet NPDU address truncated".into()))?
            as usize;
        *pos += 3 + len;
        if *pos > body.len() {
            return Err(Error::Parse("BACnet NPDU address truncated".into()));
        }
        Ok(())
    };
    skip_address(has_dnet, &mut pos)?;
    skip_address(has_snet, &mut pos)?;
    if has_dnet {
        // Hop count after a destination address.
        pos += 1;
    }
    if pos > body.len() {
        return Err(Error::Parse("BACnet NPDU header truncated".into()));
    }
    Ok((pos, network_message))
}

impl FrameParser for BacnetParser {
    type Frame = BacnetFrame;

    /// Parses exactly one BACnet/IP frame. The BVLC length must equal the
    /// number of bytes given.
    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 4 {
            return Err(Error::Parse(format!(
                "BACnet frame too short: {} bytes, need at least 4",
                bytes.len(),
            )));
        }
        let bvlc_length = u16::from_be_bytes([bytes[2], bytes[3]]);
        if bvlc_length as usize != bytes.len() {
            return Err(Error::Parse(format!(
                "BACnet BVLC length {bvlc_length} does not match {} bytes",
                bytes.len(),
            )));
        }
        parse_bvlc(bytes)
    }
}

fn parse_bvlc(bytes: &[u8]) -> Result<BacnetFrame, Error> {
    let bvlc_type = bytes[0];
    if bvlc_type != 0x81 {
        return Err(Error::Parse(format!(
            "BACnet invalid BVLC type: 0x{bvlc_type:02X}, expected 0x81",
        )));
    }
    let bvlc_function = BvlcFunction::from_u8(bytes[1]);
    let bvlc_length = u16::from_be_bytes([bytes[2], bytes[3]]);
    let mut body = &bytes[4..];

    // Forwarded-NPDU carries the 6-byte original B/IP address first.
    if bvlc_function == BvlcFunction::ForwardedNpdu {
        if body.len() < 6 {
            return Err(Error::Parse("BACnet Forwarded-NPDU missing address".into()));
        }
        body = &body[6..];
    }

    let empty = BacnetFrame {
        bvlc_type,
        bvlc_function,
        bvlc_length,
        apdu_type: ApduType::Unknown(0),
        invoke_id: None,
        service_choice: None,
        object: None,
        property_id: None,
        value: None,
        payload: Vec::new(),
    };
    if !bvlc_function.carries_npdu() || body.is_empty() {
        return Ok(BacnetFrame {
            payload: body.to_vec(),
            ..empty
        });
    }

    let (apdu_start, network_message) = parse_npdu(body)?;
    if network_message {
        return Ok(BacnetFrame {
            payload: body[apdu_start..].to_vec(),
            ..empty
        });
    }
    parse_apdu(&body[apdu_start..], empty)
}

fn parse_apdu(apdu: &[u8], mut frame: BacnetFrame) -> Result<BacnetFrame, Error> {
    let [first, rest @ ..] = apdu else {
        return Ok(frame);
    };
    let apdu_type = ApduType::from_nibble((first >> 4) & 0x0F);
    frame.apdu_type = apdu_type;
    let seg = first & 0x08 != 0;

    // Offsets are relative to `rest`, which starts after the APDU type byte.
    // Segmented messages insert a sequence number and window size before the
    // service choice.
    let (invoke_at, service_at): (Option<usize>, Option<usize>) = match apdu_type {
        ApduType::ConfirmedRequest => (Some(1), Some(if seg { 4 } else { 2 })),
        ApduType::ComplexAck => (Some(0), Some(if seg { 3 } else { 1 })),
        ApduType::SimpleAck | ApduType::Error => (Some(0), Some(1)),
        ApduType::Abort | ApduType::Reject => (Some(0), None),
        ApduType::UnconfirmedRequest => (None, Some(0)),
        ApduType::Unknown(_) => return Ok(frame),
    };
    frame.invoke_id = invoke_at.and_then(|i| rest.get(i).copied());

    let Some(service_at) = service_at else {
        frame.payload = rest.get(1..).unwrap_or(&[]).to_vec();
        return Ok(frame);
    };
    let Some(&service) = rest.get(service_at) else {
        return Ok(frame);
    };
    frame.service_choice = Some(service);
    let data = &rest[service_at + 1..];

    let read_property_answer = service == SERVICE_READ_PROPERTY
        && matches!(apdu_type, ApduType::ConfirmedRequest | ApduType::ComplexAck)
        && !seg;
    if read_property_answer {
        if let Some((packed, used)) = read_context_u32(data, 0) {
            frame.object = Some(BacnetObjectId::from_packed(packed));
            let after_object = &data[used..];
            if let Some((property, used2)) = read_context_property(after_object, 1) {
                frame.property_id = Some(property);
                let after_property = &after_object[used2..];
                // A ComplexACK answers with `[3] value [3]`: an opening tag, the value, a closing tag.
                if matches!(apdu_type, ApduType::ComplexAck) {
                    if let [0x3E, value @ ..] = after_property {
                        frame.value = read_application_value(value).map(|(v, _)| v);
                    }
                }
                frame.payload = after_property.to_vec();
                return Ok(frame);
            }
        }
    }
    frame.payload = data.to_vec();
    Ok(frame)
}

/// BACnet connection state.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct BacnetState {
    pub connected: bool,
    pub device_instance: u32,
    pub last_ok_timestamp: u64,
    pub error_count: u64,
}

/// Validates BACnet connection health.
#[derive(Clone, Debug, Default)]
pub struct BacnetValidator;

impl BacnetValidator {
    pub fn new() -> Self {
        Self
    }
}

impl StateValidator for BacnetValidator {
    type State = BacnetState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("BACnet device not connected".into()));
        }
        if state.device_instance > 4_194_303 {
            return Err(Error::State(format!(
                "BACnet invalid device instance: {}",
                state.device_instance,
            )));
        }
        if state.error_count > 10 {
            return Err(Error::State(format!(
                "BACnet device has {} consecutive errors",
                state.error_count,
            )));
        }
        Ok(())
    }
}

/// Normalizes BACnet frames into TUP DataPoints.
#[derive(Clone, Debug, Default)]
pub struct BacnetNormalizer;

impl BacnetNormalizer {
    pub fn new() -> Self {
        Self
    }
}

impl DataNormalizer for BacnetNormalizer {
    type Frame = BacnetFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = format!("bacnet:bvlc:{}", frame.bvlc_function);
        let point_id = if let Some(ref obj) = frame.object {
            format!("obj:{}:{}", obj.object_type, obj.instance)
        } else {
            format!("apdu:{}", frame.apdu_type)
        };
        let timestamp = 0;
        // A request or an unknown service carries no value, so there is nothing to report.
        let value = match &frame.value {
            Some(BacnetValue::Null) | None => {
                return Err(Error::Normalize(format!(
                    "BACnet {point_id} carries no value"
                )))
            }
            Some(BacnetValue::Boolean(b)) => TupValue::Bool(*b),
            Some(BacnetValue::Unsigned(n)) | Some(BacnetValue::Enumerated(n)) => {
                TupValue::Int(i64::from(*n))
            }
            Some(BacnetValue::Signed(n)) => TupValue::Int(i64::from(*n)),
            Some(BacnetValue::Real(x)) => TupValue::Float(f64::from(*x)),
            Some(BacnetValue::Double(x)) => TupValue::Float(*x),
            Some(BacnetValue::Other(bytes)) => TupValue::Bytes(bytes.clone()),
        };
        Ok(DataPoint::new(
            SourceProtocol::Bacnet,
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

    /// Builds a BACnet/IP frame: BVLC header with a correct length, then `body`.
    fn bvlc(function: u8, body: &[u8]) -> Vec<u8> {
        let len = (4 + body.len()) as u16;
        let mut out = vec![0x81, function];
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(body);
        out
    }

    /// Confirmed ReadProperty request for `object` (packed), property 85 (Present_Value).
    fn read_property_body(packed_object: u32, invoke: u8) -> Vec<u8> {
        let mut body = vec![0x01, 0x00]; // NPDU: version 1, no addresses, normal priority
        body.extend_from_slice(&[0x00, 0x05, invoke, SERVICE_READ_PROPERTY]); // APDU
        body.push(0x0C); // context tag 0, length 4: object identifier
        body.extend_from_slice(&packed_object.to_be_bytes());
        body.extend_from_slice(&[0x19, 85]); // context tag 1, length 1: property identifier
        body
    }

    #[test]
    fn bvlc_function_codes_follow_annex_j() {
        assert_eq!(BvlcFunction::from_u8(0x04), BvlcFunction::ForwardedNpdu);
        assert_eq!(
            BvlcFunction::from_u8(0x05),
            BvlcFunction::RegisterForeignDevice
        );
        assert_eq!(
            BvlcFunction::from_u8(0x0A),
            BvlcFunction::OriginalUnicastNpdu
        );
        assert_eq!(
            BvlcFunction::from_u8(0x0B),
            BvlcFunction::OriginalBroadcastNpdu
        );
        assert_eq!(
            BvlcFunction::from_u8(0x09),
            BvlcFunction::DistributeBroadcastToNetwork
        );
    }

    #[test]
    fn read_property_request_decodes_packed_object_id() {
        // Analog Input (type 0) instance 12345 packs to 0x00003039.
        let frame = BacnetParser::new()
            .parse(&bvlc(0x0A, &read_property_body(0x0000_3039, 7)))
            .unwrap();
        assert_eq!(frame.apdu_type, ApduType::ConfirmedRequest);
        assert_eq!(frame.invoke_id, Some(7));
        assert_eq!(frame.service_choice, Some(SERVICE_READ_PROPERTY));
        let obj = frame.object.unwrap();
        assert_eq!((obj.object_type, obj.instance), (0, 12345));
        assert_eq!(frame.property_id, Some(85));
    }

    #[test]
    fn packed_object_id_splits_type_and_instance() {
        // Analog Value (type 2) instance 7 packs to (2 << 22) | 7 = 0x0080_0007.
        let obj = BacnetObjectId::from_packed(0x0080_0007);
        assert_eq!((obj.object_type, obj.instance), (2, 7));
    }

    #[test]
    fn who_is_broadcast_with_destination_network() {
        // NPDU control 0x20: destination network present, with DNET 0xFFFF, no DLEN, hop 255.
        let body = [0x01, 0x20, 0xFF, 0xFF, 0x00, 0xFF, 0x10, 0x08];
        let frame = BacnetParser::new().parse(&bvlc(0x0B, &body)).unwrap();
        assert_eq!(frame.bvlc_function, BvlcFunction::OriginalBroadcastNpdu);
        assert_eq!(frame.apdu_type, ApduType::UnconfirmedRequest);
        assert_eq!(frame.service_choice, Some(SERVICE_WHO_IS));
    }

    #[test]
    fn forwarded_npdu_skips_original_address() {
        // 6-byte original B/IP address (192.168.1.10:47808), then NPDU and a Complex-ACK.
        let mut body = vec![192, 168, 1, 10, 0xBA, 0xC0];
        body.extend_from_slice(&[0x01, 0x00, 0x30, 0x01, SERVICE_READ_PROPERTY]);
        let frame = BacnetParser::new().parse(&bvlc(0x04, &body)).unwrap();
        assert_eq!(frame.bvlc_function, BvlcFunction::ForwardedNpdu);
        assert_eq!(frame.apdu_type, ApduType::ComplexAck);
        assert_eq!(frame.invoke_id, Some(1));
        assert_eq!(frame.service_choice, Some(SERVICE_READ_PROPERTY));
    }

    #[test]
    fn network_layer_message_has_no_apdu() {
        let frame = BacnetParser::new()
            .parse(&bvlc(0x0A, &[0x01, 0x80, 0x00]))
            .unwrap();
        assert_eq!(frame.apdu_type, ApduType::Unknown(0));
        assert_eq!(frame.payload, vec![0x00]);
    }

    #[test]
    fn bvlc_length_must_match_the_frame() {
        let mut bytes = bvlc(0x0A, &read_property_body(0x0000_3039, 1));
        bytes.push(0x00); // extra byte, length field unchanged
        assert!(BacnetParser::new().parse(&bytes).is_err());
    }

    #[test]
    fn npdu_version_must_be_one() {
        assert!(BacnetParser::new()
            .parse(&bvlc(0x0A, &[0x02, 0x00, 0x10, 0x08]))
            .is_err());
    }

    #[test]
    fn invalid_bvlc_type() {
        let mut bytes = bvlc(0x0A, &read_property_body(0x0000_3039, 1));
        bytes[0] = 0x82;
        assert!(BacnetParser::new().parse(&bytes).is_err());
    }

    #[test]
    fn truncated_input() {
        assert!(BacnetParser::new().parse(&[0x81, 0x04]).is_err());
    }

    #[test]
    fn garbage_input_is_rejected() {
        // Length says 4 but five bytes are present.
        assert!(BacnetParser::new()
            .parse(&[0x81, 0xFF, 0x00, 0x04, 0xFE])
            .is_err());
    }

    /// A ReadProperty Complex-ACK for `packed_object`, Present_Value, with `value`
    /// (already application-tagged) between the opening and closing tags.
    fn read_property_ack(packed_object: u32, value: &[u8]) -> Vec<u8> {
        let mut body = vec![0x01, 0x00, 0x30, 0x01, SERVICE_READ_PROPERTY];
        body.push(0x0C); // context tag 0, length 4: object identifier
        body.extend_from_slice(&packed_object.to_be_bytes());
        body.extend_from_slice(&[0x19, 85]); // context tag 1: Present_Value
        body.push(0x3E); // opening tag 3
        body.extend_from_slice(value);
        body.push(0x3F); // closing tag 3
        bvlc(0x0A, &body)
    }

    #[test]
    fn normalize_read_property_ack_reports_the_real_value() {
        // 21.5 as an application REAL: tag 0x44, then the big-endian IEEE 754 bits.
        let mut real = vec![0x44];
        real.extend_from_slice(&21.5f32.to_be_bytes());
        let frame = BacnetParser::new()
            .parse(&read_property_ack(0x0000_0001, &real))
            .unwrap();
        assert_eq!(frame.value, Some(BacnetValue::Real(21.5)));
        let dp = BacnetNormalizer::new().normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Bacnet);
        // Type 0 (analog input), instance 1.
        assert_eq!(dp.point_id, "obj:0:1");
        assert_eq!(dp.value, TupValue::Float(21.5));
    }

    #[test]
    fn read_property_request_carries_no_value_to_report() {
        let frame = BacnetParser::new()
            .parse(&bvlc(0x0A, &read_property_body(0x0040_002A, 1)))
            .unwrap();
        assert_eq!(frame.value, None);
        assert!(BacnetNormalizer::new().normalize(&frame).is_err());
    }

    #[test]
    fn application_values_decode_by_tag() {
        // Unsigned 300: tag 0x22, two octets.
        assert_eq!(
            read_application_value(&[0x22, 0x01, 0x2C]),
            Some((BacnetValue::Unsigned(300), 3))
        );
        // Signed -2: tag 0x31, one octet 0xFE.
        assert_eq!(
            read_application_value(&[0x31, 0xFE]),
            Some((BacnetValue::Signed(-2), 2))
        );
        // Signed -300: tag 0x32, 0xFE 0xD4 (two's complement, two octets).
        assert_eq!(
            read_application_value(&[0x32, 0xFE, 0xD4]),
            Some((BacnetValue::Signed(-300), 3))
        );
        // Boolean true keeps its value in the tag's length field.
        assert_eq!(
            read_application_value(&[0x11]),
            Some((BacnetValue::Boolean(true), 1))
        );
        // Enumerated 3: tag 0x91.
        assert_eq!(
            read_application_value(&[0x91, 0x03]),
            Some((BacnetValue::Enumerated(3), 2))
        );
        // A context-specific tag is not an application value.
        assert_eq!(read_application_value(&[0x0C, 0, 0, 0, 1]), None);
    }

    #[test]
    fn ack_without_a_decodable_value_is_an_error_not_a_zero() {
        // A character string (tag 7, length 3: encoding octet 0x00, then "ok") is kept as bytes.
        let frame = BacnetParser::new()
            .parse(&read_property_ack(0x0000_0001, &[0x73, 0x00, b'o', b'k']))
            .unwrap();
        assert!(matches!(frame.value, Some(BacnetValue::Other(_))));
        let dp = BacnetNormalizer::new().normalize(&frame).unwrap();
        assert_eq!(dp.value, TupValue::Bytes(vec![0x00, b'o', b'k']));
    }

    #[test]
    fn validator_ok() {
        let state = BacnetState {
            connected: true,
            device_instance: 100,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(BacnetValidator::new().validate(&state).is_ok());
    }

    #[test]
    fn validator_not_connected() {
        assert!(BacnetValidator::new()
            .validate(&BacnetState::default())
            .is_err());
    }

    #[test]
    fn validator_bad_instance() {
        let state = BacnetState {
            connected: true,
            device_instance: 5_000_000,
            last_ok_timestamp: 1000,
            error_count: 0,
        };
        assert!(BacnetValidator::new().validate(&state).is_err());
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..1024)) {
            let parser = BacnetParser::new();
            // The parser must never panic on arbitrary input; it returns an
            // Err for malformed frames instead.
            let _ = parser.parse(&bytes);
        }
    }

    /// A BACnet/IP frame with a correct BVLC header and length. The NPDU
    /// control bits select the DNET, SNET, hop-count and network-message
    /// fields, and the APDU is random. Forwarded-NPDU frames get their
    /// 6-byte original address.
    fn bacnet_frame() -> impl Strategy<Value = Vec<u8>> {
        const FUNCTIONS: [u8; 3] = [0x0A, 0x0B, 0x04];
        const CONTROLS: [u8; 7] = [0x00, 0x04, 0x08, 0x20, 0x28, 0x80, 0xA0];
        (
            0usize..FUNCTIONS.len(),
            any::<u8>(),
            0usize..CONTROLS.len(),
            any::<u8>(),
            proptest::collection::vec(any::<u8>(), 0..8),
            proptest::collection::vec(any::<u8>(), 0..32),
        )
            .prop_map(|(fi, raw_func, ci, raw_control, dest_addr, apdu)| {
                // Mostly the defined codes, sometimes any byte.
                let func = if raw_func < 0x80 {
                    FUNCTIONS[fi]
                } else {
                    raw_func
                };
                let control = if raw_control < 0x80 {
                    CONTROLS[ci]
                } else {
                    raw_control
                };
                let mut body = alloc::vec![0x01, control];
                if control & 0x20 != 0 {
                    body.extend_from_slice(&[0x00, 0x01, dest_addr.len() as u8]);
                    body.extend_from_slice(&dest_addr);
                }
                if control & 0x08 != 0 {
                    body.extend_from_slice(&[0x00, 0x02, 0x01, 0x55]);
                }
                if control & 0x20 != 0 {
                    body.push(0xFF);
                }
                if control & 0x80 != 0 {
                    body.push(0x01);
                }
                body.extend_from_slice(&apdu);

                let mut out = alloc::vec![0x81, func, 0, 0];
                if func == 0x04 {
                    out.extend_from_slice(&[192, 168, 1, 10, 0xBA, 0xC0]);
                }
                out.extend_from_slice(&body);
                let len = out.len() as u16;
                out[2..4].copy_from_slice(&len.to_be_bytes());
                out
            })
    }

    /// One random byte overwrite or one truncation of a well-formed frame.
    fn mutated_bacnet() -> impl Strategy<Value = Vec<u8>> {
        (bacnet_frame(), any::<usize>(), any::<u8>(), any::<bool>()).prop_map(
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
        fn structured_bacnet_never_panics(bytes in mutated_bacnet()) {
            let _ = BacnetParser::new().parse(&bytes);
        }

        #[test]
        fn well_formed_bacnet_never_panics_in_the_normalizer(bytes in bacnet_frame()) {
            if let Ok(frame) = BacnetParser::new().parse(&bytes) {
                let _ = BacnetNormalizer::new().normalize(&frame);
            }
        }
    }
}
