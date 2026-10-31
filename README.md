# TPT Protocol

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
│  ...16 more   │───┼───┼──▶│  └────────────────────┘   │   │  └────────────────┘
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
| `tpt-protocol-ocpp` | OCPP 2.0.1 | EV Charging | Pattern A (scratch — `ocpp` crate unmaintained) |
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
├── crates/tpt-protocol-<name>/    # 17 published protocol crates (Pattern A — scratch)
├── crates/out-protocol-<name>/    # 2 private from-scratch implementations (not published)
├── crates/tpt-protocol-daemon/    # Background service
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

# Replay raw bytes from a file through a protocol's parser (hex or raw)
cargo run --bin tpt-protocol-daemon -- --simulate bacnet --replay frame.hex

# In another terminal, launch the TUI
cargo run --bin tpt-protocol-cli
```

The daemon currently ships **simulation only** — every protocol produces
synthetic TUP data points so the pipeline can be exercised end-to-end without
hardware. Real transport (Serial/CAN/TCP/UDP) is future work.

## Architecture

- **no_std by default**: All protocol crates compile to bare-metal targets (`thumbv6m-none-eabi`).
- **Local-first**: The daemon binds its API to `127.0.0.1` only. Zero telemetry, zero cloud dependency.
- **TUP envelope**: Every protocol normalizes into the TPT Unified Protocol — CBOR on the wire, JSON for the UI, openly specified in `SPEC-TUP.md`.
- **Memory-safe**: Written in Rust. Critical state machines are formally verified with [`tpt-telos`](https://crates.io/crates/tpt-telos) (`tpt-protocol-telos`).

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE) at your option.
