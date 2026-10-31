# tpt-protocol-knx

KNX smart-building protocol parsing, validation, and TUP normalisation for the
[TPT Protocol](https://github.com/tpt-protocol/tpt-protocol) suite.

KNX is the standard wired/wireless bus for home and building automation
(lighting, HVAC, shading, metering). TPT decodes a cEMI-like frame header, resolves
the group address, decodes the Datapoint Type (DPT) value, and normalises it into a
TUP data point keyed by group address.

## Overview

KNX carries group-telegram communication between sensors and actuators on a building
bus. Each telegram targets a *group address* (main/middle/sub) and carries a
datapoint value of a given DPT. TPT implements the cEMI-like binary frame layout so
building-automation telemetry can be unified with the rest of the pipeline.

## Features

- cEMI-like binary frame parsing: control byte, source address, group address,
  group flag, service type, and DPT payload.
- Group-address decoding into the `main(5) / middle(3) / sub(8)` structure.
- Service-type decoding: `GroupValueRead` (0x00), `GroupValueResponse` (0x40),
  `GroupValueWrite` (0x80), with a public `as_str` round-trip helper.
- Datapoint Type (DPT) value decoding into `Switch`, `Scaling`, `Temperature`,
  `Percent`, and `Raw` variants.
- Connection-state validation via `StateValidator` (`KnxValidator`).
- TUP normalisation via `DataNormalizer` (`KnxNormalizer`).
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary
  input.

## Architecture

The crate parses a single `KnxFrame` from a cEMI-like binary layout:

- `KnxFrame` — `service_type: KnxServiceType`, `group_address: GroupAddress`,
  `source_address: u16`, `dpt: DptValue`, `priority: u8`.
- `KnxServiceType` maps the wire `u8` to `GroupValueRead` (0x00),
  `GroupValueResponse` (0x40), `GroupValueWrite` (0x80); unknown values yield
  `Error::Parse`. `as_str` provides the public name.
- `GroupAddress` holds `main: u8` (top 5 bits), `middle: u8` (next 3 bits),
  `sub: u8` (low 8 bits), with `from_u16` / public `to_u16` round-trip conversion.
- `DptValue` — `Switch(bool)`, `Scaling(u8)`, `Temperature(f32)`, `Percent(f32)`,
  `Raw(u16)`.
- `KnxParser` implements `FrameParser`. Layout (`[0]` control with priority field,
  `[1..2]` source address, `[3..4]` group address, `[5]` counter + group flag where
  bit 6 must be set, `[6]` service type, `[7+]` DPT data):
  - `GroupValueRead` requires a data byte and decodes `Raw(u16::from(byte))`.
  - `GroupValueResponse` / `GroupValueWrite` decode `Switch` for byte `0..=1`,
    otherwise `Temperature` (two-byte `i16 / 100.0`) when a second byte is present,
    else `Scaling` for values `<= 100`, else `Raw`.
  - Truncated input or a missing group flag yields `Error::Parse`.
- `KnxValidator` validates `KnxConnectionState` (`connected`, `group_address`,
  `sequence_counter`): it errors if not connected or if `group_address.main > 31` or
  `middle > 7` (out of the 5/3-bit field ranges).
- `KnxNormalizer` emits `device_id = "knx/<source_address hex>"`,
  `point_id = "<main>/<middle>/<sub>"`, and `value` from the DPT (`Bool`/`Int`/`Float`).

## Usage

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_tup::SourceProtocol;
use tpt_protocol_knx::{DptValue, KnxNormalizer, KnxParser};

fn main() {
    // control (priority=3), source 0x0101, group 0x0801 (1/0/1), group flag, Write, data=1
    let bytes: [u8; 8] = [0x30, 0x01, 0x01, 0x08, 0x01, 0x40, 0x80, 0x01];

    let frame = KnxParser.parse(&bytes).unwrap();
    assert_eq!(frame.service_type, tpt_protocol_knx::KnxServiceType::GroupValueWrite);
    assert_eq!(frame.dpt, DptValue::Switch(true));

    let dp = KnxNormalizer.normalize(&frame).unwrap();
    assert_eq!(dp.source_protocol, SourceProtocol::Knx);
    assert_eq!(dp.device_id, "knx/0x0101");
    assert_eq!(dp.point_id, "1/0/1");
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated strings (`String`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — the entire crate: KNX frame/address/DPT types, the `KnxParser`,
  `KnxValidator`, `KnxNormalizer`, and unit + `proptest` fuzz tests (write/read
  decoding, truncation, invalid service type, missing group flag).

## Status / Conformance

- Implemented: cEMI-like header parsing, group-address decoding, the three KNX
  service types, DPT value decoding for Switch/Scaling/Temperature/Percent/Raw,
  state validation, and TUP normalisation.
- Known gaps / TODOs:
  - This is a simplified cEMI-like layout, not a full cEMI (Common External Message
    Interface) frame; real cEMI medium-/transport-layer control fields and APCI/NPDU
    framing are not modelled.
  - No DPT table coverage beyond the inline heuristics above (e.g. DPT 9 float encoding
    beyond the two-byte `Temperature` case); the daemon is simulation-only (no real
    KNX/IP or TP-UART transport, see workspace `TODO.md`).

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.
- [`tpt-protocol-coap`](../tpt-protocol-coap) — another building/automation-adjacent protocol crate in the suite.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
