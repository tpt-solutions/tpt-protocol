use crate::{DataPoint, Quality, SourceProtocol, TupValue, SCHEMA_VERSION};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};
use serde_json::{self, Value};

#[derive(Serialize, Deserialize)]
struct JsonDataPoint {
    schema_version: u64,
    source_protocol: String,
    device_id: String,
    point_id: String,
    timestamp: u64,
    value: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<String>,
    quality: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw: Option<String>,
}

/// `0x`-prefixed lowercase hex, the text form of `raw` and of byte values.
fn hex_text(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn decode_hex_text(s: &str) -> Result<Vec<u8>, String> {
    let digits = s
        .strip_prefix("0x")
        .ok_or_else(|| format!("expected a 0x-prefixed hex string, got '{s}'"))?;
    hex::decode(digits).map_err(|e| format!("invalid hex string '{s}': {e}"))
}

fn non_finite_name(f: f64) -> &'static str {
    if f.is_nan() {
        "NaN"
    } else if f > 0.0 {
        "Infinity"
    } else {
        "-Infinity"
    }
}

/// Encodes a value for the JSON mirror. JSON has no bytes type and no
/// non-finite numbers, so both are wrapped in a one-key object:
/// `{"bytes": "0x…"}` and `{"float": "NaN" | "Infinity" | "-Infinity"}`.
/// Plain strings always mean text, so the round trip keeps each type.
fn tup_value_to_json_value(v: &TupValue) -> Value {
    match v {
        TupValue::Int(n) => Value::Number((*n).into()),
        TupValue::Float(f) => match serde_json::Number::from_f64(*f) {
            Some(n) => Value::Number(n),
            None => serde_json::json!({ "float": non_finite_name(*f) }),
        },
        TupValue::Bool(b) => Value::Bool(*b),
        TupValue::Text(s) => Value::String(s.clone()),
        TupValue::Bytes(b) => serde_json::json!({ "bytes": hex_text(b) }),
    }
}

fn json_value_to_tup_value(v: &Value) -> Result<TupValue, String> {
    match v {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(TupValue::Int(i))
            } else if n.is_u64() {
                // Never silently re-type an integer as a float.
                Err(format!("integer {n} is outside the i64 range"))
            } else {
                n.as_f64()
                    .map(TupValue::Float)
                    .ok_or_else(|| format!("unsupported number {n}"))
            }
        }
        Value::Bool(b) => Ok(TupValue::Bool(*b)),
        Value::String(s) => Ok(TupValue::Text(s.clone())),
        Value::Object(map) if map.len() == 1 => {
            if let Some(Value::String(h)) = map.get("bytes") {
                return decode_hex_text(h).map(TupValue::Bytes);
            }
            match map.get("float") {
                Some(Value::String(name)) => match name.as_str() {
                    "NaN" => Ok(TupValue::Float(f64::NAN)),
                    "Infinity" => Ok(TupValue::Float(f64::INFINITY)),
                    "-Infinity" => Ok(TupValue::Float(f64::NEG_INFINITY)),
                    other => Err(format!("unknown float literal '{other}'")),
                },
                _ => Err(format!("unsupported value object: {v}")),
            }
        }
        other => Err(format!("unsupported value: {other}")),
    }
}

fn to_json_dp(dp: &DataPoint) -> JsonDataPoint {
    JsonDataPoint {
        schema_version: dp.schema_version(),
        source_protocol: dp.source_protocol.as_str().to_string(),
        device_id: dp.device_id.clone(),
        point_id: dp.point_id.clone(),
        timestamp: dp.timestamp,
        value: tup_value_to_json_value(&dp.value),
        unit: dp.unit.clone(),
        quality: dp.quality.as_str().to_string(),
        raw: dp.raw.as_ref().map(|b| hex_text(b)),
    }
}

pub fn to_json_string(dp: &DataPoint) -> Result<String, serde_json::Error> {
    serde_json::to_string(&to_json_dp(dp))
}

pub fn to_json_string_pretty(dp: &DataPoint) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&to_json_dp(dp))
}

