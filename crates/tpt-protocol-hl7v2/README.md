# tpt-protocol-hl7v2

HL7 v2.x healthcare messaging protocol parsing, validation, and TUP
normalisation for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

HL7 v2 is the dominant pipe-delimited text format for clinical and administrative
data exchange between hospital systems (EHRs, lab, ADT, order entry). TPT decodes
the segment/field/component structure and surfaces each message as a normalised TUP
data point keyed by trigger event and message control ID.

## Overview

HL7 v2.x messages are human-readable ASCII/UTF-8 streams split into *segments*
(lines), *fields* (separated by `|`), and *components* (separated by `^`). Every
message begins with an `MSH` (message header) segment that carries the separators,
sending/receiving application and facility, timestamp, message type, and control
ID. TPT parses this structure so healthcare feeds can be unified with the rest of
the telemetry pipeline.

## Features

- HL7 v2 segment / field / component parsing (segment terminator `\r`, field
  separator, component separator `^`).
- `MSH` header decoding into a structured `MshSegment`.
- Message-type classification: `ADT`, `ORM`, `ORU`, `MDM`, and `Other(String)`.
- UTF-8 safety: input is validated with `str::from_utf8`, so a non-UTF-8 byte
  sequence yields `Error::Parse` rather than a panic.
- Connection-state validation via `StateValidator` (`Hl7Validator`).
- TUP normalisation via `DataNormalizer` (`Hl7Normalizer`).
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary
  input.

## Architecture

The crate is built around text parsing over a single combined frame type:

- `Hl7Frame` — the whole message: `message_type: Hl7MessageType`, `trigger_event`,
  `msh: MshSegment`, `segments: Vec<Hl7Segment>`, and the `raw` text.
- `Hl7Segment` — `name` (first 3 chars) plus `fields: Vec<Hl7Field>`, where each
  `Hl7Field` holds its `components: Vec<String>`.
- `Hl7MessageType` classifies the message from the first component of the MSH
  message-type field: `Adt` (`"ADT"`), `Orm` (`"ORM"`), `Oru` (`"ORU"`), `Mdm`
  (`"MDM"`), or `Other(String)`.
- `Hl7Parser` implements `FrameParser`. It splits the input on `\r`, requires the
  first segment to be `MSH`, derives the field separator from `MSH[3]`, then walks
  the MSH body and remaining segments into the structured frame. Empty input, a
  missing/short `MSH`, invalid UTF-8, or a missing message type all return
  `Error::Parse`.
- `Hl7Validator` validates `Hl7ConnectionState` (`connected`, `messages_processed`,
  `last_ack_received`): it errors if not connected, and flags an awaiting-ACK
  condition when messages have been processed but no ACK has been received.
- `Hl7Normalizer` maps a frame to one data point per OBX observation:
  `device_id = "hl7/<trigger_event>"`, `point_id` = the observation code (for example
  `CHOL`, or `obs-<n>` when the code is empty). A numeric result is a `Float`, and the
  unit and quality come from the result fields and status. A frame with no OBX segment
  normalizes to its message-control point, with `Text(<message_type>)`.
- `Hl7Encoder`: wire bytes from a frame, with round-trip tests against the parser.

## Usage

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_tup::SourceProtocol;
use tpt_protocol_hl7v2::{Hl7Normalizer, Hl7Parser};

fn main() {
    let message = "MSH|^~\\&|SEND_APP|SEND_FAC|RECV_APP|RECV_FAC|20240101000000||ADT^A01|ctrl123|P|2.5|\r\
                    EVN|A01|20240101000000|\r\
                    PID|1||12345^^^MRN^MRN||Doe^John||19700101|M";

    let frame = Hl7Parser.parse(message.as_bytes()).unwrap();
    assert_eq!(frame.message_type, tpt_protocol_hl7v2::Hl7MessageType::Adt);
    assert_eq!(frame.trigger_event, "A01");
    assert_eq!(frame.msh.message_control_id, "ctrl123");

    let dp = Hl7Normalizer.normalize(&frame).unwrap();
    assert_eq!(dp.source_protocol, SourceProtocol::Hl7v2);
    assert_eq!(dp.device_id, "hl7/A01");
    assert_eq!(dp.point_id, "ctrl123");
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated strings and vectors
(`String`, `Vec`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — the entire crate: HL7 frame types, the `Hl7Parser`, `Hl7Validator`,
  `Hl7Normalizer`, and unit + `proptest` fuzz tests (ADT/ORU parsing, truncation,
  non-UTF-8, missing MSH).

## Status / Conformance

- Implemented: segment/field/component decoding, `MSH` header parsing, message-type
  classification for the common ADT/ORM/ORU/MDM families, state validation, and TUP
  normalisation.
- Known gaps / TODOs:
  - No schema/version-specific field decoding per HL7 v2.x data type tables (e.g.
    `XPN`, `CX`, `TS`); all components are kept as raw `String`s.
  - No escaping/encoding-character handling beyond the standard `^`/`|` separators;
    the encoding-chars field is captured but not interpreted.
  - No ACK/NACK generation or MLLP framing (the TCP framing wrapper used to delimit
    HL7 messages on the wire) — the daemon is simulation-only.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.
- [`tpt-protocol-ieee11073-sdc`](../tpt-protocol-ieee11073-sdc) — the other medical-domain protocol crate in the suite.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
