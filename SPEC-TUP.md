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
(RFC 8949), defined here in [CDDL](https://www.rfc-editor.org/rfc/rfc8610).

The normative schema is published as standalone files at the repository root:

- **`schema/tup-v1.cddl`** — CDDL for the CBOR canonical encoding (authoritative).
  Its text is reproduced below.
- **`schema/tup-v1.schema.json`** — JSON Schema (draft-07) for the JSON mirror
  encoding (§4). A test in `tpt-protocol-tup` validates the worked example
  below against this schema.

In CBOR, the envelope is a definite-length map with **integer keys**, not the
field names in §2:

| Key | Field             | CBOR type |
|-----|-------------------|-----------|
| 0   | `schema_version`  | uint (encoders always emit it; an absent key means `1`) |
| 1   | `source_protocol` | text, one of the names in the CDDL `source-protocol` rule |
| 2   | `device_id`       | text |
| 3   | `point_id`        | text |
| 4   | `timestamp`       | uint, milliseconds |
| 5   | `value`           | array `[type_tag, payload]` (see below) |
| 6   | `unit`            | text, optional |
| 7   | `quality`         | text: `good`, `bad`, `stale`, `uncertain` |
| 8   | `raw`             | bytes, optional |

`value` is a two-element array. The first element is the type tag, and the
second is the payload: `0` int, `1` float64, `2` bool, `3` text, `4` bytes.

Decoders skip unknown keys. Encoders must not emit them. The CDDL below is the
exact content of `schema/tup-v1.cddl`:

```cddl
; TPT Unified Protocol (TUP) v1 — CDDL schema for the CBOR canonical encoding.
; See SPEC-TUP.md for the human-readable description. This file is normative
; for CBOR. The JSON mirror uses the text field names in tup-v1.schema.json.
;
; Map keys are the small unsigned integers below. Encoders MUST emit key 0.
; Decoders treat an absent key 0 as schema_version 1 and skip unknown keys
; (forward compatibility). Encoders MUST NOT emit keys outside this list.

tup-message = {
  0 => uint,                ; schema_version (1 for this version)
  1 => source-protocol,
  2 => tstr,                ; device_id
  3 => tstr,                ; point_id
  4 => uint,                ; timestamp, unix epoch, milliseconds
  5 => tup-value,
  ? 6 => tstr,              ; unit
  7 => quality,
  ? 8 => bstr,              ; raw source frame
}

; Tagged union. The first element is the value type tag:
; 0 = int, 1 = float, 2 = bool, 3 = text, 4 = bytes.
tup-value = [0, int] / [1, float64] / [2, bool] / [3, tstr] / [4, bstr]

source-protocol = "modbus" / "j1939" / "isobus" / "uds" / "canopen" /
                   "doip-someip" / "nmea" / "mavlink" / "bacnet" / "opcua" /
                   "dnp3" / "ethercat" / "sparkplugb" / "ocpp" /
                   "ieee11073-sdc" / "hl7v2" / "knx" / "matter-thread" / "coap"

quality = "good" / "bad" / "stale" / "uncertain"
```

The conformance tests in `tpt-protocol-tup` (`cddl_conformance`) check encoder
output against these rules, and check that the protocol and quality lists in
the CDDL match the Rust enums. They are a hand-written structural check, not a
general CDDL validator.

CBOR is chosen over Protocol Buffers because it is self-describing (no
external `.proto` codegen step required to decode a captured frame) and over
plain JSON-over-the-wire because it is compact and has first-class
`no_std`-compatible Rust implementations (e.g. `minicbor`), making it usable
directly on the constrained targets `tpt-protocol-core` already targets
(`spec.txt` §2).

## 4. Mirror Text Encoding (JSON)

The JSON mirror uses the field names from §2 as text keys. It does not use the
CBOR integer keys.

The daemon's local WebSocket/REST API (`spec.txt` §5) streams the same
envelope as isomorphic JSON — one field-for-field mapping, no renaming — so
browser-based UIs and human debugging don't need a CBOR decoder.

In the mirror, `value` is a JSON number, boolean, or string. A string is always
text. Byte values are the object `{"bytes": "0x…"}`. JSON has no NaN or
infinity, so non-finite floats are `{"float": "NaN"}`, `{"float": "Infinity"}`,
or `{"float": "-Infinity"}`. The optional `raw` field is a `0x`-prefixed hex
string. Decoders reject invalid hex and a `schema_version` of 0 or above the supported
maximum. Known limit: the JSON parser reads integer literals above the u64
range as floats, so such values cannot round-trip through JSON. Use CBOR for
them.

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

Unknown `source_protocol` or `quality` values are rejected, not passed
through. The set of values is closed for v1. Adding a protocol or quality is a
change to the schema, so it needs a `schema_version` bump.

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
