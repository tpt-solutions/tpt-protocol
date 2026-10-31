//! Round-trip a TUP `DataPoint` through both the JSON mirror and the CBOR
//! canonical encodings, proving they are isomorphic.
//!
//! Run with: `cargo run -p tpt-protocol-tup --example roundtrip`

use tpt_protocol_tup::{cbor, json, DataPoint, Quality, SourceProtocol, TupValue};

fn main() {
    let dp = DataPoint::new(
        SourceProtocol::Modbus,
        "modbus-tcp://10.0.4.12:502/unit-1".to_string(),
        "holding-register:40001".to_string(),
        1_753_900_000_000,
        TupValue::Float(68.5),
        Quality::Good,
    )
    .with_unit("degF");

    // JSON mirror round-trip.
    let j = json::to_json_string(&dp).expect("encode json");
    println!("JSON:  {j}");
    let from_json = json::from_json_str(&j).expect("decode json");
    assert_eq!(dp, from_json, "JSON round-trip mismatch");

    // CBOR canonical round-trip.
    let c = cbor::encode_to_vec(&dp).expect("encode cbor");
    println!("CBOR:  {} bytes", c.len());
    let from_cbor = cbor::decode_from_bytes(&c).expect("decode cbor");
    assert_eq!(dp, from_cbor, "CBOR round-trip mismatch");

    println!("roundtrip OK");
}
