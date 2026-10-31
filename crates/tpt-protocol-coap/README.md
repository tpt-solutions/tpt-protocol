# tpt-protocol-coap

CoAP (RFC 7252) constrained-IoT protocol parser, validator, and TUP normalizer for
the [TPT Protocol](https://github.com/tpt-protocol/tpt-protocol) suite.

## Overview

CoAP is the RESTful, UDP-based application protocol for constrained devices and
networks (the IoT equivalent of HTTP). TPT implements the binary CoAP message format
so captured datagrams can be validated for link state and normalized into unified
data points keyed by message type and ID. This crate is `#![no_std]` (the popular
`coap` / `coap-rs` crate requires `std` + `tokio`, which this implementation avoids).

## Features

- RFC 7252 binary header parsing: version (must be 1), message type, token length,
  code, 16-bit message ID, token, options, and payload (`CoapFrame`).
- Full CoAP code table (`CoapCode`) covering methods — Get (1), Post (2), Put (3),
  Delete (4) — and response classes (2.x success, 4.x client error, 5.x server
  error) with helpers `is_method` / `is_success` / `is_client_error` /
  `is_server_error`, plus `Unknown(u8)`.
- Delta/length option decoding with 4-bit, 1-byte (13+), and 2-byte (269+) extensions,
  and typed option extraction (`CoapOption`): UriPath, ContentFormat, MaxAge, ETag,
  Observe, and `Other`.
- Payload-marker (`0xFF`) delimiting and extraction.
- Connection-health validation via `StateValidator` (`CoapValidator`) checking
  connection state.
- TUP normalisation via `DataNormalizer` (`CoapNormalizer`) mapping each frame to a
  `tup::DataPoint`.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests (`mod fuzz`) asserting `FrameParser::parse` never panics on
  arbitrary input.

## Architecture

`CoapParser::parse` (`src/lib.rs`) decodes the fixed 4-byte header
(`CoapType` Confirmable/NonConfirmable/Acknowledgement/Reset from bits 5–4;
`token_len` from bits 3–0; `code` byte; `message_id` 2 bytes BE), then reads the
`token` and walks the option list:

- Each option byte carries a 4-bit delta and 4-bit length. Values 13 and 14 are
  escape codes for 1-byte and 2-byte extensions (`delta + 13` / `delta + 269`).
  Values 15 are reserved and rejected. A running option number is accumulated via
  delta. Truncation at any extension/value stage returns `Error::Parse`.
- Option numbers are mapped to typed `CoapOption`s: 11 → `UriPath` (split on `/`),
  12 → `ContentFormat`, 14 → `MaxAge`, 4 → `ETag`, 6 → `Observe`, anything else →
  `Other { number, value }`.
- A `0xFF` byte terminates options and introduces the payload.

`CoapValidator` validates a `CoapConnectionState` (`connected`, `last_message_id`,
`observe_counter`) — currently only the `connected` flag is enforced.

`CoapNormalizer` produces a `DataPoint` keyed by `device_id = "coap/{type}"`
(e.g. `coap/CON`) and `point_id = "mid:{message_id}"`; the value is the integer code
when the payload is empty, otherwise the raw payload bytes.

## Usage

```rust
use tpt_protocol_core::{FrameParser, DataNormalizer};
use tpt_protocol_coap::{CoapParser, CoapNormalizer, CoapType, CoapCode};

fn main() {
    // ver=1, CON(0), token_len=0 | GET(1) | msg-id=1 | Uri-Path "test" | payload "{}"
    let mut raw: Vec<u8> = vec![0b0100_0000, 1];
    raw.extend_from_slice(&0x0001u16.to_be_bytes());
    raw.push((11 << 4) | 4); // delta=11 (Uri-Path), len=4
    raw.extend_from_slice(b"test");
    raw.push(0xFF);          // payload marker
    raw.extend_from_slice(b"{}");

    let parser = CoapParser;
    let frame = parser.parse(&raw).unwrap();
    assert_eq!(frame.version, 1);
    assert_eq!(frame.msg_type, CoapType::Confirmable);
    assert_eq!(frame.code, CoapCode::Get);

    let normalizer = CoapNormalizer;
    let dp = normalizer.normalize(&frame).unwrap();
    assert_eq!(dp.source_protocol, tpt_protocol_tup::SourceProtocol::Coap);
    assert_eq!(dp.device_id, "coap/CON");
    assert_eq!(dp.point_id, "mid:1");
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`,
`String`, `format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — the full implementation: `CoapType` / `CoapCode` enums,
  `CoapOption` enum, `CoapFrame` struct, `CoapParser` (`FrameParser`, with
  delta/length option decoding), `CoapConnectionState` + `CoapValidator`
  (`StateValidator`), `CoapNormalizer` (`DataNormalizer`), unit tests, and the
  `proptest` `mod fuzz`.

## Status / Conformance

- **No block-wise transfer (RFC 7959).** Large payloads split across multiple
  messages (Block1/Block2 options) are not reassembled.
- **No DTLS / observe re-registration logic.** The `Observe` option is parsed but not
  used to drive subscription state beyond the `observe_counter` field.
- **Uri-Query / other options carried raw.** Only UriPath, ContentFormat, MaxAge,
  ETag, and Observe are typed; remaining options arrive as `CoapOption::Other`.
- **Non-UTF8 UriPath tolerated.** A non-UTF8 Uri-Path segment is stored as an empty
  string rather than rejected (the parser does not fail the frame on encoding).
- This crate is `no_std`; real CoAP transport (UDP sockets, `tokio`) is out of scope.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` /
  `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.
- [`tpt-protocol-modbus`](../tpt-protocol-modbus) — the reference implementation the
  CoAP crate's API follows.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
