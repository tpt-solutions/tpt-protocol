# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Per-crate changes are in each crate's `CHANGELOG.md`.

## [Unreleased]

## [0.1.0] - 2026-11-11

### Added

- **Daemon transports** behind one streaming framer that uses each protocol's `try_parse`:
  TCP connect (reconnects) and listen, UDP (one datagram is one message), serial
  (`--serial`, feature `serial`), SocketCAN (`--can`, feature `socketcan`, Linux), and
  `--stdin` for piped captures. The serial test runs against a real pseudo-terminal pair,
  and the SocketCAN test runs against `vcan0`.
- **Daemon capture and detection:** `--replay-format candump` and `--replay-format pcap`
  (classic pcap and pcapng, with `--pcap-port`). Modbus pcap replay pairs each response with
  its request by transaction id. `--detect FILE` ranks the protocols that decode a capture.
- **Daemon mapping and anomaly rules:** `[[map]]` in the TOML config (rename, unit, scale,
  offset, range check) and `[[anomaly]]` (timestamp order, minimum interval, maximum step).
  An anomaly marks a point uncertain and keeps its value.
- **Daemon Modbus polling:** `--modbus-poll HOST:PORT` reads holding or input registers from
  a Modbus TCP server, and reconnects after failures. Verified against pymodbus 3.6.9.
- **Daemon outputs:** `--mqtt HOST:PORT` (feature `mqtt`, QoS 0, not retained, verified against
  Mosquitto), `--influx URL` (InfluxDB line protocol over plain HTTP), `--otel URL` (OTLP/HTTP
  JSON metrics), `--prometheus-remote-write URL` (protobuf, snappy literal form), and
  `--opcua-listen ADDR` (an OPC UA server that serves the points, read-only, anonymous,
  `SecurityPolicy#None`).
- **Daemon recording:** `--record FILE` (NDJSON), `--record-mcap FILE` (feature `mcap-record`,
  unchunked so an interrupted file keeps its messages), and `--record-parquet FILE` (feature
  `parquet-record`; the footer is written only on a clean exit).
- **Daemon replay and simulation:** one replayable sample per protocol in `samples/`, with a
  daemon test for each. `--list-protocols` lists the protocols. Simulated points rotate through
  `Good`, `Uncertain`, `Bad` and `Stale` quality.
- **Daemon config and tooling:** `daemon.example.toml`, `--interval MS`, `tracing` logs to stderr
  (`RUST_LOG`), one Cargo feature per protocol (all on by default), and a `Dockerfile` (untested:
  no Docker daemon was available when it was written).
- **WebSocket API handshake policy:** the Host header is checked on loopback binds (DNS
  rebinding), the browser Origin is checked (Tauri origins by default, plus `--allow-origin`),
  and an optional `--token` is compared in constant time. Handshakes time out after 5 s, at
  most 64 clients are served, and incoming messages are capped at 64 KiB. Ping is answered, and
  Close stops the session. A client that falls behind receives `{"type":"lagged","dropped":n}`.
- **`tpt-protocol-core`:** `FrameEncoder`, the inverse of `FrameParser`, and
  `DataNormalizer::normalize_all`, which returns every point a frame carries. `normalize_batch`
  uses it, so batches fan out.
- **Encoders** (`FrameEncoder`, with round-trip tests against the parser) for Modbus (RTU and
  TCP), UDS, NMEA, CoAP, ISOBUS, J1939, DoIP/SOME-IP, KNX, EtherCAT, OPC UA, OCPP-J, DNP3,
  CANopen, Sparkplug B, HL7 v2 and IEEE 11073 SDC. BACnet, Matter and MAVLink have none yet.
- **NMEA 2000** (`tpt-protocol-nmea`, module `nmea2000`): a CAN frame parser and encoder, a
  fast-packet reassembler, and decoders for PGNs 129025, 127250, 130306, 128267 and 129029. The
  field layouts follow the published definitions and are not yet checked against a bus capture.
