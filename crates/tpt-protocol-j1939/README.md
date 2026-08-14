# tpt-protocol-j1939

SAE J1939 heavy-duty vehicle-network protocol parsing, validation, and TUP
normalisation for the [TPT Protocol](https://github.com/tpt-protocol/tpt-protocol) suite.

J1939 is the de-facto higher-layer protocol for trucks, buses, and off-road
machinery built on CAN. TPT decodes the 29-bit CAN identifier into priority/addressing
fields, resolves the Parameter Group Number (PGN), and normalises the frame into a TUP
data point keyed by source address and PGN.

## Overview

SAE J1939 defines how heavy-duty vehicles exchange engine, chassis, and body
controller data over CAN using a 29-bit identifier and standard Parameter Group
Numbers. TPT implements the frame/PGN decoding and connection-health validation so
vehicle telemetry (EEC1, engine temperature, fuel economy, cab temperature, torque
control, etc.) can be unified with the rest of the pipeline.

## Features

- 29-bit J1939 CAN-ID decoding: priority, Extended Data Page (EDP), Data Page (DP),
  PDU Format (PF), PDU Specific (PS), and Source Address (SA).
- Parameter Group Number (PGN) resolution, including the PDU1/PDU2 distinction
  (`PF >= 240` includes PS in the PGN).
- Recognition of J1939 PGNs: TC1 (0xFF01), EEC1 (0xF004), EngineTemp1 (0xFEEE),
  FuelEconomy (0xFEE9), CabTemp (0xFEF5), plus `Raw(u32)`.
- Payload bounds checking: frames require at least 4 bytes (the CAN ID) and at most
  8 payload bytes.
- An incremental `try_parse` helper that returns `Option<(frame, consumed)>` for
  streamed/de-multiplexed input.
- Connection-state validation via `StateValidator` (`J1939Validator`).
- TUP normalisation via `DataNormalizer` (`J1939Normalizer`).
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary
  input.

## Architecture

The crate parses a single `J1939Frame` from a `[u8]` of `CAN_ID (4 bytes, big-endian)
+ payload`:

- `J1939Frame` — `can_id: u32`, `priority`, `edp`, `dp`, `pf`, `ps`, `sa`,
  `pgn: Pgn`, `payload: Vec<u8>`.
- `Pgn` maps the resolved PGN to `Tc1` (0xFF01), `Eec1` (0xF004),
  `EngineTemp1` (0xFEEE), `FuelEconomy` (0xFEE9), `CabTemp` (0xFEF5), or `Raw(u32)`.
- `J1939Parser` implements `FrameParser`. It reads the big-endian `u32` CAN ID and
  derives fields via bit shifts: `priority = (id >> 26) & 0x07`, `edp = (id >> 25) &
  0x01`, `dp = (id >> 24) & 0x01`, `pf = (id >> 16) & 0xFF`, `ps = (id >> 8) & 0xFF`,
  `sa = id & 0xFF`. The PGN is `(dp << 16) | (pf << 8) | (ps)` when `pf >= 240`
  (PDU2) and `(dp << 16) | (pf << 8)` otherwise (PDU1). Payload is `bytes[4..]`,
  capped at 8 bytes. `try_parse` wraps `parse` and returns `None` for input shorter
  than 4 bytes.
- `J1939Validator` validates `J1939State` (`connected`, `source_address`,
  `last_ok_timestamp`, `error_count`): it errors if not connected, if `source_address
  == 0xFF` (global address), or if `error_count > 20`.
- `J1939Normalizer` emits `device_id = "j1939:<sa>"`, `point_id = "pgn:<pgn>"`, and
  `value`: the first 4 payload bytes as `Int` when present, otherwise `Bytes(payload)`,
  otherwise `Int(0)`.

## Usage

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_tup::SourceProtocol;
use tpt_protocol_j1939::{J1939Normalizer, J1939Parser};

fn main() {
    // 29-bit CAN ID: priority=3, PF=0xF0, PS=0x04 (EEC1, PGN 61444), SA=0x12
    let can_id = (0x03u32 << 26) | ((0xF0u32) << 16) | ((0x04u32) << 8) | 0x12;
    let mut bytes = can_id.to_be_bytes().to_vec();
    bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0x64, 0x00, 0x00, 0x00, 0x00]);

    let parser = J1939Parser::new();
    let frame = parser.parse(&bytes).unwrap();
    assert_eq!(frame.pgn, tpt_protocol_j1939::Pgn::Eec1);
    assert_eq!(frame.sa, 0x12);

    let normalizer = J1939Normalizer::new();
    let dp = normalizer.normalize(&frame).unwrap();
    assert_eq!(dp.source_protocol, SourceProtocol::J1939);
    assert_eq!(dp.device_id, "j1939:0x12");
    assert_eq!(dp.point_id, "pgn:EEC1");
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`, `String`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — the entire crate: J1939 CAN-ID/frame types, the `J1939Parser`
  (with `try_parse`), `J1939Validator`, `J1939Normalizer`, and unit + `proptest` fuzz
  tests (PGN decoding, truncation, payload bounds, address validation).

## Status / Conformance

- Implemented: 29-bit CAN-ID decoding (including EDP), PGN resolution (PDU1/PDU2),
  the five named J1939 PGNs, payload bounds checking, state validation, and TUP
  normalisation.
- Known gaps / TODOs:
  - No multi-packet transport-protocol (TP/ BAM) reassembly for PGNs larger than 8
    bytes.
  - No interpretation of PGN-specific SPN (Suspect Parameter Number) scaling — only the
    first 4 payload bytes are surfaced as a raw integer; the daemon is simulation-only
    (no real CAN transport, see workspace `TODO.md`).

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.
- [`tpt-protocol-isobus`](../tpt-protocol-isobus) — shares the 29-bit CAN-ID layout; the agricultural counterpart to J1939.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
