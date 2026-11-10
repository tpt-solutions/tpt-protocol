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
were first set up as `out-protocol-*` with `publish = false`. Auditing each
alternative's no_std support and license found a concrete gap for every one of
them, so all 9 are published `tpt-protocol-*` crates. No `out-protocol-*` crate
remains in the workspace. The verdict for each:

| Crate | crates.io original | Original's issue | Verdict |
|---|---|---|---|
| `modbus` | `tokio-modbus` | std + tokio only, no no_std | published (no_std gap) |
| `coap` | `coap` (coap-rs) | std + tokio only, no no_std | published (no_std gap) |
| `canopen` | `canopen` | std-only, depends on `socketcan` (Linux-only) | published (no_std/portability gap) |
| `ocpp` | `ocpp` | v0.1.0 from 2019, ~7 lines, effectively abandoned | published (no working alternative) |
| `nmea` | `nmea` | no_std-capable but Apache-2.0 only | published (license gap) |
| `matter-thread` | `rs-matter` | no_std, no-alloc, but Apache-2.0 only | published (license gap) |
| `uds` | *(no real ISO-14229 crate exists on crates.io)* | n/a | published (fills total absence) |
| `ethercat` | `ethercrab` | std **and** no_std, `MIT OR Apache-2.0` | published as `tpt-protocol-ethercat` (no gap; from-scratch) |
| `mavlink` | `mavlink` | std + embedded feature, `MIT/Apache-2.0` | published as `tpt-protocol-mavlink` (wraps `mavlink` for decoding) |

Every protocol crate is `#![no_std]` (using `alloc` for heap collections, not
full std) and depends only on in-workspace `tpt-protocol-core` /
`tpt-protocol-tup` (both `MIT OR Apache-2.0`). They are from-scratch
reimplementations, not wrappers, so there is no dependency-license concern.

17 protocol crates + `tup` + `core` + `ethercat` + `mavlink` = **21 published crates**. (`ethercat` and `mavlink`
were `out-protocol-*` and are now `tpt-protocol-*` with publish unset.)

Deferred regardless of protocol progress (see **Later / Deferred** at the
bottom): the embedded UI, the web UI, and formal verification (`tpt-telos`).
All three are now COMPLETE — see the **Later / Deferred** section.

**Status: Phases 0-3, 5-9 COMPLETE and verified (`cargo test --workspace` green, 0 failures). 2026-11-10: the deferred code work is largely done. Open items are marked below, each with the reason it is open; Phase 14 has four items done or partial. Phase 4 crates.io publish is blocked pending explicit go-ahead (irreversible; dependency-chain order tup→core→protocols→daemon/cli). Deferred items COMPLETE except three future-work items (low-latency IPC, real transport, MAVLink CRC_EXTRA).**

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

**Pattern B — scratch impl originally kept private; all 9 promoted to
published `tpt-protocol-*` after the no_std/license audit (see NOTE above):**
- [x] `tpt-protocol-uds` (ISO 14229) — 9 tests — published (no existing crate)
- [x] `tpt-protocol-canopen` — 9 tests — published (no_std gap)
- [x] `tpt-protocol-nmea` (NMEA 2000/0183) — 11 tests — published (license gap)
- [x] `tpt-protocol-mavlink` — 10 tests — published (wraps `mavlink`)
- [x] `tpt-protocol-modbus` (RTU/TCP) — 22 tests — published (no_std gap) — **reference implementation**
- [x] `tpt-protocol-ethercat` — 9 tests — published (from-scratch; alt. to `ethercrab`)
- [x] `tpt-protocol-ocpp` (OCPP 2.0.1) — 8 tests — published (no maintained alternative)
- [x] `tpt-protocol-matter-thread` — 9 tests — published (license gap)
- [x] `tpt-protocol-coap` — 9 tests — published (no_std gap)

## Phase 4 — crates.io Publish Readiness
Scope: the 21 published crates (`tpt-protocol-tup` + `tpt-protocol-core` +
17 `tpt-protocol-<name>` protocol crates + `tpt-protocol-ethercat` + `tpt-protocol-mavlink`).
- [x] Confirm all 19 crate names unclaimed on crates.io (all free, including the 7 promoted from `out-protocol-*`)
- [x] `cargo package --list` review per crate — all clean, no stray files
- [x] `tpt-protocol-tup` packages cleanly (31.3 KiB, 11 files — the only crate with no internal deps)
- [x] `cargo package --list` for all 21 published crates — **done 2026-11-10: it needs no registry, so it runs for dependent crates too, and a `package-list` CI job runs it. (The earlier note below said blocked; that applied to `cargo publish --dry-run`, which still needs the registry.)** Earlier note: `cargo package --list --no-verify` for dependent crates — blocked: core depends on tup (unpublished), protocols depend on core (unpublished). Must publish in order: tup → core → protocols → daemon/cli
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
- [ ] Daemon against a real/third-party-simulated endpoint, per protocol as it lands — **2026-11-10 (final): also BACnet (bacpypes3 0.0.110, ReadProperty-ACK over UDP), J1939, ISOBUS and CANopen over SocketCAN (cantools 45.1.2 on vcan0, Linux only), OPC UA read by asyncua 2.1.0 (a server, not a sender). Earlier: MAVLink (pymavlink 2.4.50), NMEA 0183 (pynmea2 1.19.0) and CoAP (aiocoap 0.4.17) are tested end to end. Each script (`scripts/endpoints/`) encodes frames with that library, sends them as UDP datagrams to the daemon, and checks every field the daemon records: 15, 9 and 2 points, no problems. The `endpoints` CI job runs them. Not done: the other protocols, which each need a third-party sender or server. Earlier note: Modbus is done. `--modbus-poll HOST:PORT` polls a Modbus TCP server (a passive transport never sees data from a server), and was run against pymodbus 3.6.9: holding registers 100–109 and input registers 7–11 arrived with the server's values, an out-of-range read logged exception 0x02, and an unreachable server reconnected. Not done: the other 18 protocols. Most need a server or device that is not available here; they stay open until one is.**
- [x] `cargo test --workspace` (446 tests at last count, all green on 2026-11-09)
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
- [x] Documented follow-up: MAVLink CRC validation omits the real per-message
      `CRC_EXTRA` seed byte — **resolved 2026-11-08: the `mavlink` crate
      validates CRC_EXTRA via its generated `Message::extra_crc` table, so
      no seed table is needed here. Regression: `crc_extra_seed_is_enforced`.**

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

