use crate::{DataPoint, Quality, SourceProtocol, TupValue};
use alloc::string::ToString;
use alloc::vec::Vec;
use core::convert::Infallible;
use minicbor::{decode, encode, Decode, Decoder, Encode, Encoder};

// --- CBOR encode keys ---

const KEY_SCHEMA_VERSION: u64 = 0;
const KEY_SOURCE_PROTOCOL: u64 = 1;
const KEY_DEVICE_ID: u64 = 2;
const KEY_POINT_ID: u64 = 3;
const KEY_TIMESTAMP: u64 = 4;
const KEY_VALUE: u64 = 5;
const KEY_UNIT: u64 = 6;
const KEY_QUALITY: u64 = 7;
const KEY_RAW: u64 = 8;

const VALUE_TYPE_INT: u64 = 0;
const VALUE_TYPE_FLOAT: u64 = 1;
const VALUE_TYPE_BOOL: u64 = 2;
const VALUE_TYPE_TEXT: u64 = 3;
const VALUE_TYPE_BYTES: u64 = 4;

impl Encode<()> for SourceProtocol {
    fn encode<W: encode::Write>(
        &self,
        e: &mut Encoder<W>,
        _ctx: &mut (),
    ) -> Result<(), encode::Error<W::Error>> {
        e.str(self.as_str())?;
        Ok(())
    }
}

impl Decode<'_, ()> for SourceProtocol {
    fn decode(d: &mut Decoder<'_>, _ctx: &mut ()) -> Result<Self, decode::Error> {
        let s = d.str()?;
        Self::from_str_opt(s).ok_or_else(|| decode::Error::message("unknown source protocol"))
    }
}

impl Encode<()> for Quality {
    fn encode<W: encode::Write>(
        &self,
        e: &mut Encoder<W>,
        _ctx: &mut (),
    ) -> Result<(), encode::Error<W::Error>> {
        e.str(self.as_str())?;
        Ok(())
    }
}

impl Decode<'_, ()> for Quality {
    fn decode(d: &mut Decoder<'_>, _ctx: &mut ()) -> Result<Self, decode::Error> {
        let s = d.str()?;
        Self::from_str_opt(s).ok_or_else(|| decode::Error::message("unknown quality"))
    }
}

impl Encode<()> for TupValue {
    fn encode<W: encode::Write>(
        &self,
        e: &mut Encoder<W>,
        _ctx: &mut (),
    ) -> Result<(), encode::Error<W::Error>> {
        e.array(2)?;
        match self {
            Self::Int(v) => {
                e.u64(VALUE_TYPE_INT)?;
                e.i64(*v)?;
            }
            Self::Float(v) => {
                e.u64(VALUE_TYPE_FLOAT)?;
                e.f64(*v)?;
            }
            Self::Bool(v) => {
                e.u64(VALUE_TYPE_BOOL)?;
                e.bool(*v)?;
            }
            Self::Text(v) => {
                e.u64(VALUE_TYPE_TEXT)?;
                e.str(v)?;
            }
            Self::Bytes(v) => {
                e.u64(VALUE_TYPE_BYTES)?;
                e.bytes(v)?;
            }
        }
        Ok(())
    }
}

impl Decode<'_, ()> for TupValue {
    fn decode(d: &mut Decoder<'_>, _ctx: &mut ()) -> Result<Self, decode::Error> {
        let len = d
            .array()?
            .ok_or_else(|| decode::Error::message("tup-value: indefinite-length array"))?;
        if len != 2 {
            return Err(decode::Error::message("tup-value: expected 2 elements"));
        }
        let type_tag = d.u64()?;
        match type_tag {
            VALUE_TYPE_INT => {
                let v = d.i64()?;
                Ok(Self::Int(v))
            }
            VALUE_TYPE_FLOAT => {
                let v = d.f64()?;
                Ok(Self::Float(v))
            }
            VALUE_TYPE_BOOL => {
                let v = d.bool()?;
                Ok(Self::Bool(v))
            }
            VALUE_TYPE_TEXT => {
                let v = d.str()?.to_string();
                Ok(Self::Text(v))
            }
            VALUE_TYPE_BYTES => {
                let v = d.bytes()?.to_vec();
                Ok(Self::Bytes(v))
            }
            _ => Err(decode::Error::message("unknown tup value type")),
        }
    }
}

