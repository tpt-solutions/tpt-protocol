# tpt-protocol-isobus

ISO 11783 (ISOBUS) agricultural-vehicle protocol parsing, validation, and TUP
normalisation for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

ISOBUS is the in-cab network standard for agricultural and forestry equipment,
built on a 29-bit CAN identifier shared with SAE J1939. TPT decodes the CAN ID into
priority/addressing fields, resolves the Parameter Group Number (PGN), and normalises
the frame into a TUP data point keyed by source address and implement.

## Overview

ISO 11783 (ISOBUS) lets tractors, implements, and terminals exchange task data,
control commands, and working-set management over CAN. Its 29-bit identifier uses the
same priority/DP/PF/PS/SA layout as J1939, so TPT shares the same decoding approach
while recognising the ISOBUS-specific PGNs. TPT ingests ISOBUS traffic so farm
equipment telemetry can be unified with the rest of the pipeline.

## Features

- 29-bit ISOBUS CAN-ID decoding: priority, data page, PDU Format (PF), PDU Specific
  (PS), and Source Address (SA).
- Parameter Group Number (PGN) resolution, including the PDU1/PDU2 distinction
  (`PF >= 240` includes PS in the PGN).
- Recognition of ISOBUS PGNs: Request for Rep (0xEC00), PTO (0xFEBA),
  Working Set Master (0xFE12), Task Controller (0xFEAB), plus `Raw(u32)`.
- Payload bounds checking: frames require at least 4 bytes (the CAN ID) and at most
  8 payload bytes.
- Implement-ID and device-class extraction from the payload.
- Connection-state validation via `StateValidator` (`IsobusValidator`).
- TUP normalisation via `DataNormalizer` (`IsobusNormalizer`).
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary
  input.

## Architecture

The crate parses a single `IsobusFrame` from a `[u8]` of `CAN_ID (4 bytes, big-endian)
+ payload`:

- `IsobusFrame` — `can_id: u32`, `priority`, `dp`, `pf`, `ps`, `sa`, `pgn:
  IsobusPgn`, `payload: Vec<u8>`, `implement_id: u16`, `device_class: u8`.
- `IsobusPgn` maps the resolved PGN to `RequestForRep` (0xEC00), `Pto` (0xFEBA),
  `WorkingSetMaster` (0xFE12), `TaskController` (0xFEAB), or `Raw(u32)`.
- `IsobusParser` implements `FrameParser`. It reads the big-endian `u32` CAN ID and
  derives fields via bit shifts: `priority = (id >> 26) & 0x07`, `dp = (id >> 24) &
  0x01`, `pf = (id >> 16) & 0xFF`, `ps = (id >> 8) & 0xFF`, `sa = id & 0xFF`. The
  PGN is `(dp << 16) | (pf << 8) | (ps)` when `pf >= 240` (PDU2) and `(dp << 16) |
  (pf << 8)` otherwise (PDU1). Payload is `bytes[4..]`, capped at 8 bytes. The
  `implement_id` comes from payload bytes `0..1` and `device_class` from byte `2`.
- `IsobusValidator` validates `IsobusState` (`connected`, `source_address`,
  `implement_id`, `last_ok_timestamp`, `error_count`): it errors if not connected, if
  `source_address` is `0xFE` or `0xFF` (reserved/global), if `implement_id == 0`, or
  if `error_count > 15`.
- `IsobusNormalizer` emits `device_id = "isobus:<sa>:<implement_id>"`,
  `point_id = "pgn:<pgn>"`, and `value`: the first 4 payload bytes as `Int` when
  present, otherwise `Bytes(payload)`, otherwise `Int(0)`.

## Usage

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_tup::SourceProtocol;
use tpt_protocol_isobus::{IsobusNormalizer, IsobusParser};

fn main() {
    // 29-bit CAN ID: priority=3, PF=0xEC, PS=0x00, SA=0x20, plus 8 payload bytes
    let can_id = (0x03u32 << 26) | ((0xECu32) << 16) | ((0x00u32) << 8) | 0x20;
    let mut bytes = can_id.to_be_bytes().to_vec();
    bytes.extend_from_slice(&[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]);

    let parser = IsobusParser::new();
    let frame = parser.parse(&bytes).unwrap();
    assert_eq!(frame.pgn, tpt_protocol_isobus::IsobusPgn::RequestForRep);
    assert_eq!(frame.implement_id, 0x0001);

    let normalizer = IsobusNormalizer::new();
    let dp = normalizer.normalize(&frame).unwrap();
    assert_eq!(dp.source_protocol, SourceProtocol::Isobus);
    assert_eq!(dp.device_id, "isobus:0x20:0x0001");
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`, `String`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — the entire crate: ISOBUS CAN-ID/frame types, the `IsobusParser`,
  `IsobusValidator`, `IsobusNormalizer`, and unit + `proptest` fuzz tests
  (PGN decoding, payload bounds, address validation).

## Status / Conformance

- Implemented: 29-bit CAN-ID decoding, PGN resolution (PDU1/PDU2), the four named
  ISOBUS PGNs, payload bounds checking, implement-ID/device-class extraction, state
  validation, and TUP normalisation.
- Known gaps / TODOs:
  - No multi-packet transport-protocol (TP) reassembly for PGNs larger than 8 bytes
    (e.g. working-set / task-controller messages that span multiple CAN frames).
  - No interpretation of PGN-specific payload layouts (only the first 4 bytes are
    surfaced as a raw integer); the daemon is simulation-only (no real CAN transport,
    see workspace `TODO.md`).

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.
- [`tpt-protocol-j1939`](../tpt-protocol-j1939) — shares the 29-bit CAN-ID layout; the heavy-duty counterpart to ISOBUS.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
