# tpt-protocol-sparkplugb

Sparkplug B (Eclipse Sparkplug, MQTT with Protobuf payloads) parser, validator, and TUP
normalizer for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

## Overview

Sparkplug B defines how industrial edge nodes and devices publish their data over MQTT.
Each message is a Protobuf `Payload` on a topic of the form
`spBv1.0/<group>/<message type>/<edge node>[/<device>]`. TPT implements a bounds-checked
Protobuf wire reader, the Sparkplug `Payload` and `Metric` messages, and the topic rules,
in a `#![no_std]` + `alloc` crate, so captured messages can be decoded and normalized into
the shared TUP envelope.

## Features

- Topic parsing with the segment rules for each message type. Wildcards (`+`, `#`) are
  rejected.
- A bounds-checked Protobuf reader: varints, fixed32 and fixed64 fields, and
  length-delimited fields. Unknown fields are skipped. Groups are rejected.
- `Payload` and `Metric` decoding. Datatypes map to TUP values. Int64 and UInt64 values
  above `i64::MAX` are errors, not truncated values. Null metrics and structured types
  (DataSet, Template) carry no value.
- `SparkplugEncoder`: wire Protobuf and topic from a frame, with round-trip tests.
- Connection-state validation via `StateValidator` (`SparkplugValidator`).
- TUP normalisation via `DataNormalizer` (`SparkplugNormalizer`). `normalize_all` gives one
  data point per metric.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests asserting the parser never panics on arbitrary input. Structured
  generators build valid payloads, then mutate them.

## Architecture

`decode(topic, payload)` reads the topic into a `SparkplugTopic` (group, message type, edge
node and optional device), then decodes the Protobuf payload into a `SparkplugFrame`. The
frame holds the message type (`SparkplugMessageType`), the sequence number and the metrics
(`Metric`).

`SparkplugNormalizer` produces:

- `device_id = "sparkplug:<group>/<edge node>"`, with `/<device>` appended when the topic
  names a device.
- `point_id = <metric name>`.
- `value` from the metric's datatype.

## Usage

A complete, runnable example decodes a DDATA message and normalizes its metrics:

```sh
cargo run -p tpt-protocol-sparkplugb --example parse_sparkplugb
```

```rust
use tpt_protocol_core::DataNormalizer;
use tpt_protocol_sparkplugb::{decode, SparkplugNormalizer};

let frame = decode("spBv1.0/plant/DDATA/edge01/line1", &payload)?;
let points = SparkplugNormalizer.normalize_all(&frame)?;
// points[0].point_id == "Temperature" for a metric named Temperature
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`, `String`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — `SparkplugMessageType`, `SparkplugTopic`, `Metric`, `SparkplugFrame`,
  `SparkplugParser` (`FrameParser`), `SparkplugEncoder` (`FrameEncoder`), `decode`,
  `SparkplugConnectionState` + `SparkplugValidator` (`StateValidator`),
  `SparkplugNormalizer` (`DataNormalizer`), unit tests, and the `proptest` `mod fuzz`.
- `examples/parse_sparkplugb.rs` — runnable decode-and-normalize example.

## Status / Conformance

- **Payloads only.** The MQTT transport is not part of this crate. The caller supplies the
  topic and payload bytes.
- **Templates and DataSets are not decoded.** Their metrics carry no value.
- **Sequence numbers are read, not checked.** Gaps and rebirth requests are not tracked.
- Normalised `timestamp` is `0`; callers must supply a real timestamp.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `FrameEncoder` /
  `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