- **Structured property tests** (valid frames, then mutation) in every protocol crate. NMEA,
  HL7 v2, OCPP-J, CANopen and J1939 generate frames in their real layouts.
- **`cddl_conformance` tests** check encoder output against the CDDL rules, and check that the
  protocol and quality lists match the Rust enums. The checker is hand-written, not a general
  CDDL validator.
- **Standalone schema files** `schema/tup-v1.cddl` and `schema/tup-v1.schema.json`, with a test
  that validates the worked example in `SPEC-TUP.md` against the JSON schema.
- **`proptest` fuzz tests** (`mod fuzz`) in every protocol crate, asserting that
  `FrameParser::parse` never panics on arbitrary input.
- **`fuzz/`** cargo-fuzz crate with one target per protocol parser (`parse` and `try_parse`),
  and for the TUP CBOR and JSON decoders. Nightly job in `.github/workflows/fuzz.yml`.
- **Runnable examples:** `examples/parse_<protocol>.rs` for all 19 protocols,
  `tpt-protocol-tup/examples/roundtrip.rs`, `tpt-protocol-core/examples/custom_protocol.rs`, and
  `tpt-protocol-modbus/examples/parse_frame.rs`.
- **Differential tests** against reference libraries, run in CI: Modbus (pymodbus 3.6.9), NMEA
  0183 (pynmea2), HL7 v2 (hl7) and UDS (udsoncan). Each pins its library version and was checked
  to catch a deliberately wrong expectation.
- **Endpoint tests** (`scripts/endpoints/`, CI job `endpoints`): pymavlink, pynmea2 and aiocoap
  encode frames, the daemon decodes them from UDP datagrams, and every recorded field is checked.
- **Bindings** (`tpt-protocol-bindings`): a C ABI (`include/tpt_protocol.h`), a WebAssembly build
  with a browser playground (`www/`), and a Python module built with maturin. All three decode one
  frame to TUP JSON.
- **`cargo xtask new-protocol <name>`** scaffolds a protocol crate and wires it into the workspace,
  the wire schema (CDDL and JSON Schema), the daemon and the fuzz targets. `--dry-run` shows the
  changes.
- `docs/bring-your-own-protocol.md`: a guide to adding a protocol.
- Release workflow (`.github/workflows/release.yml`): builds the daemon and CLI for Linux, Windows
  and macOS on a version tag.
- CI: docs (with `-D warnings`), telos verification, Windows and macOS test jobs, the UI crates
  (clippy, svelte-check, Vite build), coverage, cargo-audit, a `no_std` feature matrix on
  `thumbv6m-none-eabi`, a `package-list` job (`cargo package --list` for the 21 published crates,
  with no publishing), a MQTT integration job against Mosquitto, and a weekly scheduled run.
- Dependency versions shared by several crates are declared once in `[workspace.dependencies]`.
- README badges for CI and license.
- `CONTRIBUTING.md` accepts contributions as issues only. Pull requests are not accepted.

### Changed

- **Core:** `Error::Checksum` carries `u32` values. It was `u8`, which cannot hold a 16-bit CRC.
  Modbus RTU, DNP3 and NMEA use it for checksum mismatches, so callers can match the variant.
  `default-features = []`, so the crate builds `#![no_std]` by default.
- **Daemon:** `--replay-format` accepts `hex` (default), `raw`, `candump` and `pcap`. `--replay`
  requires `--simulate`. The WebSocket JSON output uses `tup::json::to_json_string`. CLI flags
  override the config file, and unknown config keys are rejected. The simulation feeds a separate
  source channel, and the mapping stage forwards to the clients, so the WebSocket API and
  recordings see mapped points. Logs are plain text when stderr is not a terminal. An empty read
  buffer is no longer sent to a parser, so a datagram at a frame boundary no longer logs a
  spurious warning. Replayed points with timestamp 0 are stamped at ingest. Normalizers still leave
  0, because they have no clock, and `DataNormalizer` documents this.
