//! Language bindings for TPT Protocol: decode one frame to TUP JSON from C,
//! WebAssembly or Python.
//!
//! The same dispatch serves all three. Each protocol is a parser and a
//! normalizer; a frame goes through `parse`, then `normalize_all`, and every
//! data point is written as the JSON object the daemon sends over WebSocket.
//!
//! - C: [`ffi`], declared in `include/tpt_protocol.h`.
//! - WebAssembly: `--features wasm` (see the `www/` playground).
//! - Python: `--features python`, built with `maturin` or `cargo build`.
#![deny(unsafe_op_in_unsafe_fn)]

pub mod ffi;

#[cfg(feature = "python")]
mod python;
#[cfg(feature = "wasm")]
mod wasm;

use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_tup::json;

macro_rules! protocols {
    ($( ($name:literal, $parser:expr, $normalizer:expr) ),+ $(,)?) => {
        /// Every protocol name [`decode_json`] accepts.
        pub const PROTOCOLS: &[&str] = &[$($name),+];

        /// Decodes one frame with the named protocol. Returns the data points as a
        /// JSON array, or an error message.
        pub fn decode_json(protocol: &str, bytes: &[u8]) -> Result<String, String> {
            let points = match protocol {
                $(
                    $name => {
                        let frame = ($parser).parse(bytes).map_err(|e| e.to_string())?;
                        ($normalizer).normalize_all(&frame).map_err(|e| e.to_string())?
                    }
                )+
                other => return Err(format!("unknown protocol '{other}'")),
            };
            let mut out = String::from("[");
            for (i, dp) in points.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&json::to_json_string(dp).map_err(|e| e.to_string())?);
            }
            out.push(']');
            Ok(out)
        }
    };
}

protocols! {
    ("modbus", tpt_protocol_modbus::ModbusParser::new(), tpt_protocol_modbus::ModbusNormalizer::new()),
    ("j1939", tpt_protocol_j1939::J1939Parser::new(), tpt_protocol_j1939::J1939Normalizer::new()),
    ("isobus", tpt_protocol_isobus::IsobusParser::new(), tpt_protocol_isobus::IsobusNormalizer::new()),
    ("uds", tpt_protocol_uds::UdsParser::new(), tpt_protocol_uds::UdsNormalizer::new()),
    ("canopen", tpt_protocol_canopen::CanOpenParser::new(), tpt_protocol_canopen::CanOpenNormalizer::new()),
    ("doip-someip", tpt_protocol_doip_someip::DoipSomeipParser::new(), tpt_protocol_doip_someip::DoipSomeipNormalizer::new()),
    ("nmea", tpt_protocol_nmea::NmeaParser::new(), tpt_protocol_nmea::NmeaNormalizer::new()),
    ("mavlink", tpt_protocol_mavlink::MavlinkParser::new(), tpt_protocol_mavlink::MavlinkNormalizer::new()),
    ("bacnet", tpt_protocol_bacnet::BacnetParser::new(), tpt_protocol_bacnet::BacnetNormalizer::new()),
    ("opcua", tpt_protocol_opcua::OpcuaParser::new(), tpt_protocol_opcua::OpcuaNormalizer::new()),
    ("dnp3", tpt_protocol_dnp3::Dnp3Parser::new(), tpt_protocol_dnp3::Dnp3Normalizer::new()),
    ("ethercat", tpt_protocol_ethercat::EthercatParser::new(), tpt_protocol_ethercat::EthercatNormalizer::new()),
    ("sparkplugb", tpt_protocol_sparkplugb::SparkplugParser, tpt_protocol_sparkplugb::SparkplugNormalizer),
    ("ocpp", tpt_protocol_ocpp::OcppParser, tpt_protocol_ocpp::OcppNormalizer),
    ("ieee11073-sdc", tpt_protocol_ieee11073_sdc::SdcParser, tpt_protocol_ieee11073_sdc::SdcNormalizer),
    ("hl7v2", tpt_protocol_hl7v2::Hl7Parser, tpt_protocol_hl7v2::Hl7Normalizer),
    ("knx", tpt_protocol_knx::KnxParser, tpt_protocol_knx::KnxNormalizer),
    ("matter-thread", tpt_protocol_matter_thread::MatterParser, tpt_protocol_matter_thread::MatterNormalizer),
    ("coap", tpt_protocol_coap::CoapParser, tpt_protocol_coap::CoapNormalizer),
}

/// Decodes a frame given as hex text, such as `"01 06 00 01 00 0A 58 0D"`. Spaces
/// are ignored, and a `0x` prefix is accepted.
pub fn decode_hex(protocol: &str, hex_text: &str) -> Result<String, String> {
    let trimmed = hex_text.trim();
    let trimmed = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    let compact: String = trimmed.split_whitespace().collect();
    let bytes = hex::decode(&compact).map_err(|e| format!("invalid hex: {e}"))?;
    decode_json(protocol, &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Modbus RTU: Write Single Register (slave 1, address 1, value 10), with CRC.
    const RTU_WRITE: [u8; 8] = [0x01, 0x06, 0x00, 0x01, 0x00, 0x0A, 0x58, 0x0D];

    #[test]
    fn decodes_a_modbus_frame_to_json() {
        let json = decode_json("modbus", &RTU_WRITE).unwrap();
        assert!(json.starts_with('[') && json.ends_with(']'), "{json}");
        assert!(json.contains("\"source_protocol\":\"modbus\""), "{json}");
        assert!(json.contains("\"value\":10"), "{json}");
    }

    #[test]
    fn hex_text_decodes_the_same_frame() {
        let from_hex = decode_hex("modbus", "01 06 00 01 00 0A 58 0D").unwrap();
        assert_eq!(from_hex, decode_json("modbus", &RTU_WRITE).unwrap());
        assert!(decode_hex("modbus", "0x01060001000A580D").is_ok());
        assert!(decode_hex("modbus", "zz").is_err());
    }

    #[test]
    fn unknown_protocol_and_bad_frames_are_errors() {
        assert!(decode_json("nope", &RTU_WRITE).is_err());
        assert!(decode_json("modbus", &[0xFF]).is_err());
    }

    #[test]
    fn every_listed_protocol_is_dispatched() {
        assert!(PROTOCOLS.len() >= 19);
        for protocol in PROTOCOLS {
            // Any input may fail to decode, but a listed name must never be "unknown".
            let err = decode_json(protocol, &[]).unwrap_err();
            assert!(!err.contains("unknown protocol"), "{protocol}: {err}");
        }
    }
}