impl Encode<()> for DataPoint {
    fn encode<W: encode::Write>(
        &self,
        e: &mut Encoder<W>,
        _ctx: &mut (),
    ) -> Result<(), encode::Error<W::Error>> {
        let map_len = 7u64 + u64::from(self.unit.is_some()) + u64::from(self.raw.is_some());
        e.map(map_len)?;

        e.u64(KEY_SCHEMA_VERSION)?.u64(self.schema_version())?;
        e.u64(KEY_SOURCE_PROTOCOL)?;
        self.source_protocol.encode(e, &mut ())?;
        e.u64(KEY_DEVICE_ID)?.str(&self.device_id)?;
        e.u64(KEY_POINT_ID)?.str(&self.point_id)?;
        e.u64(KEY_TIMESTAMP)?.u64(self.timestamp)?;
        e.u64(KEY_VALUE)?;
        self.value.encode(e, &mut ())?;
        e.u64(KEY_QUALITY)?;
        self.quality.encode(e, &mut ())?;

        if let Some(ref unit) = self.unit {
            e.u64(KEY_UNIT)?.str(unit)?;
        }
        if let Some(ref raw) = self.raw {
            e.u64(KEY_RAW)?.bytes(raw)?;
        }

        Ok(())
    }
}

impl Decode<'_, ()> for DataPoint {
    fn decode(d: &mut Decoder<'_>, _ctx: &mut ()) -> Result<Self, decode::Error> {
        let len = d
            .map()?
            .ok_or_else(|| decode::Error::message("indefinite-length map is not canonical"))?;
        // Keys 0..=8 may each appear once; unknown keys are skipped.
        let mut seen = [false; 9];

        let mut schema_version = None;
        let mut source_protocol = None;
        let mut device_id = None;
        let mut point_id = None;
        let mut timestamp = None;
        let mut value = None;
        let mut unit = None;
        let mut quality = None;
        let mut raw = None;

        for _ in 0..len {
            let key = d.u64()?;
            // Checked conversion: `as usize` could truncate on 32-bit targets.
            if let Some(slot) = usize::try_from(key).ok().and_then(|k| seen.get_mut(k)) {
                if core::mem::replace(slot, true) {
                    return Err(decode::Error::message("duplicate key in tup-message"));
                }
            }
            match key {
                k if k == KEY_SCHEMA_VERSION => {
                    schema_version = Some(d.u64()?);
                }
                k if k == KEY_SOURCE_PROTOCOL => {
                    source_protocol = Some(SourceProtocol::decode(d, &mut ())?);
                }
                k if k == KEY_DEVICE_ID => {
                    device_id = Some(d.str()?.to_string());
                }
                k if k == KEY_POINT_ID => {
                    point_id = Some(d.str()?.to_string());
                }
                k if k == KEY_TIMESTAMP => {
                    timestamp = Some(d.u64()?);
                }
                k if k == KEY_VALUE => {
                    value = Some(TupValue::decode(d, &mut ())?);
                }
                k if k == KEY_UNIT => {
                    unit = Some(d.str()?.to_string());
                }
                k if k == KEY_QUALITY => {
                    quality = Some(Quality::decode(d, &mut ())?);
                }
                k if k == KEY_RAW => {
                    raw = Some(d.bytes()?.to_vec());
                }
                _ => {
                    d.skip()?;
                }
            }
        }

        let schema_version = schema_version.unwrap_or(1);
        if schema_version == 0 {
            return Err(decode::Error::message("schema_version 0 is invalid"));
        }
        if schema_version > crate::SCHEMA_VERSION {
            return Err(decode::Error::message("unsupported schema_version"));
        }

        Ok(Self {
            source_protocol: source_protocol
                .ok_or_else(|| decode::Error::message("missing source_protocol"))?,
            device_id: device_id.ok_or_else(|| decode::Error::message("missing device_id"))?,
            point_id: point_id.ok_or_else(|| decode::Error::message("missing point_id"))?,
            timestamp: timestamp.ok_or_else(|| decode::Error::message("missing timestamp"))?,
            value: value.ok_or_else(|| decode::Error::message("missing value"))?,
            unit,
            quality: quality.ok_or_else(|| decode::Error::message("missing quality"))?,
            raw,
        })
    }
}

