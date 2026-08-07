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
All three are now COMPLETE — see the **Later / Deferred** section.

**Status: Phases 0-3, 5-9 COMPLETE and verified (`cargo test --workspace` green, 0 failures). Phase 4 crates.io publish is blocked pending explicit go-ahead (irreversible; dependency-chain order tup→core→protocols→daemon/cli). Deferred items COMPLETE except three future-work items (low-latency IPC, real transport, MAVLink CRC_EXTRA).**

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
- [x] Transport-agnostic plugin architecture: daemon enables protocol crates via a common trait, not hardcoded to one protocol — **closed out in Phase 9.3: all 19 protocols wired as simulated `ProtocolPlugin` impls via the `src/plugins/` module (simulation-only; `ProtocolPlugin` has `simulate()` but no real-I/O method — real transport is future work).**
- [x] Simulated data source per protocol (fake frames, no hardware needed) — Modbus with temperature/pressure/status
- [ ] Real transport clients (Serial/CAN/TCP/UDP) behind a common transport trait, added incrementally as each protocol lands — **corrected 2026-08-07: none exist (no serial/CAN dep anywhere). Moved to Later/Deferred; Phase 9.3 only adds simulation for the remaining 18 protocols. Phase 9.3 COMPLETE: all 19 protocols now have simulated `ProtocolPlugin` impls wired into the daemon; real transport stays future work (see Later/Deferred).**
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

## Phase 9 — Security Hardening, Daemon Completion, Adoption Tooling

Scope: full-repo review (2026-08-07) found the daemon overstates its own
Phase 5 status (only Modbus is wired, simulation-only, no real transport
exists), plus 8 concrete panic/overflow/allocation bugs in protocol parsers
findable with inputs as small as 8-20 bytes (no fuzzing existed to catch
them). This phase closes both gaps and adds core adoption tooling.

### 9.1 — Truth reconciliation & cleanup
- [x] Resolve 6 `#[allow(dead_code)]` spots: delete genuinely-dead
      `Hl7MessageType::as_str`, `InteractionType::as_str`; make
      `CoapCode::as_u8`, `KnxServiceType::as_str`, `GroupAddress::to_u16`
      `pub` round-trip API instead of suppressing the lint
- [x] Dedupe NMEA: production checksum logic calls
      `compute_nmea_checksum` instead of duplicating it inline

