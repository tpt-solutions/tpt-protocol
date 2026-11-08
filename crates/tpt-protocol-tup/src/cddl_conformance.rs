//! Checks the CBOR encoding against `schema/tup-v1.cddl`.
//!
//! This is a hand-written structural checker for the rules in that CDDL file.
//! It is not a general CDDL validator. It walks the bytes with minicbor's
//! untyped decoder, so it does not reuse the typed `Decode` impls under test.
//! The protocol and quality lists are parsed from the CDDL and compared with
//! the Rust enums, so drift between the two is caught too.

use crate::{cbor, DataPoint, Quality, SourceProtocol, TupValue};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use minicbor::data::Type;
use minicbor::{Decoder, Encoder};

const CDDL: &str = include_str!("../../../schema/tup-v1.cddl");

const ALL_PROTOCOLS: [SourceProtocol; 19] = [
    SourceProtocol::Modbus,
    SourceProtocol::J1939,
    SourceProtocol::Isobus,
    SourceProtocol::Uds,
    SourceProtocol::Canopen,
    SourceProtocol::DoipSomeip,
    SourceProtocol::Nmea,
    SourceProtocol::Mavlink,
    SourceProtocol::Bacnet,
    SourceProtocol::Opcua,
    SourceProtocol::Dnp3,
    SourceProtocol::Ethercat,
    SourceProtocol::Sparkplugb,
    SourceProtocol::Ocpp,
    SourceProtocol::Ieee11073Sdc,
    SourceProtocol::Hl7v2,
    SourceProtocol::Knx,
    SourceProtocol::MatterThread,
    SourceProtocol::Coap,
];

const ALL_QUALITIES: [Quality; 4] = [
    Quality::Good,
    Quality::Bad,
    Quality::Stale,
    Quality::Uncertain,
];

const INT_TYPES: [Type; 8] = [
    Type::U8,
    Type::U16,
    Type::U32,
    Type::U64,
    Type::I8,
    Type::I16,
    Type::I32,
    Type::I64,
];

/// Quoted strings inside one CDDL rule. `next` bounds the rule's body.
fn cddl_strings(rule: &str, next: Option<&str>) -> Vec<String> {
    let start = CDDL.find(rule).expect("rule is present in the CDDL");
    let rest = &CDDL[start..];
    let body = match next {
        Some(n) => &rest[..rest.find(n).expect("next rule is present in the CDDL")],
        None => rest,
    };
    body.split('"')
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, s)| s.to_string())
        .collect()
}

fn expect_type(d: &Decoder<'_>, allowed: &[Type], what: &str) -> Result<Type, String> {
    let t = d.datatype().map_err(|e| format!("{what}: {e:?}"))?;
    if allowed.contains(&t) {
        Ok(t)
    } else {
        Err(format!("{what}: expected one of {allowed:?}, found {t:?}"))
    }
}

fn text_in(d: &mut Decoder<'_>, allowed: &[String], what: &str) -> Result<(), String> {
    expect_type(d, &[Type::String], what)?;
    let s = d.str().map_err(|e| format!("{what}: {e:?}"))?;
    if allowed.iter().any(|a| a == s) {
        Ok(())
    } else {
        Err(format!("{what}: '{s}' is not in the CDDL list"))
    }
}

fn check_tup_value(d: &mut Decoder<'_>) -> Result<(), String> {
    expect_type(d, &[Type::Array], "tup-value")?;
    let len = d
        .array()
        .map_err(|e| format!("tup-value: {e:?}"))?
        .ok_or("tup-value: indefinite arrays are not canonical")?;
    if len != 2 {
        return Err(format!("tup-value: expected 2 elements, found {len}"));
    }
    let tag = d.u64().map_err(|e| format!("tup-value tag: {e:?}"))?;
    match tag {
        0 => {
            expect_type(d, &INT_TYPES, "tup-value int")?;
            d.i64().map_err(|e| format!("tup-value int: {e:?}"))?;
        }
        1 => {
            expect_type(d, &[Type::F64], "tup-value float64")?;
            d.f64().map_err(|e| format!("tup-value float64: {e:?}"))?;
        }
        2 => {
            expect_type(d, &[Type::Bool], "tup-value bool")?;
            d.bool().map_err(|e| format!("tup-value bool: {e:?}"))?;
        }
        3 => {
            expect_type(d, &[Type::String], "tup-value tstr")?;
            d.str().map_err(|e| format!("tup-value tstr: {e:?}"))?;
        }
        4 => {
            expect_type(d, &[Type::Bytes], "tup-value bstr")?;
            d.bytes().map_err(|e| format!("tup-value bstr: {e:?}"))?;
        }
        other => return Err(format!("tup-value: unknown tag {other}")),
    }
    Ok(())
}

