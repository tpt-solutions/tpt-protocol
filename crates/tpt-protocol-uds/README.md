# tpt-protocol-uds

**ISO 14229 (UDS) automotive diagnostic-services parser for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.**

UDS (Unified Diagnostic Services) is the ISO 14229 application-layer protocol
used to diagnose and configure ECUs over CAN, DoIP, or K-Line. This crate is a
from-scratch, memory-safe parser/normalizer — there is no maintained Rust crate
for this protocol — implementing the TPT `FrameParser` / `StateValidator` /
`DataNormalizer` traits so UDS traffic normalizes into TUP `DataPoint`s like
every other protocol in the suite.

## Features

- `#![no_std]` by default — only needs `alloc`.
- Implements the three core traits from
  [`tpt-protocol-core`](../tpt-protocol-core).
- Recognises the commonly used UDS service identifiers and their sub-functions,
  distinguishes request vs. response frames, and normalizes each frame to a TUP
  `DataPoint`.
- A `proptest` fuzz test asserting `UdsParser::parse` never panics on arbitrary
  input.

## Architecture

```
UDS bytes on the wire
        │  UdsParser::parse        (FrameParser)
        ▼
UdsFrame { sid, sub_function, data, is_request }
        │  UdsValidator::validate  (StateValidator, over UdsState)
        │  UdsNormalizer::normalize(DataNormalizer)
        ▼
DataPoint (SourceProtocol::Uds)
```

Three public pieces:

- **`UdsParser`** (`FrameParser<Frame = UdsFrame>`) — validates non-empty input
  and decodes the SID, optional sub-function, and payload.
- **`UdsValidator`** (`StateValidator<State = UdsState>`) — checks connection
  health and session validity.
- **`UdsNormalizer`** (`DataNormalizer<Frame = UdsFrame>`) — maps an
  `UdsFrame` to a TUP `DataPoint` with a SID-based `point_id`.

### Service IDs (`UdsSid`)

| SID (hex) | Variant | Name |
| --------- | ------- | ---- |
| `0x10` | request | `DiagnosticSessionControl` |
| `0x11` | request | `EcuReset` |
| `0x14` | request | `ClearDiagnosticInformation` |
| `0x22` | request | `ReadDataByIdentifier` |
| `0x23` | request | `ReadMemoryByAddress` |
| `0x27` | request | `SecurityAccess` |
| `0x28` | request | `CommunicationControl` |
| `0x2E` | request | `WriteDataByIdentifier` |
| `0x3E` | request | `TestPresenter` (TesterPresent) |
| `0x40 | sid` | `Response(u8)` | positive response to SID `sid` (e.g. `0x51` → `Response(0x11)`) |
| — | `Unknown(u8)` | any unrecognised SID byte |

Response frames are detected by the high bit: bytes in `0x40..=0x7E` decode to
`UdsSid::Response(sid & 0x3F)`. `UdsSid` exposes `raw_value()` (the on-wire
byte) and `is_response()`, and implements `Display`.

### Sub-functions (`SubFunction`)

Sub-functions are parsed for the SIDs that carry one: `0x10`, `0x11`, `0x14`,
`0x27`, `0x28`, `0x3E` and their responses (`0x50`, `0x51`, `0x54`, `0x67`,
`0x68`, `0x7E`). The sub-function is the second byte; its high bit (`0x80`)
marks a response sub-function, and the low 7 bits are the parameter:

- `SubFunction::None` — no sub-function (e.g. `ReadDataByIdentifier`)
- `SubFunction::Request(u8)` — `param & 0x7F`
- `SubFunction::Response(u8)` — `(param & 0x7F)` on a response frame

### Addressing / frame shape

UDS is a single-CAN-frame (or single-DoIP-DO) payload with no addressing
information of its own; this crate models the *service layer only*. A frame is
simply `sid (1 byte) [sub_function (1 byte)] [data …]`. The parser:

- rejects empty input (`Error::Parse`),
- treats any single byte as a valid `sid` (unknown SIDs become `Unknown(..)`),
- copies the remaining bytes (after the optional sub-function) verbatim into
  `data`.

