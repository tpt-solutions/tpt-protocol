//! CoAP (Constrained Application Protocol) for IoT device communication.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CoapType {
    Confirmable,
    NonConfirmable,
    Acknowledgement,
    Reset,
}

impl CoapType {
    fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Confirmable),
            1 => Some(Self::NonConfirmable),
            2 => Some(Self::Acknowledgement),
            3 => Some(Self::Reset),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Confirmable => "CON",
            Self::NonConfirmable => "NON",
            Self::Acknowledgement => "ACK",
            Self::Reset => "RST",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum CoapCode {
    Empty = 0,
    Get = 1,
    Post = 2,
    Put = 3,
    Delete = 4,
    Created = 65,
    Deleted = 66,
    Valid = 67,
    Changed = 68,
    Content = 69,
    BadRequest = 128,
    Unauthorized = 129,
    BadOption = 130,
    Forbidden = 131,
    NotFound = 132,
    MethodNotAllowed = 133,
    NotAcceptable = 134,
    PreconditionFailed = 140,
    RequestEntityTooLarge = 141,
    UnsupportedContentFormat = 143,
    InternalServerError = 160,
    NotImplemented = 161,
    BadGateway = 162,
    ServiceUnavailable = 163,
    GatewayTimeout = 164,
    ProxyingNotSupported = 165,
    Unknown(u8),
}

