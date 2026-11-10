# tpt-protocol-uds

UDS (ISO 14229, Unified Diagnostic Services) request, response and negative-response
parser, validator, and TUP normalizer for the
[TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

## Overview

UDS is the diagnostic protocol that automotive ECUs use for session control, ECU reset,
reading data and fault codes, routines, and software download. A tester sends a request
with a service identifier (SID); the ECU answers with a positive response (the SID plus
0x40) or a negative response (0x7F, the SID, and a negative response code). TPT implements
the request and response forms of these services in a `#![no_std]` + `alloc` crate, so
captured diagnostic traffic can be decoded and normalized.

## Features

- Requests and positive responses for the common services, with their sub-functions
  where the service has them.
- Negative responses (`0x7F`): the requested SID and the negative response code. They
  normalize to a `Bad`-quality point.
- Suppress-positive-response: bit 7 of a request's sub-function. Responses echo the
  sub-function unchanged.
- Services with their request and response forms, including ReadDTCInformation,
  RoutineControl, ControlDTCSetting, RequestDownload, RequestUpload, TransferData and
  RequestTransferExit. Unknown SIDs are kept as `Unknown`.
- `UdsEncoder`: wire bytes from a frame, with round-trip tests against the parser.
- Connection-state validation via `StateValidator` (`UdsValidator`).
- TUP normalisation via `DataNormalizer` (`UdsNormalizer`).
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests asserting `UdsParser::parse` never panics on arbitrary input.
  Structured generators build valid requests and responses, then mutate them.

## Architecture

`UdsParser` reads the SID from the first byte and decides whether the frame is a request
or a response from it. A response carries the SID plus 0x40, and a negative response is
0x7F. For services with sub-functions, the next byte is the sub-function, and its bit 7
is the suppress-positive-response flag.

The decoded result is the `UdsFrame` struct (`sid`, `sub_function`, `data`, `is_request`,
`suppress_positive_response`).

`UdsNormalizer` produces `device_id = "uds:ecu"` and `point_id = "sid:<sid>"` for a
request or positive response. A negative response gives `point_id = "negative:0x<sid>"`
for the rejected service, with `Quality::Bad`. Its value is the negative response code.

## Usage

A complete, runnable example parses a negative response and normalizes it:

```sh
cargo run -p tpt-protocol-uds --example parse_uds
```

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_uds::{UdsNormalizer, UdsParser};

let frame = UdsParser.parse(&wire)?;
let point = UdsNormalizer.normalize(&frame)?;
// a negative response to ReadDataByIdentifier (0x22) has quality Bad
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`, `String`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — `UdsSid`, `SubFunction`, `UdsFrame`, `UdsParser` (`FrameParser`),
  `UdsEncoder` (`FrameEncoder`), `UdsState` + `UdsValidator` (`StateValidator`),
  `UdsNormalizer` (`DataNormalizer`), unit tests, and the `proptest` `mod fuzz`.
- `examples/parse_uds.rs` — runnable parse-and-normalize example.
- `examples/diff_decode.rs` — used by the differential test against udsoncan in CI.

## Status / Conformance

- **Differential test.** `scripts/differential/uds_udsoncan.py` checks the decoding against
  udsoncan in CI.
- **Payloads are not decoded.** A ReadDataByIdentifier response's data identifier and
  value, and the body of a TransferData block, are carried as bytes.
- **No transport.** ISO-TP (ISO 15765-2) segmentation and reassembly are not implemented,
  so a multi-frame message must be reassembled by the caller.
- **No security access.** The seed and key exchange is parsed as a request and response.
  The key derivation is not performed.
- Normalised `timestamp` is `0`; callers must supply a real timestamp.

## Related crates

- [`tpt-protocol-doip-someip`](../tpt-protocol-doip-someip) — DoIP carries UDS
  diagnostic messages over Ethernet.
- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `FrameEncoder` /
  `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