### 9.2 — Security fixes (audit findings, each with a regression test)
- [x] DNP3 OOB panic on truncated frame (`dnp3/src/lib.rs:141-151`,
      truncation guard doesn't cover trailing CRC bytes) — regression:
      `truncated_length_seven_no_panic`, `truncated_short_header_no_panic`
- [x] Matter-Thread off-by-one OOB panic on Float TLV
      (`matter-thread/src/lib.rs:159-167`, guard should be `< 21` not `< 20`) —
      regression: `float_tlv_off_by_one_no_panic`
- [x] NMEA panic on empty talker/sentence slice
      (`nmea/src/lib.rs:27-30,70-74`, missing `is_empty()` guard)
- [x] HL7v2 panic on non-UTF8-char-boundary slice
      (`hl7v2/src/lib.rs:105-106,152-153`) — now str-based `from_utf8`+`split`
- [x] CLI panic on non-UTF8-char-boundary slice truncating device_id/point_id
      (`cli/src/main.rs:220-228`) — `truncate_to` respects char boundaries;
      regression: `truncate_respects_char_boundary`
- [x] DoIP/SOME-IP + OCPP `u32` length integer overflow into invalid slice
      range (`doip-someip/src/lib.rs:225`, `ocpp/src/lib.rs:155-165`) —
      `checked_add`/`saturating_sub`; regressions:
      `doip_huge_payload_length_no_overflow`, `huge_payload_length_no_overflow`
- [x] Unbounded allocation from wire-controlled count: IEEE11073-SDC
      (`ieee11073-sdc/src/lib.rs:104,179,214,240`), SparkplugB
      (`sparkplugb/src/lib.rs:92`), Modbus (`modbus/src/lib.rs:430`, lower
      severity) — cap `with_capacity` to `MAX_*` bounds; regressions:
      `huge_count_no_unbounded_alloc`, `huge_metric_count_no_unbounded_alloc`,
      `write_multiple_huge_quantity_no_unbounded_alloc`
- [x] Daemon: warn (not silently allow) when `--bind` isn't a loopback
      address (`daemon/src/main.rs` `is_loopback_bind`)
- [x] Daemon: drop unused `native-tls` feature from `tokio-tungstenite`
      dep (no TLS acceptor exists; pulls in `openssl` for nothing)
- [x] Root `Cargo.toml`: `overflow-checks = true` in `[profile.release]`
- [x] Add `proptest` dev-dependency + a per-protocol-crate property test
      feeding random bytes into `FrameParser::parse`, asserting no panic
      (all 19 protocol crates + 2 private crates have `mod fuzz`)
- [ ] Documented follow-up (not fixed this pass): MAVLink CRC validation
      omits the real per-message `CRC_EXTRA` seed byte — correctness gap,
      not memory-safety; needs the full message-ID→seed table

### 9.3 — Daemon: wire all 19 protocols as simulated plugins
- [x] Split `daemon/src/main.rs`'s inline `ModbusPlugin` into a
      `src/plugins/` module, one simulated `ProtocolPlugin` impl per
      protocol crate (19 total)
- [x] Extend daemon's protocol-name dispatch to cover all 19
- [x] Add `--list-protocols` flag
- [x] Add `--replay <file>` mode: run raw bytes from a file through a
      chosen protocol's `FrameParser`/`DataNormalizer` instead of only
      synthetic simulation
- [x] Vary simulated `Quality` (`Uncertain`/`Bad`/`Stale`, not just `Good`)
      so the CLI's color coding is demoable out of the box
- [x] Update daemon README + root README quick-start for all-19 support
- [x] Un-check "Real transport clients (Serial/CAN/TCP/UDP)" in Phase 5
      above — simulation-only, real transport is still not implemented and
      stays future work (see Later/Deferred)

### 9.4 — Adoption tooling (core essentials)
- [x] `CONTRIBUTING.md` (dev setup, workspace commands, how to add
      protocol crate #20)
- [x] `CHANGELOG.md` (Keep a Changelog format)
- [x] Runnable examples: `tup/examples/roundtrip.rs`,
      `core/examples/custom_protocol.rs`, `modbus/examples/parse_frame.rs`
- [x] `daemon/daemon.example.toml` + Python/JS client snippets showing how
      to consume the TUP WebSocket stream without Rust
      (`daemon/examples/client.py`, `daemon/examples/client.js`)
- [x] `scripts/new-protocol-crate.sh` scaffolding script (generates
      Cargo.toml + lib.rs skeleton + README template + registers the new
      workspace member)
- [x] Standalone `schema/tup-v1.cddl` + `schema/tup-v1.schema.json`,
      referenced from `SPEC-TUP.md`, with a test validating the spec's
      worked JSON example against the schema (`worked_example_matches_json_schema`)
- [x] `deny.toml` + `cargo-deny` CI job (license allow-list, advisories,
      duplicate-version bans)

## Later / Deferred
- [x] `ui-embedded` (Slint native dashboard) — `tpt-protocol-ui-embedded`, connects to daemon WS
- [x] `ui-web` (Tauri v2 + SvelteKit fleet dashboard) — `tpt-protocol-ui-web`, connects to daemon WS
- [x] Formal verification (`tpt-telos`) for critical state machines — `tpt-protocol-telos`: bundled `.telos` specs for Modbus CRC bounds, validator error-counter, and normalizer register bounds; `cargo test -p tpt-protocol-telos --features verify` discharges them with the QF_LRA verifier
- [ ] Dedicated low-latency IPC channel (Unix socket/named pipe) if WebSocket proves insufficient for the TUI
- [ ] Real transport clients (Serial/CAN/TCP/UDP) for the daemon — genuine
      hardware I/O, beyond Phase 9.3's simulation-only wiring
- [ ] MAVLink `CRC_EXTRA` per-message seed table for full spec compliance
