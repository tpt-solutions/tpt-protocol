# tpt-protocol-doip-someip

DoIP (ISO 13400) and SOME/IP automotive Ethernet protocol parsing, validation, and
TUP normalisation for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

This crate covers two companion automotive Ethernet protocols used in modern
vehicle diagnostics and service-oriented architectures (SOA): **DoIP** for
diagnostic transport over TCP/UDP, and **SOME/IP** for RPC-style service
communication. TPT unifies both behind a single combined frame/state/normalizer
chain so the daemon can treat an automotive Ethernet link as one protocol source.

## Overview

DoIP (Diagnostics over IP, ISO 13400) carries UDS diagnostics over TCP/UDP and
establishes a logical diagnostic session via routing activation. SOME/IP
(Scalable service-Oriented MiddlewarE over IP, AUTOSAR) is the service-RPC
layer that lets ECUs call methods, fire events, and exchange fields. Both are
ubiquitous in automotive Ethernet backbones, and TPT implements wire-level
header decoding plus connection-health validation so diagnostics and SOA
traffic can be ingested into the unified TUP telemetry pipeline.

## Features

- DoIP header parsing: protocol version, inverse version, payload type, payload
  length, and payload bytes.
- SOME/IP header parsing: service ID, method ID, length, client ID, session ID,
  protocol version, interface version, message type, return code, and payload.
- Auto-detection of DoIP vs SOME/IP: first byte `0x02` selects DoIP; otherwise
  frames of at least 16 bytes are parsed as SOME/IP.
- Connection-state validation via `StateValidator` (`DoipSomeipValidator`).
- TUP normalisation via `DataNormalizer` (`DoipSomeipNormalizer`).
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary
  input.

## Architecture

The crate exposes one combined parser, validator, and normalizer over a two-variant
frame enum:

- `DoipSomeipFrame` — `Doip(DoipFrame)` | `Someip(SomeipFrame)`.
- `DoipSomeipParser` implements `FrameParser`. Detection logic (in
  `DoipSomeipParser::parse`):
  - Empty input → `Error::Parse`.
  - `bytes[0] == 0x02` → `parse_doip`: decode protocol_version(1), inverse_version(1),
    payload_type(2, big-endian), payload_length(4, big-endian), then `payload`. The
    inverse version must equal `!protocol_version` or parsing fails. A `checked_add`
    guards the payload-end offset so a `u32::MAX` length cannot overflow the slice
    range (see `doip_huge_payload_length_no_overflow` test).
  - `bytes.len() >= 16` → `parse_someip`: decode the 16-byte header described above.
  - Otherwise → `Error::Parse` ("too short for SOME-IP").
- `DoipPayloadType` maps the wire `u16` to `GenericDoipHeaderNegativeAck` (0x0000),
  `VehicleIdentificationRequest` (0x0001), `VehicleIdentificationResponse` (0x0002),
  `RoutingActivationRequest` (0x0003), `RoutingActivationResponse` (0x0004),
  `AliveCheckRequest` (0x0005), `AliveCheckResponse` (0x0006), `DiagnosticMessage`
  (0x0007), `DiagnosticMessagePositiveAck` (0x0008), `DiagnosticMessageNegativeAck`
  (0x0009), or `Unknown(u16)` for anything else.
- `SomeipMessageType` maps the wire `u8` to `Request` (0x00), `RequestNoReturn` (0x01),
  `Notification` (0x02), `Response` (0x80), `Error` (0x81), or `Unknown(u8)`.
- `DoipSomeipValidator` validates `DoipSomeipState` (fields: `connected`,
  `tcp_connected`, `routing_active`, `last_ok_timestamp`, `error_count`); it errors
  unless TCP is connected, routing is activated, and `error_count <= 10`.
- `DoipSomeipNormalizer` maps DoIP frames to `device_id = "doip:gateway"`,
  `point_id = "doip:<payload_type>"`, value `Int(payload_length)`; SOME/IP frames to
  `device_id = "someip:0x<service_id>"`, `point_id = "method:0x<method_id>"`, value
  `Bytes(payload)` when payload is non-empty else `Int(return_code)`.

## Usage

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_tup::SourceProtocol;
use tpt_protocol_doip_someip::{DoipSomeipNormalizer, DoipSomeipParser};

fn main() {
    // DoIP VehicleIdentificationRequest: ver=0x02, inv=0xFD, type=0x0001, len=0
    let bytes: [u8; 8] = [0x02, 0xFD, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00];

    let parser = DoipSomeipParser::new();
    let frame = parser.parse(&bytes).unwrap();

    let normalizer = DoipSomeipNormalizer::new();
    let dp = normalizer.normalize(&frame).unwrap();

    assert_eq!(dp.source_protocol, SourceProtocol::DoipSomeip);
    assert_eq!(dp.device_id, "doip:gateway");
    assert_eq!(dp.point_id, "doip:VehIdentReq");
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`,
`String`, `format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — the entire crate: DoIP/SOME/IP frame types, the auto-detecting
  `DoipSomeipParser`, the `DoipSomeipValidator`, the `DoipSomeipNormalizer`, and
  unit + `proptest` fuzz tests.

## Status / Conformance

- Implemented: DoIP header decoding with inverse-version check, SOME/IP header
  decoding, auto-detection, connection/state validation, and TUP normalisation.
- Known gaps / TODOs:
  - Payload *semantics* are not decoded — the SOME/IP service/method payload and the
    DoIP diagnostic payload are passed through as raw bytes/`Int` length; no SD
    (Service Discovery), transformation, or event-group handling.
  - No TCP/UDP transport layer (socket, routing-activation handshake state machine,
    alive-check timers) — the daemon is simulation-only (see workspace `TODO.md`,
    "Real transport clients" deferred).
  - The DoIP payload-length overflow guard caps allocation to the available bytes
    rather than validating against a negotiated buffer; this is a hardening measure,
    not full DoIP conformance.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.
- [`tpt-protocol-j1939`](../tpt-protocol-j1939) — another automotive protocol crate in the suite.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
