# tpt-protocol-bacnet

BACnet building-automation and control-network protocol parser, validator, and TUP
normalizer for the [TPT Protocol](https://github.com/tpt-protocol/tpt-protocol)
suite.

## Overview

BACnet (ISO 16484-5) is the dominant open protocol for HVAC, lighting, and access
control in commercial buildings. TPT implements the BACnet/IP encapsulation (BVLC)
plus the APDU layer so captured UDP datagrams can be validated for device health and
normalized into unified data points keyed by BVLC function and object identifier.

## Features

- BACnet/IP BVLC parsing: BVLC type `0x81`, BVLC function code, and 16-bit BE BVLC
  length (`BvlcFunction`).
- APDU type decoding from the high nibble of the first APDU byte (`ApduType`):
  ConfirmedRequest (0), UnconfirmedRequest (1), SimpleAck (2), ComplexAck (3),
  Error (5), Reject (6), Abort (7) — plus `Unknown(u8)`.
- Object-identifier and property-ID extraction when the APDU carries them
  (`BacnetObjectId`: `object_type` (2 bytes) + `instance` (4 bytes); optional
  `property_id`).
- Connection-health validation via `StateValidator` (`BacnetValidator`) checking
  connection, a valid device-instance range (≤ 4,194,303), and a consecutive-error
  threshold.
- TUP normalisation via `DataNormalizer` (`BacnetNormalizer`) mapping each frame to
  a `tup::DataPoint`.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests (`mod fuzz`) asserting `FrameParser::parse` never panics on
  arbitrary input.

## Architecture

`BacnetParser::parse` processes the frame in two layers:

- **BVLC header** (4 bytes, `src/lib.rs`): `bvlc_type` must equal `0x81`
  (BACnet/IP); `bvlc_function` is decoded by `BvlcFunction::from_u8`
  (OriginalUnicastNpdu `0x04`, OriginalBroadcastNpdu `0x05`, AddressResolutionRequest
  `0x07`, AddressResolutionResponse `0x08`, VirtualLayerResolution `0x0A`,
  ForwardedNpdu `0x09`); `bvlc_length` is read big-endian.
- **APDU** (starts at byte 4): the APDU type comes from the high nibble of the first
  APDU byte via `ApduType::from_nibble`. When ≥ 7 APDU bytes remain, the parser
  decodes an object identifier at offsets 1–6 (`object_type` 2 bytes BE + `instance`
  4 bytes BE) and — when ≥ 9 bytes remain — a `property_id` (2 bytes BE) at offsets
  7–8; the remainder becomes the frame `payload`.

The decoded result is the `BacnetFrame` struct (`bvlc_type`, `bvlc_function`,
`bvlc_length`, `apdu_type`, `object: Option<BacnetObjectId>`, `property_id:
Option<u16>`, `payload`).

`BacnetValidator` consumes a `BacnetState` (`connected`, `device_instance`,
`last_ok_timestamp`, `error_count`) and rejects the state when disconnected, the
`device_instance` exceeds 4,194,303, or more than 10 consecutive errors occur.

`BacnetNormalizer` produces a `DataPoint` keyed by
`device_id = "bacnet:bvlc:{function}"` and `point_id = "obj:{type}:{instance}"`
when an object is present, otherwise `"apdu:{apdu_type}"`; the value is the raw
payload bytes, or `0` when the payload is empty.

## Usage

```rust
use tpt_protocol_core::{FrameParser, DataNormalizer};
use tpt_protocol_bacnet::{BacnetParser, BacnetNormalizer};

fn main() {
    // BVLC (0x81, func=0x04 UnicastNPDU) + APDU (ConfirmedReq 0x00) + object (type=1, inst=42)
    let mut raw: Vec<u8> = vec![0x81, 0x04];
    let mut body: Vec<u8> = vec![0x00];                 // APDU: ConfirmedRequest
    body.extend_from_slice(&1u16.to_be_bytes());        // object_type
    body.extend_from_slice(&42u32.to_be_bytes());       // instance
    let len = (4 + body.len()) as u16;
    raw.extend_from_slice(&len.to_be_bytes());
    raw.extend_from_slice(&body);

    let parser = BacnetParser::new();
    let frame = parser.parse(&raw).unwrap();
    assert_eq!(frame.bvlc_type, 0x81);
    assert_eq!(frame.object.as_ref().unwrap().instance, 42);

    let normalizer = BacnetNormalizer::new();
    let dp = normalizer.normalize(&frame).unwrap();
    assert_eq!(dp.source_protocol, tpt_protocol_tup::SourceProtocol::Bacnet);
    assert_eq!(dp.point_id, "obj:1:42");
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — the full implementation: `BvlcFunction` / `ApduType` enums,
  `BacnetObjectId` struct, `BacnetFrame` struct, `BacnetParser` (`FrameParser`),
  `BacnetState` + `BacnetValidator` (`StateValidator`), `BacnetNormalizer`
  (`DataNormalizer`), unit tests, and the `proptest` `mod fuzz`.

## Status / Conformance

- **Encoding only (no BER decoding).** BACnet APDU services use ASN.1 BER for
  parameters and property values. This crate identifies the APDU type and extracts
  the object identifier / property ID at fixed offsets, but does **not** fully
  decode BER-encoded service parameters or property values — the unparsed remainder
  is carried as raw bytes.
- **No NPDU addressing.** The BACnet NPDU layer (control octet, DNET/SADR/DA
  addressing) is not parsed; parsing starts at the APDU.
- **No CRC / integrity check.** BACnet/IP has no link CRC at this layer; only the
  BVLC type is structural-validated.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` /
  `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
