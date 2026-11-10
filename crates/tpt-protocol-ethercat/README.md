# tpt-protocol-ethercat

EtherCAT frame parsing, encoding, validation, and TUP normalisation for the
[TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

Published to crates.io (`publish` unset).

## Overview

EtherCAT is a real-time Ethernet fieldbus for motion control and I/O. A single
Ethernet frame carries one or more datagrams that the slaves process on the fly, and
each datagram ends with a working counter (WKC). TPT implements a from-scratch,
`#![no_std]` + `alloc` parser for the EtherCAT telegram, so captured frames can be
decoded, the datagram chain walked, and the result normalized into the shared TUP
envelope. `ethercrab` exposes no standalone frame parser to wrap, so this crate is
its own implementation.

## Features

- EtherCAT header (length and type) and the full datagram chain, walked by the
  more-follows bit. The chain must end exactly at the section length; trailing
  Ethernet padding is ignored.
- Datagram commands per IEC 61158-4-12 (`EcCmd`): APRD, APWR, APRW, FPRD, FPWR, FPRW,
  BRD, BWR, BRW, LRD, LWR, LRW, ARMW, FRMW, plus `Unknown(u8)`.
- Little-endian header and datagram fields (ADP, ADO, length flags, IRQ) and the
  working counter.
- `try_parse` for streams: `Ok(None)` for an incomplete frame, otherwise the exact
  consumed length.
- `EthercatEncoder`: wire bytes from a frame, with round-trip tests against the parser.
- Connection-health validation via `StateValidator` (`EthercatValidator`), checking
  connection, OPERATIONAL state, a non-zero slave count, and a consecutive-error
  threshold.
- TUP normalisation via `DataNormalizer` (`EthercatNormalizer`).
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.

## Architecture

The parser works on the Ethernet payload that starts at the EtherCAT header. The
outer Ethernet frame, with EtherType `0x88A4`, is removed by the transport layer.

- **EtherCAT header** (2 bytes): a little-endian 11-bit length and a type field in
  bits 12–15.
- **Datagrams**: each has a command, index, ADP/ADO addressing, a length field with
  the more-follows bit, an IRQ field, the data, and a little-endian working counter.
  The parser checks each datagram's bounds before reading it.

The decoded result is the `EthercatFrame` struct (`eth_length`, `eth_type`,
`datagrams: Vec<EcDatagram>`). Each `EcDatagram` has `cmd`, `index`, `adp`, `ado`,
`more`, `circulating`, `irq`, `data` and `working_counter`.

`EthercatValidator` consumes an `EthercatState` (`connected`, `slave_count`,
`operational`, `last_ok_timestamp`, `error_count`) and rejects the state when the device
is disconnected, not `OPERATIONAL`, has zero slaves, or has more than 10 consecutive
errors.

`EthercatNormalizer` produces a `DataPoint` keyed by `device_id = "ethercat:master"` and
`point_id = "cmd:<command>"`. The value is the first datagram's data as bytes, or its
working counter when there is no data.

## Usage

A complete, runnable example parses an LRD datagram and normalizes it:

```sh
cargo run -p tpt-protocol-ethercat --example parse_ethercat
```

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_ethercat::{EthercatNormalizer, EthercatParser};

let frame = EthercatParser::new().parse(&wire)?;
let point = EthercatNormalizer::new().normalize(&frame)?;
// point.point_id == "cmd:LRD"
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`, `String`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — `EcCmd`, `EcDatagram`, `EthercatFrame`, `EthercatParser`
  (`FrameParser`), `EthercatEncoder` (`FrameEncoder`), `EthercatState` +
  `EthercatValidator` (`StateValidator`), `EthercatNormalizer` (`DataNormalizer`),
  unit tests, and the `proptest` `mod fuzz`.
- `examples/parse_ethercat.rs` — runnable parse-and-normalize example.

## Status / Conformance

- **Multi-datagram telegrams are parsed.** The normalizer reports only the first
  datagram. Per-datagram points are not produced.
- **No FCS check.** The Ethernet frame check sequence is not validated here; the
  parser trusts the transport layer to have delivered an intact frame.
- **Addressing is not interpreted.** ADP and ADO are decoded as numbers. Auto-increment,
  configured-address and logical addressing are not resolved to slaves.
- **Process data is not mapped.** The crate does not know the slave process-data
  objects, so the data bytes are carried as raw bytes.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `FrameEncoder` /
  `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
