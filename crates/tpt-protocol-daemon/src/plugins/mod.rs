//! Simulated protocol plugins for the daemon.
//!
//! Every protocol crate in the workspace gets a corresponding `ProtocolPlugin`
//! that emits synthetic TUP data points, so the daemon can be driven end-to-end
//! with no hardware. **There is no real transport** (Serial/CAN/TCP/UDP); real
//! I/O remains future work (see the Later/Deferred section of TODO.md).

use std::pin::Pin;
use std::time::Duration;

use futures_util::stream::{self, Stream};
use tpt_protocol_core::{DataNormalizer, FrameParser, ProtocolPlugin};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Rotate through the full set of qualities so the TUI's color coding is
/// demoable out of the box (Good / Uncertain / Bad / Stale).
fn cycle_quality(tick: u64) -> Quality {
    match tick % 4 {
        0 => Quality::Good,
        1 => Quality::Uncertain,
        2 => Quality::Bad,
        _ => Quality::Stale,
    }
}

fn synthesize(proto: SourceProtocol, tick: u64, quality: Quality, ts: u64) -> DataPoint {
    let device_id = format!("{}-sim-01", proto.as_str());
    let point_id = format!("{}-point-{}", proto.as_str(), (tick % 3) + 1);
    let value = TupValue::Float(50.0 + (tick as f64 * 0.3).sin() * 25.0);
    DataPoint::new(proto, device_id, point_id, ts, value, quality)
}

macro_rules! def_protocol {
    ($(($plugin:ident, $name:literal, $parser:expr, $normalizer:expr, $proto:expr)),+ $(,)?) => {
        $(
            pub struct $plugin;

            impl ProtocolPlugin for $plugin {
                fn simulate(
                    self: Box<Self>,
                ) -> Pin<Box<dyn Stream<Item = DataPoint> + Send>> {
                    Box::pin(stream::unfold(0u64, move |tick| async move {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        // Construct the crate's parser so the dependency is
                        // genuinely exercised by the simulation build.
                        let _parser = $parser;
                        let ts = now_ms();
                        let dp = synthesize($proto, tick, cycle_quality(tick), ts);
                        Some((dp, tick.wrapping_add(1)))
                    }))
                }
            }
        )+

        /// Build a simulated plugin for the named protocol.
        pub fn build_plugin(name: &str) -> Result<Box<dyn ProtocolPlugin>, String> {
            match name {
                $(
                    $name => Ok(Box::new($plugin)),
                )+
                other => Err(format!("unknown protocol: {other}")),
            }
        }

        /// All protocol names understood by the daemon.
        pub fn list_protocols() -> &'static [&'static str] {
            &[ $($name),+ ]
        }

        /// Parse raw bytes with the named protocol's `FrameParser`/`DataNormalizer`.
        pub fn replay_one(name: &str, bytes: &[u8]) -> Result<DataPoint, String> {
            match name {
                $(
                    $name => {
                        let frame = ($parser)
                            .parse(bytes)
                            .map_err(|e| format!("{e:?}"))?;
                        let mut dp = ($normalizer)
                            .normalize(&frame)
                            .map_err(|e| format!("{e:?}"))?;
                        // Normalizers have no clock and leave timestamp 0. Stamp it at ingest.
                        if dp.timestamp == 0 {
                            dp.timestamp = now_ms();
                        }
                        Ok(dp)
                    }
                )+
                other => Err(format!("unknown protocol: {other}")),
            }
        }

        /// Frames one message from the front of `bytes` with the protocol's
        /// `try_parse`, and normalizes it.
        ///
        /// Returns `Ok(None)` when more bytes are needed. Otherwise returns the
        /// number of bytes used and the frame's points, or the normalization
        /// error for a frame that parsed but carries no data (such as a read
        /// request). Parse errors are `Err`.
        pub fn decode_one(
            name: &str,
            bytes: &[u8],
        ) -> Result<Option<(Result<Vec<DataPoint>, String>, usize)>, String> {
            match name {
                $(
                    $name => {
                        let parser = $parser;
                        let normalizer = $normalizer;
                        match parser.try_parse(bytes).map_err(|e| format!("{e:?}"))? {
                            None => Ok(None),
                            Some((frame, used)) => {
                                let points = normalizer
                                    .normalize_all(&frame)
                                    .map(|mut points| {
                                        for dp in &mut points {
                                            if dp.timestamp == 0 {
                                                dp.timestamp = now_ms();
                                            }
                                        }
                                        points
                                    })
                                    .map_err(|e| format!("{e:?}"));
                                Ok(Some((points, used)))
                            }
                        }
                    }
                )+
                other => Err(format!("unknown protocol: {other}")),
            }
        }
    };
}

