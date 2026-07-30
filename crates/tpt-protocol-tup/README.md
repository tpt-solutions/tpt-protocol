# tpt-protocol-tup

**TPT Unified Protocol — the canonical normalized schema for all TPT Protocol data.**

This crate defines the TUP message envelope that all 19 protocol crates normalize into. It provides:

- Rust structs mirroring the [TUP CDDL schema](../../SPEC-TUP.md)
- CBOR binary encoding/decoding via `minicbor` (no_std compatible)
- JSON mirror text encoding for daemon/UI use (requires `std`)

## Usage

```rust
use tpt_protocol_tup::{DataPoint, SourceProtocol, Quality, TupValue, cbor};

let dp = DataPoint::new(
    SourceProtocol::Modbus,
    "modbus-tcp://10.0.4.12:502/unit-1",
    "holding-register:40001",
    1753900000000,
    TupValue::Float(68.5),
    Quality::Good,
).with_unit("degF");

let encoded = cbor::encode_to_vec(&dp).unwrap();
let decoded = cbor::decode_from_bytes(&encoded).unwrap();
assert_eq!(dp, decoded);
```

## Features

- `std` (default): enables JSON serialization via `serde_json`
- `alloc`: enables `alloc`-based types for no_std environments with an allocator

## no_std

This crate is `#![no_std]` by default and only requires `alloc`.
