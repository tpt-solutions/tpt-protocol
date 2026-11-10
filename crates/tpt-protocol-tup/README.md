# tpt-protocol-tup

**TPT Unified Protocol (TUP) — the canonical normalized schema for all [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) data.**

This crate defines the TUP message envelope that all 19 protocol crates
normalize into. A `DataPoint` is a single named, timestamped, quality-tagged
observation from one device over one protocol. It carries:

- Rust structs that mirror the TUP CDDL schema documented in
  [`SPEC-TUP.md`](../../SPEC-TUP.md) and `schema/tup-v1.cddl`
- **CBOR** binary encoding/decoding via [`minicbor`](https://docs.rs/minicbor)
  (`#![no_std]` compatible; needs `alloc`)
- A **JSON** mirror text encoding for daemon/UI/WebSocket use (requires the `std` feature)

TUP is deliberately small and self-contained: every protocol parser in the
suite produces `tpt_protocol_tup::DataPoint`, and every UI (CLI, embedded, web)
consumes the same type. CBOR is the canonical wire format; the JSON mirror is
isomorphic and validated against `schema/tup-v1.schema.json` in tests.

## Features

| Feature | Default | Description |
| ------- | ------- | ----------- |
| `std`   | no      | Enables the JSON module (`serde_json`, `hex`) and `minicbor/std`. |
| `alloc` | no      | Enables `minicbor/alloc`. The crate always uses `alloc` (`Vec`, `String`), so an allocator is required. |

- `#![no_std]` with no default features. It always requires `alloc` (a
  `Vec`/`String`/`format!` heap), so it is not a no-alloc crate.
- Const `schema_version()` (currently `1`) on every `DataPoint` for forward
  compatibility; decoders reject a frame whose `schema_version` exceeds the
  compiled maximum.
- Round-trip property tests for every value type and quality flag.
- A `jsonschema` dev-dependency test that validates the `SPEC-TUP.md` worked
  example against `schema/tup-v1.schema.json`.

## Architecture

```
                protocol crate (e.g. tpt-protocol-modbus)
                          │  DataNormalizer::normalize(...)
                          ▼
                 DataPoint { source_protocol, device_id, point_id,
                             timestamp, value, unit, quality, raw }
                          │
            ┌─────────────┴──────────────┐
            ▼                            ▼
      cbor::encode_to_vec        json::to_json_string(_pretty)
      cbor::decode_from_bytes    json::from_json_str
   (canonical wire format)        (UI / WebSocket mirror)
```

The in-memory model (`lib.rs`) is independent of any codec. Each codec
(`cbor.rs`, `json.rs`) is a thin (de)serializer over the same struct set:

- `cbor.rs` encodes `DataPoint` as a CBOR map with stable integer keys
  (`KEY_SCHEMA_VERSION = 0` … `KEY_RAW = 8`). `TupValue` is encoded as a
  2-element `[type_tag, payload]` array so the wire format stays self-describing.
- `json.rs` (behind `std`) maps `DataPoint` to a `serde_json` document with
  string `source_protocol`/`quality` fields. The optional `raw` field is a
  `0x`-prefixed hex string. A `Bytes` value is `{"bytes": "0x…"}`, and a
  non-finite float is `{"float": "NaN" | "Infinity" | "-Infinity"}`, because
  JSON has no bytes type and no NaN. A plain JSON string is always `Text`, so
  every value type survives the round trip. Invalid hex and unsupported
  `schema_version` values are errors. Known limit: the JSON parser reads integer
  literals above the u64 range as floats, so use CBOR for those.

### `DataPoint` model

```rust
pub struct DataPoint {
    pub source_protocol: SourceProtocol, // which of the 19 protocols produced it
    pub device_id: String,               // e.g. "modbus-tcp://10.0.4.12:502/unit-1"
    pub point_id: String,                // e.g. "holding-register:40001"
    pub timestamp: u64,                  // milliseconds since the Unix epoch
    pub value: TupValue,                 // the observation
    pub unit: Option<String>,            // e.g. Some("degF")
    pub quality: Quality,                // Good | Bad | Stale | Uncertain
    pub raw: Option<Vec<u8>>,            // original wire bytes, if retained
}
```

Construct it with `DataPoint::new(...)` and chain the builder helpers
`with_unit`, `with_raw`, `with_timestamp`, `with_quality`.

### TUP value types (`TupValue`)

```rust
pub enum TupValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Text(String),
    Bytes(Vec<u8>),
}
```

All five variants implement `From<…>` (`From<i64>`, `From<f64>`, `From<bool>`,
`From<String>`, `From<&str>`, `From<Vec<u8>>`, `From<&[u8]>`) so a `DataPoint`
can be built tersely: `TupValue::from(42)` or `TupValue::Float(68.5)`.

### Quality flags (`Quality`)

```rust
pub enum Quality {
    Good,      // "good"
    Bad,       // "bad"
    Stale,     // "stale"
    Uncertain, // "uncertain"
}
```

Each variant has an `as_str()` accessor and a `FromStr` / `from_str_opt`
parser; the string forms are exactly what the wire/JSON encodings emit.

### Source protocol (`SourceProtocol`)

`SourceProtocol` enumerates all 19 supported protocols (`Modbus` … `Coap`,
including the private `Mavlink`/`Ethercat`) with `as_str()` and `FromStr`
round-tripping the kebab-case identifiers used on the wire
(`"modbus"`, `"uds"`, `"matter-thread"`, …).

## Usage

Build, encode, and decode a `DataPoint` on the CBOR path:

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

The same `DataPoint` through the JSON mirror (requires the `std` feature):

```rust
use tpt_protocol_tup::{json, TupValue};

let json = json::to_json_string(&dp).unwrap();
let parsed = json::from_json_str(&json).unwrap();
assert_eq!(dp, parsed);
```

A runnable end-to-end round-trip example lives at
[`examples/roundtrip.rs`](examples/roundtrip.rs):

```bash
cargo run -p tpt-protocol-tup --example roundtrip
```

## `no_std`

This crate is `#![no_std]` and always requires `alloc`. With the `std` feature
you also get the JSON mirror. Without it, the crate builds with only the CBOR
codec and no serde_json or hex dependency:

```bash
cargo build -p tpt-protocol-tup --no-default-features --features alloc
```

## Crate layout

```
tpt-protocol-tup/
├── Cargo.toml
├── README.md
├── examples/
│   └── roundtrip.rs        # CBOR + JSON isomorphic round-trip demo
└── src/
    ├── lib.rs              # DataPoint model, schema_version, re-exports
    ├── value.rs            # TupValue enum + From impls + Display
    ├── quality.rs          # Quality enum + str round-trip
    ├── source_protocol.rs  # SourceProtocol enum + str round-trip (19 protocols)
    ├── cbor.rs             # minicbor Encode/Decode + encode_to_vec/decode_from_bytes
    └── json.rs             # (std) serde_json mirror: to_json_string(_pretty)/from_json_str
```

## Status / Conformance

- Schema version **1** (`SCHEMA_VERSION` in `lib.rs`).
- A unit test (`schema_tests::worked_example_matches_json_schema`) asserts the
  `SPEC-TUP.md` §4 worked example validates against
  `schema/tup-v1.schema.json`.
- CBOR codec rejects unknown `source_protocol`/`quality` strings, unknown
  `TupValue` type tags, schema versions greater than the compiled maximum, and
  truncated/garbage input (see `cbor.rs` tests `reject_*`).

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `DataNormalizer` /
  `FrameParser` / `StateValidator` traits every protocol crate implements to
  produce `DataPoint`s.
- The 19 protocol crates (`tpt-protocol-modbus`, `tpt-protocol-uds`, …) which
  emit TUP `DataPoint`s.
- The consumer UIs — [`tpt-protocol-cli`](../tpt-protocol-cli),
  [`tpt-protocol-ui-embedded`](../tpt-protocol-ui-embedded),
  [`tpt-protocol-ui-web`](../tpt-protocol-ui-web) — which decode TUP over the
  daemon's local WebSocket.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or
[Apache-2.0](../../LICENSE-APACHE) at your option.
