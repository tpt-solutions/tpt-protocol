# The TPT Unified Protocol (TUP)

Version: `1` (draft)

## 1. Motivation

TPT Protocol decodes 19 heavily-gatekept industrial, medical, and hardware
protocols (see `spec.txt` §4) into normalized data. Until now, that
"normalized data" has only ever been described as an internal Rust type —
useful to the `tpt-protocol-*` crates, but invisible to anything outside
them.

TUP is the fix: a small, versioned, openly-specified message envelope that
*any* of the 19 source protocols maps into, and that any consumer — Rust or
not, TPT or not — can parse from this document alone. It is the actual
"common language" implied by the project's Rosetta Stone framing (`spec.txt`
§1, §3 Goal 1). Every `DataNormalizer` implementation across the 19 protocol
crates emits TUP messages; the daemon streams TUP messages to every UI.

## 2. Envelope Schema

| Field              | Type                                | Required | Notes |
|--------------------|--------------------------------------|----------|-------|
| `schema_version`   | uint                                  | yes      | `1` for this version; consumers reject unknown major versions |
| `source_protocol`  | enum (one per catalogued protocol)    | yes      | e.g. `modbus`, `j1939`, `opcua` — see full list in §3 |
| `device_id`        | text                                   | yes      | Identifies the physical asset (e.g. `modbus-tcp://10.0.4.12:502/unit-1`) |
| `point_id`         | text                                   | yes      | Logical measurement identifier (register name, PGN-SPN, OID, etc.) |
| `timestamp`        | uint                                   | yes      | Unix epoch, milliseconds, when the value was captured |
| `value`            | int \| float \| bool \| text \| bytes  | yes      | Tagged union — the normalized reading itself |
| `unit`             | text                                   | no       | e.g. `"rpm"`, `"kPa"`, `"degF"` |
| `quality`          | enum: `good` \| `bad` \| `stale` \| `uncertain` | yes | Trust signal — load-bearing for ICS/medical consumers deciding whether to act on a value |
| `raw`              | bytes                                  | no       | Original source frame, for audit/debugging pass-through |

## 3. Canonical Binary Encoding (CBOR)

The wire/embedded canonical form is [CBOR](https://www.rfc-editor.org/rfc/rfc8949)
(RFC 8949), defined here in [CDDL](https://www.rfc-editor.org/rfc/rfc8610)
(RFC 8610):

The normative schema is published as standalone files at the repository root:

- **`schema/tup-v1.cddl`** — CDDL for the CBOR canonical encoding (authoritative).
- **`schema/tup-v1.schema.json`** — JSON Schema (draft-07) for the JSON mirror
  encoding. A test in `tpt-protocol-tup` validates the worked example below
  against this schema.

```cddl
; TPT Unified Protocol (TUP) v1 — CDDL schema for the CBOR encoding

tup-message = {
  schema_version: uint .default 1,
  source_protocol: source-protocol,
  device_id: tstr,
  point_id: tstr,
  timestamp: uint,          ; unix epoch, milliseconds
  value: tup-value,
  ? unit: tstr,
  quality: quality,
  ? raw: bstr,
}

source-protocol = "modbus" / "j1939" / "isobus" / "uds" / "canopen" /
                   "doip-someip" / "nmea" / "mavlink" / "bacnet" / "opcua" /
                   "dnp3" / "ethercat" / "sparkplugb" / "ocpp" /
                   "ieee11073-sdc" / "hl7v2" / "knx" / "matter-thread" / "coap"

tup-value = int / float / bool / tstr / bstr

quality = "good" / "bad" / "stale" / "uncertain"
```

CBOR is chosen over Protocol Buffers because it is self-describing (no
external `.proto` codegen step required to decode a captured frame) and over
plain JSON-over-the-wire because it is compact and has first-class
`no_std`-compatible Rust implementations (e.g. `minicbor`), making it usable
directly on the constrained targets `tpt-protocol-core` already targets
(`spec.txt` §2).

## 4. Mirror Text Encoding (JSON)

The daemon's local WebSocket/REST API (`spec.txt` §5) streams the same
envelope as isomorphic JSON — one field-for-field mapping, no renaming — so
browser-based UIs and human debugging don't need a CBOR decoder.

Worked example — a Modbus holding-register read expressed as TUP:

```json
{
  "schema_version": 1,
  "source_protocol": "modbus",
  "device_id": "modbus-tcp://10.0.4.12:502/unit-1",
  "point_id": "holding-register:40001",
  "timestamp": 1753900000000,
  "value": 68.5,
  "unit": "degF",
  "quality": "good"
}
```

## 5. Versioning

`schema_version` is a single incrementing integer. Consumers must reject
messages with a `schema_version` higher than the newest version they
understand rather than guessing at unknown fields. New optional fields may
be added within the same version; any change to an existing field's meaning
or type requires a version bump.

## 6. Security Considerations

The reference `tpt-protocol-daemon` binds its API to `127.0.0.1` only
(`spec.txt` §5), so TUP has no transport-security requirement today. The
envelope is deliberately left room to add an authentication/signing field in
a future version without breaking `v1` consumers, so that exposing a TUP
stream beyond localhost later (e.g. to a fleet-wide aggregator) doesn't
default to insecure.

## 7. Relationship to `tpt-protocol-core`

`tpt-protocol-tup` is a standalone `no_std` crate containing only the schema
types and CBOR/JSON codecs — it depends on nothing else in the workspace.
`tpt-protocol-core`'s `DataNormalizer` trait (`spec.txt` §7) returns
`tup::DataPoint`, so every one of the 19 protocol crates produces TUP
messages by construction; the daemon simply forwards them.