/// Validates one encoded `tup-message` against the CDDL rules.
fn check_tup_message(bytes: &[u8]) -> Result<(), String> {
    let protocols = cddl_strings("source-protocol =", Some("quality ="));
    let qualities = cddl_strings("quality =", None);

    let mut d = Decoder::new(bytes);
    expect_type(&d, &[Type::Map], "tup-message")?;
    let len = d
        .map()
        .map_err(|e| format!("tup-message: {e:?}"))?
        .ok_or("tup-message: indefinite maps are not canonical")?;

    // Keys 0..=8 are the only ones the CDDL defines.
    let mut seen = [false; 9];
    for _ in 0..len {
        let key = d.u64().map_err(|e| format!("map key: {e:?}"))?;
        if key > 8 {
            return Err(format!("unknown key {key}"));
        }
        if core::mem::replace(&mut seen[key as usize], true) {
            return Err(format!("duplicate key {key}"));
        }
        match key {
            0 | 4 => {
                expect_type(&d, &[Type::U8, Type::U16, Type::U32, Type::U64], "uint")?;
                d.u64().map_err(|e| format!("key {key}: {e:?}"))?;
            }
            1 => text_in(&mut d, &protocols, "source-protocol")?,
            2 | 3 | 6 => {
                expect_type(&d, &[Type::String], "tstr")?;
                d.str().map_err(|e| format!("key {key}: {e:?}"))?;
            }
            5 => check_tup_value(&mut d)?,
            7 => text_in(&mut d, &qualities, "quality")?,
            8 => {
                expect_type(&d, &[Type::Bytes], "bstr")?;
                d.bytes().map_err(|e| format!("key {key}: {e:?}"))?;
            }
            _ => unreachable!("key was bounded above"),
        }
    }

    for required in [0usize, 1, 2, 3, 4, 5, 7] {
        if !seen[required] {
            return Err(format!("missing required key {required}"));
        }
    }
    if d.position() != bytes.len() {
        return Err("trailing bytes after tup-message".into());
    }
    Ok(())
}

fn encode(dp: &DataPoint) -> Vec<u8> {
    cbor::encode_to_vec(dp).expect("encoding a valid DataPoint succeeds")
}

fn sample(protocol: SourceProtocol, quality: Quality, value: TupValue) -> DataPoint {
    DataPoint::new(
        protocol,
        "dev-1",
        "point-1",
        1_753_900_000_000,
        value,
        quality,
    )
}

#[test]
fn every_protocol_and_quality_conforms() {
    for protocol in ALL_PROTOCOLS {
        for quality in ALL_QUALITIES {
            let dp = sample(protocol, quality, TupValue::Float(68.5));
            check_tup_message(&encode(&dp))
                .unwrap_or_else(|e| panic!("{protocol:?}/{quality:?}: {e}"));
        }
    }
}

#[test]
fn every_value_type_conforms_with_optional_fields() {
    let values = [
        TupValue::Int(-7),
        TupValue::Int(i64::MAX),
        TupValue::Float(-0.25),
        TupValue::Bool(true),
        TupValue::Text("ok".into()),
        TupValue::Bytes(alloc::vec![0x00, 0xff]),
    ];
    for value in values {
        let dp = sample(SourceProtocol::Modbus, Quality::Good, value.clone())
            .with_unit("degF")
            .with_raw(alloc::vec![0x01, 0x02]);
        check_tup_message(&encode(&dp)).unwrap_or_else(|e| panic!("{value:?}: {e}"));
    }
}

#[test]
fn cddl_protocol_list_matches_enum() {
    let mut from_cddl = cddl_strings("source-protocol =", Some("quality ="));
    let mut from_code: Vec<String> = ALL_PROTOCOLS
        .iter()
        .map(|p| p.as_str().to_string())
        .collect();
    from_cddl.sort();
    from_code.sort();
    assert_eq!(from_cddl, from_code);
}

#[test]
fn cddl_quality_list_matches_enum() {
    let mut from_cddl = cddl_strings("quality =", None);
    let mut from_code: Vec<String> = ALL_QUALITIES
        .iter()
        .map(|q| q.as_str().to_string())
        .collect();
    from_cddl.sort();
    from_code.sort();
    assert_eq!(from_cddl, from_code);
}

#[test]
fn rejects_text_keys() {
    // The JSON field names must not leak into the CBOR encoding.
    let mut buf = Vec::new();
    let mut e = Encoder::new(&mut buf);
    e.map(1)
        .unwrap()
        .str("schema_version")
        .unwrap()
        .u64(1)
        .unwrap();
    assert!(check_tup_message(&buf).is_err());
}

#[test]
fn rejects_missing_required_key() {
    let mut buf = Vec::new();
    let mut e = Encoder::new(&mut buf);
    e.map(1).unwrap().u64(0).unwrap().u64(1).unwrap();
    let err = check_tup_message(&buf).unwrap_err();
    assert!(err.contains("missing required key"), "got: {err}");
}

#[test]
fn rejects_unknown_value_tag() {
    let mut buf = Vec::new();
    let mut e = Encoder::new(&mut buf);
    e.array(2).unwrap().u64(9).unwrap().u64(1).unwrap();
    assert!(check_tup_value(&mut Decoder::new(&buf)).is_err());
}

#[test]
fn rejects_unknown_key_and_trailing_bytes() {
    let mut buf = Vec::new();
    let mut e = Encoder::new(&mut buf);
    e.map(1).unwrap().u64(9).unwrap().u64(1).unwrap();
    assert!(check_tup_message(&buf).is_err());

    let mut valid = encode(&sample(
        SourceProtocol::Modbus,
        Quality::Good,
        TupValue::Int(1),
    ));
    valid.push(0x00);
    let err = check_tup_message(&valid).unwrap_err();
    assert!(err.contains("trailing"), "got: {err}");
}
