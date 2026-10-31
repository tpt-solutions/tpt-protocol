# tpt-protocol-ieee11073-sdc

IEEE 11073 SDC (Service-Oriented Device Connectivity) bedside medical-device
protocol parsing, validation, and TUP normalisation for the
[TPT Protocol](https://github.com/tpt-protocol/tpt-protocol) suite.

IEEE 11073 SDC defines a service-oriented exchange for medical devices (vital-sign
monitors, infusion pumps, etc.) over networked transports. TPT decodes the binary
MDIB/Waveform/Alert/Context messages and normalises metric, waveform, alert, and
context entities into TUP data points.

## Overview

IEEE 11073 SDC is the XML/web-services-based companion to the classic 11073-20601
numeric model, used for interoperable medical device communication in acute care.
TPT implements a compact binary wire encoding of the four SDC message bodies so
bedside-device telemetry (numeric metrics, high-rate waveforms, alerts, and context
associations) can be unified with the rest of the telemetry pipeline without a full
XML/SOAP stack.

## Features

- Binary SDC frame parsing: message type (1 byte) + timestamp (8 bytes) + body.
- Four message-body decoders: `NumericMetrics`, `Waveforms`, `Alerts`, `ContextInfo`.
- Per-entity decoding of metric value (`f64`), waveform samples (`f32` with sample
  rate), alert state (`active`/`priority`), and context key/value pairs.
- Bounded allocation: wire-controlled element/sample counts are capped at
  `MAX_SDC_ELEMENTS` (1024) to prevent unbounded heap allocation.
- Connection-state validation via `StateValidator` (`SdcValidator`).
- TUP normalisation via `DataNormalizer` (`SdcNormalizer`), including unit attachment.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary
  input.

## Architecture

The crate parses a single binary `SdcFrame`:

- `SdcMessageType` — `Mdib` (0), `Waveform` (1), `Alert` (2), `Context` (3), with
  `as_str`: `"mdib"`, `"waveform"`, `"alert"`, `"context"`.
- `SdcFrame` — `message_type`, `timestamp: u64`, and `body: SdcBody`.
- `SdcBody` carries the decoded entities: `NumericMetrics(Vec<SdcMetricValue>)`
  (each with `handle`, `descriptor_handle`, `value: f64`, `unit`), `Waveforms(Vec<WaveformSample>)`
  (each with `handle`, `samples: Vec<f32>`, `sample_rate: f32`), `Alerts(Vec<AlertState>)`
  (each with `handle`, `descriptor_handle`, `active: bool`, `priority: u8`), and
  `ContextInfo(Vec<(String, String)>)`.
- `SdcParser` implements `FrameParser`. After reading the type + timestamp it
  dispatches to a body decoder:
  - **Mdib / Context**: a `u16` count, then per metric a length-prefixed `handle`,
    length-prefixed `descriptor_handle`, an 8-byte `f64` value (via `f64::from_bits`),
    and a length-prefixed `unit` string. `Context` reuses this layout and flattens
    to `(handle, unit)` pairs.
  - **Waveform**: a `u16` count, then per waveform a length-prefixed `handle`, a 4-byte
    `f32` sample rate, a `u16` sample count, and `sample_count * 4` bytes of `f32`
    samples.
  - **Alert**: a `u16` count, then per alert a length-prefixed `handle`, length-prefixed
    `descriptor_handle`, a 1-byte `active` flag, and a 1-byte `priority`.
  - Every length/offset read is bounds-checked; truncated frames return `Error::Parse`.
- `SdcValidator` validates `SdcConnectionState` (`connected`, `metric_count`,
  `last_sequence_id`) and errors if `!connected` or `metric_count > 1000`.
- `SdcNormalizer` emits one data point per frame from the *first* entity: numeric →
  `Float(value)` + unit; waveform → `Float(average sample)` with unit `"mV"`;
  alert → `Bool(active)`; context → `Text(value)`. Missing entities yield
  `Error::Normalize`.

## Usage

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_tup::SourceProtocol;
use tpt_protocol_ieee11073_sdc::{SdcNormalizer, SdcParser};

fn main() {
    // Mdib (0), timestamp = 1_000_000, 1 metric:
    // handle="temp", descriptor="descr001", value=37.5, unit="C"
    let mut bytes: Vec<u8> = vec![0];
    bytes.extend_from_slice(&1_000_000u64.to_be_bytes());
    bytes.extend_from_slice(&1u16.to_be_bytes());
    bytes.extend_from_slice(&4u16.to_be_bytes());
    bytes.extend_from_slice(b"temp");
    bytes.extend_from_slice(&7u16.to_be_bytes());
    bytes.extend_from_slice(b"descr001");
    bytes.extend_from_slice(&37.5f64.to_bits().to_be_bytes());
    bytes.extend_from_slice(&1u16.to_be_bytes());
    bytes.extend_from_slice(b"C");

    let frame = SdcParser.parse(&bytes).unwrap();
    assert_eq!(frame.message_type, tpt_protocol_ieee11073_sdc::SdcMessageType::Mdib);

    let dp = SdcNormalizer.normalize(&frame).unwrap();
    assert_eq!(dp.source_protocol, SourceProtocol::Ieee11073Sdc);
    assert_eq!(dp.point_id, "temp");
    assert_eq!(dp.unit, Some("C".to_string()));
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated strings and vectors
(`String`, `Vec`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — the entire crate: SDC frame/entity types, the `SdcParser`,
  `SdcValidator`, `SdcNormalizer`, the `MAX_SDC_ELEMENTS` allocation cap, and unit +
  `proptest` fuzz tests (numeric/waveform/alert parsing, truncation, huge count).

## Status / Conformance

- Implemented: binary decoding of all four SDC message bodies, length-prefixed
  string and `f64`/`f32` value decoding, allocation capping, state validation, and
  TUP normalisation with units.
- Known gaps / TODOs (see workspace `TODO.md` Phase 9.2 allocation-cap hardening):
  - This is a compact binary encoding, not the actual IEEE 11073 SDC XML/SOAP
    wire format; real SDC BICEPS/MDPWS messages are not parsed.
  - No descriptor/Mdib-tree association beyond the single `descriptor_handle` field,
    and no multi-entity batch normalisation (only the first entity per frame is
    emitted).

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.
- [`tpt-protocol-hl7v2`](../tpt-protocol-hl7v2) — the other medical-domain protocol crate in the suite.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
