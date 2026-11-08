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
- [ ] Replace uniform-random proptests with structured generators (valid prefix + mutation) per crate — **partial: Modbus done (MBAP header, skewed length, mutation); DNP3, MAVLink and the rest still uniform-random**
      (e.g. Modbus `00 00` prefix, DNP3 `0x0564`, MAVLink magic)
- [x] Add `fuzz/` (cargo-fuzz) targets for all parsers and tup CBOR/JSON decode — **done: 21 targets (`parse` + `try_parse`, and CBOR round-trip). Run on WSL Ubuntu, 60 s each, ~0.4–22 M inputs per target, no crashes. Nightly job: `.github/workflows/fuzz.yml`. Windows MSVC cannot link libFuzzer. Longer runs are still needed.**

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
- [x] Decide on untracked `.cargo/config.toml` (`incompatible-rust-versions = "fallback"`; it re-resolved Cargo.lock) and the
      `ui-*` crates' hard-coded `rust-version = "1.90"` — **decided: keep the config (documented; needs Cargo 1.84+, ignored with a warning on 1.81; not yet committed); keep 1.90 for the `ui-*` apps and document it in their READMEs.**
- [x] Remove stale `out-protocol-*` text (README, CLAUDE.md, TODO header/Pattern B, "2 remain private"); no such crate exists
- [ ] Fix counts: "21 published crates", README "16 more", TODO "206 tests", CI "four jobs" (five with cargo-deny)
- [ ] Fix README `tpt-telos` crates.io link name; `main` vs `master` in CONTRIBUTING.md; CONTRIBUTING no_std command fails on std crates
- [ ] CHANGELOG "Known gaps" MAVLink CRC_EXTRA entry removed (done); still open: `spec.txt` §5 layout and NMEA 2000/DoIP numbering are out of date
- [ ] Correct overstated claims: "all 19 wired" is simulation only (sine wave, parser never used), "no alloc" for tup/core
- [ ] `CODE_OF_CONDUCT.md` (done: `SECURITY.md`, issue/PR templates, `dependabot.yml`)
- [x] `daemon` is unpublishable (path-only deps); set `publish = false` on daemon/cli/ui/telos (done; hoisting `proptest`, tokio, clap, etc. to `[workspace.dependencies]` is still open)

