# TPT Protocol

[![CI](https://github.com/tpt-solutions/tpt-protocol/actions/workflows/ci.yml/badge.svg)](https://github.com/tpt-solutions/tpt-protocol/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![crates.io](https://img.shields.io/crates/v/tpt-protocol-tup.svg)](https://crates.io/crates/tpt-protocol-tup)
[![docs.rs](https://img.shields.io/docsrs/tpt-protocol-tup)](https://docs.rs/tpt-protocol-tup)

**The universal translation layer for the physical world.**

TPT Protocol is an open-source, memory-safe Rust suite that decodes, normalizes, and unifies 19 heavily gatekept industrial, medical, and hardware communication protocols into a single, openly-specified message envelope (TUP).

```text
                    ┌──────────────────────────────────────┐
                    │            TPT Protocol              │
                    ├──────────────────────────────────────┤
                    │   ┌──────────────────────────────┐   │
                    │   │     tpt-protocol-daemon       │   │
                    │   │   ┌────────────────────────┐  │   │
                    │   │   │  Transport (Serial/CAN/ │  │   │
                    │   │   │  TCP/UDP)               │  │   │
                    │   │   └───────────┬────────────┘  │   │
                    │   │   ┌───────────▼────────────┐  │   │
┌───────────────┐   │   │   │  19 Protocol Parsers   │  │   │
│  Modbus RTU/  │───┼───┼──▶│  (FrameParser,         │  │   │  ┌────────────────┐
│  TCP Device   │   │   │   │   StateValidator,      │  │   │  │  TUI (ratatui) │
├───────────────┤   │   │   │   DataNormalizer)      │  │   │  │  (CLI)         │
│  J1939/ISOBUS │───┼───┼──▶│   ──────────────────▶  │  │──┼──▶├────────────────┤
│  ECU          │   │   │   │   TUP (CBOR/JSON)      │  │   │  │  Embedded UI   │
├───────────────┤   │   │   └────────────────────────┘  │   │  │  (Slint)       │
│  BACnet       │───┼───┼──▶│  ┌────────────────────┐   │   │  ├────────────────┤
│  Building     │   │   │   │  │ Local WebSocket/   │   │   │  │  Web UI        │
├───────────────┤   │   │   │  │ REST API (127.0.0.1)│   │   │  │  (Tauri/Svelte)│
│  ...15 more   │───┼───┼──▶│  └────────────────────┘   │   │  └────────────────┘
└───────────────┘   │   └──────────────────────────────┘   │
                    └──────────────────────────────────────┘
```

## Protocol Crates

| Crate | Protocol | Category | Strategy |
|-------|----------|----------|----------|
| `tpt-protocol-modbus` | Modbus RTU/TCP | Industrial IoT | Pattern A (scratch, no_std alt. to `tokio-modbus`) |
| `tpt-protocol-j1939` | SAE J1939 | Heavy Machinery | Pattern A (scratch) |
| `tpt-protocol-isobus` | ISO 11783 (ISOBUS) | Agriculture | Pattern A (scratch) |
| `tpt-protocol-uds` | ISO 14229 (UDS) | Automotive | Pattern A (scratch — no existing crate) |
| `tpt-protocol-canopen` | CANopen | Automation | Pattern A (scratch, no_std alt. to `canopen` crate) |
| `tpt-protocol-doip-someip` | DoIP / SOME/IP | Automotive | Pattern A (scratch) |
| `tpt-protocol-nmea` | NMEA 2000/0183 | Marine | Pattern A (scratch, MIT-compatible alt. to `nmea` crate) |
| `tpt-protocol-mavlink` | MAVLink | Drone | Published (wraps `mavlink` crate) |
| `tpt-protocol-bacnet` | BACnet | Building | Pattern A (scratch) |
| `tpt-protocol-opcua` | OPC UA | Industry 4.0 | Pattern A (scratch) |
| `tpt-protocol-dnp3` | DNP3 | Energy/SCADA | Pattern A (scratch) |
| `tpt-protocol-ethercat` | EtherCAT | Factory | Published (alt. to `ethercrab`) |
| `tpt-protocol-sparkplugb` | Sparkplug B | IIoT | Pattern A (scratch) |
| `tpt-protocol-ocpp` | OCPP-J (1.6, 2.0.1) | EV Charging | Pattern A (scratch — `ocpp` crate unmaintained) |
| `tpt-protocol-ieee11073-sdc` | IEEE 11073 SDC | Medical | Pattern A (scratch) |
| `tpt-protocol-hl7v2` | HL7 v2.x | Healthcare | Pattern A (scratch) |
| `tpt-protocol-knx` | KNX | Smart Building | Pattern A (scratch) |
| `tpt-protocol-matter-thread` | Matter/Thread | Smart Home | Pattern A (scratch, MIT-compatible alt. to `rs-matter`) |
| `tpt-protocol-coap` | CoAP | IoT | Pattern A (scratch, no_std alt. to `coap` crate) |

## Repository Structure

```
tpt-protocol/
├── crates/tpt-protocol-tup/       # TUP schema (CBOR + JSON), no_std
├── crates/tpt-protocol-core/      # Core traits (FrameParser, StateValidator, DataNormalizer)
├── crates/tpt-protocol-<name>/    # 19 published protocol crates (Pattern A — scratch, or a wrapper: MAVLink)
├── crates/tpt-protocol-daemon/    # Background service
├── crates/tpt-protocol-bindings/  # C ABI, WebAssembly and Python bindings (not published)
├── crates/tpt-protocol-cli/       # TUI (ratatui dashboard)
├── crates/tpt-protocol-ui-embedded/ # Native embedded UI (Slint)
├── crates/tpt-protocol-ui-web/    # Web fleet dashboard (Tauri v2 + SvelteKit)
├── crates/tpt-protocol-telos/     # Formal-verification specs (tpt-telos)
├── SPEC-TUP.md                    # TUP schema specification
└── Cargo.toml                     # Workspace root
```

## Quick Start

```bash
# Build all crates
cargo build --workspace

# Test all crates
cargo test --workspace

# List the 19 protocols the daemon can simulate
cargo run --bin tpt-protocol-daemon -- --list-protocols

# Run the daemon with simulated data for any supported protocol
cargo run --bin tpt-protocol-daemon -- --simulate modbus

# Replay a capture through a protocol's parser (hex, raw, candump or pcap)
cargo run --bin tpt-protocol-daemon -- --simulate bacnet --replay frame.hex

# In another terminal, launch the TUI
cargo run --bin tpt-protocol-cli
```

The daemon has two data sources. By default every protocol produces synthetic
TUP data points, so the pipeline runs end-to-end without hardware. Pass a
transport flag to decode real bytes instead: `--tcp-connect`, `--tcp-listen`,
`--udp-bind`, `--stdin`, `--modbus-poll` (a Modbus TCP poller), and (with the `serial` or
`socketcan` feature) `--serial` and `--can`. Serial and SocketCAN are tested only against a
pseudo-terminal pair and a `vcan0` interface, not real hardware. The daemon can also send its
points to MQTT, InfluxDB, OpenTelemetry, Prometheus or an OPC UA server, and record them to
NDJSON, MCAP or Parquet. See the [daemon README](crates/tpt-protocol-daemon/README.md#transports).

## Architecture

- **no_std by default**: All protocol crates compile to bare-metal targets (`thumbv6m-none-eabi`).
- **Local-first**: The daemon binds its API to `127.0.0.1` by default, and logs a warning for any other address. Zero telemetry, zero cloud dependency.
- **TUP envelope**: Every protocol normalizes into the TPT Unified Protocol — CBOR on the wire, JSON for the UI, openly specified in `SPEC-TUP.md`.
- **Memory-safe**: Written in Rust. `tpt-protocol-telos` formally checks three Modbus specifications (in this repository; not published). Other state machines are not yet covered.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE) at your option.
