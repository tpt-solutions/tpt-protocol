# out-protocol-ethercat

High-speed EtherCAT industrial factory-automation protocol parser, validator, and
TUP normalizer for the [TPT Protocol](https://github.com/tpt-protocol/tpt-protocol)
suite.

> **Private crate.** This crate is `publish = false` (prefixed `out-protocol-*`).
> It is a from-scratch `#![no_std]` reimplementation used identically by the
> daemon; it stays unpublished only because the well-known `ethercrab` crate
> already matches it on `no_std` support and `MIT OR Apache-2.0` licensing (see the
> workspace `TODO.md`). Private status affects crates.io distribution only.

## Overview

EtherCAT is a real-time Ethernet fieldbus used for motion control and I/O in
machine automation. A single Ethernet frame (EtherType `0x88A4`) carries one or
more *datagrams* that are processed on-the-fly by slaves as the frame passes
through the bus. TPT implements the EtherCAT datagram layer so that captured
link-layer payloads can be validated for connection health and normalized into
unified telemetry points.

## Features

- EtherCAT datagram frame parsing (EtherType `0x88A4` payload; Ethernet header
  assumed already stripped).
- Validation of the EtherCAT header length/type and the 10-byte datagram header,
  with truncation guards for the data + working-counter (WKC) region.
- Decoding of all eight standard datagram commands (`EcCmd`): APRD, APWR, FPRD,
  FPWR, BRD, BWR, LRD, LWR — plus an `Unknown(u8)` catch-all.
- Connection-health validation via `StateValidator` (`EthercatValidator`) checking
  connection, OPERATIONAL state, non-zero slave count, and a consecutive-error
  threshold.
- TUP normalisation via `DataNormalizer` (`EthercatNormalizer`) mapping each frame
  to a `tup::DataPoint`.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests (`mod fuzz`) asserting `FrameParser::parse` never panics on
  arbitrary input.

## Architecture

The parser operates on the **Ethernet payload starting at the EtherCAT header**
(the outer Ethernet `0x88A4` EtherType is expected to already be stripped by the
transport layer).

- **EtherCAT header** (`EthercatParser::parse`, `src/lib.rs`): `eth_length`
  (2 bytes BE) + `eth_type` (1 byte). A minimum of 3 bytes is required.
- **Datagram header** (10 bytes): `datagram_cmd` (1 byte, decoded by
  `EcCmd::from_u8`), `datagram_index` (1 byte), `addressing` (4 bytes BE, the
  auto-increment / configured / logical address), `datagram_length` (2 bytes BE),
  and 2 reserved bytes. A minimum of 13 bytes is required before the datagram
  header is read.
- **Data + WKC**: the payload begins at offset 13 and is `datagram_length` bytes
  long; the **working counter (WKC)** follows it and is read little-endian. A
  length guard (`wkc_start + 2`) prevents out-of-bounds indexing on truncated
  frames.

The decoded result is the `EthercatFrame` struct
(`eth_length`, `eth_type`, `datagram_cmd`, `datagram_index`, `addressing`,
`datagram_length`, `data`, `working_counter`).

`EthercatValidator` consumes an `EthercatState` (`connected`, `slave_count`,
`operational`, `last_ok_timestamp`, `error_count`) and rejects the state when the
device is disconnected, not `OPERATIONAL`, has zero slaves, or has more than 10
consecutive errors.

`EthercatNormalizer` produces a `DataPoint` keyed by `device_id =
"ethercat:master"` and `point_id = "cmd:{command}"`; the value is the raw
`data` bytes when present, otherwise the integer WKC.

## Usage

```rust
use tpt_protocol_core::{FrameParser, DataNormalizer};
use out_protocol_ethercat::{EthercatParser, EthercatNormalizer, EcCmd};

fn main() {
    // Build an FPRD datagram (cmd=4) with 2 data bytes + WKC.
    // EtherCAT header: length(2) + type(1) | datagram: cmd+idx+addr(4)+len(2)+reserved(2)+data+WKC(LE)
    let mut raw: Vec<u8> = Vec::new();
    let eth_len: u16 = (10u16 + 2 + 2).to_be_bytes()[0] << 8
        | (10u16 + 2 + 2).to_be_bytes()[1];
    raw.extend_from_slice(&eth_len.to_be_bytes());
    raw.push(0x01);                 // eth_type
    raw.push(4);                    // FPRD
    raw.push(0x01);                 // datagram_index
    raw.extend_from_slice(&0x1000u32.to_be_bytes()); // addressing
    raw.extend_from_slice(&2u16.to_be_bytes());      // datagram_length
    raw.extend_from_slice(&[0x00, 0x00]); // reserved
    raw.extend_from_slice(&[0xAA, 0xBB]); // data
    raw.extend_from_slice(&0x0000u16.to_le_bytes()); // WKC

    let parser = EthercatParser::new();
    let frame = parser.parse(&raw).unwrap();
    assert_eq!(frame.datagram_cmd, EcCmd::Fprd);
    assert_eq!(frame.addressing, 0x1000);

    let normalizer = EthercatNormalizer::new();
    let dp = normalizer.normalize(&frame).unwrap();
    assert_eq!(dp.source_protocol, tpt_protocol_tup::SourceProtocol::Ethercat);
    assert_eq!(dp.device_id, "ethercat:master");
    assert_eq!(dp.point_id, "cmd:FPRD");
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`,
`String`, `format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — the full implementation: `EcCmd` enum, `EthercatFrame` struct,
  `EthercatParser` (`FrameParser`), `EthercatState` + `EthercatValidator`
  (`StateValidator`), `EthercatNormalizer` (`DataNormalizer`), unit tests, and the
  `proptest` `mod fuzz`.

## Status / Conformance

- **Single-datagram only.** The parser decodes one datagram per frame. Real
  EtherCAT telegrams may chain multiple datagrams (each followed by its own WKC);
  multi-datagram telegrams are not yet handled — only the first datagram's data and
  WKC are extracted.
- **No FCS check.** The Ethernet-level frame check sequence is not validated here;
  the parser trusts the transport layer to have delivered an intact frame.
- **Addressing mode not distinguished.** `addressing` is decoded as a raw 32-bit
  value; auto-increment vs. configured-address vs. logical addressing is not
  interpreted.
- Crates `out-protocol-ethercat` and `out-protocol-mavlink` are intentionally
  private (not published to crates.io) — see the workspace `TODO.md` for the
  licensing/`no_std` rationale; this is a distribution decision, not a feature gap.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` /
  `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.
- [`out-protocol-mavlink`](../out-protocol-mavlink) — the other private from-scratch
  reimplementation in the workspace.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
