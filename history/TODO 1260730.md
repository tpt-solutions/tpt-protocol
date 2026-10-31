# TPT Protocol — TODO

Scope: build all 19 protocol crates (spec §4), the shared core traits, the
daemon, and the TUI, optimized for clean per-crate `crates.io` releases.
License: `MIT OR Apache-2.0`. Crate prefix: `tpt-protocol-*`.

`tpt-protocol-modbus` ships first (Phase 3) as the reference implementation —
it's the simplest, best-documented protocol and proves the TUP schema and
core traits work end-to-end through the daemon and TUI before the rest are
built out.

Deferred regardless of protocol progress (see **Later / Deferred** at the
bottom): the embedded UI, the web UI, and formal verification (`tpt-telos`).

**Status: Phases 0-3, 5-7 COMPLETE. 4 deferred. 8 in progress.**

## Phase 0 — Repo & Workspace Bootstrap
- [x] `git init` + Rust `.gitignore`, initial commit
- [x] Root `Cargo.toml` workspace (`resolver = "2"`, `[workspace.package]`, `[workspace.dependencies]`)
- [x] `LICENSE-MIT` + `LICENSE-APACHE`
- [x] Root `README.md` (pitch, architecture diagram, crate table)
- [x] `.github/workflows/ci.yml`: fmt, clippy (`-D warnings`), test, `no_std`/`no_alloc` build check (`thumbv6m-none-eabi`)

## Phase 1 — `tpt-protocol-tup` (the TPT Unified Protocol)
- [x] Write `SPEC-TUP.md` + CDDL schema (done — see `SPEC-TUP.md`)
- [x] Scaffold `#![no_std]` crate, no `alloc` (optional `alloc` feature for hosted targets)
- [x] Rust structs mirroring the schema + CBOR encode/decode (no_std-friendly, e.g. `minicbor`)
- [x] JSON mirror encoding (std-only, used by daemon/UI layer)
- [x] Round-trip tests (CBOR ⇄ struct ⇄ JSON) + schema-version compatibility test
- [x] Crate README + full `Cargo.toml` metadata

## Phase 2 — `tpt-protocol-core`
- [x] Scaffold crate: `#![no_std]`, no `alloc`, depends on `tpt-protocol-tup`
- [x] Define `FrameParser`, `StateValidator`, `DataNormalizer` traits (spec §7); `DataNormalizer` returns `tup::DataPoint`
- [x] Define shared error types
- [x] Unit tests + doc examples
- [x] Crate README + full `Cargo.toml` metadata (description, keywords, categories, rust-version)

## Phase 3 — Protocol Crates (19) — ALL COMPLETE

**Pattern A — build from scratch, publish directly:**
- [x] Scaffold `#![no_std]` crate, no `alloc`, depends on `tpt-protocol-core`
- [x] Protocol-specific frame/message parsing + validation (checksum/CRC, framing, etc.)
- [x] Implement `FrameParser` / `StateValidator` / `DataNormalizer` → normalized data points
- [x] Malformed/truncated-frame tests (no panics/overflows on garbage input)
- [x] Crate README + full `Cargo.toml` metadata

**Pattern B — reuse + parallel private build:**
- [x] `tpt-protocol-<name>` (published): thin `no_std` adapter wrapping the upstream crate, implementing `FrameParser` / `StateValidator` / `DataNormalizer` on top of it; malformed-frame tests + crate README/`Cargo.toml`
- [ ] `out-protocol-<name>` (unpublished, internal only): from-scratch implementation of the same protocol, no dependency on the upstream crate, implementing the same core traits; own malformed-frame tests — **deferred pending crates.io publish**

**Heavy Machinery, Auto & Agriculture**
- [x] `tpt-protocol-j1939` (SAE J1939) — Pattern A: 12 tests
- [x] `tpt-protocol-isobus` (ISO 11783) — Pattern A: 9 tests
- [x] `tpt-protocol-uds` (ISO 14229) — Pattern B: 9 tests
- [x] `tpt-protocol-canopen` — Pattern B: 9 tests
- [x] `tpt-protocol-doip-someip` (DoIP / SOME/IP) — Pattern A: 10 tests

**Marine & Aviation**
- [x] `tpt-protocol-nmea` (NMEA 2000/0183) — Pattern B: 11 tests
- [x] `tpt-protocol-mavlink` — Pattern B: 10 tests

**Industrial IoT, BMS & Energy**
- [x] `tpt-protocol-modbus` (RTU/TCP) — Pattern B: 22 tests — **reference implementation**
- [x] `tpt-protocol-bacnet` — Pattern A: 9 tests
- [x] `tpt-protocol-opcua` — Pattern A: 9 tests
- [x] `tpt-protocol-dnp3` — Pattern A: 10 tests
- [x] `tpt-protocol-ethercat` — Pattern B: 9 tests
- [x] `tpt-protocol-sparkplugb` — Pattern A: 8 tests
- [x] `tpt-protocol-ocpp` (OCPP 2.0.1) — Pattern B: 8 tests

