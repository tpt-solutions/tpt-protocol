# tpt-protocol-daemon

Background service for TPT Protocol — transports, protocol plugins, and local API.

## Usage

```bash
# Run with simulated Modbus data
cargo run --bin tpt-protocol-daemon -- --simulate modbus

# Run with custom port
cargo run --bin tpt-protocol-daemon -- --port 9090 --simulate modbus

# Run with TOML config
cargo run --bin tpt-protocol-daemon -- --config daemon.toml
```

## Architecture

- **Protocol plugins**: loaded via a common trait, currently supports Modbus simulation
- **WebSocket API**: streams TUP-JSON to connected clients on `127.0.0.1:9080`
- **Config**: CLI args or TOML file

## API

WebSocket endpoint: `ws://127.0.0.1:9080/`

Messages are JSON arrays of `tpt_protocol_tup::DataPoint` objects.