- **TUI (`tpt-protocol-cli`):** the terminal is restored on every exit path, and only key presses
  are handled. Timestamps show the UTC date and time with a `Z`. The view follows the newest rows,
  and scrolling up pauses that. The data buffer and socket channel are bounded. Pings are answered.
- **Web UI and embedded UI:** standalone workspace roots, outside the main workspace, so their GUI
  trees do not affect the core build, CI or `cargo-deny`. They require Rust 1.90. The web CSP
  allows loopback on any port.
- **IEEE 11073 SDC:** the parser reads SOAP 1.2 XML, with a bounded XML parser, the WS-Addressing
  headers, and numeric observations. The previous binary layout was not SDC. BICEPS names are
  unverified against a device.
- **NMEA:** GGA, RMC, DBT, VHW and MWV normalize to their real fields. Coordinates are signed
  decimal degrees. A checksum mismatch is a parse error (`Error::Checksum`), not a frame with
  `checksum_valid = false`. The checksum range is computed on the trimmed sentence. GN, GL, GA and
  BD talkers are recognized. GLL, VTG, GSA, GSV, HDT and ZDA are decoded.
- **Matter:** the parser decodes Matter TLV, the unsecured message and protocol headers, and
  Interaction Model ReportData attribute reports. The previous binary layout was not Matter.
  Secured sessions are flagged as encrypted. `MatterNormalizer::normalize_all` emits one point per
  scalar attribute.
- **Sparkplug B:** the parser decodes the Protobuf payload (metrics by datatype) and the `spBv1.0`
  topic. The previous binary framing was not Sparkplug. `SparkplugNormalizer::normalize_all` emits
  one point per metric.
- **MAVLink:** frames are decoded with the `mavlink` 0.15 crate (`embedded` and `common`), so every
  common-dialect message is named. The frame carries the decoded `MavMessage`, and the normalizer
  reads typed fields for HEARTBEAT, SYS_STATUS, ATTITUDE and GPS_RAW_INT. The v2 signature block is
  kept (not verified). Trailing bytes after a frame are an error. `MavlinkMessageId` is replaced by
  `message_id` (u32) and `message_name`. GPS latitude and longitude are degrees, and altitude is
  metres. `tpt-protocol-mavlink` is published.
- **OCPP:** the parser reads OCPP-J (JSON over WebSocket): Call, CallResult and CallError, with the
  payload kept as raw JSON. The previous binary framing was not OCPP. Message ids are limited to 36
  characters.
- **Modbus:** `ModbusParser::response()` parses response frames. Read responses, write echoes,
  exception responses (`fc | 0x80`) and `WriteMultipleResponse` are distinct variants. Requests are
  range-checked: FC 0x05 accepts only `0xFF00` and `0x0000`, and FC 0x10 checks its byte count. Read
  requests normalize to an error, not to their quantity. Read responses normalize to their first
  value. `try_parse` returns `Ok(None)` for an incomplete frame and the exact consumed length. TCP
  uses the MBAP length, and RTU takes the shortest CRC-valid prefix. A TCP check on the whole buffer
  misread a following RTU frame as TCP, so the RTU check comes first. RTU framing can match by chance
  (about 1 in 65,536 per candidate length).
- **KNX:** the parser reads cEMI `L_Data` frames. The previous layout was not KNX. The normalizer
  uses the source individual address as the device and the destination group address as the point.
  DPT decoding is explicit: `decode_dpt1`, `decode_dpt5_percent` and `decode_dpt9`. `DptValue` is
  `Short` or `Data`.
- **CoAP:** option numbers use checked arithmetic. A payload marker with no payload is an error.
  Uri-Path is one UTF-8 segment per option, and invalid UTF-8 is an error. Max-Age, Observe and
  Content-Format longer than their maximum are errors.
