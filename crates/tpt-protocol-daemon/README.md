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

# A candump -L capture of CAN frames, decoded frame by frame through a CAN protocol
cargo run --bin tpt-protocol-daemon -- --simulate canopen --replay can.log --replay-format candump

# A pcap or pcapng capture: the TCP/UDP payloads on port 502, replayed in order
cargo run --bin tpt-protocol-daemon -- --simulate modbus --replay trace.pcapng --replay-format pcap --pcap-port 502

# Every bundled sample under samples/ replays to a data point (the test checks this)
cargo run --bin tpt-protocol-daemon -- --simulate modbus --replay samples/modbus-write-single-register-1.hex

# Which protocol is this capture? Rank all of them by how much they decode
cargo run --bin tpt-protocol-daemon -- --detect capture.bin --replay-format raw

# Read bytes piped in, record every data point as NDJSON, and simulate every 200 ms
cat capture.bin | cargo run --bin tpt-protocol-daemon -- --stdin --simulate modbus --replay-format raw --record out.ndjson
cargo run --bin tpt-protocol-daemon -- --simulate modbus --interval 200

# Require a token (as ?token= or Authorization: Bearer) and allow a dev-server origin
cargo run --bin tpt-protocol-daemon -- --simulate modbus --token "$TOKEN" --allow-origin http://localhost:5173

# Run with TOML config (it can also hold [[map]] rules)
cargo run --bin tpt-protocol-daemon -- --config daemon.example.toml