#[allow(dead_code)]
impl CoapCode {
    fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Empty,
            1 => Self::Get,
            2 => Self::Post,
            3 => Self::Put,
            4 => Self::Delete,
            65 => Self::Created,
            66 => Self::Deleted,
            67 => Self::Valid,
            68 => Self::Changed,
            69 => Self::Content,
            128 => Self::BadRequest,
            129 => Self::Unauthorized,
            130 => Self::BadOption,
            131 => Self::Forbidden,
            132 => Self::NotFound,
            133 => Self::MethodNotAllowed,
            134 => Self::NotAcceptable,
            140 => Self::PreconditionFailed,
            141 => Self::RequestEntityTooLarge,
            143 => Self::UnsupportedContentFormat,
            160 => Self::InternalServerError,
            161 => Self::NotImplemented,
            162 => Self::BadGateway,
            163 => Self::ServiceUnavailable,
            164 => Self::GatewayTimeout,
            165 => Self::ProxyingNotSupported,
            other => Self::Unknown(other),
        }
    }

    fn as_u8(&self) -> u8 {
        match self {
            Self::Empty => 0,
            Self::Get => 1,
            Self::Post => 2,
            Self::Put => 3,
            Self::Delete => 4,
            Self::Created => 65,
            Self::Deleted => 66,
            Self::Valid => 67,
            Self::Changed => 68,
            Self::Content => 69,
            Self::BadRequest => 128,
            Self::Unauthorized => 129,
            Self::BadOption => 130,
            Self::Forbidden => 131,
            Self::NotFound => 132,
            Self::MethodNotAllowed => 133,
            Self::NotAcceptable => 134,
            Self::PreconditionFailed => 140,
            Self::RequestEntityTooLarge => 141,
            Self::UnsupportedContentFormat => 143,
            Self::InternalServerError => 160,
            Self::NotImplemented => 161,
            Self::BadGateway => 162,
            Self::ServiceUnavailable => 163,
            Self::GatewayTimeout => 164,
            Self::ProxyingNotSupported => 165,
            Self::Unknown(v) => *v,
        }
    }

    fn is_method(&self) -> bool {
        matches!(self, Self::Get | Self::Post | Self::Put | Self::Delete)
    }

    fn is_success(&self) -> bool {
        matches!(
            self,
            Self::Created | Self::Deleted | Self::Valid | Self::Changed | Self::Content
        )
    }

    fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::BadRequest
                | Self::Unauthorized
                | Self::BadOption
                | Self::Forbidden
                | Self::NotFound
                | Self::MethodNotAllowed
                | Self::NotAcceptable
                | Self::PreconditionFailed
                | Self::RequestEntityTooLarge
                | Self::UnsupportedContentFormat
        )
    }

    fn is_server_error(&self) -> bool {
        matches!(
            self,
            Self::InternalServerError
                | Self::NotImplemented
                | Self::BadGateway
                | Self::ServiceUnavailable
                | Self::GatewayTimeout
                | Self::ProxyingNotSupported
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CoapOption {
    UriPath(Vec<String>),
    ContentFormat(u16),
    MaxAge(u32),
    ETag(Vec<u8>),
    Observe(u32),
    Other { number: u16, value: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoapFrame {
    pub version: u8,
    pub msg_type: CoapType,
    pub token_len: u8,
    pub code: CoapCode,
    pub message_id: u16,
    pub token: Vec<u8>,
    pub options: Vec<CoapOption>,
    pub payload: Vec<u8>,
}

pub struct CoapParser;

impl FrameParser for CoapParser {
    type Frame = CoapFrame;

    /// CoAP binary header (RFC 7252):
    /// [0] bits 7-6: version, bits 5-4: type, bits 3-0: token len
    /// [1]: code
    /// [2-3]: message ID
    /// [4..4+token_len]: token
    /// [4+token_len..]: options (type-length-value) + payload marker 0xFF + payload
    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 4 {
            return Err(Error::Parse("coap frame too short".into()));
        }

        let version = (bytes[0] >> 6) & 0x03;
        if version != 1 {
            return Err(Error::Parse("invalid coap version".into()));
        }

        let msg_type = CoapType::from_u8((bytes[0] >> 4) & 0x03)
            .ok_or_else(|| Error::Parse("invalid coap type".into()))?;

        let token_len = bytes[0] & 0x0f;
        if token_len > 8 {
            return Err(Error::Parse("token len too large".into()));
        }

        let code = CoapCode::from_u8(bytes[1]);

        let message_id = u16::from_be_bytes([bytes[2], bytes[3]]);

        let header_len = 4 + token_len as usize;
        if bytes.len() < header_len {
            return Err(Error::Parse("truncated token".into()));
        }

        let token = bytes[4..header_len].to_vec();

        let mut offset = header_len;
        let mut options = Vec::new();
        let mut prev_option_number: u16 = 0;

        loop {
            if offset >= bytes.len() {
                break;
            }
            if bytes[offset] == 0xFF {
                offset += 1;
                break;
            }

            let opt_delta_raw = (bytes[offset] >> 4) & 0x0f;
            let opt_len_raw = bytes[offset] & 0x0f;
            offset += 1;

            let delta = if opt_delta_raw < 13 {
                opt_delta_raw as u16
            } else if opt_delta_raw == 13 {
                if offset >= bytes.len() {
                    return Err(Error::Parse("truncated option delta ext".into()));
                }
                let ext = bytes[offset] as u16 + 13;
                offset += 1;
                ext
            } else if opt_delta_raw == 14 {
                if offset + 1 >= bytes.len() {
                    return Err(Error::Parse("truncated option delta ext2".into()));
                }
                let ext = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) + 269;
                offset += 2;
                ext
            } else {
                return Err(Error::Parse("option delta reserved (15)".into()));
            };

            let len = if opt_len_raw < 13 {
                opt_len_raw as u16
            } else if opt_len_raw == 13 {
                if offset >= bytes.len() {
                    return Err(Error::Parse("truncated option len ext".into()));
                }
                let ext = bytes[offset] as u16 + 13;
                offset += 1;
                ext
            } else if opt_len_raw == 14 {
                if offset + 1 >= bytes.len() {
                    return Err(Error::Parse("truncated option len ext2".into()));
                }
                let ext = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]) + 269;
                offset += 2;
                ext
            } else {
                return Err(Error::Parse("option len reserved (15)".into()));
            };

            let option_number = prev_option_number.wrapping_add(delta);
            prev_option_number = option_number;

            if offset + len as usize > bytes.len() {
                return Err(Error::Parse("truncated option value".into()));
            }
            let opt_value = bytes[offset..offset + len as usize].to_vec();
            offset += len as usize;

            let option = match option_number {
                11 => {
                    let s = core::str::from_utf8(&opt_value).unwrap_or("");
                    let parts: Vec<String> = if s.contains('/') {
                        s.split('/').map(|p| p.into()).collect()
                    } else {
                        alloc::vec![s.into()]
                    };
                    CoapOption::UriPath(parts)
                }
                12 => {
                    if opt_value.len() >= 2 {
                        CoapOption::ContentFormat(u16::from_be_bytes([opt_value[0], opt_value[1]]))
                    } else if opt_value.len() == 1 {
                        CoapOption::ContentFormat(opt_value[0] as u16)
                    } else {
                        CoapOption::ContentFormat(0)
                    }
                }
                14 => {
                    let mut age_buf = [0u8; 4];
                    let age_len = opt_value.len().min(4);
                    age_buf[4 - age_len..].copy_from_slice(&opt_value[..age_len]);
                    CoapOption::MaxAge(u32::from_be_bytes(age_buf))
                }
                4 => CoapOption::ETag(opt_value),
                6 => CoapOption::Observe({
                    let mut obs_buf = [0u8; 4];
                    let obs_len = opt_value.len().min(4);
                    obs_buf[4 - obs_len..].copy_from_slice(&opt_value[..obs_len]);
                    u32::from_be_bytes(obs_buf)
                }),
                other => CoapOption::Other {
                    number: other,
                    value: opt_value,
                },
            };

            options.push(option);
        }

        let payload = if offset < bytes.len() {
            bytes[offset..].to_vec()
        } else {
            Vec::new()
        };

        Ok(CoapFrame {
            version,
            msg_type,
            token_len,
            code,
            message_id,
            token,
            options,
            payload,
        })
    }
}

