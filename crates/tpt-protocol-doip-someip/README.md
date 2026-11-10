# tpt-protocol-doip-someip

DoIP (ISO 13400-2) and SOME/IP automotive-Ethernet frame parser, validator, and TUP
normalizer for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

## Overview

DoIP (Diagnostics over IP) carries UDS diagnostic requests from a tester to a vehicle
gateway over Ethernet. SOME/IP is the service-oriented middleware that vehicle ECUs use
to exchange methods, events and fields. TPT implements both headers in one
`#![no_std]` + `alloc` crate, so a captured datagram can be recognised as either and
normalized into the shared TUP envelope.

## Features

- DoIP header parsing: protocol version, inverse version, payload type, payload length
  and payload. The payload-type table follows ISO 13400-2. The inverse version must be
  the bitwise inverse of the version, and the version must be `0x01`, `0x02`, `0x03`
  or `0xFF`.
- Truncated and trailing DoIP payloads are errors, not empty payloads.
- SOME/IP header parsing: service ID, method ID, length, client ID, session ID,
  protocol version, interface version, message type and return code. The length must
  equal the frame length.
- Automatic detection: a DoIP version and its inverse select DoIP. Otherwise the
  frame is parsed as SOME/IP, and frames shorter than the 16-byte SOME/IP header are
  rejected.
- `DoipSomeipEncoder`: wire bytes for both frame kinds, with round-trip tests.
- Connection-state validation via `StateValidator` (`DoipSomeipValidator`).
- TUP normalisation via `DataNormalizer` (`DoipSomeipNormalizer`). A header-only message
  (for example a vehicle-identification request) is an error, because it carries no
  measured value.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
  Structured generators build valid DoIP and SOME/IP frames, then mutate them.

## Architecture

`DoipSomeipParser::parse` detects the frame kind and returns a `DoipSomeipFrame`, which
is either `Doip(DoipFrame)` or `Someip(SomeipFrame)`.

- **DoIP**: `protocol_version`, `inverse_version`, `payload_type` (`DoipPayloadType`,
  a big-endian `u16`), `payload_length` (a big-endian `u32`), then `payload`. A
  `checked_add` guards the payload end, so a huge length cannot overflow the slice.
- **SOME/IP**: the 16-byte header, then the payload. Message types are `SomeipMessageType`.

`DoipPayloadType` names the ISO 13400-2 payload types, including the diagnostic
message types (`DiagnosticMessage`, `DiagnosticMessagePositiveAck`,
`DiagnosticMessageNegativeAck`), routing activation, alive check and vehicle
identification. Unknown values are `Unknown(u16)`.

`DoipSomeipValidator` consumes a `DoipSomeipState` (`connected`, `tcp_connected`,
`routing_active`, `last_ok_timestamp`, `error_count`). It rejects the state unless TCP
is connected, routing is active, and the error count is at most 10.

`DoipSomeipNormalizer` produces:

- DoIP: `device_id = "doip:gateway"`, `point_id = "doip:<payload type>"` (for example
  `doip:DiagMsg`), and the payload as bytes.
- SOME/IP: `device_id = "someip:0x<service id>"`, `point_id = "method:0x<method id>"`,
  and the payload as bytes. With no payload, the return code.

## Usage

A complete, runnable example parses a diagnostic message and normalizes it:

```sh
cargo run -p tpt-protocol-doip-someip --example parse_doip_someip
```

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_doip_someip::{DoipSomeipNormalizer, DoipSomeipParser};

let frame = DoipSomeipParser::new().parse(&wire)?;
let point = DoipSomeipNormalizer::new().normalize(&frame)?;
// point.device_id == "doip:gateway", point.point_id == "doip:DiagMsg"
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`, `String`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — `DoipPayloadType`, `DoipFrame`, `SomeipMessageType`, `SomeipFrame`,
  `DoipSomeipFrame`, `DoipSomeipParser` (`FrameParser`), `DoipSomeipEncoder`
  (`FrameEncoder`), `DoipSomeipState` + `DoipSomeipValidator` (`StateValidator`),
  `DoipSomeipNormalizer` (`DataNormalizer`), unit tests, and the `proptest` `mod fuzz`.
- `examples/parse_doip_someip.rs` — runnable parse-and-normalize example.

## Status / Conformance

- **Payload semantics are not decoded.** The UDS message inside a DoIP diagnostic
  message, and the SOME/IP method payload, are carried as bytes. Service Discovery,
  transformation and event-group handling are not implemented.
- **No transport.** The crate does not run the TCP routing-activation handshake or the
  alive-check timers. The daemon's transports are the callers that deliver bytes.
- **Not full DoIP conformance.** The payload-length guard caps allocation to the bytes
  present. It is a hardening measure, not a negotiated buffer limit.
- Normalised `timestamp` is `0`; callers must supply a real timestamp.

## Related crates

- [`tpt-protocol-uds`](../tpt-protocol-uds) — the UDS service layer that DoIP
  diagnostic messages carry.
- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `FrameEncoder` /
  `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