def_protocol! {
    (ModbusPlugin,            "modbus",          tpt_protocol_modbus::ModbusParser::new(),        tpt_protocol_modbus::ModbusNormalizer::new(),        SourceProtocol::Modbus),
    (J1939Plugin,             "j1939",           tpt_protocol_j1939::J1939Parser::new(),          tpt_protocol_j1939::J1939Normalizer::new(),          SourceProtocol::J1939),
    (IsobusPlugin,            "isobus",          tpt_protocol_isobus::IsobusParser::new(),        tpt_protocol_isobus::IsobusNormalizer::new(),        SourceProtocol::Isobus),
    (UdsPlugin,               "uds",             tpt_protocol_uds::UdsParser::new(),              tpt_protocol_uds::UdsNormalizer::new(),              SourceProtocol::Uds),
    (CanopenPlugin,           "canopen",         tpt_protocol_canopen::CanOpenParser::new(),      tpt_protocol_canopen::CanOpenNormalizer::new(),      SourceProtocol::Canopen),
    (DoipSomeipPlugin,        "doip-someip",     tpt_protocol_doip_someip::DoipSomeipParser::new(), tpt_protocol_doip_someip::DoipSomeipNormalizer::new(), SourceProtocol::DoipSomeip),
    (NmeaPlugin,              "nmea",            tpt_protocol_nmea::NmeaParser::new(),            tpt_protocol_nmea::NmeaNormalizer::new(),            SourceProtocol::Nmea),
    (MavlinkPlugin,           "mavlink",         tpt_protocol_mavlink::MavlinkParser::new(),     tpt_protocol_mavlink::MavlinkNormalizer::new(),     SourceProtocol::Mavlink),
    (BacnetPlugin,            "bacnet",          tpt_protocol_bacnet::BacnetParser::new(),       tpt_protocol_bacnet::BacnetNormalizer::new(),       SourceProtocol::Bacnet),
    (OpcuaPlugin,             "opcua",           tpt_protocol_opcua::OpcuaParser::new(),         tpt_protocol_opcua::OpcuaNormalizer::new(),         SourceProtocol::Opcua),
    (Dnp3Plugin,              "dnp3",            tpt_protocol_dnp3::Dnp3Parser::new(),            tpt_protocol_dnp3::Dnp3Normalizer::new(),            SourceProtocol::Dnp3),
    (EthercatPlugin,          "ethercat",        tpt_protocol_ethercat::EthercatParser::new(),   tpt_protocol_ethercat::EthercatNormalizer::new(),   SourceProtocol::Ethercat),
    (SparkplugbPlugin,        "sparkplugb",      tpt_protocol_sparkplugb::SparkplugParser,       tpt_protocol_sparkplugb::SparkplugNormalizer,       SourceProtocol::Sparkplugb),
    (OcppPlugin,              "ocpp",            tpt_protocol_ocpp::OcppParser,                  tpt_protocol_ocpp::OcppNormalizer,                  SourceProtocol::Ocpp),
    (Ieee11073SdcPlugin,      "ieee11073-sdc",   tpt_protocol_ieee11073_sdc::SdcParser,          tpt_protocol_ieee11073_sdc::SdcNormalizer,          SourceProtocol::Ieee11073Sdc),
    (Hl7v2Plugin,             "hl7v2",           tpt_protocol_hl7v2::Hl7Parser,                   tpt_protocol_hl7v2::Hl7Normalizer,                   SourceProtocol::Hl7v2),
    (KnxPlugin,               "knx",             tpt_protocol_knx::KnxParser,                    tpt_protocol_knx::KnxNormalizer,                    SourceProtocol::Knx),
    (MatterThreadPlugin,      "matter-thread",   tpt_protocol_matter_thread::MatterParser,       tpt_protocol_matter_thread::MatterNormalizer,       SourceProtocol::MatterThread),
    (CoapPlugin,              "coap",            tpt_protocol_coap::CoapParser,                  tpt_protocol_coap::CoapNormalizer,                  SourceProtocol::Coap),
}