pub struct CoapValidator;

impl StateValidator for CoapValidator {
    type State = CoapConnectionState;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if !state.connected {
            return Err(Error::State("coap: not connected".into()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoapConnectionState {
    pub connected: bool,
    pub last_message_id: u16,
    pub observe_counter: u32,
}

pub struct CoapNormalizer;

impl DataNormalizer for CoapNormalizer {
    type Frame = CoapFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = alloc::format!("coap/{}", frame.msg_type.as_str());
        let point_id = alloc::format!("mid:{}", frame.message_id);

        let value = if frame.payload.is_empty() {
            TupValue::Int(frame.code.as_u8() as i64)
        } else {
            TupValue::Bytes(frame.payload.clone())
        };

        Ok(DataPoint::new(
            SourceProtocol::Coap,
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

    fn make_get_request() -> Vec<u8> {
        let mut buf = Vec::new();
        // version=1, type=CON(0), token_len=0
        buf.push(0b0100_0000);
        buf.push(1); // GET
        buf.extend(&0x0001u16.to_be_bytes()); // message ID
                                              // option: Uri-Path (opt 11), len=4, value="test"
        buf.push((11 << 4) | 4); // delta=11, len=4
        buf.extend(b"test");
        // payload marker
        buf.push(0xFF);
        buf.extend(b"{}");
        buf
    }

    fn make_post_request() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(0b0100_0000); // ver=1, CON, token_len=0
        buf.push(2); // POST
        buf.extend(&0x0002u16.to_be_bytes());
        buf.push(0xFF);
        buf.extend(b"{\"value\":42}");
        buf
    }

    fn make_response() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.push(0b0110_0000); // ver=1, ACK(2), token_len=0
        buf.push(69); // Content
        buf.extend(&0x0001u16.to_be_bytes());
        buf.push(0xFF);
        buf.extend(b"{\"state\":true}");
        buf
    }

    #[test]
    fn valid_parse_get() {
        let parser = CoapParser;
        let bytes = make_get_request();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.version, 1);
        assert_eq!(frame.msg_type, CoapType::Confirmable);
        assert_eq!(frame.code, CoapCode::Get);
        assert_eq!(frame.message_id, 1);
    }

    #[test]
    fn valid_parse_post() {
        let parser = CoapParser;
        let bytes = make_post_request();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.code, CoapCode::Post);
        assert_eq!(frame.payload, b"{\"value\":42}");
    }

    #[test]
    fn valid_parse_response() {
        let parser = CoapParser;
        let bytes = make_response();
        let frame = parser.parse(&bytes).unwrap();
        assert_eq!(frame.msg_type, CoapType::Acknowledgement);
        assert_eq!(frame.code, CoapCode::Content);
    }

    #[test]
    fn invalid_version() {
        let parser = CoapParser;
        let mut bytes = make_get_request();
        bytes[0] = 0b1000_0000; // version = 2
        let result = parser.parse(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn truncation() {
        let parser = CoapParser;
        let result = parser.parse(&[0x40]);
        assert!(result.is_err());
    }

    #[test]
    fn garbage() {
        let parser = CoapParser;
        let result = parser.parse(&[0xff; 32]);
        assert!(result.is_err());
    }

    #[test]
    fn normalize_roundtrip() {
        let parser = CoapParser;
        let normalizer = CoapNormalizer;
        let bytes = make_get_request();
        let frame = parser.parse(&bytes).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Coap);
    }

    #[test]
    fn validator_ok() {
        let v = CoapValidator;
        let s = CoapConnectionState {
            connected: true,
            last_message_id: 0,
            observe_counter: 0,
        };
        assert!(v.validate(&s).is_ok());
    }

    #[test]
    fn validator_fail() {
        let v = CoapValidator;
        let s = CoapConnectionState {
            connected: false,
            last_message_id: 0,
            observe_counter: 0,
        };
        assert!(v.validate(&s).is_err());
    }
}
