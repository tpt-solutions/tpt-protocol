# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-11

### Added

- Published to crates.io. Renamed from `out-protocol-mavlink`, with `publish = false` removed.
- Every message in the common dialect is named through the `Message` trait, and the
  frame keeps the decoded `MavMessage`.
- The v2 signature block is kept on the frame (`MavlinkSignature`). It is parsed, not
  verified.
- `normalize_all` gives one point per field for HEARTBEAT, SYS_STATUS, ATTITUDE and
  GPS_RAW_INT. `normalize` still returns the first point.
- A CRC_EXTRA regression test, which re-signs a v1 frame with the wrong seed and expects
  it to be rejected. Structured property tests, and a runnable `parse_mavlink` example.
- Initial release: MAVLink v1 / v2 (drone telemetry) support in the TPT Protocol suite.
- Frame parsing in `MavlinkParser` with automatic v1 (`0xFE`) / v2 (`0xFD`)
  detection by magic byte, decoding `sequence`, `system_id`, `component_id`,
  `message_id`, payload, and checksum for both wire versions.
- CRC-16/CCITT (IBM) validation over header + payload for both v1 and v2 frames
  (`crc16_ibm`).
- `MavlinkMessageId` decoding for HEARTBEAT (0), SYS_STATUS (1), GPS_RAW_INT (24),
  and ATTITUDE (30), with an `Unknown(u32)` fallback.
- `MavlinkState` / `MavlinkValidator` (`StateValidator`) enforcing connection, a
  received HEARTBEAT, a non-zero system ID, and a consecutive-error threshold (≤ 15).
- `MavlinkNormalizer` (`DataNormalizer`) mapping frames to `tup::DataPoint`s keyed
  by `device_id = "mavlink:{system_id}:{component_id}"` and
  `point_id = "msg:{message_id}"`, carrying the first 4 payload bytes as a
  little-endian integer, the raw payload bytes, or the integer message ID.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.

### Changed

- Frames are decoded with the `mavlink` 0.15 crate (`embedded` and `common` features),
  instead of a hand-written parser. `MavlinkFrame` no longer carries the wire checksum.
- GPS latitude and longitude are degrees (float), and altitude is metres. They were raw
  `1e-7` and millimetre integers.
- Trailing bytes after a frame are an error.
