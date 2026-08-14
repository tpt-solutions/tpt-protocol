# out-protocol-mavlink

MAVLink drone and unmanned-vehicle telemetry protocol parser (v1 and v2), validator,
and TUP normalizer for the [TPT Protocol](https://github.com/tpt-protocol/tpt-protocol)
suite.

> **Private crate.** This crate is `publish = false` (prefixed `out-protocol-*`).
> It is a from-scratch `#![no_std]` reimplementation used identically by the
> daemon; it stays unpublished only because the well-known `mavlink` crate already
> matches it on `no_std` support and `MIT/Apache-2.0` licensing (see the workspace
> `TODO.md`). Private status affects crates.io distribution only.

## Overview

MAVLink is the lightweight messaging protocol used by the ArduPilot / PX4 autopilot
ecosystem and countless companion computers. TPT implements both wire versions
(v1.0 and v2.0) so captured serial/UDP telemetry can be validated for link health
and normalized into unified data points keyed by system/component and message ID.

## Features

- Auto-detection of MAVLink v1 (`0xFE`) and v2 (`0xFD`) frames by magic byte.
- CRC-16/CCITT (IBM) validation over the header + payload for both versions
  (`crc16_ibm`).
- Decoding of four well-known message IDs (`MavlinkMessageId`): HEARTBEAT (0),
  SYS_STATUS (1), GPS_RAW_INT (24), ATTITUDE (30) — plus an `Unknown(u32)`
  catch-all.
- Connection-health validation via `StateValidator` (`MavlinkValidator`) checking
  connection, a received HEARTBEAT, a non-zero system ID, and a consecutive-error
  threshold.
- TUP normalisation via `DataNormalizer` (`MavlinkNormalizer`) mapping each frame to
  a `tup::DataPoint`.
- `#![no_std]` + `alloc` (heap collections only; no `std`).
- `proptest` fuzz tests (`mod fuzz`) asserting `FrameParser::parse` never panics on
  arbitrary input.

## Architecture

The parser (`MavlinkParser::parse`) branches on the first byte:

- **v1** (`0xFE`): `magic(1) + length(1) + seq(1) + sys(1) + comp(1) + msg_id(1) +
  payload(N) + checksum(2)`. Total length `6 + payload_len + 2`; the first 6 header
  bytes plus the payload are CRC-16/CCITT checked (init `0xFFFF`).
- **v2** (`0xFD`): `magic(1) + length(1) + incompat(1) + compat(1) + seq(1) + sys(1)
  + comp(1) + msg_id(3, LE) + payload(N) + checksum(2)`. Total length
  `10 + payload_len + 2`; the first 10 header bytes plus the payload are CRC checked.

The decoded result is the `MavlinkFrame` struct (`version`, `sequence`,
`system_id`, `component_id`, `message_id`, `payload`, `checksum`). Incompatible and
compatible flags are parsed but currently ignored (no v2 signature/extension
handling — see Status).

`MavlinkValidator` consumes a `MavlinkState` (`connected`, `system_id`,
`component_id`, `heartbeat_received`, `last_ok_timestamp`, `error_count`) and
rejects the state when disconnected, no HEARTBEAT seen, `system_id == 0`, or more
than 15 consecutive errors.

`MavlinkNormalizer` produces a `DataPoint` keyed by
`device_id = "mavlink:{system_id}:{component_id}"` and
`point_id = "msg:{message_id}"`; the value is the first 4 payload bytes as a
little-endian `i64` when the payload is ≥ 4 bytes, otherwise the raw payload bytes
or — with no payload — the integer message ID.

## Usage

```rust
use tpt_protocol_core::{FrameParser, DataNormalizer};
use out_protocol_mavlink::{MavlinkParser, MavlinkNormalizer, MavlinkVersion, MavlinkMessageId};

fn main() {
    // Build a valid v1 HEARTBEAT (magic 0xFE, len=4, seq=1, sys=1, comp=1, msg=0).
    let mut raw: Vec<u8> = vec![0xFE, 0x04, 0x01, 0x01, 0x01, 0x00];
    raw.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // payload
    // CRC over header(1..) + payload, init 0xFFFF, appended little-endian.
    // (The CRC is computed by the same CRC-16/CCITT routine the parser uses to
    // validate frames; the parser recomputes and checks it during `parse`.)
    let crc = compute_mavlink_crc(&raw[1..]);
    raw.extend_from_slice(&crc.to_le_bytes());

    let parser = MavlinkParser::new();
    let frame = parser.parse(&raw).unwrap();
    assert_eq!(frame.version, MavlinkVersion::V1);
    assert_eq!(frame.message_id, MavlinkMessageId::Heartbeat);

    let normalizer = MavlinkNormalizer::new();
    let dp = normalizer.normalize(&frame).unwrap();
    assert_eq!(dp.source_protocol, tpt_protocol_tup::SourceProtocol::Mavlink);
    assert_eq!(dp.device_id, "mavlink:1:1");
    assert_eq!(dp.point_id, "msg:HEARTBEAT");
}

// Stand-in for the parser's internal CRC-16/CCITT (IBM) routine — see `crc16_ibm`
// in src/lib.rs. Defined here only so the example is self-contained.
fn compute_mavlink_crc(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — the full implementation: `MavlinkVersion` / `MavlinkMessageId`
  enums, `MavlinkFrame` struct, `MavlinkParser` (`FrameParser`) with `parse_v1` /
  `parse_v2`, `MavlinkState` + `MavlinkValidator` (`StateValidator`),
  `MavlinkNormalizer` (`DataNormalizer`), unit tests, and the `proptest` `mod fuzz`.

## Status / Conformance

- **`CRC_EXTRA` not validated.** MAVLink v2 computes its CRC over the header,
  payload, *and* a per-message `CRC_EXTRA` seed byte. This crate validates the
  CRC-16/CCITT over header + payload only and **omits the `CRC_EXTRA` seed**; it
  therefore accepts frames with an incorrect seed. This is a documented
  correctness gap (not a memory-safety issue) requiring the full message-ID→seed
  table, and is deferred to a follow-up (see root `CHANGELOG.md` "Known gaps" and
  workspace `TODO.md`).
- **No v2 signature / incompat-flag handling.** The incompatibility and
  compatibility flags are read but ignored; signed/encrypted v2 frames and the
  optional tail byte are not processed.
- **No message-payload decoding.** Individual message fields (attitude, GPS,
  status) are not decoded; only the raw payload bytes (or first 4 bytes as an
  integer) reach the normalizer.
- Crates `out-protocol-ethercat` and `out-protocol-mavlink` are intentionally
  private (not published to crates.io) — see the workspace `TODO.md`; this is a
  distribution decision, not a feature gap.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` /
  `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.
- [`out-protocol-ethercat`](../out-protocol-ethercat) — the other private from-scratch
  reimplementation in the workspace.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
