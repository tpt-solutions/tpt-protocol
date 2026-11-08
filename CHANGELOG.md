# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `FrameEncoder` trait in `tpt-protocol-core`, the inverse of `FrameParser`, and `ModbusEncoder`, with round-trip tests.
- `cddl_conformance` tests check encoder output against the CDDL rules, and check that the
  protocol and quality lists match the Rust enums.
- Structured Modbus MBAP property test (valid header, skewed length, mutation).
- `fuzz/` cargo-fuzz crate with one target per protocol parser (`parse` and `try_parse`) and
  for the TUP CBOR and JSON decoders. Nightly job in `.github/workflows/fuzz.yml`.
- `msrv` CI job (Rust 1.81). Dependabot, SECURITY.md, and issue/PR templates.
- Daemon `--replay-format hex|raw` (default `hex`); `--replay` now requires `--simulate`.
- Daemon `--token` and `--allow-origin` flags.
- Daemon answers WebSocket `Ping` frames and stops on `Close`. Clients that fall
  behind receive `{"type":"lagged","dropped":n}`.
- Daemon now simulates **all 19** protocol crates via dedicated `ProtocolPlugin`
  implementations (`tpt-protocol-daemon/src/plugins/mod.rs`).
- `--list-protocols` flag lists every supported protocol.
- `--replay <file>` mode runs raw bytes through a protocol's `FrameParser` /
  `DataNormalizer` instead of synthetic simulation.
- Simulated data points rotate through `Good` / `Uncertain` / `Bad` / `Stale`
  quality so the CLI color coding is demoable out of the box.
- `proptest` fuzz tests (`mod fuzz`) added to all 19 protocol crates, asserting
  `FrameParser::parse` never panics on arbitrary input.
- Standalone schema files `schema/tup-v1.cddl` and `schema/tup-v1.schema.json`,
  with a test validating SPEC-TUP.md's worked example against the JSON schema.
- Runnable examples: `tpt-protocol-tup/examples/roundtrip.rs`,
  `tpt-protocol-core/examples/custom_protocol.rs`,
  `tpt-protocol-modbus/examples/parse_frame.rs`.
- `daemon.example.toml` and Python/JS WebSocket consumer snippets.
- `scripts/new-protocol-crate.sh` scaffolding script.
- `deny.toml` and a `cargo-deny` CI job (license/advisory/duplicate-version bans).

### Security
- Daemon WebSocket API: handshakes are checked for a loopback `Host` header
  (DNS-rebinding defence) and an allowed browser `Origin` (Tauri webview origins
  by default, plus `--allow-origin`). An optional `--token` requires a token,
  compared in constant time.
- Daemon: handshakes time out after 5 s, at most 64 clients are served, and
  incoming messages are capped at 64 KiB.
- Modbus: an MBAP length below 2 no longer panics (`bytes[8..6]` slicing).
- HL7 v2: non-char-boundary slicing of multi-byte input no longer panics.

- Fixed DNP3 out-of-bounds panic on a truncated frame (trailing CRC guard).
- Fixed Matter-Thread off-by-one OOB panic on Float TLV (`< 21` vs `< 20`).
- Fixed DoIP/SOME-IP and OCPP `u32` length-integer overflow into an invalid
  slice range (`checked_add` / `saturating_sub`).
- Capped wire-controlled `with_capacity` allocations in IEEE 11073-SDC,
  Sparkplug B, and Modbus to prevent unbounded allocation.
- CLI no longer panics when truncating a multi-byte `device_id` / `point_id`
  (`truncate_to` respects UTF-8 char boundaries).
- Daemon warns (instead of silently allowing) when `--bind` is not loopback.
- Dropped the unused `native-tls` feature from `tokio-tungstenite` (no TLS
  acceptor existed).
- Root workspace sets `overflow-checks = true` in `[profile.release]`.

### Changed
- **Matter:** the parser decodes Matter TLV, the unsecured message and protocol headers, and Interaction Model
  ReportData attribute reports. The previous binary layout was not Matter. Secured sessions are flagged as encrypted.
  `MatterNormalizer::points` emits one point per scalar attribute.
- **Sparkplug B:** the parser decodes the real Protobuf payload (metrics by datatype) and the `spBv1.0` topic.
  The previous binary framing was not Sparkplug. `SparkplugNormalizer::points` emits one point per metric.
- **MAVLink:** every dialect message has its name and numeric id (previously four were named). Frames carry the
  decoded `MavMessage`, and the normalizer reads typed fields for HEARTBEAT, SYS_STATUS, ATTITUDE and GPS_RAW_INT. The
  v2 signature block is kept on signed frames. Trailing bytes after a frame are an error. `MavlinkMessageId` is replaced
  by `message_id` (u32) and `message_name`.
- **OCPP:** the parser reads OCPP-J (JSON over WebSocket): Call, CallResult and CallError, with the payload kept as
  raw JSON. The previous binary framing was not OCPP. Message ids are limited to 36 characters.