- **CANopen:** COB-ID function codes follow CiA 301. Function code 1 is SYNC only for node 0 and
  Emergency otherwise. TPDO3, RPDO3, TPDO4 and RPDO4 are added. The normalizer reads the payload as
  little-endian.
- **J1939:** the PGN includes the EDP bit, and PDU1 frames mask out the destination address. TC1 is
  PGN 0. CCVS (0xFEF1) is added, and 0xFEF5 is Ambient Conditions. The normalizer decodes EEC1, ET1,
  CCVS and fuel-economy signals in their units, and gives other PGNs their payload as bytes.
- **ISOBUS:** PGN names follow ISO 11783 (0xEC00 is TP.CM, not RequestForRep). PDU1 masking matches
  J1939. The ECU identity is the 64-bit NAME of Address Claimed frames (ISO 11783-5 layout).
  `implement_id` and `device_class` are no longer read from arbitrary payload bytes.
- **OPC UA:** Binary encoding is little-endian. `message_size` must equal the frame length. The MSG
  body is decoded as an ExtensionObject (TypeId NodeId and body). `OpcuaFrame::node_id` is replaced
  by `type_namespace`.
- **EtherCAT:** command codes follow IEC 61158-4-12 (APWR=2, FPWR=5, BWR=8, LRD=10, LWR=11). The
  header is little-endian with an 11-bit length. The whole datagram chain is parsed, and
  `EthercatFrame` holds a `datagrams` list.
- **DNP3:** the header CRC covers the start bytes. User data is parsed in 16-byte blocks with
  per-block CRCs. Transport and application headers are parsed. Response codes (0x81, 0x82) are
  recognized. The normalizer reports the data body, and a bare request is an error.
- **DoIP/SOME-IP:** payload types follow ISO 13400-2 (0x0004–0x0008 and 0x8001–0x8003 were wrong).
  Truncated or trailing DoIP payloads and wrong SOME/IP lengths are errors. The normalizer reports
  the payload as bytes, and a header-only message is an error.
- **BACnet:** rewritten against ASHRAE 135 Annex J. BVLC function codes, the NPDU, and the packed
  object identifier are decoded, and ReadProperty values are decoded. The BVLC length must match the
  frame. Requests normalize to an error.
- **UDS:** negative responses (0x7F) parse and normalize to a `Bad`-quality point. 0x14 takes a
  group of DTC, not a sub-function. `suppressPosRsp` is exposed. Services 0x19, 0x31, 0x34–0x37 and
  0x85 are added.
- **HL7 v2:** one data point per OBX observation, with numeric results as floats, units and quality
  from the result status.
- **TUP JSON mirror:** `Bytes` values are `{"bytes": "0x…"}`, and non-finite floats are
  `{"float": "NaN" | "Infinity" | "-Infinity"}`. Before, bytes came back as text and NaN/Infinity
  became `null` and failed to parse. `schema/tup-v1.schema.json` is updated to match.
- **TUP CBOR decoder** rejects indefinite-length maps and value arrays, extra value array elements,
  duplicate keys, and `schema_version` 0. `encode_to_vec` returns the encoder's error.
- **TUP JSON decoder** rejects `schema_version` 0 or above the supported maximum, invalid `raw` hex,
  and integers above the u64 range. It no longer re-types them as floats.
- `schema/tup-v1.cddl` and `SPEC-TUP.md` §3 describe the CBOR encoding the code produces: integer map
  keys and `[tag, value]` values. Previously they described text keys.
- `Quality` and `SourceProtocol` are `#[non_exhaustive]`.
- `tpt-protocol-tup` and `tpt-protocol-core` use `default-features = false` in the workspace. The
  daemon, CLI and UI crates opt into `std` explicitly.
- Workspace MSRV is 1.85, for `minicbor` 2.3 (edition 2024). `.cargo/config.toml` sets
  `incompatible-rust-versions = "fallback"`.
