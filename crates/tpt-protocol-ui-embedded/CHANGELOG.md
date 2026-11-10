# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- A status line that shows the daemon connection.
- Pings from the daemon are answered with pongs.

### Changed
- A standalone workspace root with its own dependency pins. It is outside the main workspace,
  so its Slint tree does not affect the core build, CI or `cargo-deny`.
- Requires Rust 1.90 (`rust-version`). The library workspace stays at 1.85.
- The data channel is bounded.
- `Slint` spawn errors are returned instead of unwrapped.
- Marked `publish = false`.

## [0.1.0] - 2026-07-30

### Added
- Initial release: native embedded UI (Slint) for the TPT Protocol daemon in the TPT Protocol suite.
- `tpt-protocol-ui-embedded` binary with a `slint-build` compiled `src/ui.slint` `MainWindow` (dark, high-contrast theme).
- WebSocket connection to the daemon (`tokio-tungstenite`) with a 3-second auto-reconnect loop, decoding TUP JSON via `tpt_protocol_tup::json::from_json_str`.
- Live `ListView` of TUP `DataPoint`s bounded to the most recent 200 rows.
- clap `--url` flag (default `ws://127.0.0.1:9080`) for the daemon WebSocket endpoint.
- `DataRow` model (`time`, `protocol`, `device`, `point`, `value`, `quality`) feeding the Slint table.
