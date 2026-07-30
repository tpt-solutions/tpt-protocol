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

pub mod cbor;
