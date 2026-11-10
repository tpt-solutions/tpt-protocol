# tpt-protocol-opcua

OPC UA Binary (UA-TCP) message parser, validator, and TUP normalizer for the
[TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

## Overview

OPC UA is the platform-independent industrial interoperability standard. Its Binary
encoding runs over TCP as UA-TCP messages. Each message has a header, a secure-channel
or session part, and a service body that is an `ExtensionObject`. TPT implements the
message header, the chunk types, the secure-channel identifiers and the ExtensionObject
TypeId, in a `#![no_std]` + `alloc` crate, so captured messages can be validated and
normalized into the shared TUP envelope.

## Features

- UA-TCP message header: message type and chunk type (`F`, `C`, `A`), and the message
  size. The message size must equal the frame length, so bytes after a message are never
  read as its payload.
- OPC UA Binary is little-endian throughout.
- Secure-channel and token identifiers, the request id, the sequence number, and the
  service body.
- The service body of a MSG chunk is decoded as an `ExtensionObject`: its TypeId
  (two-byte, four-byte and numeric NodeId forms), its encoding byte and its
  length-prefixed body.
- `OpcuaEncoder`: wire bytes from a frame, with round-trip tests against the parser.
- `try_parse` for streams: `Ok(None)` for an incomplete message, otherwise the consumed
  length.
- Connection-state validation via `StateValidator` (`OpcuaValidator`).
- TUP normalisation via `DataNormalizer` (`OpcuaNormalizer`).
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
  Structured generators build valid messages, then mutate them.

## Architecture

`OpcuaParser::parse` reads the 8-byte header, validates the chunk and message types, then
reads the channel and token identifiers and the sequence and request ids. The service
body follows. The decoded result is the `OpcuaFrame` struct, which holds
`message_type`, `chunk_type`, `message_size`, `channel_id`, `token_id`,
`sequence_num`, `request_id`, `service_id`, `type_namespace` and `payload`.

`OpcuaNormalizer` produces:

- `device_id = "opcua:chan:0x<channel id>"`.
- `point_id = "node:<service id>"` when the body carries an ExtensionObject TypeId, or
  `"msg:<message type>"` otherwise.
- `value = Bytes(<payload>)`.

## Usage

A complete, runnable example parses a MSG chunk and normalizes it:

```sh
cargo run -p tpt-protocol-opcua --example parse_opcua
```

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_opcua::{OpcuaNormalizer, OpcuaParser};

let frame = OpcuaParser.parse(&wire)?;
let point = OpcuaNormalizer.normalize(&frame)?;
// point.point_id == "node:42" for a TypeId of 42
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`, `String`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — `OpcuaMessageType`, `OpcuaFrame`, `OpcuaParser` (`FrameParser`),
  `OpcuaEncoder` (`FrameEncoder`), `OpcuaState` + `OpcuaValidator` (`StateValidator`),
  `OpcuaNormalizer` (`DataNormalizer`), unit tests, and the `proptest` `mod fuzz`.
- `examples/parse_opcua.rs` — runnable parse-and-normalize example.

## Status / Conformance

- **Binary encoding only.** The XML and JSON encodings are not parsed.
- **Services are not decoded.** The body is an ExtensionObject. Its TypeId is read, but the
  service parameters inside it are carried as bytes. Only the TypeId is named.
- **No secure channel.** Messages are parsed as they appear on the wire. Signing and
  encryption (the security policies) are not applied, so encrypted bodies are carried
  as bytes.
- **Chunks are not reassembled.** A multi-chunk message is parsed chunk by chunk.
- Normalised `timestamp` is `0`; callers must supply a real timestamp.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `FrameEncoder` /
  `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
