# tpt-protocol-mavlink

MAVLink drone telemetry adapter (v1 and v2) for the TPT telemetry pipeline.
Published to crates.io (`publish` unset).

## Implementation

- **Decoding**: wraps the [`mavlink`](https://crates.io/crates/mavlink) crate (`default-features = false`, `embedded` + `common`). Magic byte, header, CRC-16 and CRC_EXTRA checks are done there.
- **Parser**: auto-detects v1 (0xFE) vs v2 (0xFD) by magic byte, then decodes with `read_v1_msg` / `read_v2_msg`
- **Messages**: HEARTBEAT (0), SYS_STATUS (1), GPS_RAW_INT (24), ATTITUDE (30); other IDs are kept as `Unknown`
- **Validator**: Checks connection, heartbeat received flag, system ID range, error thresholds
- **Normalizer**: Maps to DataPoints keyed by system:component and message ID

## Notes

- `MavlinkFrame` no longer carries the wire checksum, since the `mavlink` crate validates it during decode.
- The payload is re-serialized from the decoded message, so it matches the wire layout for the message.
