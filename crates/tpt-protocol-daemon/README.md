# tpt-protocol-daemon

Background service for the [TPT Protocol](https://github.com/tpt-protocol/tpt-protocol) — simulated protocol plugins and a local WebSocket API.

The daemon is a `std` binary crate that drives every protocol crate in the
workspace through a common plugin interface and streams normalized TUP data
to local clients over WebSocket. It is **simulation-only**: it does not
perform real hardware I/O (see [_Known gaps_](#known-gaps) below).

## Status / Conformance

- **Version:** `0.1.0` (initial release).
- **Simulation only:** all 19 protocol crates are wired in as simulated
  `ProtocolPlugin` implementations that emit synthetic TUP data points. There
  is **no real transport** (Serial/CAN/TCP/UDP) — genuine hardware I/O is
  future work (see the workspace `TODO.md`, Later/Deferred section, and the
  root `CHANGELOG.md`).
- **Security posture:** the WebSocket API has no authentication and is bound
  to loopback (`127.0.0.1`) by default. Binding to a non-loopback address
  prints a warning but is not refused.

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
cargo run --bin tpt-protocol-daemon -- --config daemon.example.toml
```

Ctrl-C stops the daemon.

## Architecture

### Protocol plugins (`src/plugins/mod.rs`)

The daemon's plugin architecture is transport-agnostic and data-source
agnostic: every protocol crate gets a `ProtocolPlugin` implementation behind
the `tpt_protocol_core::ProtocolPlugin` trait, and the daemon dispatches by
protocol name rather than hard-coding any one protocol.

```rust
pub trait ProtocolPlugin: Send + 'static {
    fn simulate(
        self: Box<Self>,
    ) -> Pin<Box<dyn futures_util::stream::Stream<Item = tup::DataPoint> + Send>>;
}
```

- `src/plugins/mod.rs` defines one simulated plugin per protocol crate via a
  `def_protocol!` macro (19 entries). Each plugin's `simulate` runs a
  `futures_util` `Stream` that sleeps ~1 s per tick and emits a synthetic
  `DataPoint`.
- `build_plugin(name)` constructs the simulated plugin for a given protocol
  name (returns `Err` for unknown names).
- `list_protocols()` returns the static list of all 19 supported protocol
  names (used by `--list-protocols`).
- `replay_one(name, bytes)` runs raw bytes through the named protocol's
  `FrameParser`/`DataNormalizer` instead of synthetic simulation (used by
  `--replay`).
- Synthetic data points rotate through `Good` / `Uncertain` / `Bad` /
  `Stale` quality (`cycle_quality`) so the CLI/TUI color coding is demoable
  out of the box.

The 19 supported protocols (dispatched by the lowercase name shown):

| Name | Crate |
| --- | --- |
| `modbus` | `tpt-protocol-modbus` |
| `j1939` | `tpt-protocol-j1939` |
| `isobus` | `tpt-protocol-isobus` |
| `uds` | `tpt-protocol-uds` |
| `canopen` | `tpt-protocol-canopen` |
| `doip-someip` | `tpt-protocol-doip-someip` |
| `nmea` | `tpt-protocol-nmea` |
| `mavlink` | `out-protocol-mavlink` (private) |
| `bacnet` | `tpt-protocol-bacnet` |
| `opcua` | `tpt-protocol-opcua` |
| `dnp3` | `tpt-protocol-dnp3` |
| `ethercat` | `out-protocol-ethercat` (private) |
| `sparkplugb` | `tpt-protocol-sparkplugb` |
| `ocpp` | `tpt-protocol-ocpp` |
| `ieee11073-sdc` | `tpt-protocol-ieee11073-sdc` |
| `hl7v2` | `tpt-protocol-hl7v2` |
| `knx` | `tpt-protocol-knx` |
| `matter-thread` | `tpt-protocol-matter-thread` |
| `coap` | `tpt-protocol-coap` |

### Transports

There is **no real transport layer** yet. The `ProtocolPlugin` trait exposes
only `simulate()` — there is no real-I/O method. Real Serial/CAN/TCP/UDP
clients behind a common transport trait are future work (TODO.md Phase 5
"Real transport clients", Later/Deferred). In the meantime, "transport" is
synthetic: per-tick generated `DataPoint`s (simulation mode) or bytes read
from a file (`--replay`).

### Local API (WebSocket)

The daemon runs a WebSocket server using `tokio-tungstenite`. It binds to
`--bind` (default `127.0.0.1`) on `--port` (default `9080`) and streams TUP
data to all connected clients via a `tokio::sync::broadcast` channel.

- **Endpoint:** `ws://127.0.0.1:9080/`
- **Messages:** a JSON array (one element per tick) of `tpt_protocol_tup::DataPoint`
  objects, serialized by the daemon's `datapoint_to_json` helper. Each object
  has the fields `schema_version`, `source_protocol`, `device_id`, `point_id`,
  `timestamp`, `value`, `quality`, and an optional `unit`. Example:

  ```json
  [{"schema_version":1,"source_protocol":"modbus","device_id":"modbus-sim-01",
    "point_id":"modbus-point-1","timestamp":1753900000000,"value":68.5,
    "quality":"good"}]
  ```

- The server warns (does not refuse) when `--bind` is not a loopback address,
  since the API has no authentication.

### Configuration (`daemon.example.toml`)

The daemon accepts a TOML config via `--config`. Fields:

```toml
# Protocol to simulate (see `--list-protocols` for all 19).
simulate = "modbus"

# WebSocket API port.
port = 9080

# Bind address. Loopback only by default — the API has no authentication.
# A non-loopback bind prints a warning.
bind = "127.0.0.1"
```

CLI flags override config-file values (`--simulate` and `--port`/`--bind`);
the config file supplies defaults when flags are absent.

## Running it (clients)

Start the daemon (it prints `WebSocket server listening on ws://127.0.0.1:9080`),
then point a client at that endpoint:

```bash
# Rust CLI (tpt-protocol-cli) connects to the same WebSocket automatically.
# Python consumer (no Rust required): pip install websocket-client
python crates/tpt-protocol-daemon/examples/client.py

# Node consumer:
#   node crates/tpt-protocol-daemon/examples/client.js
```

Both `examples/client.py` and `examples/client.js` subscribe to the TUP
stream and print each data point. The daemon's default simulate target is
`modbus` if neither `--simulate` nor a config `simulate` is given.

## Known gaps

- **Simulation only.** No real Serial/CAN/TCP/UDP transport exists; the
  `ProtocolPlugin` trait has a `simulate()` method but no real-I/O method.
  Real hardware I/O is future work (workspace `TODO.md`, Later/Deferred;
  root `CHANGELOG.md` Phase 5).
- **No authentication.** The WebSocket API is unauthenticated and loopback by
  default; do not expose it on an untrusted network.
- **MAVLink `CRC_EXTRA`** validation gap lives in the `out-protocol-mavlink`
  crate (a correctness gap, not memory-safety), not the daemon itself.

## Crate layout

```
crates/tpt-protocol-daemon/
├── Cargo.toml
├── README.md
├── CHANGELOG.md
├── daemon.example.toml        # Example TOML config
├── src/
│   ├── main.rs                # CLI, WebSocket server, simulation loop
│   └── plugins/
│       └── mod.rs             # ProtocolPlugin impls for all 19 protocols
└── examples/
    ├── client.py              # Python WebSocket consumer
    └── client.js              # Node WebSocket consumer
```

## Related crates

- [`tpt-protocol-core`](https://github.com/tpt-protocol/tpt-protocol) — defines
  the `ProtocolPlugin`, `FrameParser`, `StateValidator`, `DataNormalizer`
  traits and the `Error` type the daemon relies on.
- `tpt-protocol-tup` — the TUP envelope (`DataPoint`, `TupValue`, `Quality`,
  `SourceProtocol`) streamed over the WebSocket API.
- The 19 protocol crates wired in via `src/plugins/mod.rs`.
- `tpt-protocol-cli` — the `ratatui` TUI that connects to this daemon's
  WebSocket API.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