- Reconciled `dead_code` suppressions in `tpt-protocol-knx` and `tpt-protocol-coap`: dead helpers
  (`KnxServiceType::as_str`, `GroupAddress::to_u16`, `CoapCode` classification methods) are now part
  of the public round-trip API instead of being `#[allow(dead_code)]`.
- Phase 5 "Real transport clients" is partly done. The daemon has real TCP, UDP, serial, SocketCAN and
  stdin transports and a Modbus TCP poller. The `ProtocolPlugin` trait still exposes only `simulate()`.

### Removed

- `CODE_OF_CONDUCT.md`, `SECURITY.md` and the pull request template, by maintainer decision.
- `scripts/new-protocol-crate.sh`, replaced by `cargo xtask new-protocol`.
- The single-file `transport.rs` module, replaced by the `transport/` module.
- The `msrv` CI job pinned Rust 1.81. It now pins 1.85.

### Fixed

- The TUI answered no pings, because its read loop dropped the write half.
- The web UI's Vite config imported from the wrong package and was missing `src/app.html`, so it never
  built.
- The web UI rendered daemon lagged-notices as data rows.
- The embedded UI panicked if the Slint event task could not start, and its channel grew without bound.
- `rustdoc` rejected bracketed byte-index comments (`[0]`) under `-D warnings` (CoAP, KNX), and two
  intra-doc links to private items (daemon, NMEA 2000).
- The `msrv` CI job failed because `minicbor 2.3` and `clap_lex` need edition 2024. Resolved by raising
  the MSRV to 1.85.
- CoAP option parsing overflowed in debug builds (`+ 269` on a 2-byte extension). It is now a parse error.
- The MAVLink CRC_EXTRA seed is enforced. A regression test re-signs a frame with the wrong seed.
- `deny.toml` allows CC0-1.0, which is reached only through `tiny-keccak` in a dev-dependency tree.
- The daemon's MQTT client uses plain TCP only (`rumqttc` built without its default TLS stack). Its
  unused TLS dependency had four RUSTSEC advisories.

### Security

- Daemon WebSocket API: handshakes are checked for a loopback `Host` header (DNS-rebinding defence)
  and an allowed browser `Origin`. The `--token` comparison is constant time.
- Daemon: the daemon warns (instead of silently allowing) when `--bind` is not loopback.
- Modbus: an MBAP length below 2 no longer panics (`bytes[8..6]` slicing).
- HL7 v2 and NMEA: non-char-boundary slicing of multi-byte input no longer panics.
- CLI: truncating a multi-byte `device_id` or `point_id` no longer panics (`truncate_to` respects
  UTF-8 char boundaries).
- DNP3: out-of-bounds panic on a truncated frame fixed (trailing CRC guard).
- Matter: off-by-one out-of-bounds panic on a Float TLV element fixed (`< 21` vs `< 20`).
- DoIP/SOME-IP and OCPP: `u32` length overflow into an invalid slice range fixed (`checked_add` and
  `saturating_sub`).
- Capped wire-controlled `with_capacity` allocations in IEEE 11073 SDC, Sparkplug B and Modbus, to
  prevent unbounded allocation.
- Dropped the unused `native-tls` feature from `tokio-tungstenite` (no TLS acceptor existed).
- Root workspace sets `overflow-checks = true` in `[profile.release]`.

### Known gaps (documented, not fixed)

- **Plugins simulate.** Each `ProtocolPlugin` still simulates. Real data comes from the daemon's
  transports, not from the plugin trait.
- **Serial and SocketCAN are not tested on real hardware.** They are tested against a pseudo-terminal
  and a `vcan0` virtual interface.
- **Capture replay** is checked on synthetic captures and one loopback capture, not on a physical
  network.
- **NMEA 2000 and Matter field layouts** are not yet checked against a bus or network capture.
- **No TLS on the daemon's outputs.** The InfluxDB and OpenTelemetry sinks and the MQTT sink use plain
  TCP. The WebSocket API is loopback-only, and it is unauthenticated unless `--token` is given.
