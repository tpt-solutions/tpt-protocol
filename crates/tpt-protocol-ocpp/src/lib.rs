//! OCPP-J (Open Charge Point Protocol, JSON over WebSocket) for EV charging.
//!
//! OCPP 1.6 and 2.0.1 share the same JSON message framing. One WebSocket text
//! message is one OCPP message: `[2, id, action, payload]` for a Call,
//! `[3, id, payload]` for a CallResult, and `[4, id, code, description, details]`
//! for a CallError. The payload is kept as the raw JSON object text.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// Maximum length of a message id (OCPP-J section 4.2.3).
const MAX_UNIQUE_ID: usize = 36;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OcppMessageType {
    Call,
    CallResult,
    CallError,
}

impl OcppMessageType {
    fn from_id(v: u64) -> Option<Self> {
        match v {
            2 => Some(Self::Call),
            3 => Some(Self::CallResult),
            4 => Some(Self::CallError),
            _ => None,
        }
    }

    fn to_id(self) -> u8 {
        match self {
            Self::Call => 2,
            Self::CallResult => 3,
            Self::CallError => 4,
        }
    }
}

/// Action names common to OCPP 1.6 and 2.0.1. Anything else is `Other`.
#[derive(Debug, Clone, PartialEq)]
pub enum OcppAction {
    Authorize,
    BootNotification,
    ChangeConfiguration,
    DataTransfer,
    Heartbeat,
    MeterValues,
    RemoteStartTransaction,
    RemoteStopTransaction,
    Reset,
    StartTransaction,
    StatusNotification,
    StopTransaction,
    Other(String),
}

impl OcppAction {
    fn from_name(s: &str) -> Self {
        match s {
            "Authorize" => Self::Authorize,
            "BootNotification" => Self::BootNotification,
            "ChangeConfiguration" => Self::ChangeConfiguration,
            "DataTransfer" => Self::DataTransfer,
            "Heartbeat" => Self::Heartbeat,
            "MeterValues" => Self::MeterValues,
            "RemoteStartTransaction" => Self::RemoteStartTransaction,
            "RemoteStopTransaction" => Self::RemoteStopTransaction,
            "Reset" => Self::Reset,
            "StartTransaction" => Self::StartTransaction,
            "StatusNotification" => Self::StatusNotification,
            "StopTransaction" => Self::StopTransaction,
            other => Self::Other(other.into()),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Authorize => "Authorize",
            Self::BootNotification => "BootNotification",
            Self::ChangeConfiguration => "ChangeConfiguration",
            Self::DataTransfer => "DataTransfer",
            Self::Heartbeat => "Heartbeat",
            Self::MeterValues => "MeterValues",
            Self::RemoteStartTransaction => "RemoteStartTransaction",
            Self::RemoteStopTransaction => "RemoteStopTransaction",
            Self::Reset => "Reset",
            Self::StartTransaction => "StartTransaction",
            Self::StatusNotification => "StatusNotification",
            Self::StopTransaction => "StopTransaction",
            Self::Other(s) => s.as_str(),
        }
    }
}

/// One OCPP-J message.
#[derive(Debug, Clone, PartialEq)]
pub struct OcppFrame {
    pub message_type: OcppMessageType,
    pub unique_id: String,
    /// The action of a Call. `None` for CallResult and CallError, which do not
    /// name it (they are matched to a Call by `unique_id`).
    pub action: Option<OcppAction>,
    /// Raw JSON object text of the payload (for CallError, the error details).
    pub payload: String,
    /// Error code of a CallError.
    pub error_code: Option<String>,
    /// Error description of a CallError.
    pub error_description: Option<String>,
}

impl OcppFrame {
    /// Encodes the message as OCPP-J JSON text (UTF-8 bytes).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = format!("[{},", self.message_type.to_id());
        push_json_string(&mut out, &self.unique_id);
        match self.message_type {
            OcppMessageType::Call => {
                out.push(',');
                let action = self.action.as_ref().map(|a| a.as_str()).unwrap_or("");
                push_json_string(&mut out, action);
                out.push(',');
                out.push_str(&self.payload);
            }
            OcppMessageType::CallResult => {
                out.push(',');
                out.push_str(&self.payload);
            }
            OcppMessageType::CallError => {
                out.push(',');
                push_json_string(&mut out, self.error_code.as_deref().unwrap_or(""));
                out.push(',');
                push_json_string(&mut out, self.error_description.as_deref().unwrap_or(""));
                out.push(',');
                out.push_str(&self.payload);
            }
        }
        out.push(']');
        out.into_bytes()
    }
}

