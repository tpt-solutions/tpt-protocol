# tpt-protocol-bacnet

BACnet building-automation and control-network protocol parser, validator, and TUP
normalizer for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol)
suite.

## Overview

BACnet (ISO 16484-5) is the dominant open protocol for HVAC, lighting, and access
control in commercial buildings. TPT implements the BACnet/IP encapsulation (BVLC),
the NPDU network layer and the APDU application layer, so captured UDP datagrams can
be validated for device health and normalized into unified data points keyed by
object identifier.

## Features

- BACnet/IP BVLC parsing: BVLC type `0x81`, the BVLC function (`BvlcFunction`), and
  a 16-bit big-endian length that must equal the frame length.
- NPDU parsing: version, the DNET/DLEN/SNET/SLEN address fields, hop count and
  network-layer messages. Forwarded-NPDU skips its 6-byte original address.
- APDU parsing (`ApduType`): ConfirmedRequest, UnconfirmedRequest, SimpleAck,
  ComplexAck, Error, Reject, Abort, plus `Unknown(u8)`. Invoke ID and service
  choice are exposed.
- Object identifiers decoded from the packed 10-bit type and 22-bit instance form,
  and the ReadProperty object and property ID.
- ReadProperty Complex-ACK values decoded as `BacnetValue` (null, boolean, unsigned,
  signed, REAL, DOUBLE, enumerated, or the raw octets of any other application tag).
- Connection-health validation via `StateValidator` (`BacnetValidator`) checking
  connection, a valid device-instance range (≤ 4,194,303), and a consecutive-error
  threshold.
- TUP normalisation via `DataNormalizer` (`BacnetNormalizer`). Requests and unknown
  services return a normalize error, because they carry no measured value.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests (`mod fuzz`) asserting `FrameParser::parse` never panics on
  arbitrary input.

## Architecture

`BacnetParser::parse` decodes three layers:

- **BVLC** (4 bytes): `bvlc_type` must equal `0x81`; `bvlc_function` is decoded by
  `BvlcFunction` from the Annex J codes; `bvlc_length` must equal the frame length.
- **NPDU**: version, control octet, optional DNET/SNET addresses, hop count, and the
  network-layer message type when the control octet sets it.
- **APDU**: the type comes from the high nibble of the first APDU byte. Confirmed
  and unconfirmed requests, complex ACKs and errors carry their service choice; for
  ReadProperty the object identifier and property ID are read and the application
  value is decoded.

The decoded result is the `BacnetFrame` struct (`bvlc_type`, `bvlc_function`,
`bvlc_length`, `apdu_type`, `invoke_id`, `service_choice`, `object`,
`property_id`, `value`, `payload`).

`BacnetValidator` consumes a `BacnetState` (`connected`, `device_instance`,
`last_ok_timestamp`, `error_count`) and rejects the state when disconnected, the
`device_instance` exceeds 4,194,303, or more than 10 consecutive errors occur.

`BacnetNormalizer` produces a `DataPoint` keyed by
`device_id = "bacnet:bvlc:{function}"` and `point_id = "obj:{type}:{instance}"` when
an object is present, otherwise `"apdu:{apdu_type}"`. A ReadProperty answer's
decoded value is the measurement.

## Usage

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_bacnet::{BacnetNormalizer, BacnetParser};

// `wire` holds one BVLC + NPDU + ComplexAck (ReadProperty) datagram.
let parsed = BacnetParser::new().parse(&wire)?;
let point = BacnetNormalizer::new().normalize(&parsed)?;
// point.point_id == "obj:0:1" for an Analog Input, instance 1
```

A complete, runnable version (it builds the datagram itself) is in
[`examples/parse_bacnet.rs`](examples/parse_bacnet.rs):

```sh
cargo run -p tpt-protocol-bacnet --example parse_bacnet
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`,
`String`, `format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — the implementation: `BvlcFunction` / `ApduType` enums,
  `BacnetObjectId`, `BacnetValue`, `BacnetFrame`, `BacnetParser` (`FrameParser`),
  `BacnetState` + `BacnetValidator` (`StateValidator`), `BacnetNormalizer`
  (`DataNormalizer`), unit tests, and the `proptest` `mod fuzz`.
- `examples/parse_bacnet.rs` — runnable parse-and-normalize example.

## Status / Conformance

- **Layouts follow ASHRAE 135 Annex J and the NPDU/APDU encoding.** Tests use
  hand-built frames from the standard's layouts. They are not checked against a
  captured BACnet device.
- **ReadProperty values only.** Other services' parameters are carried as raw
  `payload` bytes. Segmented messages are not reassembled.
- **No CRC / integrity check.** BACnet/IP has no link CRC at this layer; the BVLC
  type and length are structurally validated.
- Normalised `timestamp` is `0`; callers must supply a real timestamp.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` /
  `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
