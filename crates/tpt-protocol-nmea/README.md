# tpt-protocol-nmea

NMEA 0183 marine navigation protocol parser for the
[TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

## Overview

NMEA 0183 is the ubiquitous ASCII sentence format used by GPS, depth sounders,
compasses, and wind instruments on boats and in survey equipment. TPT provides a
memory-safe `#![no_std]` + `alloc` parser that extracts talker/sentence identity,
fields, and the XOR checksum, validates connection health, and normalises
sentences into the shared TUP telemetry envelope.

## Features

- NMEA 0183 ASCII sentence parsing via `FrameParser`.
- Talker-id decoding: GP (GPS), SD (sounder/depth), GI, HE, IN, plus `Unknown`.
- Sentence-type decoding: GGA, RMC, DBT, VHW, MWV, plus `Unknown`.
- XOR checksum computation and validation (reported via `checksum_valid`, not rejected).
- Connection health validation via the `StateValidator` trait (connection,
  signal-quality range, error threshold).
- TUP normalisation via the `DataNormalizer` trait, attaching the raw sentence.
- `#![no_std]` + `alloc` support.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.

## Architecture

`NmeaParser` expects the canonical wire shape
`$<talker><sentence>,<fields>*<checksum_hex>\r\n`:

- Validates UTF-8 and the leading `$`, then locates the `*` checksum separator.
- Computes the NMEA XOR checksum over the body between `$` and `*` and compares
  it to the hex value after `*` (reported in `checksum_valid`). Note: a checksum
  mismatch yields `checksum_valid = false` but still produces a parsed frame
  rather than an error.
- Splits the body into a `Talker` (2 bytes), a `SentenceType` (3 bytes), and a
  `Vec<String>` of comma-separated fields.

`NmeaFrame` carries `talker`, `sentence`, `fields`, `checksum`,
`checksum_valid`, and the `raw` sentence string. `NmeaValidator` validates a
`NmeaState` (connected flag, `signal_quality` must be ≤ 5, `error_count` ≤ 20).
`NmeaNormalizer` emits a `DataPoint` with `device_id = "nmea:<talker>"`,
`point_id = "<sentence>"`, quality `Good`/`Bad` from `checksum_valid`, and the
first field parsed as `Float` if numeric else `Text`; the raw sentence is kept
in `DataPoint::raw`.

## Usage

```rust
use tpt_protocol_core::{FrameParser, DataNormalizer};
use tpt_protocol_nmea::{NmeaParser, NmeaNormalizer};

fn main() {
    // $GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,0000*<csum>\r\n
    let raw = b"$GPGGA,123519,4807.038,N,01131.000,E,1,08,0.9,545.4,M,46.9,M,,0000*6E\r\n";

    let parser = NmeaParser::new();
    let frame = parser.parse(raw).unwrap();
    assert_eq!(frame.talker, tpt_protocol_nmea::Talker::Gp);
    assert_eq!(frame.sentence, tpt_protocol_nmea::SentenceType::Gga);
    assert!(frame.checksum_valid);

    let normalizer = NmeaNormalizer::new();
    let dp = normalizer.normalize(&frame).unwrap();
    assert_eq!(dp.source_protocol, tpt_protocol_tup::SourceProtocol::Nmea);
    assert_eq!(dp.device_id, "nmea:GP");
    assert_eq!(dp.point_id, "GGA");
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for `String`/`Vec` (fields, raw
sentence, device-id formatting). It works on alloc-capable embedded targets.

## Crate layout

- `src/lib.rs` — `Talker`, `SentenceType`, `NmeaFrame`, `NmeaParser` (ASCII +
  XOR checksum), `NmeaValidator` / `NmeaState`, `NmeaNormalizer`,
  `compute_nmea_checksum`, unit/fuzz tests.

## Status / Conformance

- **Implemented:** parsing of the NMEA 0183 `$...*XX` envelope, the five
  recognised talkers and sentences (plus `Unknown` fallbacks), XOR checksum
  validation, connection/quality state validation, TUP normalisation with raw
  sentence preservation, and `proptest` fuzzing.
- **Known gaps / TODOs:**
  - The parser does not *semantically* interpret fields (e.g. it does not decode
    lat/long, fix quality, or depth units); only the first field is typed as
    `Float`/`Text`. Per-field semantic decoding is future work.
  - Checksum failures are reported (`checksum_valid = false`, quality `Bad`) but
    not rejected with an error.
  - Normalised `timestamp` is `0`; callers must supply a real timestamp.
  - NMEA 2000 (`nmea2000` module): the CAN frame parser, an encoder, and a
    `FastPacketReassembler` for fast-packet PGNs (126996, 129029, 129540). Decoded
    PGNs: 129025 (position, rapid), 127250 (heading), 130306 (wind), 128267 (depth),
    and 129029 (GNSS position, latitude, longitude and altitude; the date and time
    are not decoded). A fast-packet PGN passed as a single frame is refused, so a
    partial message is never normalized. Other PGNs are parsed but not decoded. The
    field layouts follow the published PGN definitions and have not been checked
    against a real bus capture.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE) at your option.
