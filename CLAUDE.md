# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

TPT Protocol is an open-source, memory-safe Rust workspace that decodes, normalizes, and unifies 19 industrial/medical/hardware communication protocols (Modbus, CANopen, NMEA, MAVLink, BACnet, OPC UA, DNP3, EtherCAT, OCPP, HL7 v2.x, Matter/Thread, CoAP, etc.) into one openly-specified message envelope: **TUP** (TPT Unified Protocol). Full schema/CDDL is in `SPEC-TUP.md`. `TODO.md` is the authoritative, actively-maintained phase-by-phase task tracker — check it before assuming what is/isn't done.

License: `MIT OR Apache-2.0` for every crate (`license.workspace = true`).

## Commands

```bash
# Build / test everything
cargo build --workspace
cargo test --workspace

# Single crate
cargo test -p tpt-protocol-modbus
cargo build -p tpt-protocol-modbus

# Lint / format (CI runs these with -D warnings)
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

# no_std build check (matches the CI `no_std` job)
cargo build --workspace --target thumbv6m-none-eabi

# Verify a crate packages cleanly before publishing
cargo package --list -p <crate-name>

# Run the daemon (simulated data) + TUI in separate terminals
cargo run --bin tpt-protocol-daemon -- --simulate modbus
cargo run --bin tpt-protocol-cli
```

CI (`.github/workflows/ci.yml`) runs independent jobs: `fmt`, `clippy`, `test`, `docs` (`cargo doc` with `-D warnings`), `telos` (formal verification, `--features verify`), `no_std` (build for `thumbv6m-none-eabi`), `msrv` (check on Rust 1.81), `cargo-deny`, `ui-embedded` (clippy, Windows runner), and `ui-web` (svelte-check and Vite build). It also runs weekly on a schedule. Nightly fuzzing of every parser runs from `.github/workflows/fuzz.yml`; cargo-fuzz does not link on Windows, so run fuzz targets on Linux (`cargo +nightly fuzz run <target> --fuzz-dir fuzz`). Any change to a protocol/core/tup crate must stay `no_std`-clean or it breaks that job.

## Architecture

**Layering**, bottom to top:

1. **`tpt-protocol-tup`** — the wire schema. `#![no_std]`, no `alloc` by default. Defines `DataPoint` (source_protocol, device_id, point_id, timestamp, `TupValue`, unit, quality, raw bytes), `TupValue`, `Quality`, `SourceProtocol`. Canonical binary encoding is CBOR (`cbor.rs`, via `minicbor`); a `std`-gated `json` module (`json.rs`) mirrors it isomorphically for the daemon's local API/UI. This is the *only* crate with zero internal deps.

2. **`tpt-protocol-core`** — shared traits, `#![no_std]` + `alloc`, depends only on `tpt-protocol-tup`:
   - `FrameParser` — raw bytes → protocol-specific `Frame`
   - `StateValidator` — validates connection/session `State`
   - `DataNormalizer` — `Frame` → `tup::DataPoint` (has a default `normalize_batch`)
   - `ProtocolPlugin` (behind the `std` feature, needs `futures-util`) — daemon-facing: yields a `Stream<Item = DataPoint>` from simulated or real hardware.

3. **Protocol crates** (`crates/tpt-protocol-<name>`) — one per protocol. Each is `#![no_std]` + `alloc`, depends only on `tpt-protocol-core` + `tpt-protocol-tup`, and implements `FrameParser` + `StateValidator` + `DataNormalizer` for that protocol's frame format (checksums/CRC/framing handled per-protocol). No protocol crate depends on another protocol crate.

4. **`tpt-protocol-daemon`** — std binary. Implements `ProtocolPlugin` per protocol (see `ModbusPlugin` in `main.rs` for the pattern — simulated streams today, real transports added incrementally), serves a local-only WebSocket API (binds `127.0.0.1`, via `tokio-tungstenite`) streaming TUP as JSON.

5. **`tpt-protocol-cli`** — std binary, `ratatui` TUI, connects to the daemon's WebSocket for a live data dashboard.

**Naming**: every protocol crate is `tpt-protocol-<name>` and is published to crates.io (`publish` unset). Each is a from-scratch reimplementation (never a wrapper around the crates.io crate it's compared to), with the same dependency shape (`tpt-protocol-core` + `tpt-protocol-tup` only). A crate is published because there is a concrete reason its alternative falls short: a no_std gap (e.g. `tokio-modbus`, `coap-rs`, `canopen`), a license gap (e.g. `nmea`, `rs-matter`), an unmaintained crate (`ocpp`), or no crate at all (`uds`). The exception is `tpt-protocol-ethercat` and `tpt-protocol-mavlink`, which are also published. See the reasoning table in `TODO.md` (Phase 3/4) before changing publish status on any crate.

**Design invariants** (from `spec.txt` / root `README.md`):
- **no_std by default** — core parsing/encoding logic must run on bare-metal (`thumbv6m-none-eabi` is the CI target). `std` is opt-in via feature flags (`tpt-protocol-tup/std`, `tpt-protocol-core/std`) and only needed by the daemon/CLI layer.
- **Local-first** — the daemon's API binds `127.0.0.1` only; no telemetry, no cloud dependency.
- **Memory-safe replacement, not a wrapper** — the point of every protocol crate is to be a genuine from-scratch reimplementation of that protocol's parsing logic, not a thin adapter over an existing crate.
- Workspace-shared metadata (`license`, `edition`, `rust-version`, `authors`, `homepage`, `repository`) lives in `[workspace.package]` in the root `Cargo.toml`; shared deps (`minicbor`, `serde`, `serde_json`, `hex`, `futures-util`, plus the two internal crates) live in `[workspace.dependencies]` — new crates should reference these via `{ workspace = true }` rather than repinning versions.
