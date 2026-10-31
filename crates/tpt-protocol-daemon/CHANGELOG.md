# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
