# tpt-protocol-ieee11073-sdc

IEEE 11073 SDC (Service-oriented Device Connectivity) medical-device message parser,
validator, and TUP normalizer for the
[TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

## Overview

IEEE 11073 SDC carries medical-device data (vital signs, alarms, device state) over
web services. Messages are SOAP 1.2 envelopes whose bodies are BICEPS XML documents
from ISO/IEEE 11073-10207 and 11073-20701. TPT implements a bounded XML parser for the
subset SDC needs, the SOAP and WS-Addressing headers, and the numeric observation
reports, in a `#![no_std]` + `alloc` crate.

## Features

- A bounded XML parser: elements, attributes, namespace scope, character data,
  comments, CDATA and processing instructions, with entity and numeric character
  references decoded. DOCTYPE is rejected, which blocks entity expansion. Depth and
  element counts are limited.
- SOAP 1.2 envelope checks by namespace, the WS-Addressing `Action` and `MessageID`
  headers, and the body operation (for example `EpisodicMetricReport`).
- Numeric observations (`NumericObservation`): the descriptor handle and a finite value.
  Non-finite values are dropped.
- `SdcEncoder`: wire XML from a frame, with round-trip tests against the parser.
- Connection-state validation via `StateValidator` (`SdcValidator`).
- TUP normalisation via `DataNormalizer` (`SdcNormalizer`). `normalize_all` returns one
  point per numeric observation.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.

## Architecture

`SdcParser` reads the SOAP envelope, then the body:

1. The XML parser builds a bounded element tree.
2. The SOAP envelope is checked by its SOAP 1.2 namespace. The WS-Addressing headers
   give the `Action` and `MessageID`.
3. The body's operation element is named (`SdcFrame::operation`), and each
   `NumericMetricState` in it becomes a `NumericObservation`.

The decoded result is the `SdcFrame` struct (`action`, `message_id`, `operation`,
`observations`).

`SdcNormalizer` produces `device_id = "sdc:device"` and `point_id` equal to the
observation's descriptor handle (for example `mds0.vmd0.metric1`). The value is
`Float(value)`.

## Usage

A complete, runnable example parses an episodic metric report and normalizes it:

```sh
cargo run -p tpt-protocol-ieee11073-sdc --example parse_sdc
```

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_ieee11073_sdc::{SdcNormalizer, SdcParser};

let frame = SdcParser.parse(&wire)?;
let points = SdcNormalizer.normalize_all(&frame)?;
// points[0].point_id == "mds0.vmd0.metric1"
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`, `String`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — the bounded XML parser, `NumericObservation`, `SdcFrame`, `SdcParser`
  (`FrameParser`), `SdcEncoder` (`FrameEncoder`), `SdcConnectionState` +
  `SdcValidator` (`StateValidator`), `SdcNormalizer` (`DataNormalizer`), unit tests,
  and the `proptest` `mod fuzz`.
- `examples/parse_sdc.rs` — runnable parse-and-normalize example.

## Status / Conformance

- **Element and namespace names are unverified against a real device.** The BICEPS
  element names and namespace version follow the published standards, but no captured
  device traffic has been checked against them.
- **Numeric observations only.** Waveforms, alerts and context states are not decoded
  yet.
- **No transport.** The crate parses a captured message. It does not run the WS-Discovery
  or WS-Eventing exchange or open an HTTP connection.
- Normalised `timestamp` is `0`; callers must supply a real timestamp.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `FrameEncoder` /
  `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
