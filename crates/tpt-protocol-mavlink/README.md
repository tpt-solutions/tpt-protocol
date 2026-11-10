# tpt-protocol-mavlink

MAVLink v1 and v2 frame parser, validator, and TUP normalizer for the
[TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

Published to crates.io (`publish` unset).

## Overview

MAVLink is the lightweight messaging protocol that drones, rovers and ground stations
use to exchange telemetry and commands. TPT decodes MAVLink frames with the
[`mavlink`](https://crates.io/crates/mavlink) crate (`default-features = false`, with
the `embedded` and `common` features), so every message in the common dialect is named
and typed. This crate adds the TUP normalization and the connection-state validation.

## Features

- MAVLink v1 and v2 framing. Magic byte, header, CRC-16 and the CRC_EXTRA seed are
  checked by the `mavlink` crate. A frame with a bad CRC, or with the wrong CRC_EXTRA,
  is rejected.
- Every message in the common dialect is named through the `Message` trait, and its
  fields are typed (`MavMessage`).
- v2 signature block: kept on the frame (`MavlinkSignature`: link id, timestamp and
  value). The signature is parsed, not verified.
- Trailing bytes after a frame are an error.
- Connection-state validation via `StateValidator` (`MavlinkValidator`).
- TUP normalisation via `DataNormalizer` (`MavlinkNormalizer`). `normalize_all` gives
  one point per field of the messages it decodes.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
  Structured generators build valid v1 and v2 frames, then mutate them.

## Architecture

`MavlinkParser::parse` reads one frame and returns a `MavlinkFrame`, which holds the
system and component IDs, the sequence number, the decoded message, its name, the
MAVLink version and the optional v2 signature.

`MavlinkNormalizer` produces `device_id = "mavlink:<system id>:<component id>"`.
Point ids are `<MESSAGE>.<field>` for typed fields (for example `HEARTBEAT.autopilot`,
`SYS_STATUS.voltage_battery`, `ATTITUDE.roll`, `GPS_RAW_INT.lat`). GPS latitude and
longitude are in degrees and altitude is in metres. Messages the normalizer does not
decode give a single `msg:<NAME>` point.

`MavlinkValidator` consumes a `MavlinkState` (connection flag, timestamp and error
count) and rejects a disconnected link or one with too many consecutive errors.

## Usage

A complete, runnable example parses a HEARTBEAT and normalizes its fields:

```sh
cargo run -p tpt-protocol-mavlink --example parse_mavlink
```

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_mavlink::{MavlinkNormalizer, MavlinkParser};

let frame = MavlinkParser.parse(&wire)?;
let points = MavlinkNormalizer.normalize_all(&frame)?;
// points include "HEARTBEAT.autopilot" and "HEARTBEAT.base_mode"
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`, `String`,
`format!`). The `mavlink` dependency is built without its `std` feature.

## Crate layout

- `src/lib.rs` — `MavlinkVersion`, `MavlinkSignature`, `MavlinkFrame`, `MavlinkParser`
  (`FrameParser`), `MavlinkState` + `MavlinkValidator` (`StateValidator`),
  `MavlinkNormalizer` (`DataNormalizer`), unit tests (including the CRC_EXTRA
  regression test), and the `proptest` `mod fuzz`.
- `examples/parse_mavlink.rs` — runnable parse-and-normalize example.

## Status / Conformance

- **Common dialect only.** Messages from other dialects are not named unless the
  `mavlink` crate's features are changed.
- **Signatures are not verified.** The v2 signature block is kept, but the link key and
  the timestamp check are not applied.
- **Typed normalization for four messages.** HEARTBEAT, SYS_STATUS, ATTITUDE and
  GPS_RAW_INT give named points. Other messages give a `msg:<NAME>` point.
- Normalised `timestamp` is `0`; callers must supply a real timestamp.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` /
  `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