## Phase 11 — Parsers Must Match the Real Protocols
Add `tests/vectors/` per crate from real captures/vendor vectors so tests stop being circular (they currently pass against each crate's own builder).
- [x] EtherCAT: command table (APWR=2, FPWR=5, BWR=8, LRD=10, LWR=11), LE 11-bit length header, multiple datagrams — **done: IEC 61158 command table, little-endian header with 11-bit length and type field, datagram chain walked to the section length, padding ignored, `try_parse` Ok(None) when incomplete.**
- [x] DNP3: frame size (`length+2` wrong), per-16-byte-block data CRCs, Application Control byte, responses (0x81) — **done: header CRC now covers the start bytes (checked against the published CRC-16/DNP vector), user data is split into 16-byte blocks with per-block CRCs, transport and application headers are parsed, first-fragment-only app header, 0x00/0x81/0x82 function codes, `try_parse` reports exact consumed length and `Ok(None)` for incomplete frames.**
- [x] OPC UA: little-endian fields, validate `message_size`, don't leak bytes beyond it, fix nodeid packing — **done: little-endian Binary encoding, message_size must equal the frame (`try_parse` returns Ok(None) when incomplete and the consumed length otherwise), chunk type and message type validated, TypeId NodeId and ExtensionObject decoded (two-byte, four-byte and numeric forms).**
- [x] DoIP/SOME-IP: payload-type table (0x0004–0x0008, 0x8001–0x8003), better discriminator than `bytes[0]==0x02`, check SOME/IP length, truncated payload — **done: ISO 13400-2 payload-type table, discriminator is version (0x01/0x02/0x03/0xFF) plus its inverse, with SOME/IP fallback; truncated and trailing DoIP payloads are errors; SOME/IP length must equal the frame length. Two existing tests encoded the wrong table and length and were corrected.**
- [x] BACnet: BVLC codes (0x0A/0x0B/0x04), NPDU layer, packed object ID, validate `bvlc_length` — **done: Annex J function codes, NPDU with DNET/SNET/hop count and network-layer messages, Forwarded-NPDU address, packed 10-bit type / 22-bit instance, invoke ID and service choice, ReadProperty object and property decode, exact BVLC length. The old tests used a layout that is not BACnet and were rewritten.**
- [x] Modbus: read responses, exception responses (fc|0x80), TCP transaction ID, `byte_count`, FC 0x05 values, normalizer emits `quantity` as value — **done: `ModbusParser::response()` parses responses, read data and write echoes are distinct variants, exceptions parse in both directions, byte counts and quantities are checked, FC 0x05 accepts only 0xFF00/0x0000, the TCP transaction ID is exposed, read requests no longer normalize to their quantity (they error), and RTU frames with a valid CRC are never mistaken for TCP.**
- [ ] CANopen/J1939/ISOBUS `normalize` read big-endian but data is little-endian; CANopen Emergency and PDO3/4; J1939 EDP bit and PGN names; ISOBUS 0xEC00 = TP.CM; dedupe isobus/j1939 copy-paste — **mostly done: little-endian decode (earlier); CANopen Emergency (fc 1 with node ≠ 0), PDO3/PDO4 and the full CiA 301 table; J1939 EDP bit in the PGN, PDU1 destination masking, TC1 = PGN 0, CCVS and Ambient Conditions names; ISOBUS TP.CM/TP.DT/Request/Address Claim names and PDU1 masking. Declined: dedupe isobus/j1939 copy-paste. Protocol crates may not depend on each other (CLAUDE.md), so the 29-bit ID split stays duplicated, about ten lines. A shared helper would have to live in core, which is generic. Still open: ISOBUS `implement_id` and `device_class` are read from arbitrary payload bytes, which is not an ISOBUS concept for most PGNs.**
- [x] UDS: negative response 0x7F, 0x14 has no sub-function, suppressPosRsp bit, missing services (0x19, 0x31, 0x34-0x37, 0x85) — **done: 0x7F parsed with requested SID and NRC (normalized as a Bad-quality point), 0x14 has no sub-function, bit 7 of a request's sub-function is `suppress_positive_response` and is no longer masked away in responses, all listed services added with their responses.**
- [ ] NMEA: checksum slicing with leading whitespace, bad checksum should be `Err` (like Modbus/DNP3), talkers GN/GL/GA, first-field-only normalizer, NMEA 2000 not implemented — **partial: checksum fixed and bad checksums are errors; GN/GL/GA/BD added; per-sentence normalization for GGA (position, fix, satellites, altitude), RMC (position, speed, course; Bad quality without a valid fix), DBT, VHW and MWV, with coordinates in signed degrees. Other sentences keep field 0. Still open: NMEA 2000 (not NMEA 0183), and more sentence types.**
- [ ] IEEE 11073 SDC — **partial: a bounded XML parser (no DOCTYPE, depth and element limits, entity decoding), SOAP 1.2 envelopes with WS-Addressing Action and MessageID, and numeric observations from `NumericMetricState`. Still open: waveforms (`RealTimeSampleArrayMetricState`), alerts and context states, and verifying the BICEPS element and attribute names and namespace against a real device. Until then, the SDC README should say "experimental".** KNX cEMI, OCPP-J, Sparkplug B and Matter are implemented (see their items).
- [x] OCPP-J: real JSON framing (`[2,id,action,payload]`, `[3,id,payload]`, `[4,id,code,desc,details]`), 1.6 and 2.0.1 action names, a 36-character message id limit, and no `as u16` truncation — **done: a hand-written JSON array scanner (no serde, so the crate still depends only on core and tup), string escapes decoded, payloads kept as raw object text, element counts checked per message type. The binary framing it replaces was not OCPP.**
- [x] Sparkplug B: real Protobuf payloads and `spBv1.0` topics — **done: a Protobuf wire reader (varints, fixed32/64, length-delimited, unknown fields skipped) decodes the Payload and Metric messages, with datatypes mapped to TUP values (Int64 and UInt64 above i64::MAX are errors, not truncated). The topic gives the group, message type, edge node and device, and wildcards are rejected. `SparkplugNormalizer::points` emits one point per metric.**
- [x] Matter TLV: real Matter TLV (all element types, tag forms, bounded nesting), unsecured message and protocol headers, and Interaction Model ReportData attribute reports. Secured sessions are reported as encrypted, not decoded. The opcode values follow the connectedhomeip implementation and should be checked against the spec with a real capture. Matter's dead duplicate length check and the "Other" cluster are gone with the old layout.
- [x] KNX: service_type ignored in normalizer, priority bit comment vs code mismatch, redundant validator; Matter: dead duplicate length check, "Other" cluster — **KNX done: the parser now reads cEMI L_Data frames (message code, control fields, source individual address, group or individual destination, NPDU length, APCI service) and DPT decoding is explicit (decode_dpt1/5/9). The normalizer uses the source and group address. Matter is still open.**
- [x] CoAP: option-number wrap, empty payload after 0xFF marker, Uri-Path invalid UTF-8, device_id/point_id choice — **done: option numbers use checked arithmetic, a payload marker with no payload is an error, Uri-Path is one UTF-8 segment per option, and Max-Age, Observe and Content-Format reject values longer than their maximum. device_id/point_id choice is unchanged.**
- [x] MAVLink: only 4 message IDs named, signature lost on re-serialize, normalizer reads first 4 bytes not decoded fields; CRC_EXTRA test only covers HEARTBEAT v1 — add v2 and other IDs — **done: every dialect message is named through the `Message` trait; the frame keeps the decoded `MavMessage`, and the normalizer reads typed fields for HEARTBEAT, SYS_STATUS, ATTITUDE and GPS_RAW_INT; the v2 signature block (link id, timestamp, value) is kept on the frame. The signature is parsed, not verified: the mavlink crate's verifier needs std. CRC_EXTRA is tested for v1 and v2 and for several messages.**
- [ ] Core: use the unused `Error` variants (`Checksum` should carry a u16/u32 — **done: `Checksum` carries u32 values, and Modbus RTU, DNP3 header and block, and NMEA use it. `Overflow` and `InvalidFrame` are still unused, which is fine until a protocol needs them.**), `try_parse` must report real consumed/need-more-data (Modbus and J1939 still return `bytes.len()`), add `FrameEncoder` trait with round-trip tests, give `ProtocolPlugin` a real I/O path — **partial: Modbus `try_parse` now frames streams (`Ok(None)` for incomplete frames, exact consumed length; RTU is the shortest CRC-valid prefix, TCP uses the MBAP length). J1939 is not a stream format (one CAN frame per slice), so it is unchanged. `FrameEncoder` is in core, and `ModbusEncoder` is its first implementation, with round-trip tests for requests and responses. Other encoders and the ProtocolPlugin I/O path are still open.**
- [ ] Normalizers: real timestamps (15 hard-code 0), fan out via `normalize_batch`, stop emitting meaningless values; uniform constructors/derives/state structs across older vs newer crates; consistent `Error::parse` use — **partial: normalizers leave timestamp 0 by design (they have no clock), and the daemon's replay path stamps it at ingest. Several crates now have a `points()` fan-out (Sparkplug, Matter, NMEA), and `normalize_batch` still maps one frame to one point. Still open: `normalize_batch` fan-out, the meaningless values in the older crates, and uniform constructors.**

## Phase 12 — Real Data Path & Daemon/UI Polish
- [ ] `Transport` trait + Serial/TCP/UDP/SocketCAN backends, streaming framer using `try_parse`; plugins run real bytes through the protocol crate instead of a sine wave (supersedes the two Deferred items below)
- [ ] `tracing` + `RUST_LOG`, metrics (clients, lagged/dropped), `/healthz`, `--interval`; feature-gate protocols in the daemon
- [ ] CLI: restore terminal on every exit path, filter `key.kind == Press` (double scroll on Windows), `VecDeque` + bounded channel, date/timezone in timestamps, auto-follow, pong handling
- [ ] ui-web: CSP hard-codes `ws://127.0.0.1:9080` vs `TPT_DAEMON_URL`, remove production devtools permission, reconnect timers stack, O(n²) slice, no teardown; ui-embedded: `unwrap()` on `spawn_local`, show connection status
- [ ] Drop unused `native-tls` from cli/ui crates; build and lint ui-web/ui-embedded in CI
- [ ] Daemon tests: `datapoint_to_json`, `is_loopback_bind`, config precedence, replay hex/binary, end-to-end WS with good/bad Origin; CLI tests for `parse_data_points`, `format_timestamp`, scrolling

## Phase 13 — Adoption Kit
- [ ] Dockerfile + `docker run` quickstart for the daemon
- [ ] Release workflow (cargo-dist or release-plz) with prebuilt Windows/Linux/macOS binaries; `cargo package` dry-run CI job; README badges
- [ ] Runnable `examples/parse_frame` for every protocol (only tup, core, modbus have one) plus sample frame files usable with `--replay`
- [ ] Windows-friendly scaffolder (`cargo xtask new-protocol` or cargo-generate template) that also wires the daemon plugin and adds proptest + fuzz target (current script is bash-only and does neither)
- [ ] "Bring your own protocol in 30 lines" tutorial based on `core/examples/custom_protocol.rs`
- [ ] WASM build (`wasm-bindgen`) + browser "paste hex, see TUP" playground
- [ ] `pyo3` wheel exposing `decode(protocol, bytes) -> dict`; `cbindgen` C header for embedded/PLC vendors
- [ ] CI: MSRV job, Windows/macOS matrix, coverage, criterion benchmarks, cargo-semver-checks, cargo-audit, `--no-default-features`/`--all-features` no_std matrix, `cargo doc -D warnings`, telos `--features verify`, pin `cargo-deny-action`, `rust-cache`, scheduled advisory run, `rust-toolchain.toml`

## Phase 14 — Innovative Additions (pick later)
- [ ] Protocol auto-detect: try all parsers on raw bytes and rank by confidence
- [ ] Capture import/export: pcap/pcapng and candump replay; TUP out to MCAP/Parquet/NDJSON
- [ ] Sinks/bridges: MQTT, OPC UA server, InfluxDB/Prometheus remote-write, OpenTelemetry
- [ ] Declarative mapping config (TOML/YAML) from (protocol, device, point) to names, units, scaling
- [ ] Differential testing against reference implementations (tshark dissectors, pymodbus, …) in CI
- [ ] Anomaly/security rules on TUP streams (unexpected function codes, replay, rate spikes)
- [ ] Link telos specs to real code (extend beyond Modbus; consider Kani/Creusot); telos `verify_dir` is cwd-relative; only 3 Modbus specs exist today

## Later / Deferred
- [x] `ui-embedded` (Slint native dashboard) — `tpt-protocol-ui-embedded`, connects to daemon WS
- [x] `ui-web` (Tauri v2 + SvelteKit fleet dashboard) — `tpt-protocol-ui-web`, connects to daemon WS
- [x] Formal verification (`tpt-telos`) for critical state machines — `tpt-protocol-telos`: bundled `.telos` specs for Modbus CRC bounds, validator error-counter, and normalizer register bounds; `cargo test -p tpt-protocol-telos --features verify` discharges them with the QF_LRA verifier
- [ ] Dedicated low-latency IPC channel (Unix socket/named pipe) if WebSocket proves insufficient for the TUI
- [ ] Real transport clients (Serial/CAN/TCP/UDP) for the daemon — genuine
      hardware I/O, beyond Phase 9.3's simulation-only wiring
- [x] MAVLink `CRC_EXTRA` per-message seed table for full spec compliance — **resolved 2026-11-08: provided by the `mavlink` crate's generated `extra_crc` table (see Phase 9.2).**