### Validator (`UdsState`)

`UdsState` tracks `connected`, `session_type`, `security_level`,
`last_ok_timestamp`, and `error_count`. `UdsValidator::validate` rejects when:

- `!connected` → `"UDS device not connected"`, or
- `session_type > 3` → `"UDS invalid session type"`, or
- `error_count > 10` → `"UDS device has N consecutive errors"`.

This is the same `connected`/`error_count > 10` threshold contract that
[`tpt-protocol-telos`](../tpt-protocol-telos) proves for the Modbus validator.

### Normalizer

`UdsNormalizer::normalize` produces a `DataPoint` with `device_id = "uds:ecu"`,
`point_id = format!("sid:{}", frame.sid)`, `timestamp = 0`, `quality = Good`,
and a `value` chosen by data length:

- empty → `TupValue::Int(sid.raw_value() as i64)`
- `1..=4` bytes → big-endian `u32` as `TupValue::Int`
- `> 4` bytes → `TupValue::Bytes(data.clone())`

## Usage

```rust
use tpt_protocol_core::{FrameParser, DataNormalizer};
use tpt_protocol_tup::{DataPoint, SourceProtocol, TupValue};
use tpt_protocol_uds::{UdsParser, UdsNormalizer, UdsSid, SubFunction};

fn main() {
    // ReadDataByIdentifier (0x22) for DID 0xF190, no sub-function.
    let raw: [u8; 3] = [0x22, 0xF1, 0x90];

    let parser = UdsParser::new();
    let frame = parser.parse(&raw).unwrap();
    assert_eq!(frame.sid, UdsSid::ReadDataByIdentifier);
    assert_eq!(frame.sub_function, SubFunction::None);
    assert!(frame.is_request);

    let normalizer = UdsNormalizer::new();
    let dp: DataPoint = normalizer.normalize(&frame).unwrap();
    assert_eq!(dp.source_protocol, SourceProtocol::Uds);
    assert_eq!(dp.point_id, "sid:ReadDataByIdentifier");
    assert_eq!(dp.value, TupValue::Int(0xF190));
}
```

A response frame (`0x51` = `0x11 | 0x40`) decodes to `UdsSid::Response(0x11)`
with `is_request == false`; a sub-function response (`0x51 0x01`) yields
`SubFunction::Response(1)`.

## `no_std`

This crate is `#![no_std]` and uses `alloc` for the `data` payload
(`Vec<u8>`) and the `format!`-built `point_id`. It compiles for bare-metal
targets that provide an allocator.

## Crate layout

```
tpt-protocol-uds/
├── Cargo.toml
├── README.md
└── src/
    └── lib.rs   # UdsSid, SubFunction, UdsFrame, UdsParser, UdsState,
                 # UdsValidator, UdsNormalizer + unit & proptest fuzz tests
```

## Status / Conformance

- Implements `FrameParser`, `StateValidator`, `DataNormalizer` from
  `tpt-protocol-core`.
- Covers the 8 request SIDs above plus the `Response`/`Unknown` envelopes and
  sub-function parsing for the 6 sub-function-bearing SIDs. Other SIDs (e.g.
  `RoutineControl`, `IOControlByIdentifier`) arrive as `UdsSid::Unknown` and are
  still parseable/normalizable.
- Unit tests cover session-control, response, `ReadDataByIdentifier`, empty and
  garbage input, normalizer round-trip, and validator states.
- `proptest` fuzz test `parse_never_panics` confirms `UdsParser::parse` never
  panics on arbitrary `0..=1024` byte inputs.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the trait definitions.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the `DataPoint` envelope produced.
- [`tpt-protocol-doip-someip`](../tpt-protocol-doip-someip) — the transport UDS
  commonly rides over (DoIP/SOME-IP) at the layers below this service parser.
- [`tpt-protocol-telos`](../tpt-protocol-telos) — formal verification of the
  `connected` / `error_count > 10` validator threshold.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or
[Apache-2.0](../../LICENSE-APACHE) at your option.
