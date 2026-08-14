# tpt-protocol-matter-thread

Matter / Thread smart-home connectivity protocol parser for the
[TPT Protocol](https://github.com/tpt-protocol/tpt-protocol) suite.

## Overview

Matter (CSA) and its Thread networking layer are the dominant smart-home
interoperability standards. TPT provides a memory-safe `#![no_std]` + `alloc`
parser for a compact Matter/Thread command frame so device clusters and
interactions can be decoded, connection health validated, and normalised into
the shared TUP telemetry envelope — without a `std`/async stack.

## Features

- Binary Matter/Thread frame parsing via `FrameParser` (protocol layer, cluster
  id, command id, endpoint, data version, interaction type, TLV payload).
- Cluster id decoding for OnOff (0x0006), LevelControl (0x0008),
  TemperatureMeasurement (0x0402), OccupancySensing (0x0406), plus a catch-all
  `ClusterId::Other(u16)`.
- Interaction type decoding: Read (0), Write (1), Invoke (2), Subscribe (3).
- A simplified TLV value decoder supporting Bool / Int / Float / Text / Bytes / Null.
- Connection health validation via the `StateValidator` trait.
- TUP normalisation via the `DataNormalizer` trait, mapping frames to `tup::DataPoint`s.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.

## Architecture

`MatterParser` decodes a fixed binary layout defined in `MatterFrame`:

```
[0]      protocol layer       (0 = Matter, 1 = Thread)  -> ProtocolLayer
[1..3]   cluster id           (big-endian u16)          -> ClusterId
[3..5]   command id           (big-endian u16)
[5..7]   endpoint             (big-endian u16)
[7..11]  data version         (big-endian u32)
[11]     interaction type      (0..3)                   -> InteractionType
[12..]   TLV-like data        (tag byte + typed value)  -> MatterData
```

The TLV tag byte selects the value encoding: `0` Bool (1 byte), `1` Int
(8-byte big-endian `i64`), `2` Float (8-byte big-endian `f64`), `3` Text
(2-byte big-endian length + UTF-8), `4` Bytes (2-byte big-endian length +
octets), `5` Null. Out-of-range tag or truncation returns `Error::Parse`.

`MatterValidator` validates a `MatterConnectionState` (connected flag,
`fabric_id`, `node_id`). `MatterNormalizer` emits a `DataPoint` with
`device_id = "matter/<Cluster>/ep<endpoint>"` and `point_id = "cmd:<command_id>"`,
mapping `MatterData` onto the corresponding `TupValue`.

## Usage

```rust
use tpt_protocol_core::{FrameParser, DataNormalizer};
use tpt_protocol_matter_thread::{MatterParser, MatterNormalizer};

fn main() {
    // Matter / OnOff cluster / Read interaction / Bool TLV = true
    let mut raw: Vec<u8> = vec![
        0,                       // protocol: Matter
        0x00, 0x06,             // cluster: OnOff
        0x00, 0x00,             // command id
        0x00, 0x01,             // endpoint 1
        0, 0, 0, 1,             // data version
        0,                      // interaction: Read
        0,                      // TLV type: Bool
        1,                      // value: true
    ];

    let parser = MatterParser;
    let frame = parser.parse(&raw).unwrap();
    assert_eq!(frame.cluster_id, tpt_protocol_matter_thread::ClusterId::OnOff);

    let normalizer = MatterNormalizer;
    let dp = normalizer.normalize(&frame).unwrap();
    assert_eq!(dp.source_protocol, tpt_protocol_tup::SourceProtocol::MatterThread);
    assert_eq!(dp.value, tpt_protocol_tup::TupValue::Bool(true));
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` (for `String`/`Vec` and the
`format!` device-id builder). It works on alloc-capable embedded targets.

## Crate layout

- `src/lib.rs` — `ProtocolLayer`, `InteractionType`, `ClusterId`, `MatterData`,
  `MatterFrame`, `MatterParser` (binary + TLV decoder), `MatterValidator` /
  `MatterConnectionState`, `MatterNormalizer`, unit/fuzz tests.

## Status / Conformance

- **Implemented:** parsing of the simplified Matter/Thread binary frame above,
  the four well-known clusters plus a catch-all, the four interaction types, the
  six TLV value kinds, state validation, TUP normalisation, and `proptest` fuzzing.
- **Known gaps / TODOs:**
  - This is a *simplified* frame model for TPT's telemetry pipeline, **not** the
    full Matter wire protocol: it does not implement real Matter TLV/CBOR tag
    structures, the ZCL attribute/command encoding, Thread mesh/security headers,
    or group/session addressing.
  - Normalised `timestamp` is `0`; callers must supply a real timestamp.
  - `MatterData::Null` normalises to `TupValue::Int(0)`; downstream consumers
    cannot distinguish a real `0` from a null.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
