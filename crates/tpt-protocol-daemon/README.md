# tpt-protocol-daemon

Background service for TPT Protocol — simulated protocol plugins and a local WebSocket API.

## Usage

```bash
# List all 19 protocols the daemon can simulate
cargo run --bin tpt-protocol-daemon -- --list-protocols

# Run with simulated Modbus data
cargo run --bin tpt-protocol-daemon -- --simulate modbus

# Run with custom port
cargo run --bin tpt-protocol-daemon -- --port 9090 --simulate modbus

# Replay raw bytes (hex or raw file) through a protocol's parser/normalizer
cargo run --bin tpt-protocol-daemon -- --simulate bacnet --replay frame.hex

# Run with TOML config
cargo run --bin tpt-protocol-daemon -- --config daemon.toml
```

## Architecture

- **Protocol plugins**: one simulated `ProtocolPlugin` per protocol crate (all 19),
  dispatched by name. Each emits synthetic TUP data points with rotating quality
  (`Good`/`Uncertain`/`Bad`/`Stale`) so the TUI color coding is demoable.
- **WebSocket API**: streams TUP-JSON to connected clients on `127.0.0.1:9080`
  (binds to loopback only; a non-loopback `--bind` prints a warning, since the
  API has no authentication).
- **Replay mode**: `--replay <file>` runs raw bytes through the selected
  protocol's `FrameParser`/`DataNormalizer` instead of the synthetic simulation.
- **Simulation only**: there is no real transport yet (Serial/CAN/TCP/UDP).

## API

WebSocket endpoint: `ws://127.0.0.1:9080/`

Messages are JSON arrays of `tpt_protocol_tup::DataPoint` objects.