fn push_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// One element of the top-level JSON array.
enum Element<'a> {
    Str(String),
    Num(u64),
    /// An object or array, kept as raw text.
    Raw(&'a str),
}

struct Scanner<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn skip_ws(&mut self) {
        while let Some(c) = self.text[self.pos..].chars().next() {
            if matches!(c, ' ' | '\t' | '\r' | '\n') {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }

    fn expect(&mut self, b: u8) -> Result<(), Error> {
        if self.peek() == Some(b) {
            self.pos += 1;
            Ok(())
        } else {
            Err(Error::Parse(format!(
                "OCPP-J expected '{}' at byte {}",
                b as char, self.pos
            )))
        }
    }

    /// Parses a JSON string at the current position, decoding escapes.
    fn string(&mut self) -> Result<String, Error> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let c = self.text[self.pos..]
                .chars()
                .next()
                .ok_or_else(|| Error::Parse("OCPP-J unterminated string".into()))?;
            self.pos += c.len_utf8();
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let e = self.text[self.pos..]
                        .chars()
                        .next()
                        .ok_or_else(|| Error::Parse("OCPP-J unterminated escape".into()))?;
                    self.pos += e.len_utf8();
                    match e {
                        '"' | '\\' | '/' => out.push(e),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'u' => {
                            let hex = self.text.get(self.pos..self.pos + 4).ok_or_else(|| {
                                Error::Parse("OCPP-J truncated \\u escape".into())
                            })?;
                            let code = u32::from_str_radix(hex, 16)
                                .map_err(|_| Error::Parse("OCPP-J invalid \\u escape".into()))?;
                            self.pos += 4;
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        other => {
                            return Err(Error::Parse(format!("OCPP-J invalid escape \\{other}")))
                        }
                    }
                }
                c if (c as u32) < 0x20 => {
                    return Err(Error::Parse("OCPP-J control character in string".into()))
                }
                c => out.push(c),
            }
        }
    }

    /// Returns the raw text of the object or array starting at the current
    /// position, matching brackets and skipping over strings.
    fn raw_container(&mut self) -> Result<&'a str, Error> {
        let start = self.pos;
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        let bytes = self.text.as_bytes();
        while self.pos < bytes.len() {
            let b = bytes[self.pos];
            self.pos += 1;
            if in_string {
                if escaped {
                    escaped = false;
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'"' {
                    in_string = false;
                }
                continue;
            }
            match b {
                b'"' => in_string = true,
                b'{' | b'[' => depth += 1,
                b'}' | b']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(&self.text[start..self.pos]);
                    }
                }
                _ => {}
            }
        }
        Err(Error::Parse("OCPP-J unterminated object or array".into()))
    }

    fn element(&mut self) -> Result<Element<'a>, Error> {
        self.skip_ws();
        match self.peek() {
            Some(b'"') => Ok(Element::Str(self.string()?)),
            Some(b'{') | Some(b'[') => Ok(Element::Raw(self.raw_container()?)),
            Some(b'0'..=b'9') => {
                let start = self.pos;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
                let n: u64 = self.text[start..self.pos]
                    .parse()
                    .map_err(|_| Error::Parse("OCPP-J message type too large".into()))?;
                Ok(Element::Num(n))
            }
            _ => Err(Error::Parse("OCPP-J unexpected element".into())),
        }
    }
}

/// Parses OCPP-J text. Use it for one WebSocket message at a time.
pub struct OcppParser;

fn check_unique_id(id: &str) -> Result<(), Error> {
    if id.is_empty() || id.chars().count() > MAX_UNIQUE_ID {
        return Err(Error::Parse(format!(
            "OCPP-J message id must be 1-{MAX_UNIQUE_ID} characters"
        )));
    }
    Ok(())
}

fn expect_object(raw: &str, what: &str) -> Result<String, Error> {
    if raw.starts_with('{') {
        Ok(raw.into())
    } else {
        Err(Error::Parse(format!("OCPP-J {what} must be a JSON object")))
    }
}

