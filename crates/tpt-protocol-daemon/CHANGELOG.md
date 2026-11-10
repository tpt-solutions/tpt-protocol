# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Transports behind one streaming framer, which uses each protocol's `try_parse`:
  - `--tcp-connect HOST:PORT` (reconnects), `--tcp-listen ADDR:PORT` (one task per
    connection), `--udp-bind ADDR:PORT` (one datagram is one message).
  - `--serial PATH --baud N` (feature `serial`) and `--can IFACE` (feature `socketcan`,
    Linux). CAN identifiers are wrapped for CANopen, J1939 and ISOBUS.
  - `--stdin`, for piped captures.
- `--modbus-poll HOST:PORT` polls a Modbus TCP server for registers, with
  `--poll-unit`, `--poll-function`, `--poll-address` and `--poll-quantity`. Each register is
  a point. It reconnects after failures.
- Replay formats: `--replay-format candump` and `--replay-format pcap` (classic pcap and
  pcapng, with `--pcap-port` to keep one conversation). The Modbus pcap replay pairs each
  response with its request by transaction id. One replayable sample per protocol is in
  `samples/`, and a daemon test replays each one.
- `--detect FILE` ranks the protocols that decode a capture.
- `[[map]]` rules in the TOML config: rename, unit, scale, offset and a range check that
  marks an out-of-range value as uncertain.
- `[[anomaly]]` rules on the TUP stream, which mark a point uncertain (its value is kept):
  a timestamp that does not advance, points of one series closer than a minimum interval,
  and a numeric step larger than a limit.
- Recording: `--record FILE` (NDJSON), `--record-mcap FILE` (feature `mcap-record`) and
  `--record-parquet FILE` (feature `parquet-record`; the footer is written only on a clean
  exit).
- Outputs: `--mqtt HOST:PORT` with `--mqtt-prefix` (feature `mqtt`), `--influx URL`,
  `--otel URL`, `--prometheus-remote-write URL`, and `--opcua-listen ADDR`, an OPC UA
  server (anonymous, `SecurityPolicy#None`, read-only).
- WebSocket hardening: the Host header is checked on loopback binds (DNS rebinding), the
  browser Origin is checked (Tauri origins by default, plus `--allow-origin`), and an
  optional `--token` is compared in constant time. Handshakes time out after 5 s, at most
  64 clients are served, and incoming messages are capped at 64 KiB.
- Clients that fall behind receive `{"type":"lagged","dropped":n}`.
- `daemon.example.toml`, and a config file that rejects unknown keys. CLI flags override
  the file.
- Replayed points are stamped at ingest when their source has no time.

### Changed
- `--replay-format` accepts `hex` (default), `raw`, `candump` and `pcap`. `--replay`
  requires `--simulate`.
- Plain-HTTP output (InfluxDB, OpenTelemetry) has no TLS. An `https://` URL is refused.
- The MQTT client is built without its default TLS stack. The sink uses plain TCP only.

### Fixed
- Rustdoc intra-doc links to private items, which failed `cargo doc -D warnings`.
- A lockfile dependency with four RUSTSEC advisories came in through the MQTT client's TLS
  stack. Removing it makes `cargo audit` and `cargo deny` pass.

## [0.1.0] - 2026-07-30

### Added
- Initial release: background service for the TPT Protocol suite — simulated protocol plugins plus a local WebSocket API.
- `std` binary crate (`tpt-protocol-daemon`) driving every protocol crate via the `tpt_protocol_core::ProtocolPlugin` trait.
- Plugin architecture in `src/plugins/mod.rs`: a `def_protocol!` macro generating one simulated `ProtocolPlugin` per protocol crate (19 total), with `build_plugin(name)`, `list_protocols()`, and `replay_one(name, bytes)`.
- WebSocket API (`tokio-tungstenite` + `tokio`) bound to `127.0.0.1:9080` by default, streaming JSON arrays of TUP `DataPoint`s over a `tokio::sync::broadcast` channel.
- CLI flags: `--simulate <protocol>`, `--port`, `--bind`, `--config <toml>`, `--list-protocols`, `--replay <file>` (hex or raw bytes through a protocol's `FrameParser`/`DataNormalizer`).
- TOML configuration (`daemon.example.toml`) with `simulate`, `port`, and `bind` fields; CLI flags override config-file values.
- Simulated data points that rotate through `Good` / `Uncertain` / `Bad` / `Stale` quality so CLI/TUI color coding is demoable out of the box.
- Non-loopback `--bind` warning (the API has no authentication); `overflow-checks = true` in the release profile is set at the workspace root.
- Runnable WebSocket consumer snippets: `examples/client.py` (Python) and `examples/client.js` (Node).