- **Modbus:** `try_parse` frames streams. It returns `Ok(None)` for an incomplete frame and the exact consumed
  length. TCP uses the MBAP length, and RTU takes the shortest CRC-valid prefix. RTU framing can be ambiguous
  by chance (about 1 in 65,536 per candidate length).
- **KNX:** the parser reads cEMI `L_Data` frames. The previous layout was not KNX. Priority, hop count, source
  individual address, group or individual destination, and APCI service are decoded. `DptValue` is now `Short` or `Data`.
  DPT decoding is explicit: `decode_dpt1`, `decode_dpt5_percent` and `decode_dpt9`.
- **CoAP:** option numbers no longer wrap silently. A payload marker with no payload is an error. Uri-Path is one
  UTF-8 segment per option, and invalid UTF-8 is an error. Max-Age, Observe and Content-Format longer than their
  maximum are errors.
- **CANopen:** COB-ID function codes follow CiA 301. Function code 1 is SYNC only for node 0 and Emergency
  otherwise. TPDO3, RPDO3, TPDO4 and RPDO4 are added.
- **J1939:** the PGN includes the EDP bit, and PDU1 frames mask out the destination address. TC1 is PGN 0,
  and 0xFF01 is no longer named TC1. CCVS (0xFEF1) is added, and 0xFEF5 is Ambient Conditions (`CabTemp` is renamed).
- **ISOBUS:** PGN names follow ISO 11783 (0xEC00 is TP.CM, not RequestForRep). PDU1 masking matches J1939.
  `implement_id` is little-endian.
- **OPC UA:** Binary encoding is little-endian. `message_size` must equal the frame length. The MSG TypeId
  NodeId and ExtensionObject body are decoded. `OpcuaFrame::node_id` is replaced by `type_namespace`.
- **EtherCAT:** command codes follow IEC 61158 (APWR=2, FPWR=5, BWR=8, LRD=10, LWR=11 were wrong). The
  header is little-endian with an 11-bit length. `EthercatFrame` now holds a `datagrams` list.
- **NMEA:** a checksum mismatch is a parse error. The checksum range is correct with leading whitespace.
  GN, GL, GA and BD talkers are recognized.
- **Modbus:** `ModbusParser::response()` parses response frames. Read responses, write echoes and exception
  responses (`fc | 0x80`) are distinct variants. Requests are range-checked, FC 0x05 accepts only 0xFF00/0x0000,
  and FC 0x10 checks its byte count. Read requests no longer normalize to their quantity; they return an error.
  `ModbusRtuFrame` has a `transaction_id` field.
- **DNP3:** the header CRC covers the start bytes, user data is parsed in 16-byte blocks with per-block CRCs,
  and transport and application headers are parsed. Frames built with the old layout are rejected.
- **DoIP/SOME-IP:** payload types follow ISO 13400-2 (0x0004-0x0008 and 0x8001-0x8003 were wrong). Truncated
  or trailing DoIP payloads and wrong SOME/IP lengths are errors.
- **BACnet:** BVLC function codes follow Annex J, the NPDU is parsed, and the object identifier is decoded
  from the packed form. The BVLC length must match the frame.
- **UDS:** negative responses (0x7F) parse. 0x14 has no sub-function. `suppressPosRsp` is exposed. Services
  0x19, 0x31, 0x34-0x37 and 0x85 are added.
- **TUP JSON mirror:** `Bytes` values are `{"bytes": "0x…"}` and non-finite floats are
  `{"float": "NaN" | "Infinity" | "-Infinity"}`. Before, bytes came back as text and
  NaN/Infinity became `null` and failed to parse. `schema/tup-v1.schema.json` is updated to match.
- **TUP CBOR decoder** rejects indefinite-length maps and value arrays, extra value array
  elements, duplicate keys, and `schema_version` 0. `encode_to_vec` returns the encoder's error.
- **TUP JSON decoder** rejects `schema_version` 0 or above the supported maximum, invalid
  `raw` hex, and integers above the u64 range. It no longer re-types them as floats.
- `schema/tup-v1.cddl` and `SPEC-TUP.md` §3 describe the CBOR encoding the code produces: integer
  map keys and `[tag, value]` values. Previously they described text keys.
- `Quality` and `SourceProtocol` are `#[non_exhaustive]`.
- Workspace MSRV raised from 1.75 to 1.81. `core::error::Error` needs 1.81.
- CANopen, J1939 and ISOBUS normalizers read the 4-byte payload value as little-endian,
  as CAN and SAE J1939 specify. The previous big-endian read gave wrong values.

- Reconciled `dead_code` suppressions in `tpt-protocol-knx` and
  `tpt-protocol-coap`: dead helpers (`KnxServiceType::as_str`,
  `GroupAddress::to_u16`, `CoapCode` classification methods) are now part of the
  public round-trip API instead of being `#[allow(dead_code)]`.
- Phase 5 "Real transport clients" item un-checked: the daemon remains
  simulation-only; real Serial/CAN/TCP/UDP transport is future work.

### Known gaps (documented, not fixed this pass)
