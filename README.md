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
├───────────────┤   │   │   └────────────────────────┘  │   │  │  (deferred)    │
│  BACnet       │───┼───┼──▶│  ┌────────────────────┐   │   │  ├────────────────┤
│  Building     │   │   │   │  │ Local WebSocket/   │   │   │  │  Web UI        │
├───────────────┤   │   │   │  │ REST API (127.0.0.1)│   │   │  │  (deferred)    │
│  ...16 more   │───┼───┼──▶│  └────────────────────┘   │   │  └────────────────┘
└───────────────┘   │   └──────────────────────────────┘   │
                    └──────────────────────────────────────┘
```

## Protocol Crates

| Crate | Protocol | Category | Strategy |
|-------|----------|----------|----------|
| `tpt-protocol-modbus` | Modbus RTU/TCP | Industrial IoT | Pattern B (wraps `rmodbus`) |
| `tpt-protocol-j1939` | SAE J1939 | Heavy Machinery | Pattern A (scratch) |
| `tpt-protocol-isobus` | ISO 11783 (ISOBUS) | Agriculture | Pattern A (scratch) |
| `tpt-protocol-uds` | ISO 14229 (UDS) | Automotive | Pattern B (wraps `iso14229-1`) |
| `tpt-protocol-canopen` | CANopen | Automation | Pattern B (wraps `zencan`) |
| `tpt-protocol-doip-someip` | DoIP / SOME/IP | Automotive | Pattern A (scratch) |
| `tpt-protocol-nmea` | NMEA 2000/0183 | Marine | Pattern B (wraps `nmea`) |
| `tpt-protocol-mavlink` | MAVLink | Drone | Pattern B (wraps `mavlink-core`) |
| `tpt-protocol-bacnet` | BACnet | Building | Pattern A (scratch) |
| `tpt-protocol-opcua` | OPC UA | Industry 4.0 | Pattern A (scratch) |
| `tpt-protocol-dnp3` | DNP3 | Energy/SCADA | Pattern A (scratch) |
| `tpt-protocol-ethercat` | EtherCAT | Factory | Pattern B (wraps `ethercrab`) |
| `tpt-protocol-sparkplugb` | Sparkplug B | IIoT | Pattern A (scratch) |
| `tpt-protocol-ocpp` | OCPP 2.0.1 | EV Charging | Pattern B (wraps `rust-ocpp`) |
| `tpt-protocol-ieee11073-sdc` | IEEE 11073 SDC | Medical | Pattern A (scratch) |
| `tpt-protocol-hl7v2` | HL7 v2.x | Healthcare | Pattern A (scratch) |
| `tpt-protocol-knx` | KNX | Smart Building | Pattern A (scratch) |
| `tpt-protocol-matter-thread` | Matter/Thread | Smart Home | Pattern B (wraps `rs-matter`) |
| `tpt-protocol-coap` | CoAP | IoT | Pattern B (wraps `coap-lite`) |

## Repository Structure

```
tpt-protocol/
├── crates/tpt-protocol-tup/       # TUP schema (CBOR + JSON), no_std
├── crates/tpt-protocol-core/      # Core traits (FrameParser, StateValidator, DataNormalizer)
├── crates/tpt-protocol-<name>/    # 19 protocol crates
├── crates/tpt-protocol-daemon/    # Background service
├── crates/tpt-protocol-cli/       # TUI (ratatui dashboard)
├── out-protocol-<name>/           # Internal private implementations (not published)
├── SPEC-TUP.md                    # TUP schema specification
└── Cargo.toml                     # Workspace root
```

## Quick Start

```bash
# Build all crates
cargo build --workspace

# Test all crates
cargo test --workspace

# Run the daemon with simulated Modbus data
cargo run --bin tpt-protocol-daemon -- --simulate modbus

# In another terminal, launch the TUI
cargo run --bin tpt-protocol-cli
```

## Architecture

- **no_std by default**: All protocol crates compile to bare-metal targets (`thumbv6m-none-eabi`).
- **Local-first**: The daemon binds its API to `127.0.0.1` only. Zero telemetry, zero cloud dependency.
- **TUP envelope**: Every protocol normalizes into the TPT Unified Protocol — CBOR on the wire, JSON for the UI, openly specified in `SPEC-TUP.md`.
- **Memory-safe**: Written in Rust. Critical state machines target formal verification.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE) at your option.
