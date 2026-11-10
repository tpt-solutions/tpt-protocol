# tpt-protocol-ocpp

OCPP-J (Open Charge Point Protocol, JSON over WebSocket) message parser, validator,
and TUP normalizer for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol)
suite.

Published to crates.io (`publish` unset).

## Overview

OCPP is the protocol between EV chargers (charge points) and their central systems. The
JSON-over-WebSocket form, OCPP-J, is used by OCPP 1.6 and 2.0.1. TPT implements the
OCPP-J message envelope, so a captured message can be decoded, checked, and normalized
into the shared TUP envelope. The crate depends only on `tpt-protocol-core` and
`tpt-protocol-tup`, and it is `#![no_std]` + `alloc`.

## Features

- OCPP-J framing for the three message types, in the shape `[2, id, action, payload]`
  (Call), `[3, id, payload]` (CallResult) and `[4, id, code, description, details]`
  (CallError).
- A small scanner for the top-level JSON array. It decodes strings, including escapes,
  and skips brackets that appear inside strings. Payloads are kept as their raw JSON
  text.
- Limits: message ids are at most 36 characters, and element counts are checked for each
  message type.
- The action list covers the common OCPP 1.6 and 2.0.1 names (`OcppAction`), with an
  unknown-action fallback.
- `OcppEncoder`: the same JSON, from a frame, with round-trip tests.
- Connection-state validation via `StateValidator` (`OcppValidator`).
- TUP normalisation via `DataNormalizer` (`OcppNormalizer`). A CallError is `Quality::Bad`.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
  Structured generators build well-formed CALL, CALLRESULT and CALLERROR arrays.

## Architecture

`OcppParser` scans the top-level array and builds an `OcppFrame`, which holds the
`message_type` (`OcppMessageType`), the `unique_id`, the `action` (for a Call), the raw
`payload`, and the error code and description (for a CallError).

`OcppNormalizer` produces:

- `device_id = "ocpp/<action>"` for a Call (for example `ocpp/BootNotification`), or
  `ocpp/<message type>` when there is no action.
- `point_id = <unique id>`, which pairs a Call with its response.
- `value = Text(<raw JSON payload>)`.

## Usage

A complete, runnable example parses a BootNotification Call and normalizes it:

```sh
cargo run -p tpt-protocol-ocpp --example parse_ocpp
```

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_ocpp::{OcppNormalizer, OcppParser};

let frame = OcppParser.parse(&wire)?;
let point = OcppNormalizer.normalize(&frame)?;
// point.device_id == "ocpp/BootNotification", point.point_id == frame.unique_id
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`, `String`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — `OcppMessageType`, `OcppAction`, `OcppFrame`, `OcppParser`
  (`FrameParser`), `OcppEncoder` (`FrameEncoder`), `OcppConnectionState` +
  `OcppValidator` (`StateValidator`), `OcppNormalizer` (`DataNormalizer`), unit tests, and
  the `proptest` `mod fuzz`.
- `examples/parse_ocpp.rs` — runnable parse-and-normalize example.

## Status / Conformance

- **Envelope only.** The payload is kept as raw JSON text. It is not checked against the
  OCPP JSON schemas for each action, and the fields inside it are not decoded.
- **Action coverage.** Actions outside the common 1.6 and 2.0.1 list are kept as
  unknown names.
- **No WebSocket transport.** The crate parses a message. It does not open a connection
  or handle the OCPP subprotocol handshake.
- Normalised `timestamp` is `0`; callers must supply a real timestamp.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `FrameEncoder` /
  `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
