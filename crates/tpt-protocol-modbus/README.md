# tpt-protocol-modbus

Modbus RTU and Modbus TCP frame parsing, validation, and TUP normalisation
for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

## Overview

Modbus is the de-facto serial/ethernet fieldbus for industrial automation
(PLCs, RTUs, drives, meters). TPT implements a memory-safe `#![no_std]` +
`alloc` parser covering the common Modbus function codes so that Modbus traffic
can be decoded, connection health validated, and normalized into the shared TUP
telemetry envelope without pulling in a `std`/tokio stack.

## Features

- Modbus RTU and Modbus TCP (MBAP) parsing, for requests and for responses
  (`ModbusParser::new()` for requests, `ModbusParser::response()` for responses).
- Modbus CRC-16 (CRC-16/Modbus, poly `0xA001`) verification on RTU frames. A frame
  with a valid CRC is never read as TCP.
- Streaming parse via `try_parse`: `Ok(None)` for an incomplete frame, otherwise the
  exact consumed length. RTU has no length field, so the shortest prefix with a valid
  CRC is taken.
- Strict request checks: ranges, `0xFF00`/`0x0000` for single-coil writes, and the
  byte count of multiple-register writes. Exception responses (`fc | 0x80`) are
  parsed as their own variant.
- `ModbusEncoder` (RTU) and `ModbusTcpEncoder` (MBAP, for master-side requests), with
  round-trip tests against the parser.
- Connection health validation via the `StateValidator` trait.
- TUP normalisation via the `DataNormalizer` trait, mapping frames to `tup::DataPoint`s.
- `#![no_std]` + `alloc` (no `std` dependency, runs on alloc-capable embedded targets).
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input,
  plus structured MBAP generators (valid header, skewed length, mutation).
- Wire-controlled register counts are capped (`MAX_MODBUS_REGISTERS = 1024`) to
  prevent unbounded allocation.

## Architecture

`ModbusParser` detects the transport: if `bytes[2..4] == 0x0000` and the MBAP length
field matches the buffer, the frame is **Modbus TCP** (transaction id, protocol id,
length, unit id); otherwise it is **Modbus RTU**. TCP frames are normalised into the
same `ModbusRtuFrame` representation used for RTU so the rest of the pipeline is
transport-agnostic.

The decoded PDU is modelled by `ModbusFrame`:

| FC | Name                     | Variant |
|----|--------------------------|---------|
| 01 | Read Coils               | `ReadCoils` (request), `ReadResponse` (response) |
| 02 | Read Discrete Inputs     | `ReadDiscreteInputs`, `ReadResponse` |
| 03 | Read Holding Registers   | `ReadHoldingRegisters`, `ReadResponse` |
| 04 | Read Input Registers     | `ReadInputRegisters`, `ReadResponse` |
| 05 | Write Single Coil        | `WriteSingleCoil` (request and echo) |
| 06 | Write Single Register    | `WriteSingleRegister` (request and echo) |
| 07 | Read Exception Status    | `ReadExceptionStatus` |
| 08 | Diagnostics              | `Diagnostics` |
| 16 | Write Multiple Registers | `WriteMultipleRegisters`, `WriteMultipleResponse` |

Any response with bit 7 of the function code set is `Exception { function_code,
exception_code }`. Unrecognised function codes are captured as `ModbusFrame::Raw`.

`ModbusValidator` consumes a `ModbusState` (connected flag, `last_ok_timestamp`,
`error_count`, `unit_id`) and rejects disconnected devices or devices that have
exceeded 10 consecutive errors.

`ModbusNormalizer` maps a frame to a `DataPoint` keyed by `device_id =
"modbus:<slave_address>"`:

- **Read responses** give the first coil, bit or register of the response, named by
  its index in the response (`holding_register[0]`). A response does not carry the
  start address, so the point id cannot include it.
- **Write requests and echoes** give the written coil or register at its address
  (`coil:<address>`, `holding_register:<address>`).
- **Read requests, exceptions and write-multiple responses** return a normalize error,
  because they carry no measured value.
- **Unknown function codes** normalise with `Quality::Uncertain`.

## Usage

A complete, runnable example parses an RTU request and a response and normalizes the
response:

```sh
cargo run -p tpt-protocol-modbus --example parse_frame
```

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_modbus::{ModbusNormalizer, ModbusParser};

// `wire` holds one complete RTU or TCP response (CRC included for RTU).
let frame = ModbusParser::response().parse(&wire)?;
let point = ModbusNormalizer::new().normalize(&frame)?;
// e.g. point.device_id == "modbus:1", point.point_id == "holding_register[0]"
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data
(`Vec`, `String`, `format!`). It works in embedded environments that provide an
allocator.

## Crate layout

- `src/lib.rs` — CRC-16/Modbus, `ModbusFrame` / `ModbusRtuFrame` / `ModbusTcpFrame`,
  `ModbusParser` (RTU + TCP, requests and responses), `ModbusEncoder` /
  `ModbusTcpEncoder`, `ModbusValidator` / `ModbusState`, `ModbusNormalizer`, unit,
  fuzz and structured tests.
- `examples/parse_frame.rs` — runnable parse, validate and normalize example.
- `examples/diff_decode.rs` — used by the differential test against pymodbus in CI.
- `benches/parse.rs` — criterion benchmark (compiled in CI, not run).

## Status / Conformance

- **Implemented:** RTU and TCP parsing and encoding for function codes 0x01–0x08 and
  0x10, exception responses, CRC-16/Modbus validation, state validation, TUP
  normalisation, `proptest` fuzzing, and bounded allocation for wire-controlled counts.
- **Differential test.** `scripts/differential/modbus_pymodbus.py` checks encoding and
  decoding against pymodbus 3.6.9 in CI, and a pcap replay test uses a real loopback
  capture.
- **Known gaps:**
  - `WriteMultipleRegisters` normalizes only its first register value. `normalize_all`
    does not fan it out (the default returns that one point).
  - Read responses normalize only their first value (see Architecture). A batch API
    for whole responses is not provided.
  - Normalised `timestamp` is `0`; callers must supply a real timestamp.
  - This is a PDU and frame codec, not a Modbus application stack: it does not manage
    connections, retries or polling. The daemon's `--modbus-poll` does polling.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `FrameEncoder` /
  `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
