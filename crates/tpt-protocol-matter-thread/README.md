# tpt-protocol-matter-thread

Matter (CSA smart-home standard) message parser, validator, and TUP normalizer for the
[TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite. Matter runs over
Thread (IPv6 over IEEE 802.15.4) or Wi-Fi and Ethernet.

## Overview

Matter exchanges messages made of a message header, a protocol header and an Interaction
Model payload encoded in Matter TLV. TPT implements the three layers of the stack that
matter for telemetry: Matter TLV, the message and protocol headers for unsecured
(session 0) messages, and the Interaction Model `ReportData` that carries attribute
values. The crate is `#![no_std]` + `alloc`.

## Features

- Matter TLV decoding (Appendix A): all element types, the context, profile and
  fully-qualified tag forms, and nested containers with a depth limit. Reserved element
  types and trailing bytes are errors.
- Message header and protocol header for unsecured (session 0) messages. Secured payloads
  are reported as encrypted, not decoded.
- Interaction Model `ReportData`, decoded into attribute reports: endpoint, cluster,
  attribute and value.
- `MatterParser` as `FrameParser` and a `decode_tlv` helper for raw TLV.
- Connection-state validation via `StateValidator` (`MatterValidator`).
- TUP normalisation via `DataNormalizer` (`MatterNormalizer`). `normalize_all` gives one
  point per scalar attribute. Unsigned values above `i64::MAX` are errors.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
  Structured generators build valid TLV and messages, then mutate them.

## Architecture

The parser works in three layers:

1. **TLV** (`decode_tlv`, `TlvTag`, `TlvValue`): element control byte, tag form, length and
   value, with nested containers.
2. **Headers**: the message header (flags, session id, message counter, source and
   destination node ids when present) and the protocol header (exchange flags, opcode,
   exchange id, protocol id). Session 0 messages are decoded. Other sessions are encrypted.
3. **Interaction Model**: a `ReportData` opcode is decoded into `AttributeReport`s.

`MatterFrame` holds the headers and the attribute reports, or marks the payload as
encrypted.

`MatterNormalizer` produces:

- `device_id = "matter:<node id>/ep<endpoint>"`. The node id is in hex.
- `point_id = "<cluster>/0x<attribute id>"`, for example `OnOff/0x0000`. The cluster is
  named when it is known, and in hex otherwise.
- `value` from the attribute's TLV type.

## Usage

A complete, runnable example decodes a ReportData message and normalizes it:

```sh
cargo run -p tpt-protocol-matter-thread --example parse_matter
```

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_matter_thread::{MatterNormalizer, MatterParser};

let frame = MatterParser.parse(&wire)?;
let points = MatterNormalizer.normalize_all(&frame)?;
// points[0].point_id == "OnOff/0x0000" for an OnOff attribute report
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`, `String`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — `TlvTag`, `TlvValue`, `decode_tlv`, `AttributeReport`, `MatterFrame`,
  `MatterParser` (`FrameParser`), `MatterConnectionState` + `MatterValidator`
  (`StateValidator`), `MatterNormalizer` (`DataNormalizer`), unit tests, and the
  `proptest` `mod fuzz`.
- `examples/parse_matter.rs` — runnable parse-and-normalize example.

## Status / Conformance

- **Unsecured messages only.** Session-keyed (secured) payloads are reported as encrypted.
  Decryption and the session keys are outside this crate.
- **Interaction Model opcodes are unverified.** The opcode values follow the
  connectedhomeip implementation. They have not been checked against a real capture.
- **Only ReportData is decoded.** Other Interaction Model messages (for example invoke
  commands and subscriptions) are not decoded.
- **No Thread or Wi-Fi transport.** The crate parses a captured message.
- Normalised `timestamp` is `0`; callers must supply a real timestamp.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `FrameEncoder` /
  `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