/// Encodes `dp` as canonical CBOR. Writing to a `Vec` cannot fail, so the
/// error type is `Infallible` in practice; the encoder's own error is kept.
pub fn encode_to_vec(dp: &DataPoint) -> Result<Vec<u8>, encode::Error<Infallible>> {
    let mut buf = Vec::new();
    {
        let mut encoder = Encoder::new(&mut buf);
        dp.encode(&mut encoder, &mut ())?;
    }
    Ok(buf)
}

pub fn decode_from_bytes(bytes: &[u8]) -> Result<DataPoint, decode::Error> {
    let mut decoder = Decoder::new(bytes);
    DataPoint::decode(&mut decoder, &mut ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TupValue;
    use alloc::vec;

    fn sample_datapoint() -> DataPoint {
        DataPoint::new(
            SourceProtocol::Modbus,
            "modbus-tcp://10.0.4.12:502/unit-1",
            "holding-register:40001",
            1753900000000,
            TupValue::Float(68.5),
            Quality::Good,
        )
        .with_unit("degF")
    }

    fn sample_datapoint_int() -> DataPoint {
        DataPoint::new(
            SourceProtocol::J1939,
            "j1939://bus-0/addr:33",
            "pgn:65262-spn:100",
            1753900000001,
            TupValue::Int(1500),
            Quality::Good,
        )
        .with_unit("rpm")
    }

    fn sample_datapoint_text() -> DataPoint {
        DataPoint::new(
            SourceProtocol::Hl7v2,
            "hl7://hospital-a/ward-3",
            "msh-10:message-id",
            1753900000002,
            TupValue::from("ACK^Z99^ACK"),
            Quality::Good,
        )
    }

    fn sample_datapoint_bool() -> DataPoint {
        DataPoint::new(
            SourceProtocol::Bacnet,
            "bacnet://building-1/device:42",
            "analog-input:1",
            1753900000003,
            TupValue::Bool(true),
            Quality::Good,
        )
    }

    fn datapoint_with_raw() -> DataPoint {
        DataPoint::new(
            SourceProtocol::Modbus,
            "modbus-tcp://10.0.4.12:502/unit-1",
            "holding-register:40001",
            1753900000000,
            TupValue::Float(68.5),
            Quality::Good,
        )
        .with_unit("degF")
        .with_raw(vec![0x01, 0x03, 0x02, 0x00, 0x45])
    }

    #[test]
    fn roundtrip_float() {
        let dp = sample_datapoint();
        let encoded = encode_to_vec(&dp).unwrap();
        let decoded = decode_from_bytes(&encoded).unwrap();
        assert_eq!(dp, decoded);
    }

    #[test]
    fn roundtrip_int() {
        let dp = sample_datapoint_int();
        let encoded = encode_to_vec(&dp).unwrap();
        let decoded = decode_from_bytes(&encoded).unwrap();
        assert_eq!(dp, decoded);
    }

    #[test]
    fn roundtrip_text() {
        let dp = sample_datapoint_text();
        let encoded = encode_to_vec(&dp).unwrap();
        let decoded = decode_from_bytes(&encoded).unwrap();
        assert_eq!(dp, decoded);
    }

    #[test]
    fn roundtrip_bool() {
        let dp = sample_datapoint_bool();
        let encoded = encode_to_vec(&dp).unwrap();
        let decoded = decode_from_bytes(&encoded).unwrap();
        assert_eq!(dp, decoded);
    }

    #[test]
    fn roundtrip_with_raw() {
        let dp = datapoint_with_raw();
        let encoded = encode_to_vec(&dp).unwrap();
        let decoded = decode_from_bytes(&encoded).unwrap();
        assert_eq!(dp, decoded);
    }

    #[test]
    fn roundtrip_all_qualities() {
        for quality in &[
            Quality::Good,
            Quality::Bad,
            Quality::Stale,
            Quality::Uncertain,
        ] {
            let dp = DataPoint {
                quality: *quality,
                ..sample_datapoint()
            };
            let encoded = encode_to_vec(&dp).unwrap();
            let decoded = decode_from_bytes(&encoded).unwrap();
            assert_eq!(dp, decoded);
        }
    }

    #[test]
    fn reject_unknown_schema_version() {
        let mut buf = Vec::new();
        {
            let mut e = Encoder::new(&mut buf);
            e.map(1).unwrap();
            e.u64(0).unwrap();
            e.u64(99).unwrap();
        }
        let result = decode_from_bytes(&buf);
        assert!(result.is_err());
    }

    #[test]
    fn reject_truncated() {
        let buf = vec![0xbf, 0x00, 0x01];
        let result = decode_from_bytes(&buf);
        assert!(result.is_err());
    }

    #[test]
    fn reject_garbage() {
        let buf = vec![0xff, 0xfe, 0xfd, 0xfc];
        let result = decode_from_bytes(&buf);
        assert!(result.is_err());
    }

    /// Encodes a full message whose `value` (key 5) is written by `value`.
    fn message_with_value(value: impl Fn(&mut Encoder<&mut Vec<u8>>)) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut e = Encoder::new(&mut buf);
            e.map(7).unwrap();
            e.u64(0).unwrap().u64(1).unwrap();
            e.u64(1).unwrap().str("modbus").unwrap();
            e.u64(2).unwrap().str("d").unwrap();
            e.u64(3).unwrap().str("p").unwrap();
            e.u64(4).unwrap().u64(1).unwrap();
            e.u64(5).unwrap();
            value(&mut e);
            e.u64(7).unwrap().str("good").unwrap();
        }
        buf
    }

    #[test]
    fn manual_message_with_valid_value_decodes() {
        let buf = message_with_value(|e| {
            e.array(2).unwrap().u64(0).unwrap().i64(7).unwrap();
        });
        assert!(decode_from_bytes(&buf).is_ok());
    }

    #[test]
    fn reject_indefinite_length_map() {
        let mut buf = encode_to_vec(&sample_datapoint()).unwrap();
        buf[0] = 0xbf; // indefinite-length map header
        buf.push(0xff); // break
        let err = decode_from_bytes(&buf).unwrap_err().to_string();
        assert!(err.contains("indefinite"), "got: {err}");
    }

    #[test]
    fn reject_indefinite_length_value_array() {
        let buf = message_with_value(|e| {
            e.begin_array()
                .unwrap()
                .u64(0)
                .unwrap()
                .i64(7)
                .unwrap()
                .end()
                .unwrap();
        });
        let err = decode_from_bytes(&buf).unwrap_err().to_string();
        assert!(err.contains("indefinite"), "got: {err}");
    }

    #[test]
    fn reject_extra_value_array_elements() {
        let buf = message_with_value(|e| {
            e.array(3)
                .unwrap()
                .u64(0)
                .unwrap()
                .i64(7)
                .unwrap()
                .u64(0)
                .unwrap();
        });
        let err = decode_from_bytes(&buf).unwrap_err().to_string();
        assert!(err.contains("tup-value"), "got: {err}");
    }

    #[test]
    fn reject_duplicate_key() {
        let mut buf = Vec::new();
        {
            let mut e = Encoder::new(&mut buf);
            e.map(2).unwrap();
            e.u64(0).unwrap().u64(1).unwrap();
            e.u64(0).unwrap().u64(1).unwrap();
        }
        let err = decode_from_bytes(&buf).unwrap_err();
        assert!(err.to_string().contains("duplicate"), "got: {err}");
    }

    #[test]
    fn reject_schema_version_zero() {
        let mut buf = Vec::new();
        {
            let mut e = Encoder::new(&mut buf);
            e.map(1).unwrap().u64(0).unwrap().u64(0).unwrap();
        }
        let err = decode_from_bytes(&buf).unwrap_err().to_string();
        assert!(err.contains("schema_version"), "got: {err}");
    }
}