# Health check: active clients, messages sent, lagged events, rejected handshakes
curl http://127.0.0.1:9080/healthz
```

Ctrl-C stops the daemon. Logs go to stderr; set `RUST_LOG=debug` for more
detail (the default level is `info`).

The `samples/` directory holds one replayable frame per protocol: `.hex` files
for binary frames and `.raw` files for text protocols (NMEA, OCPP, HL7 v2 and
IEEE 11073 SDC, which replay with `--replay-format raw`). The
`every_bundled_sample_replays_to_a_data_point` test replays each one.

### Mapping rules (`[[map]]`)

A TOML rule renames points, sets units, scales values, and checks a range. It
applies to the points that match its `protocol`, `device` and `point` fields
(each optional). The first matching rule applies, and points that match none
pass through unchanged.

```toml
[[map]]
protocol = "modbus"
point = "modbus-point-1"
name = "tank_level"   # the new point id
unit = "m"
scale = 0.001        # value = raw * scale + offset
min = 0.0            # outside [min, max] the quality becomes "uncertain"
max = 5.0
```

Unknown keys, a `min` above `max`, and an unknown protocol name are config
errors, reported at startup. The stage runs between the devices and the
clients, so the WebSocket API and `--record` both see mapped points.

### Anomaly rules (`[[anomaly]]`)

Stateful checks that mark a point `Uncertain` (its value is kept) when it looks
replayed, arrives too fast, or jumps. Each rule matches by protocol, device and
point, and needs at least one check:

- `order = true`: a timestamp that does not advance past the series' last one
  (a replay or a clock step).
- `min_interval_ms = N`: two points of a series closer than N ms (a rate spike).
- `max_step = X`: a numeric value that moves more than X from the last one (a
  step spike).

The checks run after `[[map]]`, so they see the point ids clients see. The table
of tracked series is capped at 10,000, so many distinct point ids cannot grow it
without bound. **Not done:** function-code rules. A TUP point does not carry the
Modbus or other function code it came from, so they need frame metadata the
plugins would have to pass on.

### Recording and detection

- `--record FILE` appends every data point to `FILE` as NDJSON, one object per
  line. Each line is the JSON the WebSocket sends, so `jq` and any JSON-lines
  reader can read it.
- `--detect FILE` runs every protocol's framer over a capture and prints its
  frames, points and errors, best first. It is a heuristic: a framer without a
  length field (UDS) can accept almost any byte, so read the counts, not only the order.

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

### Transports (`src/transport/`)

One endpoint supplies the bytes, chosen with one of these flags:

- `--tcp-connect HOST:PORT`: connects to a device and reconnects when the link drops.
- `--tcp-listen ADDR:PORT`: accepts devices, one task per connection.
- `--udp-bind ADDR:PORT`: receives datagrams. Each datagram is one message.
- `--serial PATH --baud N`: reads a serial port, reopening it when it disappears.
  Build with `--features serial`. Without the feature the flag fails with a clear error.
- `--can IFACE`: reads a SocketCAN interface such as `can0` or `vcan0`. CANopen
  takes the 11-bit COB-ID and J1939/ISOBUS take the 29-bit identifier. Build with
  `--features socketcan` on Linux. Other platforms fail with a clear error.
- `--stdin`: reads bytes piped into the daemon. It needs no hardware, which makes
  it the simplest way to feed a capture through the live pipeline.
- `--modbus-poll HOST:PORT` (with `--simulate modbus`): polls a Modbus TCP server.
  A Modbus server sends nothing unprompted, so the passive transports never see
  its data; this one asks. Every `--interval` ms it reads `--poll-quantity`
  registers (1 to 125) from `--poll-address`, of `--poll-unit`, using function
  `--poll-function` (3 holding, the default, or 4 input). Each register becomes a
  point named by its absolute address, such as `holding_register[100]`. It
  reconnects after a failure, and it logs exception responses and timeouts (3 s).
  It cannot be combined with another transport.

Every transport implements two small traits in `transport/mod.rs`. `Transport`
yields links (a connection, an accepted peer, a socket, a port), and `ByteLink`
yields the bytes of one link as a stream or as whole messages. The generic
`supervise` loop frames each link with the protocol's `try_parse`, normalizes
the frames, reconnects where the transport allows, and logs each step. Adding a
transport means implementing the two traits; the framer and the supervisor stay
the same. `supervisor_retries_a_failed_link_then_stops_at_none` and
`datagram_links_decode_each_read_on_its_own` test that path without any network.

A stream that fails to parse discards its buffer and resyncs. Frames that parse
but carry no data (such as Modbus read requests) are skipped. The
`ProtocolPlugin` trait still exposes only `simulate()`. The transport runs
beside the plugin, and the plugin is not used when an endpoint is set.
Without a flag the daemon simulates, and `--replay` reads bytes from a file.

### Sinks (`src/sink.rs`)

A sink sends the points somewhere other than the local WebSocket. The MQTT sink
is behind the `mqtt` feature (`cargo build --features mqtt`):

- `--mqtt HOST:PORT [--mqtt-prefix tpt]`: publishes each point as its JSON to
  `<prefix>/<protocol>/<device>/<point>`, QoS 0, not retained. Publishing never
  waits: when the broker is slow or down, points are dropped and counted in the
  log, so the devices are not held up. Plain TCP only (no TLS, no auth yet).

Tested against a real Mosquitto 2.1.2 broker (the `mqtt` CI job, and an end-to-end
run: pymodbus server, `--modbus-poll`, `--mqtt`, `mosquitto_sub`). Not done: InfluxDB,
Prometheus remote-write, OpenTelemetry, and an OPC UA server.

### Simulation

Without a transport the daemon simulates one protocol. `--interval MS` sets the
time between points (1 to 3600000 ms, default 1000). The points rotate through
the four qualities, so every colour in the TUI shows.

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
  real-I/O method, so the transports live beside it in `transport/`. Serial and
  SocketCAN have not run against real hardware.
- **Protocols are feature-gated, not runtime-gated.** Each protocol is a Cargo
  feature (on by default). A build without a protocol feature does not compile.
- **Mapping is point-by-point.** `[[map]]` rules cannot compute across points,
  such as sums or derived quantities. They also cannot match on a value.
- **Capture import covers CAN and pcap.** `--replay-format candump` reads CAN
  captures. `--replay-format pcap` reads classic pcap and pcapng, over Ethernet or
  raw IP, IPv4 only. It does not reassemble TCP (segments are taken in capture
  order, with no gap filling), does not reorder, and skips fragments and IPv6.
  Tested with synthetic captures; no capture from a real network has been replayed.
  MCAP and Parquet export are not done.
- **Sinks are limited to the WebSocket and NDJSON files.** There is no MQTT,
  OPC UA server, InfluxDB or OpenTelemetry output.
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
│   ├── main.rs                # CLI, WebSocket server, health check, replay
│   ├── capture.rs             # candump -L reader
│   ├── detect.rs              # protocol auto-detection (--detect)
│   ├── mapping.rs             # [[map]] rules and the mapping stage
│   ├── record.rs              # NDJSON recorder (--record)
│   ├── plugins/
│   │   └── mod.rs             # ProtocolPlugin impls, one feature per protocol
│   └── transport/
│       ├── mod.rs             # Transport and ByteLink traits, framer, supervisor
│       ├── net.rs             # TCP connect/listen and UDP
│       ├── serial.rs          # serial ports (feature `serial`)
│       ├── can.rs             # SocketCAN (feature `socketcan`, Linux)
│       └── stdin.rs           # --stdin
├── samples/                   # one replayable frame per protocol
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
- The 19 protocol crates, each with a simulated plugin in `src/plugins/mod.rs`.
  The simulation does not run the parser; decoding real bytes goes through
  the transports.
- `tpt-protocol-cli` — the `ratatui` TUI that connects to this daemon's
  WebSocket API.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