impl FrameParser for OcppParser {
    type Frame = OcppFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        let text =
            core::str::from_utf8(bytes).map_err(|_| Error::Parse("OCPP-J is not UTF-8".into()))?;
        let mut s = Scanner { text, pos: 0 };
        s.skip_ws();
        s.expect(b'[')?;

        let mut elements = Vec::new();
        s.skip_ws();
        if s.peek() == Some(b']') {
            return Err(Error::Parse("OCPP-J array is empty".into()));
        }
        loop {
            elements.push(s.element()?);
            s.skip_ws();
            match s.peek() {
                Some(b',') => {
                    s.pos += 1;
                }
                Some(b']') => {
                    s.pos += 1;
                    break;
                }
                _ => return Err(Error::Parse("OCPP-J expected ',' or ']'".into())),
            }
        }
        s.skip_ws();
        if s.pos != text.len() {
            return Err(Error::Parse(
                "OCPP-J has trailing bytes after the message".into(),
            ));
        }

        let message_type = match elements.first() {
            Some(Element::Num(n)) => OcppMessageType::from_id(*n)
                .ok_or_else(|| Error::Parse(format!("OCPP-J unknown message type {n}")))?,
            _ => return Err(Error::Parse("OCPP-J message type must be a number".into())),
        };
        let unique_id = match elements.get(1) {
            Some(Element::Str(id)) => id.clone(),
            _ => return Err(Error::Parse("OCPP-J message id must be a string".into())),
        };
        check_unique_id(&unique_id)?;

        let expected_len = match message_type {
            OcppMessageType::Call => 4,
            OcppMessageType::CallResult => 3,
            OcppMessageType::CallError => 5,
        };
        if elements.len() != expected_len {
            return Err(Error::Parse(format!(
                "OCPP-J {message_type:?} needs {expected_len} elements, found {}",
                elements.len()
            )));
        }

        let frame = match message_type {
            OcppMessageType::Call => {
                let action = match &elements[2] {
                    Element::Str(a) => OcppAction::from_name(a),
                    _ => return Err(Error::Parse("OCPP-J action must be a string".into())),
                };
                let payload = match &elements[3] {
                    Element::Raw(raw) => expect_object(raw, "payload")?,
                    _ => return Err(Error::Parse("OCPP-J payload must be an object".into())),
                };
                OcppFrame {
                    message_type,
                    unique_id,
                    action: Some(action),
                    payload,
                    error_code: None,
                    error_description: None,
                }
            }
            OcppMessageType::CallResult => {
                let payload = match &elements[2] {
                    Element::Raw(raw) => expect_object(raw, "payload")?,
                    _ => return Err(Error::Parse("OCPP-J payload must be an object".into())),
                };
                OcppFrame {
                    message_type,
                    unique_id,
                    action: None,
                    payload,
                    error_code: None,
                    error_description: None,
                }
            }
            OcppMessageType::CallError => {
                let code = match &elements[2] {
                    Element::Str(c) => c.clone(),
                    _ => return Err(Error::Parse("OCPP-J error code must be a string".into())),
                };
                let description = match &elements[3] {
                    Element::Str(d) => d.clone(),
                    _ => {
                        return Err(Error::Parse(
                            "OCPP-J error description must be a string".into(),
                        ))
                    }
                };
                let details = match &elements[4] {
                    Element::Raw(raw) => expect_object(raw, "error details")?,
                    _ => {
                        return Err(Error::Parse(
                            "OCPP-J error details must be an object".into(),
                        ))
                    }
                };
                OcppFrame {
                    message_type,
                    unique_id,
                    action: None,
                    payload: details,
                    error_code: Some(code),
                    error_description: Some(description),
                }
            }
        };
        Ok(frame)
    }
}

/// Validates OCPP connection state.
pub struct OcppValidator;

