//! Show how to add a brand-new protocol to the TPT ecosystem by implementing the
//! three core traits (`FrameParser`, `StateValidator`, `DataNormalizer`) for a
//! hypothetical "SensorX" protocol.
//!
//! Run with: `cargo run -p tpt-protocol-core --example custom_protocol`

use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// The decoded form of a SensorX frame.
#[derive(Debug, Clone, PartialEq)]
struct SensorXFrame {
    sensor_id: u8,
    reading: f32,
}

/// Parses the trivial SensorX wire format: `[0x5X, sensor_id, f32_le...]`.
struct SensorXParser;

impl FrameParser for SensorXParser {
    type Frame = SensorXFrame;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 6 {
            return Err(Error::Parse("sensorx frame too short".into()));
        }
        if bytes[0] != 0x5A {
            return Err(Error::Parse("sensorx bad magic byte".into()));
        }
        let sensor_id = bytes[1];
        let reading = f32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
        Ok(SensorXFrame { sensor_id, reading })
    }
}

/// Validates that the sensor is online (id != 0).
struct SensorXValidator;

impl StateValidator for SensorXValidator {
    type State = u8;

    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if *state == 0 {
            return Err(Error::State("sensorx sensor offline".into()));
        }
        Ok(())
    }
}

/// Normalizes a SensorX frame into a TUP `DataPoint`.
struct SensorXNormalizer;

impl DataNormalizer for SensorXNormalizer {
    type Frame = SensorXFrame;

    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        let device_id = format!("sensorx/{}", frame.sensor_id);
        let point_id = "temperature".to_string();
        let value = TupValue::Float(frame.reading as f64);
        Ok(DataPoint::new(
            SourceProtocol::Modbus, // map onto the closest existing source
            device_id,
            point_id,
            0,
            value,
            Quality::Good,
        )
        .with_unit("degC"))
    }
}

fn main() {
    let parser = SensorXParser;
    let normalizer = SensorXNormalizer;
    let validator = SensorXValidator;

    let frame_bytes = [0x5A, 0x07, 0x00, 0x00, 0x48, 0x42]; // sensor 7, 50.0 degC
    let frame = parser.parse(&frame_bytes).expect("parse");
    println!(
        "parsed sensor {} reading {}",
        frame.sensor_id, frame.reading
    );

    assert!(validator.validate(&frame.sensor_id).is_ok());

    let dp = normalizer.normalize(&frame).expect("normalize");
    println!(
        "normalized: {} {} = {:?} {}",
        dp.device_id,
        dp.point_id,
        dp.value,
        dp.unit.as_deref().unwrap_or("")
    );
    assert_eq!(dp.source_protocol, SourceProtocol::Modbus);
}
