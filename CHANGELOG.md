# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Daemon now simulates **all 19** protocol crates via dedicated `ProtocolPlugin`
  implementations (`tpt-protocol-daemon/src/plugins/mod.rs`).
- `--list-protocols` flag lists every supported protocol.
- `--replay <file>` mode runs raw bytes through a protocol's `FrameParser` /
  `DataNormalizer` instead of synthetic simulation.
- Simulated data points rotate through `Good` / `Uncertain` / `Bad` / `Stale`
  quality so the CLI color coding is demoable out of the box.
- `proptest` fuzz tests (`mod fuzz`) added to all 19 protocol crates, asserting
  `FrameParser::parse` never panics on arbitrary input.
- Standalone schema files `schema/tup-v1.cddl` and `schema/tup-v1.schema.json`,
  with a test validating SPEC-TUP.md's worked example against the JSON schema.
- Runnable examples: `tpt-protocol-tup/examples/roundtrip.rs`,
  `tpt-protocol-core/examples/custom_protocol.rs`,
  `tpt-protocol-modbus/examples/parse_frame.rs`.
- `daemon.example.toml` and Python/JS WebSocket consumer snippets.
- `scripts/new-protocol-crate.sh` scaffolding script.
- `deny.toml` and a `cargo-deny` CI job (license/advisory/duplicate-version bans).

### Security
- Fixed DNP3 out-of-bounds panic on a truncated frame (trailing CRC guard).
- Fixed Matter-Thread off-by-one OOB panic on Float TLV (`< 21` vs `< 20`).
- Fixed DoIP/SOME-IP and OCPP `u32` length-integer overflow into an invalid
  slice range (`checked_add` / `saturating_sub`).
- Capped wire-controlled `with_capacity` allocations in IEEE 11073-SDC,
  Sparkplug B, and Modbus to prevent unbounded allocation.
- CLI no longer panics when truncating a multi-byte `device_id` / `point_id`
  (`truncate_to` respects UTF-8 char boundaries).
- Daemon warns (instead of silently allowing) when `--bind` is not loopback.
- Dropped the unused `native-tls` feature from `tokio-tungstenite` (no TLS
  acceptor existed).
- Root workspace sets `overflow-checks = true` in `[profile.release]`.

### Changed
- Reconciled `dead_code` suppressions in `tpt-protocol-knx` and
  `tpt-protocol-coap`: dead helpers (`KnxServiceType::as_str`,
  `GroupAddress::to_u16`, `CoapCode` classification methods) are now part of the
  public round-trip API instead of being `#[allow(dead_code)]`.
- Phase 5 "Real transport clients" item un-checked: the daemon remains
  simulation-only; real Serial/CAN/TCP/UDP transport is future work.

### Known gaps (documented, not fixed this pass)
- MAVLink CRC validation omits the real per-message `CRC_EXTRA` seed byte. This
  is a correctness gap (not memory-safety) requiring the full message-ID→seed
  table; deferred to a follow-up.
