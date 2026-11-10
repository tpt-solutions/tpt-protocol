# tpt-protocol-opcua

OPC UA (Open Platform Communications Unified Architecture) Industry 4.0 protocol
parser for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

## Overview

OPC UA is the leading industrial-interoperability standard for secure,
platform-independent exchange of process data between PLCs, SCADA/Historians, and
enterprise systems. TPT provides a memory-safe `#![no_std]` + `alloc` parser for
the OPC UA binary message header and key secure-channel/MSG fields so traffic can
be decoded, connection health validated, and normalised into the shared TUP
telemetry envelope.

## Features

- OPC UA binary message-header parsing via `FrameParser`.
- Message-type decoding: HEL (Hello), ACK (Acknowledge), ERR (Error), MSG
  (Message), OPN (OpenSecureChannel), CLO (CloseSecureChannel), plus `Unknown`.
- Extraction of the message size, secure-channel header
  (channel id, token id, sequence number, request id), and — for MSG frames —
  service id and node id from the request body.
- Connection health validation via the `StateValidator` trait.
- TUP normalisation via the `DataNormalizer` trait, mapping frames to `tup::DataPoint`s.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.

## Architecture

`OpcuaParser` decodes the OPC UA binary header:

```
[0..3]   message type       (ASCII: HEL/ACK/ERR/MSG/OPN/CLO) -> OpcuaMessageType
[3]      chunk type         (e.g. 'F' = final)
[4..8]   message size       (big-endian u32; must fit the buffer)
[8..24]  secure-channel hdr (channel id, token id, sequence num, request id)  // if >= 24 bytes
[24..]   MSG body: service id (u32) + node id (u32) + payload                 // MSG frames, if >= 32 bytes
```

The secure-channel fields and the MSG service/node id are only read when the
buffer is long enough; shorter buffers yield zeroed header fields and `None`
service/node ids. `message_size` is bounds-checked against the buffer length.

`OpcuaFrame` exposes `message_type`, `chunk_type`, `message_size`, `channel_id`,
`token_id`, `sequence_num`, `request_id`, `service_id`, `node_id`, and `payload`.
`OpcuaValidator` validates an `OpcuaState` (connected flag, non-zero `channel_id`
and `token_id`, `error_count` ≤ 8). `OpcuaNormalizer` emits a `DataPoint` with
`device_id = "opcua:chan:0x<channel_id>"` and either `node:<service_id>` or
`msg:<message_type>` as `point_id`, packing service/node ids into an `Int` when
present, else the payload bytes, else the message size.

## Usage

```rust
use tpt_protocol_core::{FrameParser, DataNormalizer};
use tpt_protocol_opcua::{OpcuaParser, OpcuaNormalizer};

fn main() {
    // Build a minimal MSG frame: "MSG" + chunk 'F' + size + channel/token/seq/req
    // then service id = 1, node id = 2, 1 payload byte.
    let mut raw: Vec<u8> = Vec::new();
    raw.extend_from_slice(b"MSG");
    raw.push(b'F');
    raw.extend_from_slice(&[0; 4]); // size placeholder
    raw.extend_from_slice(&0x1234u32.to_be_bytes()); // channel
    raw.extend_from_slice(&0x5678u32.to_be_bytes()); // token
    raw.extend_from_slice(&1u32.to_be_bytes());      // seq
    raw.extend_from_slice(&2u32.to_be_bytes());      // req
    raw.extend_from_slice(&1u32.to_be_bytes());      // service id
    raw.extend_from_slice(&2u32.to_be_bytes());      // node id
    raw.push(0xAA);                                  // payload
    let size = raw.len() as u32;
    raw[4..8].copy_from_slice(&size.to_be_bytes());

    let parser = OpcuaParser::new();
    let frame = parser.parse(&raw).unwrap();
    assert_eq!(frame.message_type, tpt_protocol_opcua::OpcuaMessageType::Message);
    assert_eq!(frame.service_id, Some(1));
    assert_eq!(frame.node_id, Some(2));

    let normalizer = OpcuaNormalizer::new();
    let dp = normalizer.normalize(&frame).unwrap();
    assert_eq!(dp.source_protocol, tpt_protocol_tup::SourceProtocol::Opcua);
    assert_eq!(dp.point_id, "node:1");
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for `String`/`Vec` (payload,
device-id formatting). It works on alloc-capable embedded targets.

## Crate layout

- `src/lib.rs` — `OpcuaMessageType`, `OpcuaFrame`, `OpcuaParser` (binary header
  decoder), `OpcuaValidator` / `OpcuaState`, `OpcuaNormalizer`, unit/fuzz tests.

## Status / Conformance

- **Implemented:** decoding of the OPC UA binary message header, the six known
  message types plus a catch-all, secure-channel header fields, MSG service/node
  id extraction, connection/channel/token state validation, TUP normalisation,
  and `proptest` fuzzing.
- **Known gaps / TODOs:**
  - This is a *structural* header parser, **not** a full OPC UA stack: it does
    not implement the secure-channel handshake/crypto, full NodeId encoding
    (namespace/identifier types), ExtensionObjects, or service request/response
    schemas. Service and node ids are read as raw 4-byte big-endian integers.
  - Normalised `timestamp` is `0`; callers must supply a real timestamp.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
