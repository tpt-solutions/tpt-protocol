# tpt-protocol-isobus

ISOBUS (ISO 11783) agricultural-implement protocol parser, validator, and TUP
normalizer for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

## Overview

ISOBUS is the implement-to-tractor communication standard built on J1939's CAN
transport. It defines the Parameter Groups (PGNs) for implement control and the
identity of each electronic control unit (ECU). TPT implements the 29-bit CAN
identifier, the PGN, the ECU NAME carried in Address Claimed frames, and the
transport-protocol control messages, in a `#![no_std]` + `alloc` crate that depends
only on `tpt-protocol-core` and `tpt-protocol-tup`.

## Features

- 29-bit identifier decoding: priority, data page, PDU format (PF), PDU specific (PS)
  and source address (SA). PDU1 frames (PF below 240) carry no destination in the PGN.
- Named PGNs (`IsobusPgn`): TP.CM (0xEC00), TP.DT (0xEB00), Request (0xEA00), Address
  Claimed (0xEE00), and the engine and vehicle PGNs that the normalizer decodes, plus
  `Raw`.
- `IsobusName`: the 64-bit ECU NAME in Address Claimed frames, decoded with the ISO
  11783-5 bit layout (identity number, manufacturer code, ECU and function instance,
  function, device class, industry group, arbitrary-address capability).
- Signals for EEC1, ET1, CCVS and fuel economy, in their units, matching the J1939
  crate. Other PGNs give their payload as bytes.
- `IsobusEncoder`: wire bytes from a frame, with round-trip tests against the parser.
- Connection-state validation via `StateValidator` (`IsobusValidator`), which rejects
  a zero NAME.
- TUP normalisation via `DataNormalizer` (`IsobusNormalizer`). A frame with no payload
  is an error, not a zero.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.

## Architecture

The input is a 4-byte big-endian CAN identifier followed by the payload (up to 8
bytes).

- `IsobusParser` derives `priority = (id >> 26) & 0x07`, `dp`, `pf`, `ps` and `sa`.
  The PGN includes PS only for PDU2 frames. It keeps the ECU NAME only for Address
  Claimed (`0xEE00`) frames.
- `IsobusFrame` holds `can_id`, `priority`, `dp`, `pf`, `ps`, `sa`, `pgn`, `payload`
  and `name: Option<IsobusName>`.
- `IsobusState` carries the connection flag, source address, the raw `ecu_name`,
  timestamp and error count. The validator rejects a zero NAME.
- `IsobusNormalizer` produces `device_id = "isobus:0x<sa>"`, extended with the
  manufacturer code and identity from the NAME when it is known. Point ids are
  `pgn:<name>.<signal>` for decoded signals (for example `pgn:EEC1.engine_speed`) and
  `pgn:<name>.payload` for other PGNs.

## Usage

A complete, runnable example parses an EEC1-style frame and normalizes it:

```sh
cargo run -p tpt-protocol-isobus --example parse_isobus
```

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_isobus::{IsobusNormalizer, IsobusParser};

let frame = IsobusParser::new().parse(&wire)?;
let points = IsobusNormalizer::new().normalize_all(&frame)?;
// points[0].point_id == "pgn:EEC1.engine_speed" for a decoded EEC1 frame
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`, `String`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — `IsobusPgn`, `IsobusName`, `IsobusFrame`, `IsobusParser`
  (`FrameParser`), `IsobusEncoder` (`FrameEncoder`), `IsobusState` +
  `IsobusValidator` (`StateValidator`), `IsobusNormalizer` (`DataNormalizer`), unit
  tests, and the `proptest` `mod fuzz`.
- `examples/parse_isobus.rs` — runnable parse-and-normalize example.

## Status / Conformance

- **Single frames only.** Transport-protocol (TP.CM / TP.DT) control messages are
  parsed, but the multi-packet payload is not reassembled, so a message larger than
  8 bytes is not decoded as one value.
- **A small signal set.** Only EEC1, ET1, CCVS and the fuel-economy PGN are decoded.
  Working-set and task-controller messages are carried as bytes.
- **Identity is the NAME.** `implement_id` and `device_class` are not read from payload
  bytes; they come from the ECU NAME in Address Claimed frames only.
- Normalised `timestamp` is `0`; callers must supply a real timestamp.

## Related crates

- [`tpt-protocol-j1939`](../tpt-protocol-j1939) — the J1939 transport and signal set that
  ISOBUS builds on.
- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `FrameEncoder` /
  `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