impl StateValidator for OcppValidator {
    type State = OcppConnectionState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("ocpp: not connected".into()));
        }
        if state.last_message_type == OcppMessageType::CallError && state.error_count > 5 {
            return Err(Error::State("ocpp: too many errors".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OcppConnectionState {
    pub connected: bool,
    pub last_message_type: OcppMessageType,
    pub error_count: u32,
}

/// Normalizes OCPP-J messages. The device is the action (for a Call) or the
/// message type (for a result or error). The point is the message id.
pub struct OcppNormalizer;

impl DataNormalizer for OcppNormalizer {
    type Frame = OcppFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = match &frame.action {
            Some(action) => format!("ocpp/{}", action.as_str()),
            None => format!("ocpp/{:?}", frame.message_type),
        };
        let value = TupValue::Text(frame.payload.clone());
        let quality = match frame.message_type {
            OcppMessageType::CallError => Quality::Bad,
            _ => Quality::Good,
        };
        Ok(DataPoint::new(
            SourceProtocol::Ocpp,
            device_id,
            frame.unique_id.clone(),
            0,
            value,
            quality,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<OcppFrame, Error> {
        OcppParser.parse(text.as_bytes())
    }

    #[test]
    fn call_boot_notification_parses() {
        let frame = parse(
            r#"[2,"19223201",
            "BootNotification",{"chargePointVendor":"VendorX","chargePointModel":"M1"}]"#,
        )
        .unwrap();
        assert_eq!(frame.message_type, OcppMessageType::Call);
        assert_eq!(frame.unique_id, "19223201");
        assert_eq!(frame.action, Some(OcppAction::BootNotification));
        assert_eq!(
            frame.payload,
            r#"{"chargePointVendor":"VendorX","chargePointModel":"M1"}"#
        );
    }

    #[test]
    fn call_result_has_no_action() {
        let frame = parse(r#"[3,"19223201",{"status":"Accepted","interval":300}]"#).unwrap();
        assert_eq!(frame.message_type, OcppMessageType::CallResult);
        assert_eq!(frame.action, None);
        assert_eq!(frame.payload, r#"{"status":"Accepted","interval":300}"#);
    }

    #[test]
    fn call_error_carries_code_description_and_details() {
        let frame = parse(r#"[4,"19223201","NotImplemented","Not supported",{}]"#).unwrap();
        assert_eq!(frame.message_type, OcppMessageType::CallError);
        assert_eq!(frame.error_code.as_deref(), Some("NotImplemented"));
        assert_eq!(frame.error_description.as_deref(), Some("Not supported"));
        assert_eq!(frame.payload, "{}");
    }

    #[test]
    fn escapes_in_strings_are_decoded() {
        // "BootNotification" written with a \u escape.
        let frame = parse(r#"[2,"a\"b","BootNotification",{}]"#).unwrap();
        assert_eq!(frame.unique_id, "a\"b");
        assert_eq!(frame.action, Some(OcppAction::BootNotification));
    }

    #[test]
    fn nested_brackets_and_strings_in_payload_are_kept_raw() {
        let raw = r#"{"a":"}]","b":{"c":[1,2,{"d":"\"x"}]}}"#;
        let frame = parse(&format!(r#"[2,"id","Heartbeat",{raw}]"#)).unwrap();
        assert_eq!(frame.payload, raw);
    }

    #[test]
    fn unknown_action_is_kept() {
        let frame = parse(r#"[2,"id","VendorSpecific",{}]"#).unwrap();
        assert_eq!(
            frame.action,
            Some(OcppAction::Other("VendorSpecific".into()))
        );
    }

    #[test]
    fn message_id_is_limited_to_36_characters() {
        let long = "x".repeat(37);
        assert!(parse(&format!(r#"[3,"{long}",{{}}]"#)).is_err());
        let ok = "x".repeat(36);
        assert!(parse(&format!(r#"[3,"{ok}",{{}}]"#)).is_ok());
    }

    #[test]
    fn wrong_element_count_is_rejected() {
        assert!(parse(r#"[2,"id","Heartbeat"]"#).is_err());
        assert!(parse(r#"[3,"id",{},{}]"#).is_err());
    }

    #[test]
    fn non_object_payload_is_rejected() {
        assert!(parse(r#"[3,"id",[1,2]]"#).is_err());
        assert!(parse(r#"[3,"id","text"]"#).is_err());
    }

    #[test]
    fn unknown_message_type_and_non_array_are_rejected() {
        assert!(parse(r#"[5,"id",{}]"#).is_err());
        assert!(parse(r#"{"id":1}"#).is_err());
        assert!(parse("").is_err());
    }

    #[test]
    fn trailing_bytes_are_rejected() {
        assert!(parse(r#"[3,"id",{}] x"#).is_err());
    }

    #[test]
    fn unterminated_input_is_rejected() {
        assert!(parse(r#"[2,"id","Heartbeat",{"a":1"#).is_err());
        assert!(parse(r#"[2,"id"#).is_err());
    }

    #[test]
    fn non_utf8_is_rejected() {
        assert!(OcppParser.parse(&[0xFF, 0xFE]).is_err());
    }

    #[test]
    fn encode_then_parse_round_trips() {
        let frame = OcppFrame {
            message_type: OcppMessageType::CallError,
            unique_id: "q\"1".into(),
            action: None,
            payload: "{\"k\":1}".into(),
            error_code: Some("FormationViolation".into()),
            error_description: Some("bad\nfield".into()),
        };
        let bytes = frame.to_bytes();
        assert_eq!(OcppParser.parse(&bytes).unwrap(), frame);
    }

    #[test]
    fn validator_rules() {
        let ok = OcppConnectionState {
            connected: true,
            last_message_type: OcppMessageType::Call,
            error_count: 0,
        };
        assert!(OcppValidator.validate(&ok).is_ok());
        let down = OcppConnectionState {
            connected: false,
            ..ok.clone()
        };
        assert!(OcppValidator.validate(&down).is_err());
        let errs = OcppConnectionState {
            connected: true,
            last_message_type: OcppMessageType::CallError,
            error_count: 6,
        };
        assert!(OcppValidator.validate(&errs).is_err());
    }

    #[test]
    fn normalize_call_uses_action_and_message_id() {
        let frame = parse(r#"[2,"id-7","Heartbeat",{}]"#).unwrap();
        let dp = OcppNormalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Ocpp);
        assert_eq!(dp.device_id, "ocpp/Heartbeat");
        assert_eq!(dp.point_id, "id-7");
        assert_eq!(dp.value, TupValue::Text("{}".into()));
    }

    #[test]
    fn normalize_call_error_is_bad_quality() {
        let frame = parse(r#"[4,"id","InternalError","x",{}]"#).unwrap();
        let dp = OcppNormalizer.normalize(&frame).unwrap();
        assert_eq!(dp.quality, Quality::Bad);
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
            let _ = OcppParser.parse(&bytes);
        }
    }

    /// An OCPP-J message array: CALL `[2,id,action,payload]`, CALLRESULT `[3,id,payload]`
    /// or CALLERROR `[4,id,code,description,details]`. Ids run from empty to past the
    /// 36-character limit, and payloads are nested objects with escapes in strings.
    fn ocpp_message() -> impl Strategy<Value = Vec<u8>> {
        let id = "[A-Za-z0-9\\-\"\\\\]{0,40}";
        let action = proptest::sample::select(alloc::vec![
            "BootNotification",
            "Heartbeat",
            "StatusNotification",
            "Authorize",
            "Nope",
        ]);
        let payload = "[a-zA-Z0-9 ]{0,8}".prop_map(|s| {
            alloc::format!(
                r#"{{"vendor":"{}","n":[1,2.5,true,null]}}"#,
                s.replace('"', "")
            )
        });
        (0u8..3, id, action, payload, 0u8..3).prop_map(|(kind, id, action, payload, skew)| {
            let id = id.replace('\\', "");
            match kind {
                0 => alloc::format!(r#"[2,"{id}","{action}",{payload}]"#),
                1 => alloc::format!(r#"[3,"{id}",{payload}]"#),
                _ => alloc::format!(r#"[4,"{id}","GenericError","x",{payload}]"#),
            }
            .into_bytes()
            .into_iter()
            // A skewed copy drops or repeats the closing bracket, so the array
            // length checks run too.
            .chain(core::iter::repeat_n(b']', skew as usize))
            .collect()
        })
    }

    /// One random byte overwrite or one truncation applied to a valid-looking message.
    fn mutated_ocpp() -> impl Strategy<Value = Vec<u8>> {
        (ocpp_message(), any::<usize>(), any::<u8>(), any::<bool>()).prop_map(
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
        fn structured_ocpp_never_panics(bytes in mutated_ocpp()) {
            if let Ok(frame) = OcppParser.parse(&bytes) {
                let _ = OcppNormalizer.normalize_all(&frame);
            }
        }
    }
}