## Phase 10 — Safety & Trust Fixes (review 2026-11-08, from reading the code only; reproduce each with a failing test first)

### 10.1 — Parser panics on untrusted input
- [x] Modbus: 8 zero bytes passes `looks_like_tcp` then slices `bytes[8..6+length]` with start > end
      (`modbus/src/lib.rs:190-193,344-346`) — require MBAP `length >= 2`
- [x] HL7v2: byte-slicing a `&str` panics on multi-byte chars (`hl7v2/src/lib.rs:95,100,142,146`);
      the Phase 9.2 "fixed" entry above is only half true
- [x] Replace uniform-random proptests with structured generators (valid prefix + mutation) per crate — **done 2026-11-10 for all 19 protocol crates. Modbus (MBAP header), DNP3 (CRC-correct frames, skewed length), MAVLink (v1/v2 headers), NMEA (checksummed sentences), HL7 v2 (MSH and segments), OCPP-J (CALL/CALLRESULT/CALLERROR), CANopen (COB-IDs) and J1939 (29-bit IDs, PDU1 and PDU2) were done earlier. The last 11 are now done too: BACnet (BVLC and NPDU), CoAP (options and token), DoIP/SOME-IP (both discriminators), EtherCAT (datagram chain), IEEE 11073 SDC (SOAP envelopes), ISOBUS (29-bit IDs), KNX (cEMI L_Data), Matter (message header), OPC UA (message header and chunk types), Sparkplug B (Protobuf payloads) and UDS (service IDs and negative responses). Each has a mutation strategy and a normalizer check. All 11 test suites pass, with fmt and clippy clean.**
      (e.g. Modbus `00 00` prefix, DNP3 `0x0564`, MAVLink magic)
- [x] Add `fuzz/` (cargo-fuzz) targets for all parsers and tup CBOR/JSON decode — **done: 21 targets (`parse` + `try_parse`, and CBOR round-trip). Run on WSL Ubuntu, 60 s each, ~0.4–22 M inputs per target, no crashes. Nightly job: `.github/workflows/fuzz.yml`. Windows MSVC cannot link libFuzzer. Longer runs are still needed. 2026-11-10 (final): all 21 targets ran for 60 s each in WSL (nightly cargo-fuzz): exit 0, no crashes, 0.27 M to 19.9 M executions each. A soak run of an hour or more per target is still open.**

### 10.2 — Daemon security & correctness (`daemon/src/main.rs`)
- [x] WebSocket: check `Origin` and `Host` via `accept_hdr_async` (blocks web pages and DNS rebinding
      on 127.0.0.1); optional `--token`; handshake timeout, connection cap, max message size
- [x] Accept loop exits silently on any accept error (`:122`) — log and continue
- [x] `--replay` uses `read_to_string` before the raw-bytes fallback (`:170-183`); add `--replay-format hex|raw`;
      stop defaulting silently to `modbus`
- [x] CLI flags must override config file (`:159-165`); `deny_unknown_fields` on config
- [x] Remove duplicate `datapoint_to_json` (`:56-80`), use `tup::json::to_json_string` (it currently drops `raw`
      and turns bytes into `"<n bytes>"`)
- [x] Process client Close/Ping frames (read half is dropped); tell clients when messages were lagged/dropped
- [x] Log when the simulation stream ends; fix `localhost`/hostname handling in `is_loopback_bind`

### 10.3 — TUP spec vs code mismatch (the "open spec" risk)
- [x] `tup/src/cbor.rs` uses integer keys 0..8 and `TupValue` as `[tag, v]`; `schema/tup-v1.cddl` and
      `SPEC-TUP.md` §3 use text keys and native values. Make them agree (recommended: integer keys normative
      for CBOR, text keys for JSON) and add a test validating encoded CBOR against the CDDL
      — **done 2026-11-08: CDDL and §3 rewritten to integer keys; `cddl_conformance` tests check encoder output
      and the protocol/quality lists. The checker is hand-written, not a general CDDL validator.**
