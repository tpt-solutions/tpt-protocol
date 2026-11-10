# tpt-protocol-j1939

SAE J1939 heavy-vehicle CAN protocol parser, validator, and TUP normalizer for the
[TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

## Overview

SAE J1939 is the CAN-based protocol for trucks, buses and off-highway machines. It
defines Parameter Groups (PGNs) that each carry a set of Suspect Parameter Numbers
(SPNs) for engine, transmission, brake and vehicle data. TPT implements the 29-bit
identifier, the PGN, and the SPN scaling for the common engine and vehicle messages,
in a `#![no_std]` + `alloc` crate that depends only on `tpt-protocol-core` and
`tpt-protocol-tup`.

## Features

- 29-bit identifier decoding: priority, extended data page (EDP), data page (DP),
  PDU format (PF), PDU specific (PS) and source address (SA). PDU1 frames (PF below
  240) carry a destination address, not a group extension, so PS is excluded from the
  PGN.
- Named PGNs (`Pgn`): TC1 (PGN 0), EEC1 (0xF004), ET1 (0xFEEE), CCVS (0xFEF1), fuel
  economy (0xFEE9), ambient conditions (0xFEF5), plus `Raw(u32)`.
- SPN decoding in engineering units for the common signals: `EEC1.engine_speed`,
  `EngineTemp1.coolant_temp`, `CCVS.wheel_speed`, `FuelEconomy.fuel_rate`. Other PGNs
  give their payload as bytes.
- Multi-byte fields are little-endian, as on the wire.
- `J1939Encoder`: wire bytes from a frame, with round-trip tests against the parser.
- Connection-state validation via `StateValidator` (`J1939Validator`), which rejects
  the global address (`0xFF`).
- TUP normalisation via `DataNormalizer` (`J1939Normalizer`). A frame with no payload
  is an error, not a zero.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
  Structured generators build 29-bit identifiers across PDU1 and PDU2.

## Architecture

The input is a 4-byte big-endian CAN identifier followed by the payload (up to 8
bytes).

- `J1939Parser` derives `priority = (id >> 26) & 0x07`, `edp`, `dp`, `pf`, `ps` and
  `sa`. The PGN is `(edp, dp, pf, ps)` for PDU2 frames and `(edp, dp, pf)` for PDU1.
- `J1939Frame` holds `can_id`, `priority`, `edp`, `dp`, `pf`, `ps`, `sa`, `pgn` and
  `payload`.
- `J1939State` carries the connection flag, source address, timestamp and error
  count.
- `J1939Normalizer` produces `device_id = "j1939:0x<sa>"` and `point_id =
  "pgn:<PGN>.<signal>"` (for example `pgn:EEC1.engine_speed`). `normalize_all` returns
  one point per decoded signal in the frame.

## Usage

A complete, runnable example parses an EEC1 frame and normalizes it:

```sh
cargo run -p tpt-protocol-j1939 --example parse_j1939
```

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_j1939::{J1939Normalizer, J1939Parser};

let frame = J1939Parser::new().parse(&wire)?;
let points = J1939Normalizer::new().normalize_all(&frame)?;
// points[0].point_id == "pgn:EEC1.engine_speed"
// points[0].device_id == "j1939:0x12" for source address 0x12
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`, `String`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — `Pgn`, `J1939Frame`, `J1939Parser` (`FrameParser`), `J1939Encoder`
  (`FrameEncoder`), `J1939State` + `J1939Validator` (`StateValidator`),
  `J1939Normalizer` (`DataNormalizer`), unit tests, and the `proptest` `mod fuzz`.
- `examples/parse_j1939.rs` — runnable parse-and-normalize example.

## Status / Conformance

- **Single frames only.** Transport Protocol (BAM and connection-mode) reassembly is not
  implemented, so a message larger than 8 bytes is not decoded as one value.
- **A small SPN set.** Only the signals listed above are decoded. Other SPNs in those
  PGNs and the rest of the J1939 PGN catalogue are carried as bytes.
- Normalised `timestamp` is `0`; callers must supply a real timestamp.

## Related crates

- [`tpt-protocol-isobus`](../tpt-protocol-isobus) — ISOBUS, which runs on J1939's CAN
  transport.
- [`tpt-protocol-canopen`](../tpt-protocol-canopen) — another CAN-based protocol in the
  suite (11-bit identifiers).
- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `FrameEncoder` /
  `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
