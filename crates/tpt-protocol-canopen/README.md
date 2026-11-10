# tpt-protocol-canopen

CANopen embedded motion-control protocol parser, validator, and TUP normalizer for
the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

## Overview

CANopen runs on top of Classical CAN (ISO 11898) and is the dominant higher-layer
protocol for servo drives, I/O modules, and PLCs in machinery. TPT implements the
CANopen framing layer — splitting the 11-bit COB-ID into function code and node ID —
so captured CAN frames can be validated for node health and normalized into unified
data points. This crate is `#![no_std]` (the popular `canopen` crate requires `std`
and the Linux-only `socketcan` dependency, which this implementation avoids).

## Features

- 11-bit COB-ID decoding into function code (bits 10–7) and node ID (bits 6–0)
  (`CanOpenFunctionCode`, `CanOpenFrame`).
- Payload bounds checking (max 8 bytes, the Classical CAN data-field limit).
- Decoding of the standard CANopen function codes (`CanOpenFunctionCode`): NMT (0),
  SYNC (1), Timestamp (2), PDO1-TX (3), PDO1-RX (4), PDO2-TX (5), PDO2-RX (6),
  SDO-TX (0x0B), SDO-RX (0x0C), Heartbeat (0x0E), Emergency — plus `Unknown(u8)`.
- Connection-health validation via `StateValidator` (`CanOpenValidator`) checking
  connection, a valid node ID (1–127), and a consecutive-error threshold.
- TUP normalisation via `DataNormalizer` (`CanOpenNormalizer`) mapping each frame to
  a `tup::DataPoint`.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests (`mod fuzz`) asserting `FrameParser::parse` never panics on
  arbitrary input.

## Architecture

`CanOpenParser::parse` (`src/lib.rs`) treats the first two bytes as a big-endian
COB-ID, then masks to the 11-bit field (`& 0x07FF`):

- `function_code` = `(cob_id >> 7) & 0x0F`, decoded by `CanOpenFunctionCode::from_u8`.
- `node_id` = `cob_id & 0x7F`.
- The remaining bytes are the `payload` (up to 8); a frame claiming more than 8
  payload bytes is rejected with `Error::Parse`.

The decoded result is the `CanOpenFrame` struct (`cob_id`, `function_code`,
`node_id`, `payload`).

`CanOpenValidator` consumes a `CanOpenState` (`connected`, `node_id`,
`heartbeat_interval_ms`, `last_heartbeat`, `error_count`) and rejects the state when
disconnected, `node_id` is `0` or `> 127`, or more than 10 consecutive errors occur.

`CanOpenNormalizer` produces a `DataPoint` keyed by `device_id = "canopen:{node_id}"`
and `point_id = "fc:{function_code}"`; the value is the first 4 payload bytes as a
big-endian `i64` when the payload is ≥ 4 bytes, otherwise the raw payload bytes or —
with no payload — the integer COB-ID.

## Usage

```rust
use tpt_protocol_core::{FrameParser, DataNormalizer};
use tpt_protocol_canopen::{CanOpenParser, CanOpenNormalizer, CanOpenFunctionCode};

fn main() {
    // COB-ID = (FC 0x03 << 7) | node 0x01 ; PDO1-TX with a 4-byte value.
    let cob_id: u16 = ((0x03u16) << 7) | 0x01;
    let mut raw: Vec<u8> = Vec::new();
    raw.extend_from_slice(&cob_id.to_be_bytes());
    raw.extend_from_slice(&[0x00, 0x64, 0x00, 0x00]); // payload (0x6400)

    let parser = CanOpenParser::new();
    let frame = parser.parse(&raw).unwrap();
    assert_eq!(frame.function_code, CanOpenFunctionCode::Pdo1Tx);
    assert_eq!(frame.node_id, 0x01);

    let normalizer = CanOpenNormalizer::new();
    let dp = normalizer.normalize(&frame).unwrap();
    assert_eq!(dp.source_protocol, tpt_protocol_tup::SourceProtocol::Canopen);
    assert_eq!(dp.device_id, "canopen:1");
    assert_eq!(dp.point_id, "fc:PDO1-TX");
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — the full implementation: `CanOpenFunctionCode` enum,
  `CanOpenFrame` struct, `CanOpenParser` (`FrameParser`), `CanOpenState` +
  `CanOpenValidator` (`StateValidator`), `CanOpenNormalizer` (`DataNormalizer`),
  unit tests, and the `proptest` `mod fuzz`.

## Status / Conformance

- **Emergency function code not mapped.** `CanOpenFunctionCode::Emergency` exists as
  a variant but has no numeric mapping in `from_u8`; an Emergency COB-ID decodes to
  `Unknown(u8)` rather than `Emergency`. (Bits 10–7 == `0x0D`/13 is the EMCY
  function code and is currently unhandled.)
- **PDO/SDO contents not decoded.** Process data (PDO) and service data (SDO)
  payloads are carried as raw bytes; object dictionary indices/subindices and SDO
  expedited/segmented transfers are not interpreted.
- **No CAN FD.** Only Classical CAN 8-byte frames are supported; CAN FD payloads
  (> 8 bytes) are rejected by the bounds check.
- **No transport/CAN driver.** This crate parses an already-captured frame buffer;
  it does not open a CAN socket (avoiding the Linux-only `socketcan` dependency).

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` /
  `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.
- [`tpt-protocol-j1939`](../tpt-protocol-j1939) — another CAN-based protocol in the
  suite (29-bit extended ID, heavy-vehicle focused).

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
