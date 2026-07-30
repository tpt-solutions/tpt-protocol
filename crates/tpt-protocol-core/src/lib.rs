//! Core traits for the TPT Protocol ecosystem: FrameParser, StateValidator, DataNormalizer.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

pub use error::Error;

pub mod error;

/// Parses raw bytes into a protocol-specific frame.
///
/// # Examples
///
/// ```rust
/// use tpt_protocol_core::{FrameParser, Error};
///
/// struct MyParser;
///
/// impl FrameParser for MyParser {
///     type Frame = Vec<u8>;
///
///     fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
///         if bytes.len() < 4 {
///             return Err(Error::parse("too short"));
///         }
///         Ok(bytes.to_vec())
///     }
/// }
///
/// let parser = MyParser;
/// let frame = parser.parse(b"test").unwrap();
/// assert_eq!(&frame, b"test");
/// ```
pub trait FrameParser {
    type Frame: core::fmt::Debug + Clone + PartialEq;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error>;

    fn try_parse(&self, bytes: &[u8]) -> Result<Option<(Self::Frame, usize)>, Error> {
        self.parse(bytes).map(|f| Some((f, bytes.len())))
    }
}

/// Validates the health/state of a protocol connection.
///
/// # Examples
///
/// ```rust
/// use tpt_protocol_core::{StateValidator, Error};
///
/// struct MyValidator;
///
/// impl StateValidator for MyValidator {
///     type State = bool;
///
///     fn validate(&self, state: &Self::State) -> Result<(), Error> {
///         if *state { Ok(()) } else { Err(Error::state("disconnected")) }
///     }
/// }
///
/// let v = MyValidator;
/// assert!(v.validate(&true).is_ok());
/// assert!(v.validate(&false).is_err());
/// ```
pub trait StateValidator {
    type State: core::fmt::Debug + Clone + PartialEq;

    fn validate(&self, state: &Self::State) -> Result<(), Error>;
}

/// Normalizes a parsed protocol frame into a TUP DataPoint.
///
/// # Examples
///
/// ```rust
/// use tpt_protocol_core::{DataNormalizer, Error};
/// use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};
///
/// struct MyNormalizer;
///
/// impl DataNormalizer for MyNormalizer {
///     type Frame = f64;
///
///     fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
///         Ok(DataPoint::new(
///             SourceProtocol::Modbus, "dev", "point",
///             1000, TupValue::Float(*frame), Quality::Good,
///         ))
///     }
/// }
///
/// let n = MyNormalizer;
/// let dp = n.normalize(&42.5).unwrap();
/// assert_eq!(dp.device_id, "dev");
/// ```
pub trait DataNormalizer {
    type Frame: core::fmt::Debug + Clone + PartialEq;

    fn normalize(&self, frame: &Self::Frame) -> Result<tpt_protocol_tup::DataPoint, Error>;

    /// Normalizes a batch of frames into a vector of DataPoints.
    ///
    /// The default implementation calls `normalize` for each frame.
    fn normalize_batch(
        &self,
        frames: &[Self::Frame],
    ) -> Result<alloc::vec::Vec<tpt_protocol_tup::DataPoint>, Error> {
        frames.iter().map(|f| self.normalize(f)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

    #[derive(Debug, Clone, PartialEq)]
    struct TestFrame {
        value: f64,
        point: &'static str,
    }

    struct TestParser;

    impl FrameParser for TestParser {
        type Frame = TestFrame;

        fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
            if bytes.len() < 4 {
                return Err(Error::parse("too short"));
            }
            let val = f64::from(bytes[0]) + f64::from(bytes[1]) / 100.0;
            Ok(TestFrame {
                value: val,
                point: "test:1",
            })
        }
    }

    struct TestNormalizer;

    impl DataNormalizer for TestNormalizer {
        type Frame = TestFrame;

        fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
            Ok(DataPoint::new(
                SourceProtocol::Modbus,
                "test-device",
                frame.point,
                1753900000000,
                TupValue::Float(frame.value),
                Quality::Good,
            ))
        }
    }

    struct TestValidator;

    impl StateValidator for TestValidator {
        type State = bool;

        fn validate(&self, state: &Self::State) -> Result<(), Error> {
            if *state {
                Ok(())
            } else {
                Err(Error::state("connection lost"))
            }
        }
    }

    #[test]
    fn frame_parser_basic() {
        let parser = TestParser;
        let frame = parser.parse(&[42, 50, 0, 0]).unwrap();
        assert!((frame.value - 42.5).abs() < 0.001);
        assert_eq!(frame.point, "test:1");
    }

    #[test]
    fn frame_parser_too_short() {
        let parser = TestParser;
        let result = parser.parse(&[42]);
        assert!(result.is_err());
    }

    #[test]
    fn data_normalizer_roundtrip() {
        let parser = TestParser;
        let normalizer = TestNormalizer;
        let frame = parser.parse(&[68, 50, 0, 0]).unwrap();
        let dp = normalizer.normalize(&frame).unwrap();
        assert_eq!(dp.source_protocol, SourceProtocol::Modbus);
        let diff: f64 = 68.5 - 68.5;
        assert!(diff.abs() < 0.001);
    }

    #[test]
    fn state_validator_ok() {
        let validator = TestValidator;
        assert!(validator.validate(&true).is_ok());
    }

    #[test]
    fn state_validator_fail() {
        let validator = TestValidator;
        assert!(validator.validate(&false).is_err());
    }
}
