# tpt-protocol-knx

KNX building-automation telegram parser, validator, and TUP normalizer for the
[TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

## Overview

KNX (ISO/IEC 14543-3) is the open standard for lighting, blinds, heating and other
building services on a twisted-pair or IP network. Devices exchange group telegrams
that address a group of devices (the group address) and carry a datapoint value. TPT
implements the cEMI `L_Data` frame, the group addressing, and the explicit datapoint
type (DPT) decoders for the common types, in a `#![no_std]` + `alloc` crate.

## Features

- cEMI `L_Data` frame parsing: message code (request, confirmation, indication),
  additional-information length, control fields (priority and hop count), source
  individual address, destination (group or individual), NPDU length and the APCI
  service.
- Group addresses in the 3-level `main/middle/sub` form, and individual addresses in
  the `area.line.device` form (`IndividualAddress`).
- Service types (`KnxServiceType`): GroupValueRead, GroupValueResponse,
  GroupValueWrite, and `Other(u8)`.
- Explicit datapoint decoding, since a bus frame does not say which type a payload
  has: `decode_dpt1` (boolean), `decode_dpt5_percent` (8-bit scaling) and `decode_dpt9`
  (2-byte float, with the `0x7FFF` invalid marker).
- `KnxEncoder`: wire bytes from a frame, with round-trip tests against the parser.
- Connection-state validation via `StateValidator` (`KnxValidator`).
- TUP normalisation via `DataNormalizer` (`KnxNormalizer`). The source individual
  address is the device and the destination group address is the point.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
  Structured generators build well-formed telegrams.

## Architecture

`KnxParser` reads the cEMI `L_Data` layout:

- Message code, additional-info length and additional info, then the two control
  octets (priority, hop count, address type).
- Source individual address (2 bytes) and destination (2 bytes). The destination is a
  group address when the address-type bit is set.
- NPDU length, then the APCI service. A one-octet NPDU carries its six-bit value in the
  APCI field (`DptValue::Short`). Longer NPDUs carry their data octets
  (`DptValue::Data`).

The decoded result is the `KnxFrame` struct (`message_code`, `priority`, `hop_count`,
`source`, `destination`, `destination_is_group`, `service_type`, `dpt`).

`KnxValidator` consumes a `KnxConnectionState` and rejects the state when the device
is not connected or the sequence is out of range.

`KnxNormalizer` produces `device_id = "knx:<source individual address>"` (for example
`knx:1.1.1`) and `point_id = "<main>/<middle>/<sub>"` (for example `1/0/1`). The value
is the decoded datapoint. A telegram with no data returns a normalize error.

## Usage

A complete, runnable example parses a group write and normalizes it:

```sh
cargo run -p tpt-protocol-knx --example parse_knx
```

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_knx::{KnxNormalizer, KnxParser};

let frame = KnxParser.parse(&wire)?;
let point = KnxNormalizer.normalize(&frame)?;
// point.device_id == "knx:1.1.1", point.point_id == "1/0/1"
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`, `String`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — `MessageCode`, `KnxServiceType`, `GroupAddress`, `IndividualAddress`,
  `DptValue`, `KnxFrame`, `KnxParser` (`FrameParser`), `KnxEncoder` (`FrameEncoder`),
  `KnxConnectionState` + `KnxValidator` (`StateValidator`), `KnxNormalizer`
  (`DataNormalizer`), the DPT decoders, unit tests, and the `proptest` `mod fuzz`.
- `examples/parse_knx.rs` — runnable parse-and-normalize example.

## Status / Conformance

- **cEMI `L_Data` only.** Other cEMI message types (for example property services and
  management frames) are not decoded.
- **No KNXnet/IP or TP-UART transport.** The crate parses a captured telegram; it does
  not open a tunnel or talk to an interface.
- **Datapoint types are explicit.** Only DPT 1, 5 (percent) and 9 are decoded. Other
  DPTs are carried as raw data octets.
- **No secure-mode (KNX Data Secure) decryption.** Encrypted payloads are carried as
  bytes.
- Normalised `timestamp` is `0`; callers must supply a real timestamp.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `FrameEncoder` /
  `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
