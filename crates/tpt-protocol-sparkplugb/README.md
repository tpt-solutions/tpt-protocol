# tpt-protocol-sparkplugb

Sparkplug B MQTT-based IIoT protocol parser for the
[TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

## Overview

Sparkplug B is the payload specification (over MQTT) that gives industrial IoT
devices a standard, self-describing way to report state to SCADA/Historian
systems. TPT provides a memory-safe `#![no_std]` + `alloc` parser for the
Sparkplug B metric payload so device metrics can be decoded, connection health
validated, and normalised into the shared TUP telemetry envelope.

## Features

- Sparkplug B metric-payload parsing via `FrameParser` (version, message type,
  sequence, timestamp, metrics).
- Message-type decoding: NBIRTH, DBIRTH, NCMD, DCMD, NDATA, DDATA, NDEATH, DDEATH.
- A metric TLV decoder supporting Int32, Int64, Float, Double, Boolean, and
  String value types.
- Connection health validation via the `StateValidator` trait.
- TUP normalisation via the `DataNormalizer` trait, mapping frames to `tup::DataPoint`s.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.

## Architecture

`SparkplugParser` decodes a fixed binary framing (a compact, protobuf-like layout
specific to this TPT crate) with a 14-byte header followed by a list of metrics:

```
[0]      version            (u8)
[1]      message type       (0..7)              -> SparkplugMessageType
[2..4]   seq                (big-endian u16, must be 0..=255)
[4..12]  timestamp          (big-endian u64)
[12..14] metric count       (big-endian u16)
[14..]   metrics:  name_len (u8) + name (UTF-8)
                   + data_type (u8) + value (typed)
```

Each `Metric` carries a `name`, `data_type`, and `TupValue`. The value encoding
is selected by `data_type`: `0` Int32 (4 bytes), `1` Int64 (8 bytes), `2` Float
(4-byte `f32`), `3` Double (8-byte `f64`), `4` Boolean (1 byte), `5` String
(2-byte length + UTF-8). Unknown data types return `Error::Parse`. The metric
count is capped at `MAX_SPARKPLUG_METRICS = 1024` to prevent unbounded allocation.

`SparkplugFrame` holds `version`, `message_type`, `seq`, `timestamp`, and
`metrics`. `SparkplugValidator` validates a `SparkplugConnectionState` (connected
flag, `last_seq` ≤ 255, `birth_received`). `SparkplugNormalizer` emits a
`DataPoint` with `device_id = "sparkplug/<message_type>"`, the first metric's
name (or `seq:<n>` if none) as `point_id`, and the first metric's value (or the
sequence number) as the value, carrying the frame `timestamp` through.

## Usage

```rust
use tpt_protocol_core::{FrameParser, DataNormalizer};
use tpt_protocol_sparkplugb::{SparkplugParser, SparkplugNormalizer};

fn main() {
    // version=1, NBIRTH, seq=0, timestamp=1000000, 1 metric
    //   name "temp" (len 4), data_type=3 (Double), value 2.5
    let mut raw: Vec<u8> = vec![1, 0];
    raw.extend_from_slice(&0u16.to_be_bytes());        // seq
    raw.extend_from_slice(&1000000u64.to_be_bytes());  // timestamp
    raw.extend_from_slice(&1u16.to_be_bytes());        // metric count
    raw.push(4);                                       // name len
    raw.extend_from_slice(b"temp");                    // name
    raw.push(3);                                       // data_type = Double
    raw.extend_from_slice(&2.5f64.to_bits().to_be_bytes()); // value

    let parser = SparkplugParser;
    let frame = parser.parse(&raw).unwrap();
    assert_eq!(frame.message_type, tpt_protocol_sparkplugb::SparkplugMessageType::NBIRTH);
    assert_eq!(frame.metrics[0].name, "temp");

    let normalizer = SparkplugNormalizer;
    let dp = normalizer.normalize(&frame).unwrap();
    assert_eq!(dp.source_protocol, tpt_protocol_tup::SourceProtocol::Sparkplugb);
    assert_eq!(dp.point_id, "temp");
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for `String`/`Vec` (metric names,
values, device-id formatting). It works on alloc-capable embedded targets.

## Crate layout

- `src/lib.rs` — `SparkplugMessageType`, `Metric`, `SparkplugFrame`,
  `SparkplugParser` (binary header + metric TLV decoder), `SparkplugValidator` /
  `SparkplugConnectionState`, `SparkplugNormalizer`, unit/fuzz tests.

## Status / Conformance

- **Implemented:** parsing of the Sparkplug B binary framing (header + metric
  TLV) for all eight message types, six metric value types (Int32/Int64/Float/
  Double/Boolean/String), sequence-range and timestamp handling, connection state
  validation, TUP normalisation (preserving the frame timestamp), `proptest`
  fuzzing, and a capped metric count to prevent unbounded allocation.
- **Known gaps / TODOs:**
  - This is a *simplified* framing for TPT's telemetry pipeline, **not** the
    official Sparkplug B protobuf payload: it does not decode the real protobuf
    encoding, the MQTT topic namespace (`spBv1.0/...`), BIRTH/DEATH/CMD state
    machine, dataset/property-set types, or aliases. Only scalar metric types
    0–5 are supported; others return an error.
  - `SparkplugNormalizer` emits a single `DataPoint` per frame (the first
    metric); multi-metric payloads would require a batch API.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