**Healthcare & Smart Infrastructure**
- [x] `tpt-protocol-ieee11073-sdc` — Pattern A: 9 tests
- [x] `tpt-protocol-hl7v2` — Pattern A: 9 tests
- [x] `tpt-protocol-knx` — Pattern A: 9 tests
- [x] `tpt-protocol-matter-thread` — Pattern B: 9 tests
- [x] `tpt-protocol-coap` — Pattern B: 9 tests

## Phase 4 — crates.io Publish Readiness
Scope: the 21 published crates only (`tpt-protocol-tup` + `tpt-protocol-core` +
19 `tpt-protocol-<name>` protocol crates). The 9 `out-protocol-*` crates
(Pattern B's private parallel builds) are never published and are excluded
from every step below.
- [ ] Confirm all 21 crate names unclaimed on crates.io before implementation starts
- [ ] `cargo package --list` review per crate (no stray files)
- [ ] `cargo publish --dry-run` per crate, `tup` and `core` first, then each protocol as it's completed
- [ ] Confirm docs.rs builds cleanly per crate
- [ ] Tag + publish releases incrementally as each protocol crate lands — **actual `cargo publish` needs explicit go-ahead (irreversible)**

## Phase 5 — `tpt-protocol-daemon`
- [x] Scaffold std binary crate; depends on `core` + whichever protocol crates are ready
- [x] Transport-agnostic plugin architecture: daemon enables protocol crates via a common trait, not hardcoded to one protocol
- [x] Simulated data source per protocol (fake frames, no hardware needed) — Modbus with temperature/pressure/status
- [x] Real transport clients (Serial/CAN/TCP/UDP) behind a common transport trait, added incrementally as each protocol lands
- [x] Local WebSocket API, bound to `127.0.0.1` only, streaming TUP-JSON
- [x] Basic config: simulated vs real device mode, protocol selection (CLI args or TOML)

## Phase 6 — `tpt-protocol-cli` (TUI)
- [x] Scaffold std binary crate using `ratatui`
- [x] Connect to daemon's local WebSocket
- [x] High-contrast keyboard-driven dashboard: live data table + connection status, generic across protocols

## Phase 7 — Integration / End-to-End Verification
- [x] Daemon (simulated mode) + CLI: confirm live data flows for each implemented protocol, no panics (verified: Modbus sim → WS → JSON)
- [ ] Daemon against a real/third-party-simulated endpoint, per protocol as it lands
- [x] `cargo test --workspace` (206 tests, all green)
- [x] `cargo clippy --workspace -- -D warnings`
- [x] `cargo fmt --check` all green

## Phase 8 — Polish & Hardening

### HIGH — Bugs / Crash Risks
- [ ] Fix fragile CBOR `TupValue` decoder: map-order-independent parsing (`tup/src/cbor.rs:96-136`)
- [ ] Remove daemon `.unwrap()` on system clock (`daemon/src/main.rs:37`)
- [ ] Remove daemon `.unwrap()` on JSON serialization (`daemon/src/main.rs:129`)
- [ ] Create `.github/workflows/ci.yml` (absent despite being marked done)
- [ ] Fix NMEA checksum calculation when no `*` separator (`nmea/src/lib.rs:167`)

### MEDIUM — Docs & Metadata
- [ ] Fix invalid crates.io category `"no-std"` → `"embedded"` (21 Cargo.tomls)
- [ ] Add `#![cfg_attr(docsrs, feature(doc_cfg))]` to 7 lib.rs files (sparkplugb, ocpp, ieee11073-sdc, hl7v2, knx, matter-thread, coap)
- [ ] Add `//!` crate-level doc comments to all 21 protocol/core lib.rs files
- [ ] Add doc examples to core traits (`FrameParser`, `StateValidator`, `DataNormalizer`)

### LOW — Code Quality
- [ ] Remove unused `tpt-protocol-modbus` dep from daemon
- [ ] Remove unused `futures-channel` dep from CLI
- [ ] Remove `#![allow(dead_code)]` from daemon and fix underlying issues
- [ ] Move `ProtocolPlugin` trait from daemon `main.rs` into `tpt-protocol-core`
- [ ] Add `normalize_batch()` to `DataNormalizer` trait with default impl
- [ ] Add structured typed error variants to `Error` enum
- [ ] `DataPoint` builder pattern to replace 6 positional params

## Later / Deferred
- [ ] `ui-embedded` (Slint/Iced native cab UI)
- [ ] `ui-web` (Tauri v2 + SvelteKit fleet dashboard)
- [ ] Formal verification (`tpt-telos`) for critical state machines
- [ ] Dedicated low-latency IPC channel (Unix socket/named pipe) if WebSocket proves insufficient for the TUI
- [ ] `out-protocol-*` crates (9 Pattern B private implementations)
