//! TUP (Telemetry Unified Packet) schema: CBOR-based binary encoding with a JSON mirror format.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

pub use quality::Quality;
pub use source_protocol::SourceProtocol;
pub use value::TupValue;

mod quality;
mod source_protocol;
mod value;

const SCHEMA_VERSION: u64 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct DataPoint {
    pub source_protocol: SourceProtocol,
    pub device_id: String,
    pub point_id: String,
    pub timestamp: u64,
    pub value: TupValue,
    pub unit: Option<String>,
    pub quality: Quality,
    pub raw: Option<Vec<u8>>,
}

impl DataPoint {
    pub fn new(
        source_protocol: SourceProtocol,
        device_id: impl Into<String>,
        point_id: impl Into<String>,
        timestamp: u64,
        value: TupValue,
        quality: Quality,
    ) -> Self {
        Self {
            source_protocol,
            device_id: device_id.into(),
            point_id: point_id.into(),
            timestamp,
            value,
            unit: None,
            quality,
            raw: None,
        }
    }

    pub fn with_unit(mut self, unit: impl Into<String>) -> Self {
        self.unit = Some(unit.into());
        self
    }

    pub fn with_raw(mut self, raw: Vec<u8>) -> Self {
        self.raw = Some(raw);
        self
    }

    pub fn with_timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }

    pub fn with_quality(mut self, quality: Quality) -> Self {
        self.quality = quality;
        self
    }

    pub const fn schema_version(&self) -> u64 {
        SCHEMA_VERSION
    }
}

#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub mod json;

#[cfg(feature = "alloc")]
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
pub mod cbor;

#[cfg(test)]
mod schema_tests {
    use super::*;

    /// The worked example from SPEC-TUP.md §4, kept in sync with
    /// `schema/tup-v1.schema.json`.
    const WORKED_EXAMPLE: &str = r#"{
      "schema_version": 1,
      "source_protocol": "modbus",
      "device_id": "modbus-tcp://10.0.4.12:502/unit-1",
      "point_id": "holding-register:40001",
      "timestamp": 1753900000000,
      "value": 68.5,
      "unit": "degF",
      "quality": "good"
    }"#;

    #[test]
    fn worked_example_matches_json_schema() {
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../../../schema/tup-v1.schema.json"))
                .expect("schema file is valid JSON");
        let instance: serde_json::Value =
            serde_json::from_str(WORKED_EXAMPLE).expect("worked example is valid JSON");

        let compiled = jsonschema::JSONSchema::compile(&schema).expect("schema compiles");
        let result = compiled.validate(&instance);
        assert!(
            result.is_ok(),
            "worked example failed schema validation: {:?}",
            result.err().map(|e| e.collect::<Vec<_>>())
        );
    }
}
