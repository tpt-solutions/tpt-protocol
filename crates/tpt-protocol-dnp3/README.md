# tpt-protocol-dnp3

DNP3 electrical-grid SCADA protocol parser, validator, and TUP normalizer for the
[TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

## Overview

DNP3 (IEEE 1815) is the dominant polling protocol for electrical-utility SCADA —
substation RTUs talking to control centers over serial and TCP/UDP. TPT implements
the DNP3 data-link, transport, and application layers so captured frames can be
validated for link health (with CRC checking) and normalized into unified data
points keyed by source address and application function.

## Features

- DNP3 data-link-layer parsing: start `0x0564`, length, control, 2-byte LE
  destination and source, and CRC-16/DNP over bytes 2–7 (`Dnp3Frame`).
- CRC-16/DNP validation of the 10-byte data-link header (`crc16_dnp`), with a
  truncation guard covering the trailing CRC bytes (no out-of-bounds indexing).
- Transport-layer parsing: final-segment flag (bit 7) and sequence number (bits 0–5).
- Application-layer function-code decoding (`Dnp3FunctionCode`): Read (1), Write (2),
  Select (3), Operate (4), DirectOperate (5), DirectOperateNoAck (6), Freeze (7),
  FreezeNoAck (8), FreezeClear (9), FreezeClearNoAck (10) — plus `Unknown(u8)`.
- Connection-health validation via `StateValidator` (`Dnp3Validator`) checking
  connection, a valid source address (not `0` / not `0xFFFF`), and a consecutive-error
  threshold.
- TUP normalisation via `DataNormalizer` (`Dnp3Normalizer`) mapping each frame to a
  `tup::DataPoint`.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests (`mod fuzz`) asserting `FrameParser::parse` never panics on
  arbitrary input.

## Architecture

`Dnp3Parser::parse` (`src/lib.rs`) works upward through three layers:

- **Data link** (10 bytes minimum): `start` must be `0x0564` (read big-endian);
  `length` (byte 2), `control` (byte 3), `destination` and `source` (2 bytes each,
  little-endian, bytes 4–7). The `data_link_crc` at bytes 8–9 is validated against
  `crc16_dnp(&bytes[2..8])`. A guard requires the buffer to hold the full 10-byte
  header *and* the advertised `length + 2` total before indexing the CRC, so a
  maliciously small `length` cannot trigger an out-of-bounds read.
- **Transport** (byte 10, if present): `transport_final = (th & 0x80) != 0`,
  `transport_seq = th & 0x3F`.
- **Application** (byte 11, if present): `app_function = Dnp3FunctionCode::from_u8(...)`
  and the trailing bytes become `payload`.

The decoded result is the `Dnp3Frame` struct (`start`, `length`, `control`,
`destination`, `source`, `data_link_crc`, `transport_final`, `transport_seq`,
`app_function`, `payload`).

`Dnp3Validator` consumes a `Dnp3State` (`connected`, `source_address`,
`destination_address`, `last_ok_timestamp`, `error_count`) and rejects the state when
disconnected, `source_address` is `0` or `0xFFFF`, or more than 10 consecutive errors
occur.

`Dnp3Normalizer` produces a `DataPoint` keyed by `device_id = "dnp3:{source}"` and
`point_id = "func:{app_function}"`; the value is the raw payload bytes when present,
otherwise the integer `length`.

## Usage

```rust
use tpt_protocol_core::{FrameParser, DataNormalizer};
use tpt_protocol_dnp3::{Dnp3Parser, Dnp3Normalizer, Dnp3FunctionCode};

fn main() {
    // Assemble a minimal Read frame: start(2) + length(1) + ctrl/dest/src(5) + CRC(2)
    //   + transport(1, final+seq0) + func(1) + payload.
    let mut header: Vec<u8> = vec![0x44];              // control
    header.extend_from_slice(&0x0Au16.to_le_bytes());  // destination
    header.extend_from_slice(&0x0001u16.to_le_bytes()); // source
    let payload: &[u8] = &[];
    let total_len = 2 + 1 + 5 + 2 + 1 + 1 + payload.len();
    let length = (total_len - 2) as u8;

    let mut raw: Vec<u8> = Vec::new();
    raw.extend_from_slice(&0x0564u16.to_be_bytes());
    raw.push(length);
    raw.extend_from_slice(&header);
    let crc = dnp3_crc(&raw[2..8]);
    raw.extend_from_slice(&crc.to_le_bytes());
    raw.push(0xC0); // transport: final + seq 0
    raw.push(1);    // Read

    let parser = Dnp3Parser::new();
    let frame = parser.parse(&raw).unwrap();
    assert_eq!(frame.source, 1);
    assert_eq!(frame.app_function, Dnp3FunctionCode::Read);

    let normalizer = Dnp3Normalizer::new();
    let dp = normalizer.normalize(&frame).unwrap();
    assert_eq!(dp.source_protocol, tpt_protocol_tup::SourceProtocol::Dnp3);
    assert_eq!(dp.device_id, "dnp3:1");
    assert_eq!(dp.point_id, "func:Read");
}

// Stand-in for the parser's internal CRC-16/DNP routine — see `crc16_dnp` in
// src/lib.rs. Defined here only so the example is self-contained.
fn dnp3_crc(data: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &b in data {
        crc ^= b as u16;
        for _ in 0..8 {
            crc = if crc & 0x0001 != 0 { (crc >> 1) ^ 0xA6BC } else { crc >> 1 };
        }
    }
    !crc
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — the full implementation: `Dnp3FunctionCode` enum, `Dnp3Frame` struct,
  `crc16_dnp` helper, `Dnp3Parser` (`FrameParser`), `Dnp3State` + `Dnp3Validator`
  (`StateValidator`), `Dnp3Normalizer` (`DataNormalizer`), unit tests (including the
  truncation regression tests), and the `proptest` `mod fuzz`.

## Status / Conformance

- **Single transport segment only.** The transport layer is parsed for the final
  flag / sequence number, but multi-segment application fragments (segmentation
  reassembly across several transport PDUs) are not reassembled — only the first
  segment's application function and payload are extracted.
- **No application-object decoding.** Internal-indication, binary-input, analog-input
  (group/var) objects and their points are not decoded; the application payload is
  carried as raw bytes.
- **Data-link CRC only.** The transport-layer CRC is not separately validated; only
  the 10-byte data-link header CRC is checked.
- The truncation guard that prevents an out-of-bounds panic on a malformed frame is a
  security hardening fix retained from workspace Phase 9.2 (regression tests
  `truncated_short_header_no_panic`, `truncated_length_seven_no_panic`).

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` /
  `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.
- [`tpt-protocol-modbus`](../tpt-protocol-modbus) — another polling/SCADA-style
  protocol in the suite (the reference implementation).

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
