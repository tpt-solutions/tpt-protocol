# Changelog

All notable changes to this crate are documented in this file.

The format is based on [Keep a Changelog](https://keepachangel.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-30

### Added
- Initial release: ratatui terminal dashboard for live TPT Protocol telemetry in the TPT Protocol suite.
- `tpt-protocol-cli` binary connecting to the daemon's local WebSocket (`tokio-tungstenite`) with a 3-second auto-reconnect loop.
- clap-derived flags `--url` (default `ws://127.0.0.1:9080`) and `--daemon <HOST:PORT>` (expands to `ws://<HOST:PORT>`).
- Live UI: title bar with CONNECTED/DISCONNECTED status, message counter, and uptime; scrollable `Table` of Time/Protocol/Device/Point/Value/Quality.
- Quality color coding (Good=green, Bad=red, Uncertain=yellow, Stale=dark gray).
- Rolling buffer of the last 100 data points and UTF-8-safe column truncation (`truncate_to`) for long device/point ids.
- Parsing of both JSON arrays and single JSON objects via `tpt_protocol_tup::json::from_json_str`.
- Keyboard controls: `q`/`Esc`/`Ctrl+C` quit, `↑`/`↓` scroll.
- Unit tests for `truncate_to` covering multi-byte char boundaries.
