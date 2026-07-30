# TPT Protocol — TODO

Scope: build all protocol crates (spec §4), the shared core traits, the
daemon, and the TUI, optimized for clean per-crate `crates.io` releases.
License: `MIT OR Apache-2.0`. Crate prefix: `tpt-protocol-*` (published) /
`out-protocol-*` (private, not published).

`tpt-protocol-modbus` ships first (Phase 3) as the reference implementation —
it's the simplest, best-documented protocol and proves the TUP schema and
core traits work end-to-end through the daemon and TUI before the rest are
built out.

**NOTE:** 9 crates that have existing well-known Rust alternatives on crates.io
were initially renamed to `out-protocol-*` and set `publish = false`, on the
assumption that publishing a redundant reimplementation next to an
already-good crates.io crate adds nothing. Auditing each alternative's
no_std support and license turned up concrete gaps for 7 of the 9, so those
7 were promoted back to published `tpt-protocol-*` crates. Only 2 remain
private, where the existing crate genuinely has no gap:

| Crate | crates.io original | Original's issue | Verdict |
|---|---|---|---|
| `modbus` | `tokio-modbus` | std + tokio only, no no_std | published (no_std gap) |
| `coap` | `coap` (coap-rs) | std + tokio only, no no_std | published (no_std gap) |
| `canopen` | `canopen` | std-only, depends on `socketcan` (Linux-only) | published (no_std/portability gap) |
| `ocpp` | `ocpp` | v0.1.0 from 2019, ~7 lines, effectively abandoned | published (no working alternative) |
| `nmea` | `nmea` | no_std-capable but Apache-2.0 only | published (license gap) |
| `matter-thread` | `rs-matter` | no_std, no-alloc, but Apache-2.0 only | published (license gap) |
| `uds` | *(no real ISO-14229 crate exists on crates.io)* | n/a | published (fills total absence) |
| `ethercat` | `ethercrab` | std **and** no_std, `MIT OR Apache-2.0` | private (no gap) |
| `mavlink` | `mavlink` | std + embedded feature, `MIT/Apache-2.0` | private (no gap) |

`out-protocol-*` crates are all `#![no_std]` (using `alloc` for heap
collections, not full std) and depend only on in-workspace `tpt-protocol-core`
/ `tpt-protocol-tup` (both `MIT OR Apache-2.0`) — they're from-scratch
reimplementations, not wrappers, so there's never a dependency-license
concern regardless of publish status. "Private" (`publish = false`) only
affects crates.io distribution; private crates are still full workspace
members with complete TUP normalization, used identically by the daemon.

17 protocol crates + `tup` + `core` = **19 published crates**. `ethercat`
and `mavlink` stay `out-protocol-*` (private).

Deferred regardless of protocol progress (see **Later / Deferred** at the
bottom): the embedded UI, the web UI, and formal verification (`tpt-telos`).

**Status: Phases 0-3, 5-7 COMPLETE. 4 in progress (blocked on go-ahead). 8 COMPLETE.**

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

**Pattern B — scratch impl originally kept private; 7 of 9 promoted to
published `tpt-protocol-*` after the no_std/license audit (see NOTE above),
2 remain private (`out-protocol-*`, no gap vs. the existing crate):**
- [x] `tpt-protocol-uds` (ISO 14229) — 9 tests — published (no existing crate)
- [x] `tpt-protocol-canopen` — 9 tests — published (no_std gap)
- [x] `tpt-protocol-nmea` (NMEA 2000/0183) — 11 tests — published (license gap)
- [x] `out-protocol-mavlink` — 10 tests — private (no gap)
- [x] `tpt-protocol-modbus` (RTU/TCP) — 22 tests — published (no_std gap) — **reference implementation**
- [x] `out-protocol-ethercat` — 9 tests — private (no gap)
- [x] `tpt-protocol-ocpp` (OCPP 2.0.1) — 8 tests — published (no maintained alternative)
- [x] `tpt-protocol-matter-thread` — 9 tests — published (license gap)
- [x] `tpt-protocol-coap` — 9 tests — published (no_std gap)

## Phase 4 — crates.io Publish Readiness
Scope: the 19 published crates (`tpt-protocol-tup` + `tpt-protocol-core` +
17 `tpt-protocol-<name>` protocol crates). `out-protocol-ethercat` and
`out-protocol-mavlink` stay `publish = false` and are excluded from every
step below (no crates.io gap vs. `ethercrab`/`mavlink` — see NOTE above).
- [x] Confirm all 19 crate names unclaimed on crates.io (all free, including the 7 promoted from `out-protocol-*`)
- [x] `cargo package --list` review per crate — all clean, no stray files
- [x] `tpt-protocol-tup` packages cleanly (31.3 KiB, 11 files — the only crate with no internal deps)
- [ ] `cargo package --list --no-verify` for dependent crates — blocked: core depends on tup (unpublished), protocols depend on core (unpublished). Must publish in order: tup → core → protocols → daemon/cli
- [ ] `cargo publish --dry-run` per crate — same dependency-chain blocker; publish in DAG order
- [ ] Confirm docs.rs builds cleanly per crate — requires publishing first
- [ ] **`cargo publish` for all 21 crates requires explicit go-ahead (irreversible)**

Publish order: `tpt-protocol-tup` → `tpt-protocol-core` → 19 protocol crates (any order) → daemon/cli

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

### HIGH — Bugs / Crash Risks (ALL DONE)
- [x] Fix fragile CBOR `TupValue` decoder: map-order-independent parsing (`tup/src/cbor.rs`)
- [x] Remove daemon `.unwrap()` on system clock (`daemon/src/main.rs`)
- [x] Remove daemon `.unwrap()` on JSON serialization (`daemon/src/main.rs`)
- [x] Create `.github/workflows/ci.yml` (fmt, clippy, test, no_std thumbv6m)
- [x] Fix NMEA checksum calculation when no `*` separator (`nmea/src/lib.rs`)

### MEDIUM — Docs & Metadata (ALL DONE)
- [x] Fix invalid crates.io category `"no-std"` → `"embedded"` (21 Cargo.tomls)
- [x] Add `#![cfg_attr(docsrs, feature(doc_cfg))]` to 7 lib.rs files
- [x] Add `//!` crate-level doc comments to all 21 protocol/core lib.rs files
- [x] Add doc examples to core traits (`FrameParser`, `StateValidator`, `DataNormalizer`)

### LOW — Code Quality
- [x] Remove unused `tpt-protocol-modbus` dep from daemon
- [x] Remove unused `futures-channel` dep from CLI
- [x] Remove `#![allow(dead_code)]` from daemon and fix underlying issues
- [x] Move `ProtocolPlugin` trait from daemon `main.rs` into `tpt-protocol-core` (behind `std` feature, conditional `futures-util` dep)
- [x] Add `normalize_batch()` to `DataNormalizer` trait with default impl
- [x] Add structured typed error variants to `Error` enum
- [x] `DataPoint` builder (`with_timestamp`, `with_quality`) to complement `with_unit`/`with_raw`

## Later / Deferred
- [ ] `ui-embedded` (Slint/Iced native cab UI)
- [ ] `ui-web` (Tauri v2 + SvelteKit fleet dashboard)
- [ ] Formal verification (`tpt-telos`) for critical state machines
- [ ] Dedicated low-latency IPC channel (Unix socket/named pipe) if WebSocket proves insufficient for the TUI