pub fn from_json_str(s: &str) -> Result<DataPoint, String> {
    let json_dp: JsonDataPoint = serde_json::from_str(s).map_err(|e| format!("json error: {e}"))?;

    if json_dp.schema_version == 0 || json_dp.schema_version > SCHEMA_VERSION {
        return Err(format!(
            "unsupported schema_version {}",
            json_dp.schema_version
        ));
    }
    let source_protocol = SourceProtocol::from_str_opt(&json_dp.source_protocol)
        .ok_or_else(|| format!("unknown source protocol: {}", json_dp.source_protocol))?;
    let quality = Quality::from_str_opt(&json_dp.quality)
        .ok_or_else(|| format!("unknown quality: {}", json_dp.quality))?;
    let value = json_value_to_tup_value(&json_dp.value)?;
    let raw = match json_dp.raw {
        Some(text) => Some(decode_hex_text(&text)?),
        None => None,
    };

    Ok(DataPoint {
        source_protocol,
        device_id: json_dp.device_id,
        point_id: json_dp.point_id,
        timestamp: json_dp.timestamp,
        value,
        unit: json_dp.unit,
        quality,
        raw,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TupValue;
    use alloc::vec;

    #[test]
    fn json_roundtrip() {
        let dp = DataPoint::new(
            SourceProtocol::Modbus,
            "modbus-tcp://10.0.4.12:502/unit-1",
            "holding-register:40001",
            1753900000000,
            TupValue::Float(68.5),
            Quality::Good,
        )
        .with_unit("degF");

        let json = to_json_string(&dp).unwrap();
        let parsed = from_json_str(&json).unwrap();
        assert_eq!(dp, parsed);
    }

    #[test]
    fn json_roundtrip_with_raw() {
        let dp = DataPoint::new(
            SourceProtocol::Modbus,
            "modbus-tcp://10.0.4.12:502/unit-1",
            "holding-register:40001",
            1753900000000,
            TupValue::Int(42),
            Quality::Good,
        )
        .with_raw(vec![0x01, 0x02, 0x03]);

        let json = to_json_string(&dp).unwrap();
        let parsed = from_json_str(&json).unwrap();
        assert_eq!(dp, parsed);
    }

    #[test]
    fn json_roundtrip_all_value_types() {
        let cases = vec![
            DataPoint::new(
                SourceProtocol::Modbus,
                "dev/1",
                "int",
                1,
                TupValue::Int(-42),
                Quality::Good,
            ),
            DataPoint::new(
                SourceProtocol::Modbus,
                "dev/1",
                "float",
                1,
                TupValue::Float(2.5f64),
                Quality::Good,
            ),
            DataPoint::new(
                SourceProtocol::Modbus,
                "dev/1",
                "bool",
                1,
                TupValue::Bool(true),
                Quality::Good,
            ),
            DataPoint::new(
                SourceProtocol::Modbus,
                "dev/1",
                "text",
                1,
                TupValue::from("hello"),
                Quality::Good,
            ),
        ];

        for dp in cases {
            let json = to_json_string(&dp).unwrap();
            let parsed = from_json_str(&json).unwrap();
            assert_eq!(dp, parsed);
        }
    }

    #[test]
    fn json_equivalent_to_cbor() {
        use crate::cbor;
        let dp = DataPoint::new(
            SourceProtocol::Modbus,
            "modbus-tcp://10.0.4.12:502/unit-1",
            "holding-register:40001",
            1753900000000,
            TupValue::Float(68.5),
            Quality::Good,
        )
        .with_unit("degF");

        let cbor_bytes = cbor::encode_to_vec(&dp).unwrap();
        let from_cbor = cbor::decode_from_bytes(&cbor_bytes).unwrap();

        let json = to_json_string(&dp).unwrap();
        let from_json = from_json_str(&json).unwrap();

        assert_eq!(from_cbor, from_json);
    }

    fn sample_json(schema_version: &str, value: &str, extra: &str) -> String {
        format!(
            r#"{{"schema_version":{schema_version},"source_protocol":"modbus","device_id":"d","point_id":"p","timestamp":1,"value":{value},"quality":"good"{extra}}}"#
        )
    }

    fn dp_with(value: TupValue) -> DataPoint {
        DataPoint::new(
            SourceProtocol::Modbus,
            "dev/1",
            "pt",
            1,
            value,
            Quality::Good,
        )
    }

    #[test]
    fn bytes_value_keeps_its_type() {
        let dp = dp_with(TupValue::Bytes(vec![0x00, 0xff]));
        let json = to_json_string(&dp).unwrap();
        assert_eq!(from_json_str(&json).unwrap(), dp);
    }

    #[test]
    fn text_that_looks_like_hex_stays_text() {
        let dp = dp_with(TupValue::Text("0x00".into()));
        let json = to_json_string(&dp).unwrap();
        assert_eq!(from_json_str(&json).unwrap(), dp);
    }

    #[test]
    fn non_finite_floats_roundtrip() {
        for f in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let json = to_json_string(&dp_with(TupValue::Float(f))).unwrap();
            match from_json_str(&json).unwrap().value {
                TupValue::Float(g) if f.is_nan() => assert!(g.is_nan()),
                TupValue::Float(g) => assert_eq!(g, f),
                other => panic!("expected Float, got {other:?}"),
            }
        }
    }

    #[test]
    fn integer_beyond_i64_is_rejected_not_turned_into_float() {
        let json = sample_json("1", "9223372036854775808", "");
        assert!(from_json_str(&json).is_err());
    }

    #[test]
    fn invalid_raw_hex_is_rejected() {
        let json = sample_json("1", "1", r#","raw":"0xzz""#);
        assert!(from_json_str(&json).is_err());
    }

    #[test]
    fn schema_version_is_checked() {
        for v in ["0", "2"] {
            let json = sample_json(v, "1", "");
            assert!(from_json_str(&json).is_err(), "schema_version {v} accepted");
        }
        assert!(from_json_str(&sample_json("1", "1", "")).is_ok());
    }

    #[test]
    fn encoded_values_validate_against_json_schema() {
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../../../schema/tup-v1.schema.json")).unwrap();
        let validator = jsonschema::validator_for(&schema).expect("schema compiles");
        let values = [
            TupValue::Int(-3),
            TupValue::Float(2.5),
            TupValue::Float(f64::NAN),
            TupValue::Float(f64::NEG_INFINITY),
            TupValue::Bool(false),
            TupValue::Text("0x00".into()),
            TupValue::Bytes(vec![0x00, 0xff]),
        ];
        for value in values {
            let json = to_json_string(&dp_with(value.clone())).unwrap();
            let instance: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert!(validator.validate(&instance).is_ok(), "{value:?} -> {json}");
        }
    }
}
