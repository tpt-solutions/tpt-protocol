# tpt-protocol-dnp3

DNP3 electrical-grid SCADA protocol parser, validator, and TUP normalizer for the
[TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

## Overview

DNP3 (IEEE 1815) is the dominant polling protocol for electrical-utility SCADA —
substation RTUs talking to control centers over serial and TCP/UDP. TPT implements
the DNP3 data-link, transport, and application headers so captured frames can be
validated (every CRC is checked) and normalized into unified data points keyed by
source address and application function.

## Features

- Data-link parsing: start `0x0564`, length, control, 2-byte little-endian
  destination and source. The header CRC covers all eight bytes before it, start
  bytes included, and is checked against the published CRC-16/DNP value.
- User data is split into 16-byte blocks, and each block carries its own CRC. A
  corrupt block is a checksum error.
- Transport header: first-segment and final-segment flags and the sequence number.
- Application header (first fragment only): application control and the function
  code (`Dnp3FunctionCode`): Confirm (0), Read (1), Write (2), Select (3), Operate
  (4), DirectOperate (5), DirectOperateNoAck (6), Freeze (7), FreezeNoAck (8),
  FreezeClear (9), FreezeClearNoAck (10), Response (0x81), UnsolicitedResponse
  (0x82), plus `Unknown(u8)`.
- `Dnp3Encoder`: wire bytes from a frame, with round-trip tests against the parser.
- Connection-health validation via `StateValidator` (`Dnp3Validator`) checking
  connection, a valid source address (not `0` / not `0xFFFF`), and a consecutive-error
  threshold.
- TUP normalisation via `DataNormalizer` (`Dnp3Normalizer`). A frame with no user
  data (a bare request) returns a normalize error, because it carries no measurement.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests (`mod fuzz`) asserting `FrameParser::parse` never panics on
  arbitrary input. Structured generators build well-formed frames with per-block CRCs.

## Architecture

`Dnp3Parser::parse` works upward through the layers:

- **Data link** (10 bytes): `start` must be `0x0564`; `length` counts the bytes
  after it, and the frame must hold the full `length + 2` bytes. The header CRC at
  bytes 8–9 covers bytes 0–7. Parsing never indexes past a truncated buffer.
- **User data**: 16-byte blocks, each followed by its CRC, until `length` is
  consumed. `try_parse` reports `Ok(None)` for an incomplete frame and the exact
  consumed length otherwise.
- **Transport** (first user byte): `transport_first`, `transport_final`,
  `transport_seq`.
- **Application** (second user byte, first fragment only): `application_control`
  and `app_function`; the rest is `payload`.

The decoded result is the `Dnp3Frame` struct (`start`, `length`, `control`,
`destination`, `source`, `data_link_crc`, `transport_first`, `transport_final`,
`transport_seq`, `application_control`, `app_function`, `payload`).

`Dnp3Validator` consumes a `Dnp3State` (`connected`, `source_address`,
`destination_address`, `last_ok_timestamp`, `error_count`) and rejects the state when
disconnected, `source_address` is `0` or `0xFFFF`, or more than 10 consecutive errors
occur.

`Dnp3Normalizer` produces a `DataPoint` keyed by `device_id = "dnp3:{source}"` and
`point_id = "func:{app_function}"`. The value is the payload as bytes. A frame with no
payload returns an error.

## Usage

A complete, runnable example parses a Response frame and normalizes it:

```sh
cargo run -p tpt-protocol-dnp3 --example parse_dnp3
```

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_dnp3::{Dnp3Normalizer, Dnp3Parser};

// `wire` holds one complete data-link frame, with its block CRCs.
let frame = Dnp3Parser::new().parse(&wire)?;
let point = Dnp3Normalizer::new().normalize(&frame)?;
// point.device_id == "dnp3:1", point.point_id == "func:Response"
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — the implementation: `Dnp3FunctionCode` enum, `Dnp3Frame` struct,
  `crc16_dnp` helper, `Dnp3Parser` (`FrameParser`), `Dnp3Encoder` (`FrameEncoder`),
  `Dnp3State` + `Dnp3Validator` (`StateValidator`), `Dnp3Normalizer`
  (`DataNormalizer`), unit tests (including the truncation and block-CRC regression
  tests), and the `proptest` `mod fuzz`.
- `examples/parse_dnp3.rs` — runnable parse-and-normalize example.

## Status / Conformance

- **Single transport segment only.** The transport flags and sequence number are
  parsed, but application fragments spanning several transport segments are not
  reassembled. Only the first segment's application function and payload are
  extracted.
- **No application-object decoding.** Internal-indication, binary-input and
  analog-input objects (group/variation) are not decoded; the payload is carried as
  raw bytes.
- **Block CRCs are checked; the payload is not interpreted.** A block CRC mismatch is
  a checksum error. Object headers inside the payload are left to the caller.
- Normalised `timestamp` is `0`; callers must supply a real timestamp.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` /
  `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.
- [`tpt-protocol-modbus`](../tpt-protocol-modbus) — another polling/SCADA-style
  protocol in the suite.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
