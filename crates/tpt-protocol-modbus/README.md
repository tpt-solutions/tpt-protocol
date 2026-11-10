# tpt-protocol-modbus

Modbus RTU and Modbus TCP frame parsing, validation, and TUP normalisation
for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

## Overview

Modbus is the de-facto serial/ethernet fieldbus for industrial automation
(PLCs, RTUs, drives, meters). TPT implements a memory-safe `#![no_std]` +
`alloc` parser covering the most common Modbus function codes so that
Modbus traffic can be decoded, connection health validated, and normalized
into the shared TUP telemetry envelope without pulling in a `std`/tokio stack.

## Features

- Modbus RTU and Modbus TCP (MBAP) frame parsing via a single `FrameParser`.
- Modbus CRC-16 (CRC-16/Modbus, poly `0xA001`) verification on RTU frames.
- Connection health validation via the `StateValidator` trait.
- TUP normalisation via the `DataNormalizer` trait, mapping frames to `tup::DataPoint`s.
- `#![no_std]` + `alloc` (no `std` dependency, runs on alloc-capable embedded targets).
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
- Wire-controlled register counts are capped (`MAX_MODBUS_REGISTERS = 1024`) to
  prevent unbounded allocation.

## Architecture

`ModbusParser` detects the transport by inspecting the protocol identifier:
if `bytes[2..4] == 0x0000` *and* the MBAP length field fits the buffer, the
frame is treated as **Modbus TCP** (MBAP header: transaction id, protocol id,
length, unit id); otherwise it is parsed as **Modbus RTU**. TCP frames are
normalised into the same `ModbusRtuFrame` representation used for RTU so the
rest of the pipeline is transport-agnostic.

The decoded PDU is modelled by `ModbusFrame`, one variant per function code:

| FC | Name                     | Variant |
|----|--------------------------|---------|
| 01 | Read Coils               | `ReadCoils` |
| 02 | Read Discrete Inputs     | `ReadDiscreteInputs` |
| 03 | Read Holding Registers   | `ReadHoldingRegisters` |
| 04 | Read Input Registers     | `ReadInputRegisters` |
| 05 | Write Single Coil        | `WriteSingleCoil` |
| 06 | Write Single Register    | `WriteSingleRegister` |
| 07 | Read Exception Status    | `ReadExceptionStatus` |
| 08 | Diagnostics              | `Diagnostics` |
| 16 | Write Multiple Registers | `WriteMultipleRegisters` |

Unrecognised function codes are captured as `ModbusFrame::Raw`.

`ModbusValidator` consumes a `ModbusState` (connected flag, `last_ok_timestamp`,
`error_count`, `unit_id`) and rejects disconnected devices or devices that have
exceeded 10 consecutive errors. `ModbusNormalizer` maps a `ModbusRtuFrame` to a
`DataPoint`, keyed by `device_id = "modbus:<slave_address>"` and a
`point_id` like `holding_register:<address>` / `coil:<address>` /
`raw:0x<fc>`. Unknown function codes are normalised with `Quality::Uncertain`.

## Usage

```rust
use tpt_protocol_core::{FrameParser, DataNormalizer};
use tpt_protocol_modbus::{ModbusParser, ModbusNormalizer};

fn main() {
    // RTU request: slave=1, FC=3, addr=0, qty=10
    let raw: [u8; 8] = [
        0x01, 0x03, 0x00, 0x00, 0x00, 0x0A,
        0xC5, 0xCD,  // CRC-16/Modbus
    ];

    let parser = ModbusParser::new();
    let frame = parser.parse(&raw).unwrap();
    assert_eq!(frame.slave_address, 0x01);

    let normalizer = ModbusNormalizer::new();
    let datapoint = normalizer.normalize(&frame).unwrap();

    assert_eq!(datapoint.source_protocol, tpt_protocol_tup::SourceProtocol::Modbus);
    assert_eq!(datapoint.device_id, "modbus:1");
    assert_eq!(datapoint.point_id, "holding_register:0");
}
```

A runnable example is also provided at
[`examples/parse_frame.rs`](examples/parse_frame.rs).

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data
(`Vec`, `String`, `format!`). It works in embedded environments that
provide an allocator.

## Crate layout

- `src/lib.rs` — CRC-16/Modbus, `ModbusFrame`/`ModbusRtuFrame`/`ModbusTcpFrame`,
  `ModbusParser` (RTU + TCP detection/parsing), `ModbusValidator`/`ModbusState`,
  `ModbusNormalizer`, unit/fuzz tests.
- `examples/parse_frame.rs` — runnable round-trip example.

## Status / Conformance

- **Implemented:** RTU + TCP (MBAP) parsing for function codes 0x01–0x08 and
  0x10, CRC-16/Modbus validation, state validation, TUP normalisation,
  `proptest` fuzzing, and bounded allocation for wire-controlled register counts.
- **Known gaps / TODOs:**
  - `ModbusNormalizer` emits a *single* `DataPoint` for `WriteMultipleRegisters`,
    reporting only the first register value (the source comment notes full
    multi-register handling would require a batch API).
  - Normalised `timestamp` is `0`; callers must supply a real timestamp.
  - Exception/error responses (Modbus exception PDUs, function code `0x80 | fc`)
    are not specially recognised — they surface as `ModbusFrame::Raw`.
  - This is a structural PDU parser, not a Modbus *application* stack: it does
    not model coils/discrete-input/register *values* from read responses (only
    the request addressing), nor does it build/send frames.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