- [x] CBOR decoder: indefinite-length maps (`:171`), extra array elements (`:96`), duplicate keys,
      `schema_version` 0, unknown protocol/quality strings should not hard-fail (`:38,56`)
- [x] JSON round-trip: Bytes vs Text ambiguity, NaN/Inf, invalid hex `raw`, u64 > i64::MAX, check
      `schema_version` (`json.rs:27-42,76,85-87`)
- [x] `#[non_exhaustive]` on `Quality` and `SourceProtocol` before first publish
- [x] `encode_to_vec` discards the real encode error (`cbor.rs:230`); add `From<minicbor::…>` for core `Error` — **done for encode_to_vec (returns minicbor's error). The core `From` impl is not needed: core does not call minicbor.**
- [x] `tup`: README says `std` default and "no alloc" but `default = []` and `extern crate alloc` is unconditional; `hex` should be optional — **done: README corrected (`std` is not a default; alloc is always required); `hex` is optional behind `std`.**

### 10.4 — Build hygiene & docs drift
- [x] `core/src/error.rs:118` uses `core::error::Error` (Rust 1.81) but `rust-version = "1.75"`; fix or raise MSRV and add an MSRV CI job — **done: workspace MSRV raised to 1.81, `msrv` CI job added; 1.81 verified locally, 1.75 fails on the minicbor build script**
      **Correction 2026-11-09: the `msrv` job failed on master because `minicbor` 2.3.1 uses edition 2024 (Cargo 1.85). Resolved 2026-11-10: `rust-version` and the `msrv` job are now 1.85, and `instability` is pinned to 0.3.7 so the lock does not need rustc 1.88. `cargo check --workspace` passes on 1.85.1.**
- [x] Decide on untracked `.cargo/config.toml` (`incompatible-rust-versions = "fallback"`; it re-resolved Cargo.lock) and the
      `ui-*` crates' hard-coded `rust-version = "1.90"` — **decided: keep the config (documented; needs Cargo 1.84+, ignored with a warning on 1.81; not yet committed); keep 1.90 for the `ui-*` apps and document it in their READMEs.**
- [x] Remove stale `out-protocol-*` text (README, CLAUDE.md, TODO header/Pattern B, "2 remain private"); no such crate exists
- [x] Fix counts: "21 published crates", README "16 more", TODO "206 tests", CI "four jobs" (five with cargo-deny)
- [x] Fix README `tpt-telos` crates.io link name. (CONTRIBUTING.md was rewritten as issues-only, so its `main`/`master` and no_std command issues no longer apply.)
- [x] CHANGELOG "Known gaps" MAVLink CRC_EXTRA entry removed (done); `spec.txt` §5 layout now matches the crates, the protocol list is numbered 1-19, and NMEA is described as NMEA 0183 (NMEA 2000 not implemented)
- [x] Correct overstated claims: "all 19 wired" is simulation only (sine wave, parser never used), "no alloc" for tup/core — **partial: README no longer says critical state machines are formally verified (telos checks three Modbus specs). The "all 19 wired" claim should be read as simulation only, and "no alloc" is fixed in tup's README. Checked 2026-11-09: the root README (which still said "simulation only"), the daemon README and the telos README are corrected.**
- [x] `CODE_OF_CONDUCT.md` and `SECURITY.md` were removed by maintainer decision. Contributions are issues only. The issue template and `dependabot.yml` remain.
- [x] `daemon` is unpublishable (path-only deps); set `publish = false` on daemon/cli/ui/telos (done. Hoisting `proptest`, tokio, clap, anyhow, tokio-tungstenite and the tracing crates to `[workspace.dependencies]` is done as well, 2026-11-10. Versions and features are unchanged.)

## Phase 11 — Parsers Must Match the Real Protocols
Add `tests/vectors/` per crate from real captures/vendor vectors so tests stop being circular (they currently pass against each crate's own builder).
- [x] EtherCAT: command table (APWR=2, FPWR=5, BWR=8, LRD=10, LWR=11), LE 11-bit length header, multiple datagrams — **done: IEC 61158 command table, little-endian header with 11-bit length and type field, datagram chain walked to the section length, padding ignored, `try_parse` Ok(None) when incomplete.**
- [x] DNP3: frame size (`length+2` wrong), per-16-byte-block data CRCs, Application Control byte, responses (0x81) — **done: header CRC now covers the start bytes (checked against the published CRC-16/DNP vector), user data is split into 16-byte blocks with per-block CRCs, transport and application headers are parsed, first-fragment-only app header, 0x00/0x81/0x82 function codes, `try_parse` reports exact consumed length and `Ok(None)` for incomplete frames.**
- [x] OPC UA: little-endian fields, validate `message_size`, don't leak bytes beyond it, fix nodeid packing — **done: little-endian Binary encoding, message_size must equal the frame (`try_parse` returns Ok(None) when incomplete and the consumed length otherwise), chunk type and message type validated, TypeId NodeId and ExtensionObject decoded (two-byte, four-byte and numeric forms).**
- [x] DoIP/SOME-IP: payload-type table (0x0004–0x0008, 0x8001–0x8003), better discriminator than `bytes[0]==0x02`, check SOME/IP length, truncated payload — **done: ISO 13400-2 payload-type table, discriminator is version (0x01/0x02/0x03/0xFF) plus its inverse, with SOME/IP fallback; truncated and trailing DoIP payloads are errors; SOME/IP length must equal the frame length. Two existing tests encoded the wrong table and length and were corrected.**
- [x] BACnet: BVLC codes (0x0A/0x0B/0x04), NPDU layer, packed object ID, validate `bvlc_length` — **done: Annex J function codes, NPDU with DNET/SNET/hop count and network-layer messages, Forwarded-NPDU address, packed 10-bit type / 22-bit instance, invoke ID and service choice, ReadProperty object and property decode, exact BVLC length. The old tests used a layout that is not BACnet and were rewritten.**
- [x] Modbus: read responses, exception responses (fc|0x80), TCP transaction ID, `byte_count`, FC 0x05 values, normalizer emits `quantity` as value — **done: `ModbusParser::response()` parses responses, read data and write echoes are distinct variants, exceptions parse in both directions, byte counts and quantities are checked, FC 0x05 accepts only 0xFF00/0x0000, the TCP transaction ID is exposed, read requests no longer normalize to their quantity (they error), and RTU frames with a valid CRC are never mistaken for TCP.**
- [x] CANopen/J1939/ISOBUS `normalize` read big-endian but data is little-endian; **2026-11-10 (final): J1939 signals are now decoded per PGN (EEC1 engine speed SPN 190, ET1 coolant temperature, CCVS wheel speed, fuel economy fuel rate), each in its unit with its not-available value skipped. The normalizer had reported the first four payload bytes as a number for every PGN, which the new cantools endpoint test caught. ISOBUS had the same bug and is fixed the same way (its sample was a 4-byte EEC1 frame and is now a full frame).** Earlier: CANopen Emergency and PDO3/4; J1939 EDP bit and PGN names; ISOBUS 0xEC00 = TP.CM; dedupe isobus/j1939 copy-paste — **mostly done: little-endian decode (earlier); CANopen Emergency (fc 1 with node ≠ 0), PDO3/PDO4 and the full CiA 301 table; J1939 EDP bit in the PGN, PDU1 destination masking, TC1 = PGN 0, CCVS and Ambient Conditions names; ISOBUS TP.CM/TP.DT/Request/Address Claim names and PDU1 masking. Declined: dedupe isobus/j1939 copy-paste. Protocol crates may not depend on each other (CLAUDE.md), so the 29-bit ID split stays duplicated, about ten lines. A shared helper would have to live in core, which is generic. ISOBUS identity now comes from the 64-bit NAME of Address Claimed frames (ISO 11783-5 bit layout), not from arbitrary payload bytes. The state carries the raw NAME as `ecu_name`. 2026-11-10: a frame with no payload is now an error, not a zero. Dedupe stays declined.**
- [x] UDS: negative response 0x7F, 0x14 has no sub-function, suppressPosRsp bit, missing services (0x19, 0x31, 0x34-0x37, 0x85) — **done: 0x7F parsed with requested SID and NRC (normalized as a Bad-quality point), 0x14 has no sub-function, bit 7 of a request's sub-function is `suppress_positive_response` and is no longer masked away in responses, all listed services added with their responses.**
- [ ] NMEA: checksum slicing with leading whitespace, bad checksum should be `Err` (like Modbus/DNP3), talkers GN/GL/GA, first-field-only normalizer, NMEA 2000 not implemented — **partial: checksum fixed and bad checksums are errors; GN/GL/GA/BD added; per-sentence normalization for GGA (position, fix, satellites, altitude), RMC (position, speed, course; Bad quality without a valid fix), DBT, VHW and MWV, with coordinates in signed degrees. Other sentences keep field 0. Still open: NMEA 2000 fast-packet PGNs (reassembly and more PGNs; single-frame PGNs are done in `nmea2000`, see the 2026-11-10 (later) note below). 2026-11-10: GLL, VTG, GSA, GSV, HDT and ZDA added (each with checked-value tests); a sentence with no decodable field is an error, not a zero. The crate still builds for thumbv6m. 2026-11-10 (later): the `nmea2000` module decodes single-frame PGNs 129025 (position), 127250 (heading), 130306 (wind) and 128267 (depth), with a parser, an encoder, a normalizer and tests. Fast-packet PGNs are not reassembled; the normalizer refuses them. Later the same day: a `FastPacketReassembler` joins fast-packet sequences (126996, 129029, 129540 are the PGNs it knows) and 129029 (GNSS position) decodes latitude, longitude and altitude. The reassembler has tests for ordering and restart. Still open: more fast-packet PGNs, and checks against a real bus capture.**
- [ ] IEEE 11073 SDC — **partial: a bounded XML parser (no DOCTYPE, depth and element limits, entity decoding), SOAP 1.2 envelopes with WS-Addressing Action and MessageID, and numeric observations from `NumericMetricState`. Still open: waveforms (`RealTimeSampleArrayMetricState`), alerts and context states, and verifying the BICEPS element and attribute names and namespace against a real device. Until then, the SDC README should say "experimental".** KNX cEMI, OCPP-J, Sparkplug B and Matter are implemented (see their items).
- [x] OCPP-J: real JSON framing (`[2,id,action,payload]`, `[3,id,payload]`, `[4,id,code,desc,details]`), 1.6 and 2.0.1 action names, a 36-character message id limit, and no `as u16` truncation — **done: a hand-written JSON array scanner (no serde, so the crate still depends only on core and tup), string escapes decoded, payloads kept as raw object text, element counts checked per message type. The binary framing it replaces was not OCPP.**
- [x] Sparkplug B: real Protobuf payloads and `spBv1.0` topics — **done: a Protobuf wire reader (varints, fixed32/64, length-delimited, unknown fields skipped) decodes the Payload and Metric messages, with datatypes mapped to TUP values (Int64 and UInt64 above i64::MAX are errors, not truncated). The topic gives the group, message type, edge node and device, and wildcards are rejected. `SparkplugNormalizer::points` emits one point per metric.**
- [x] Matter TLV: real Matter TLV (all element types, tag forms, bounded nesting), unsecured message and protocol headers, and Interaction Model ReportData attribute reports. Secured sessions are reported as encrypted, not decoded. The opcode values follow the connectedhomeip implementation and should be checked against the spec with a real capture. Matter's dead duplicate length check and the "Other" cluster are gone with the old layout.
- [x] KNX: service_type ignored in normalizer, priority bit comment vs code mismatch, redundant validator; Matter: dead duplicate length check, "Other" cluster — **KNX done: the parser now reads cEMI L_Data frames (message code, control fields, source individual address, group or individual destination, NPDU length, APCI service) and DPT decoding is explicit (decode_dpt1/5/9). The normalizer uses the source and group address. Matter is still open.**
- [x] CoAP: option-number wrap, empty payload after 0xFF marker, Uri-Path invalid UTF-8, device_id/point_id choice — **done: option numbers use checked arithmetic, a payload marker with no payload is an error, Uri-Path is one UTF-8 segment per option, and Max-Age, Observe and Content-Format reject values longer than their maximum. device_id/point_id choice is unchanged.**
- [x] MAVLink: only 4 message IDs named, signature lost on re-serialize, normalizer reads first 4 bytes not decoded fields; CRC_EXTRA test only covers HEARTBEAT v1 — add v2 and other IDs — **done: every dialect message is named through the `Message` trait; the frame keeps the decoded `MavMessage`, and the normalizer reads typed fields for HEARTBEAT, SYS_STATUS, ATTITUDE and GPS_RAW_INT; the v2 signature block (link id, timestamp, value) is kept on the frame. The signature is parsed, not verified: the mavlink crate's verifier needs std. CRC_EXTRA is tested for v1 and v2 and for several messages.**
- [ ] Core: use the unused `Error` variants (`Checksum` should carry a u16/u32 — **done: `Checksum` carries u32 values, and Modbus RTU, DNP3 header and block, and NMEA use it. `Overflow` and `InvalidFrame` are still unused, which is fine until a protocol needs them.**), `try_parse` must report real consumed/need-more-data (Modbus and J1939 still return `bytes.len()`), add `FrameEncoder` trait with round-trip tests, give `ProtocolPlugin` a real I/O path — **partial: Modbus `try_parse` now frames streams (`Ok(None)` for incomplete frames, exact consumed length; RTU is the shortest CRC-valid prefix, TCP uses the MBAP length). J1939 is not a stream format (one CAN frame per slice), so it is unchanged. `FrameEncoder` is in core. `ModbusEncoder` was the first implementation. 2026-11-10: encoders now also exist for UDS, NMEA, CoAP, ISOBUS, J1939, DoIP/SOME-IP, KNX, EtherCAT, OPC UA, OCPP-J, DNP3, CANopen, Sparkplug B, HL7 v2 and IEEE 11073 SDC. Each has round-trip tests against its own parser, and each crate builds for `thumbv6m-none-eabi`. Still open: BACnet and Matter. Their parsers keep decoded views (the NPDU routing fields and most of the APDU for BACnet, and the decoded TLV for Matter), so an encoder would have to rebuild those layers from the values, a spec-level job. MAVLink is also open: the wrapped `mavlink` crate's writer needs std. 2026-11-10: the I/O path now exists in the daemon, where the transports read real bytes beside the plugin (`ProtocolPlugin` itself still has only `simulate()`).**
- [ ] Normalizers: real timestamps (15 hard-code 0), fan out via `normalize_batch`, stop emitting meaningless values; uniform constructors/derives/state structs across older vs newer crates; consistent `Error::parse` use — **partial: normalizers leave timestamp 0 by design (they have no clock), and the daemon's replay path stamps it at ingest. `DataNormalizer::normalize_all` returns every point a frame carries, and `normalize_batch` fans out through it. Sparkplug, Matter, NMEA and SDC override it. 2026-11-10: the placeholder values are gone from DNP3, DoIP/SOME-IP, ISOBUS, J1939, KNX, NMEA and Sparkplug, and BACnet and HL7 now report real values. Still open: uniform constructors, and any remaining older crates.**

## Phase 12 — Real Data Path & Daemon/UI Polish
- [x] `Transport` trait + Serial/TCP/UDP/SocketCAN backends, streaming framer using `try_parse`; plugins run real bytes through the protocol crate instead of a sine wave (supersedes the two Deferred items below) — **2026-11-10: done. The `Transport`/`ByteLink` traits are in `daemon/src/transport/`, with TCP, UDP, serial, SocketCAN and `--stdin` behind one framer and supervisor. The sine wave stays the default when no transport flag is set, and the plugins themselves still only simulate. Earlier note: partial: TCP (connect with reconnect, or listen) and UDP are implemented in `daemon/src/transport.rs`, with a streaming framer over each protocol's `try_parse` and loopback tests. Serial is implemented behind the `serial` feature (`tokio-serial`) and was tested against a socat pseudo-terminal pair in WSL (ignored test, see its doc comment). SocketCAN is implemented behind the `socketcan` feature (Linux only) and was tested on a `vcan0` virtual interface (ignored test, see its doc comment). The `Transport` trait itself is still deferred. The sine-wave simulation remains when no transport flag is set.**
- [x] `tracing` + `RUST_LOG`, metrics (clients, lagged/dropped), `/healthz`, `--interval`; feature-gate protocols in the daemon — **done 2026-11-09: `tracing` to stderr with `RUST_LOG` (default `info`); `/healthz` on the WebSocket port (active clients, messages sent, lagged events, dropped messages, rejected handshakes; counts only, no data); `--interval <ms>` for the simulation; one Cargo feature per protocol, all on by default. A build with no protocol feature is a compile error. Smoke-tested against the binary.**
- [x] CLI: restore terminal on every exit path, filter `key.kind == Press` (double scroll on Windows), `VecDeque` + bounded channel, date/timezone in timestamps, auto-follow, pong handling — **done 2026-11-09: timestamps show the UTC date and time with a `Z` (local time is not shown, because that needs a timezone database the std-only formatter does not carry); auto-follow tracks the newest rows, scrolling up pauses it, End resumes; pings are answered on the write half (the old read-only loop never sent pongs).**
- [x] ui-web: CSP hard-codes `ws://127.0.0.1:9080` vs `TPT_DAEMON_URL`, remove production devtools permission, reconnect timers stack, O(n²) slice, no teardown; ui-embedded: `unwrap()` on `spawn_local`, show connection status — **done 2026-11-09: the CSP allows loopback on any port (a non-loopback `TPT_DAEMON_URL` still needs a CSP edit); the devtools permission is gone from the capability; one reconnect timer, cleared on teardown; lagged notices shown as a message, not a table row; one trim per message. ui-embedded: spawn errors are returned, a status line shows the connection, the channel is bounded, pings are answered. Also fixed: the Vite config imported from the wrong package and `src/app.html` and the CSS import were missing, so the web UI never built. `vite build` and `svelte-check` pass.**
- [x] Drop unused `native-tls` from cli/ui crates (done: the clients only use `ws://` to the local daemon; cli, ui-embedded and ui-web still build). ui-web and ui-embedded are built and linted in CI (2026-11-09)
- [x] Daemon tests: `datapoint_to_json`, `is_loopback_bind`, config precedence, replay hex/binary, end-to-end WS with good/bad Origin; CLI tests for `parse_data_points`, `format_timestamp`, scrolling — **partial: handshake authorization tests cover loopback Host, foreign Origin, DNS rebinding, non-loopback bind and the token. Config precedence and hex replay were already covered. Added 2026-11-09: an end-to-end WebSocket test with an allowed and a foreign Origin, binary `raw` replay, `/healthz`, and the simulation interval. 2026-11-10: CLI tests now cover valid data (`parse_data_points` on an array and on one object) and malformed JSON, alongside the timestamp, scroll and lagged-notice tests. The daemon also has tests for mapping rules, recording, candump replay and detection.**

## Phase 13 — Adoption Kit
- [x] Dockerfile + `docker run` quickstart for the daemon — **done 2026-11-10: `docker build` succeeds on Docker Desktop 29.9.2, and the container serves `/healthz` on the host port with the documented `-p 127.0.0.1:9080:9080` mapping.**
- [ ] Release workflow (cargo-dist or release-plz) with prebuilt Windows/Linux/macOS binaries; `cargo package` dry-run CI job; README badges — **partial, 2026-11-10: `.github/workflows/release.yml` builds the daemon and CLI for Linux, Windows (MSVC) and macOS (arm64) on a `v*` tag and attaches the archives to a GitHub release. It has not run (no GitHub runner here); the YAML parses. Still open: a `cargo publish --dry-run` job (it needs the registry, so it waits for the publish go-ahead; the `package-list` job, which lists the package contents of the 21 crates, is done 2026-11-10), crates.io and docs.rs badges (they wait for the publish), and cargo-dist's installers. CI and license badges were added to the README on 2026-11-10. The repository URL mismatch is resolved: every URL now uses `tpt-solutions/tpt-protocol`, the git remote (`Cargo.toml`, the crate READMEs, the README badges and the `xtask` template).**
- [x] Runnable `examples/parse_frame` for every protocol (only tup, core, modbus have one) plus sample frame files usable with `--replay` — **done 2026-11-10: `examples/parse_<protocol>.rs` for all 19 protocols. Modbus keeps `parse_frame`; the others are `parse_<name>`, because Cargo copies every example to one shared path and same-named examples run the wrong binary. One replayable sample per protocol is in `tpt-protocol-daemon/samples/`, and a daemon test replays every one.**
- [x] Windows-friendly scaffolder (`cargo xtask new-protocol` or cargo-generate template) that also wires the daemon plugin and adds proptest + fuzz target (current script is bash-only and does neither) — **done 2026-11-10: `cargo xtask new-protocol <name>` (with `--dry-run`) writes the crate, its proptest and fuzz target, the workspace entry, the `SourceProtocol` variant, the CDDL and JSON schema names, the conformance list, and the daemon's dependency, feature and plugin. Every edit is computed before anything is written. The bash script is removed. Verified by scaffolding a copy of the repo, then building and testing it, including the daemon.**
- [x] "Bring your own protocol in 30 lines" tutorial based on `core/examples/custom_protocol.rs` — **done 2026-11-10: `docs/bring-your-own-protocol.md`. The core snippet is about 35 lines, not 30; the guide also covers the scaffolder and the limits (the wire-name choice, and detection being a heuristic).**
- [x] WASM build (`wasm-bindgen`) + browser "paste hex, see TUP" playground — **done 2026-11-10: `tpt-protocol-bindings` with `--features wasm`. The exports are tested in Node 22 through the generated glue, and `www/index.html` is the playground. The page itself has not been opened in a browser here.**
- [x] `pyo3` wheel exposing `decode(protocol, bytes) -> dict`; `cbindgen` C header for embedded/PLC vendors — **done 2026-11-10: a Python module (`--features python`) that is imported and called in Python 3. It returns JSON text, not a dict, so consumers call `json.loads`. The C header (`include/tpt_protocol.h`) is hand-written, not generated: cbindgen is MPL-2.0, which `deny.toml` does not allow yet. A Rust test checks the header against the exports, and a C program loads the DLL and calls it. 2026-11-10: a maturin wheel is built (`crates/tpt-protocol-bindings/pyproject.toml`, `maturin build --release`), installed in a clean virtualenv and exercised; CI builds it on each push. Not uploaded anywhere.**
- [ ] CI: — **partial, 2026-11-10: added a Windows and macOS test job (`test-os`) and the release workflow (see above); the `no_std` job now skips the host-only crates. 2026-11-09: added `cargo doc -D warnings`, the telos `--features verify` job, ui-embedded and ui-web jobs, `rust-cache`, and a weekly scheduled run (advisories). 2026-11-10, done: the Windows/macOS matrix (`test-os`), coverage (`cargo llvm-cov` lcov artifact), criterion benches compiled in CI (`bench`; the Modbus parse bench exists), cargo-audit (`audit` job, with `.cargo/audit.toml` holding the accepted advisories), the no_std feature matrix (tup and core with `--no-default-features`, tup with `alloc`), and `cargo-deny-action` pinned to a SHA. Also added: a `python-wheel` job, an `mqtt` job against a Mosquitto container, and a `differential` job. Still open: cargo-semver-checks, which needs a published baseline and so waits for the publish go-ahead. `rust-toolchain.toml` was declined: it would override the MSRV job's rustup 1.85 toolchain.**

## Phase 14 — Innovative Additions (pick later)
- [x] Protocol auto-detect: try all parsers on raw bytes and rank by confidence — **done 2026-11-10 as `--detect FILE`, which ranks by decoded points, then frames, then errors. It is a heuristic, not a confidence score: UDS has no length field, so it accepts any single byte, and the README says to read the counts, not only the order.**
- [ ] Capture import/export: pcap/pcapng and candump replay; TUP out to MCAP/Parquet/NDJSON — **partial, 2026-11-10: candump (`--replay-format candump`), pcap and pcapng (`--replay-format pcap`, with `--pcap-port`; IPv4 TCP/UDP, no TCP reassembly), NDJSON (`--record`) and MCAP (`--record-mcap`, unchunked so an interrupted file keeps its messages) are done. Tested on synthetic captures and, for MCAP, read back with the Python `mcap` package. Open: a capture from a physical network (done for loopback: see the note below). **2026-11-10 (last pass): a real Modbus TCP capture is replayed. It is a pymodbus 3.6.9 client and server on loopback, recorded with tcpdump (`samples/captures/modbus-loopback.pcap`, `scripts/capture-modbus-loopback.sh`), and decodes to the values the client printed. The replay of this capture found that merging both directions into one stream misreads Modbus responses, so Modbus pcap replay now pairs requests with responses by transaction id (it needs `--pcap-port`). 2026-11-10 (final pass): Parquet export is done (`--record-parquet`, feature `parquet-record`, the `parquet` crate 59 with no default features). One row per point, row groups of 4096. Read back with pyarrow, including nulls. Caveat: the footer is written only on a clean exit, so an interrupted daemon's file is unreadable.**
- [x] Sinks/bridges: MQTT, OPC UA server, InfluxDB/Prometheus remote-write, OpenTelemetry — **done for the four (follow-ups: OPC UA security, subscriptions, history). Earlier note: partial, 2026-11-10: the MQTT sink (`--mqtt`, feature `mqtt`) is done and tested against a real Mosquitto 2.1.2 broker, including an end-to-end run from a pymodbus server. Not started: OPC UA server, InfluxDB/Prometheus remote-write, OpenTelemetry. Each needs its own server to test against.** **2026-11-10 (later): the InfluxDB sink (`--influx URL`) is done: line protocol over plain HTTP, batched, with no new dependency. It is tested against a local HTTP server and not yet against a real InfluxDB. Still open: Prometheus remote-write (needs protobuf and snappy), OpenTelemetry, and the OPC UA server.** **2026-11-10 (later still): the OpenTelemetry sink (`--otel URL`) is done. It sends OTLP/HTTP JSON gauges, one per protocol, over plain HTTP, sharing the HTTP client with `--influx` (`http_out.rs`). Tested against a local HTTP server, not a real collector. **2026-11-10 (final): the OPC UA server is done (`--opcua-listen`): anonymous, `SecurityPolicy#None`, read-only (Browse and Read, with the GetEndpoints, session and secure-channel services). Tested with asyncua 2.1.0 as an independent client, which browses and reads the points and matches their values. Not supported: security, subscriptions, history, writes and units. The message layout was checked against asyncua's own server on a recorded loopback capture. **2026-11-10 (last pass): Prometheus remote-write is done (`--prometheus-remote-write URL`): protobuf `WriteRequest`, snappy in its literal-only form, remote-write 0.1.0 headers. Its body was decoded with python-snappy and an independent protobuf reader (values, timestamps, sorted labels). Nothing open in this item except OPC UA security, subscriptions and history.**
- [x] Declarative mapping config (TOML/YAML) from (protocol, device, point) to names, units, scaling — **done 2026-11-10 as `[[map]]` tables in the TOML config (not YAML). A rule matches on protocol, device and point, and sets the name, unit, scale and offset. A range check marks out-of-range values uncertain.**
- [ ] Differential testing against reference implementations (tshark dissectors, pymodbus, …) in CI — **partial, 2026-11-10: `scripts/differential/modbus_pymodbus.py` encodes and decodes 654 Modbus frames (TCP and RTU, 600 valid and 54 negative) with pymodbus 3.6.9; the `diff_decode` example parses the same bytes. 0 mismatches. It runs in the `differential` CI job. Not done: other protocols, and tshark. **2026-11-10 (last pass): four more differential tests, each in CI and each checked to catch a deliberately wrong expectation. NMEA 0183 against pynmea2 1.19.0 (`nmea_pynmea2.py`, 350 sentences, GGA and RMC, both hemispheres, corrupted checksums). HL7 v2 against hl7 0.4.5 (`hl7_hl7py.py`, 330 messages with components, and negative cases). UDS against udsoncan 1.26.1 (`uds_udsoncan.py`, 400 payloads, all compared, none refused). Modbus is unchanged. Still not done: tshark, and the other protocols (no reference library checked for CoAP, KNX, Sparkplug, OCPP, DNP3 or the rest yet).****
- [ ] Anomaly/security rules on TUP streams (unexpected function codes, replay, rate spikes) — **partial, 2026-11-10: `[[anomaly]]` rules (`anomaly.rs`) flag a timestamp that does not advance (replay or clock step), a rate spike (`min_interval_ms`) and a step change (`max_step`), marking the point Uncertain. Not done: function-code rules, because a TUP point does not carry the function code; plugins would need to pass it on.**
- [ ] Link telos specs to real code — **partial, 2026-11-10: the CRC window, the WriteMultipleRegisters quantity bound and the validator threshold are checked against the real `tpt-protocol-modbus` code (conformance tests, a dev-dependency, no cycle). The conformance tests sample inputs, so they check behaviour, not a proof. They found a real Modbus bug (a whole-buffer parse accepted trailing bytes after an MBAP frame, fixed) and a wrong description in the telos crate (fixed). Still open: specs for protocols other than Modbus, and Kani/Creusot. `verify_dir` is now anchored to the crate directory.**

## Later / Deferred
- [x] `ui-embedded` (Slint native dashboard) — `tpt-protocol-ui-embedded`, connects to daemon WS
- [x] `ui-web` (Tauri v2 + SvelteKit fleet dashboard) — `tpt-protocol-ui-web`, connects to daemon WS
- [x] Formal verification (`tpt-telos`) for critical state machines — `tpt-protocol-telos`: bundled `.telos` specs for Modbus CRC bounds, validator error-counter, and normalizer register bounds; `cargo test -p tpt-protocol-telos --features verify` discharges them with the QF_LRA verifier
- [ ] Dedicated low-latency IPC channel (Unix socket/named pipe) if WebSocket proves insufficient for the TUI — **not started. No measurement shows the WebSocket is too slow, so this stays deferred until one does.**
- [x] Real transport clients (Serial/CAN/TCP/UDP) for the daemon — genuine
      hardware I/O, beyond Phase 9.3's simulation-only wiring. Done in Phase 12: TCP, UDP, serial and SocketCAN, tested without hardware (pseudo-terminal pair and `vcan0`).
- [x] MAVLink `CRC_EXTRA` per-message seed table for full spec compliance — **resolved 2026-11-08: provided by the `mavlink` crate's generated `extra_crc` table (see Phase 9.2).**
