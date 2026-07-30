use crate::{DataPoint, Quality, SourceProtocol, TupValue};
use alloc::format;
use alloc::string::{String, ToString};
use serde::{Deserialize, Serialize};
use serde_json;

#[derive(Serialize, Deserialize)]
struct JsonDataPoint {
    schema_version: u64,
    source_protocol: String,
    device_id: String,
    point_id: String,
    timestamp: u64,
    value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit: Option<String>,
    quality: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw: Option<String>,
}

fn tup_value_to_json_value(v: &TupValue) -> serde_json::Value {
    match v {
        TupValue::Int(n) => serde_json::Value::Number((*n).into()),
        TupValue::Float(n) => serde_json::Number::from_f64(*n)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        TupValue::Bool(b) => serde_json::Value::Bool(*b),
        TupValue::Text(s) => serde_json::Value::String(s.clone()),
        TupValue::Bytes(b) => serde_json::Value::String(format!("0x{}", hex::encode(b))),
    }
}

fn json_value_to_tup_value(v: &serde_json::Value) -> Option<TupValue> {
    match v {
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(TupValue::Int)
            .or_else(|| n.as_f64().map(TupValue::Float)),
        serde_json::Value::Bool(b) => Some(TupValue::Bool(*b)),
        serde_json::Value::String(s) => Some(TupValue::Text(s.clone())),
        _ => None,
    }
}

pub fn to_json_string(dp: &DataPoint) -> Result<String, serde_json::Error> {
    let json_dp = JsonDataPoint {
        schema_version: dp.schema_version(),
        source_protocol: dp.source_protocol.as_str().to_string(),
        device_id: dp.device_id.clone(),
        point_id: dp.point_id.clone(),
        timestamp: dp.timestamp,
        value: tup_value_to_json_value(&dp.value),
        unit: dp.unit.clone(),
        quality: dp.quality.as_str().to_string(),
        raw: dp.raw.as_ref().map(|b| format!("0x{}", hex::encode(b))),
    };
    serde_json::to_string(&json_dp)
}

pub fn to_json_string_pretty(dp: &DataPoint) -> Result<String, serde_json::Error> {
    let json_dp = JsonDataPoint {
        schema_version: dp.schema_version(),
        source_protocol: dp.source_protocol.as_str().to_string(),
        device_id: dp.device_id.clone(),
        point_id: dp.point_id.clone(),
        timestamp: dp.timestamp,
        value: tup_value_to_json_value(&dp.value),
        unit: dp.unit.clone(),
        quality: dp.quality.as_str().to_string(),
        raw: dp.raw.as_ref().map(|b| format!("0x{}", hex::encode(b))),
    };
    serde_json::to_string_pretty(&json_dp)
}

pub fn from_json_str(s: &str) -> Result<DataPoint, String> {
    let json_dp: JsonDataPoint = serde_json::from_str(s).map_err(|e| format!("json error: {e}"))?;

    let source_protocol = SourceProtocol::from_str_opt(&json_dp.source_protocol)
        .ok_or_else(|| format!("unknown source protocol: {}", json_dp.source_protocol))?;
    let quality = Quality::from_str_opt(&json_dp.quality)
        .ok_or_else(|| format!("unknown quality: {}", json_dp.quality))?;
    let value = json_value_to_tup_value(&json_dp.value)
        .ok_or_else(|| format!("unsupported value type: {:?}", json_dp.value))?;
    let raw = json_dp
        .raw
        .and_then(|s| s.strip_prefix("0x").and_then(|h| hex::decode(h).ok()));

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
}
