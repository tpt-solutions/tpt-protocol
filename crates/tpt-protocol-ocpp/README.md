# tpt-protocol-ocpp

OCPP (Open Charge Point Protocol) 2.0.1 EV-charging communication parser for the
[TPT Protocol](https://github.com/tpt-protocol/tpt-protocol) suite.

## Overview

OCPP is the open standard that charge points and charging stations use to talk to
a central system (boot, status, metering, transactions). TPT provides a
memory-safe `#![no_std]` + `alloc` parser for an OCPP message framing so charge-point
messages can be decoded, connection health validated, and normalised into the
shared TUP telemetry envelope — without a `std`/WebSocket stack.

## Features

- OCPP message framing parsing via `FrameParser` (message type, unique id, action, payload).
- Message-type decoding: Call (2), CallResult (3), CallError (4).
- Action decoding for BootNotification, StatusNotification, MeterValues,
  StartTransaction, StopTransaction, Heartbeat, Authorize, plus `Other`.
- Round-trippable framing via `OcppFrame::to_bytes`.
- Connection health validation via the `StateValidator` trait.
- TUP normalisation via the `DataNormalizer` trait, mapping messages to `tup::DataPoint`s.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.

## Architecture

`OcppParser` decodes a length-prefixed binary framing produced by
`OcppFrame::to_bytes`:

```
[0]        message type     (2 = Call, 3 = CallResult, 4 = CallError)
[1..3]     unique id len    (big-endian u16) + unique id bytes (UTF-8)
[if Call]  action len       (big-endian u16) + action bytes (UTF-8)
[len..]    payload len      (big-endian u32) + payload bytes (UTF-8)
```

`OcppMessageType` (from `OcppAction`) selects whether an `action` field is
present; `CallResult`/`CallError` frames carry only a `unique_id` and `payload`.
All length fields are bounds-checked against the buffer using `saturating_sub` so
a hostile `u32` payload length cannot overflow into an invalid slice range.

`OcppValidator` validates an `OcppConnectionState` (connected flag,
`last_message_type`, `error_count`), rejecting disconnected sessions and more than
5 consecutive `CallError` messages. `OcppNormalizer` emits a `DataPoint` with
`device_id = "ocpp/<action>"`, `point_id = <unique_id>`, the payload as
`TupValue::Text`, and quality `Bad` for `CallError` frames.

## Usage

```rust
use tpt_protocol_core::{FrameParser, DataNormalizer};
use tpt_protocol_ocpp::{OcppParser, OcppNormalizer, OcppFrame, OcppMessageType, OcppAction};

fn main() {
    // Build a Call: BootNotification with an empty JSON payload.
    let frame = OcppFrame {
        message_type: OcppMessageType::Call,
        unique_id: "abc123".into(),
        action: OcppAction::BootNotification,
        payload: "{}".into(),
    };
    let raw = frame.to_bytes();

    let parser = OcppParser;
    let parsed = parser.parse(&raw).unwrap();
    assert_eq!(parsed.action, OcppAction::BootNotification);

    let normalizer = OcppNormalizer;
    let dp = normalizer.normalize(&parsed).unwrap();
    assert_eq!(dp.source_protocol, tpt_protocol_tup::SourceProtocol::Ocpp);
    assert_eq!(dp.point_id, "abc123");
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for `String`/`Vec` (ids, actions,
payloads, device-id formatting). It works on alloc-capable embedded targets.

## Crate layout

- `src/lib.rs` — `OcppMessageType`, `OcppAction`, `OcppFrame` (+ `to_bytes`),
  `OcppParser`, `OcppValidator` / `OcppConnectionState`, `OcppNormalizer`,
  unit/fuzz tests.

## Status / Conformance

- **Implemented:** binary framing/parsing for Call / CallResult / CallError
  messages, the seven recognised actions plus a catch-all, round-trip serialisation,
  connection/error state validation, TUP normalisation, `proptest` fuzzing, and
  overflow-safe length handling.
- **Known gaps / TODOs:**
  - This is a *framing* parser for OCPP, **not** the full OCPP 2.0.1
    JSON-over-WebSocket stack: the `payload` is carried as opaque UTF-8 text and
    is not parsed into the OCPP request/response schema (no JSON decoding of
    payload fields, no message-routing semantics).
  - Normalised `timestamp` is `0`; callers must supply a real timestamp.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
