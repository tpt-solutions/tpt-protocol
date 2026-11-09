# tpt-protocol-daemon

Background service for the [TPT Protocol](https://github.com/tpt-protocol/tpt-protocol) — simulated protocol plugins and a local WebSocket API.

The daemon is a `std` binary crate that drives every protocol crate in the
workspace through a common plugin interface and streams normalized TUP data
to local clients over WebSocket. It can read real bytes over **TCP** (client
or listener), **UDP**, **serial** (with the `serial` cargo feature), and
**SocketCAN** (Linux only, with the `socketcan` cargo feature).

## Status / Conformance

- **Version:** `0.1.0` (initial release).
- **Transports:** TCP (connect or listen) and UDP read real bytes. Each
  protocol's `try_parse` frames them and its normalizer produces TUP data
  points. Without a transport flag the daemon runs the simulation. Serial and
  SocketCAN are implemented behind cargo features. Serial was tested against a
  socat pseudo-terminal pair, and SocketCAN against a `vcan0` virtual interface.
  Neither has been tested on real hardware.
- **Security posture:** the WebSocket API is bound to loopback (`127.0.0.1`)
  by default. On a loopback bind, the handshake rejects any `Host` header that
  is not a loopback name (DNS-rebinding defence) and any browser `Origin` that
  is not allowed (Tauri webview origins are always allowed; add others with
  `--allow-origin`). Setting `--token` additionally requires a token. A
  non-loopback bind prints a warning, disables the `Host` check, and is not
  refused.

## Usage

```bash
# List all 19 protocols the daemon can simulate
cargo run --bin tpt-protocol-daemon -- --list-protocols

# Run with simulated Modbus data
cargo run --bin tpt-protocol-daemon -- --simulate modbus

# Run with custom port
cargo run --bin tpt-protocol-daemon -- --port 9090 --simulate modbus

# Replay bytes through a protocol's parser/normalizer. The file is ASCII hex
# by default; pass --replay-format raw for binary captures.
cargo run --bin tpt-protocol-daemon -- --simulate bacnet --replay frame.hex
cargo run --bin tpt-protocol-daemon -- --simulate bacnet --replay capture.bin --replay-format raw

# The samples/ directory holds two Modbus RTU frames to try this with, e.g.
# cargo run --bin tpt-protocol-daemon -- --simulate modbus --replay samples/modbus-write-single-register-1.hex
# Their CRCs are checked by the bundled_replay_samples_decode test.

# Require a token (as ?token= or Authorization: Bearer) and allow a dev-server origin
cargo run --bin tpt-protocol-daemon -- --simulate modbus --token "$TOKEN" --allow-origin http://localhost:5173

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
| `mavlink` | `tpt-protocol-mavlink` |
| `bacnet` | `tpt-protocol-bacnet` |
| `opcua` | `tpt-protocol-opcua` |
| `dnp3` | `tpt-protocol-dnp3` |
| `ethercat` | `tpt-protocol-ethercat` |
| `sparkplugb` | `tpt-protocol-sparkplugb` |
| `ocpp` | `tpt-protocol-ocpp` |
| `ieee11073-sdc` | `tpt-protocol-ieee11073-sdc` |
| `hl7v2` | `tpt-protocol-hl7v2` |
| `knx` | `tpt-protocol-knx` |
| `matter-thread` | `tpt-protocol-matter-thread` |
| `coap` | `tpt-protocol-coap` |

### Transports

The daemon's `transport` module reads bytes from one endpoint, chosen with one
of three flags:

- `--tcp-connect HOST:PORT`: connects to a device and reconnects when the link drops.
- `--tcp-listen ADDR:PORT`: accepts devices, one task per connection.
- `--udp-bind ADDR:PORT`: receives datagrams. Each datagram must hold whole frames.
- `--serial PATH --baud N`: reads a serial port, reopening it when it disappears.
  Build with `--features serial`. Without the feature the flag fails with a clear error.
- `--can IFACE`: reads a SocketCAN interface such as `can0` or `vcan0`. CANopen
  takes the 11-bit COB-ID and J1939/ISOBUS take the 29-bit identifier. Build with
  `--features socketcan` on Linux. Other platforms fail with a clear error.

A stream that fails to parse discards its buffer and resyncs. Frames that parse
but carry no data (such as Modbus read requests) are skipped. The
`ProtocolPlugin` trait still exposes only `simulate()`. The transport runs
beside the plugin, and the plugin is not used when an endpoint is set.
Without a flag the daemon simulates, and `--replay` reads bytes from a file.

### Local API (WebSocket)

The daemon runs a WebSocket server using `tokio-tungstenite`. It binds to
`--bind` (default `127.0.0.1`) on `--port` (default `9080`) and streams TUP
data to all connected clients via a `tokio::sync::broadcast` channel.

- **Endpoint:** `ws://127.0.0.1:9080/`
- **Messages:** a JSON array (one element per tick) of `tpt_protocol_tup::DataPoint`
  objects, serialized with `tpt_protocol_tup::json::to_json_string`. Each object
  has the fields `schema_version`, `source_protocol`, `device_id`, `point_id`,
  `timestamp`, `value`, `quality`, and an optional `unit`. Example:

  ```json
  [{"schema_version":1,"source_protocol":"modbus","device_id":"modbus-sim-01",
    "point_id":"modbus-point-1","timestamp":1753900000000,"value":68.5,
    "quality":"good"}]
  ```

- **Control messages:** a client that falls behind the broadcast channel is
  sent `{"type":"lagged","dropped":n}` instead of an array, so clients must
  check for a non-array message before iterating. The server answers WebSocket
  `Ping` frames and stops on `Close`.
- **Limits:** handshakes must complete within 5 seconds, at most 64 clients are
  served at once, and incoming messages are capped at 64 KiB.
- The server warns (does not refuse) when `--bind` is not a loopback address.

### Configuration (`daemon.example.toml`)

The daemon accepts a TOML config via `--config`. Fields:

```toml
# Protocol to simulate (see `--list-protocols` for all 19).
simulate = "modbus"

# WebSocket API port.
port = 9080

# Bind address. Loopback only by default. A non-loopback bind prints a warning.
bind = "127.0.0.1"
```

Unknown keys are rejected. CLI flags override config-file values
(`--simulate`, `--port`, `--bind`); the config file supplies defaults when
flags are absent.

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

- **Simulation without a transport flag.** Without a transport flag the daemon
  simulates data. The `ProtocolPlugin` trait has a `simulate()` method but no
  real-I/O method, so the transports live beside it in `transport.rs`. None of
  them has run against real hardware.
- **No authentication.** The WebSocket API is unauthenticated and loopback by
  default; do not expose it on an untrusted network.
- **MAVLink `CRC_EXTRA`** validation gap lives in the `tpt-protocol-mavlink`
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
