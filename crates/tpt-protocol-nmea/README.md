# tpt-protocol-nmea

NMEA 0183 sentence and NMEA 2000 frame parser, validator, and TUP normalizer for the
[TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) suite.

Published to crates.io (`publish` unset).

## Overview

NMEA 0183 is the ASCII sentence format that GNSS receivers, depth sounders, wind
sensors and most marine instruments use. NMEA 2000 is the CAN-based successor that
carries the same kinds of data in binary PGNs. TPT implements a from-scratch,
`#![no_std]` + `alloc` parser for both, so captured sentences and CAN frames can be
decoded and normalized into the shared TUP envelope.

## Features

- NMEA 0183 `$<talker><sentence>,<fields>*<checksum>` parsing. The XOR checksum is
  checked, and a mismatch is a parse error. Talkers include GP, GN, GL, GA and BD.
- Decoded sentences, each with its own field mapping:
  - GGA: position, fix quality, satellite count and altitude.
  - RMC: position, speed and course. Bad quality without a valid fix.
  - GLL: position, with status `A` (Good) or `V` (Bad).
  - VTG: true course, and speed in knots and km/h.
  - GSA: fix type. PDOP, HDOP and VDOP are Bad without a fix.
  - GSV: satellites in view. Per-satellite groups are not decoded.
  - DBT: depth. VHW: heading and speed. MWV: wind angle and speed.
  - HDT: true heading. ZDA: UTC time and date.
- Coordinates are converted from `ddmm.mmmm` to signed degrees.
- Other sentences keep their raw fields.
- `NmeaEncoder`: wire bytes from a frame, with round-trip tests against the parser.
- NMEA 2000 (`nmea2000` module): a CAN frame parser and encoder, a fast-packet
  reassembler, and decoders for PGNs 129025, 127250, 130306, 128267 and 129029.
- Connection-state validation via `StateValidator` (`NmeaValidator`).
- TUP normalisation via `DataNormalizer` (`NmeaNormalizer`). `normalize_all` gives one
  point per decoded field.
- `#![no_std]` + `alloc` (heap collections only; no `std`). Builds for `thumbv6m-none-eabi`.
- `proptest` fuzz tests asserting `FrameParser::parse` never panics on arbitrary input.
  Structured generators build sentences whose checksums are right or off by one.

## Architecture

`NmeaParser` expects `$<talker><sentence>,<fields>*<checksum>\r\n`:

- It validates UTF-8 and the leading `$`, then locates the `*` separator.
- It computes the XOR checksum over the body between `$` and `*`, and compares it with the
  two hex digits after `*`. A mismatch is `Error::Checksum`.
- It splits the body into a `Talker`, a `SentenceType` and the comma-separated fields.

`NmeaFrame` holds `talker`, `sentence`, `fields`, `checksum` and the `raw` sentence.

`NmeaNormalizer` produces `device_id = "nmea:<talker>"` and `point_id = "<SENTENCE>.<field>"`
(for example `GGA.longitude`, `GGA.fix_quality`, `GGA.satellites`, `GGA.altitude_m`).
A sentence with no decodable field returns a normalize error.

## Usage

A complete, runnable example parses a GGA sentence and normalizes it:

```sh
cargo run -p tpt-protocol-nmea --example parse_nmea
```

```rust
use tpt_protocol_core::{DataNormalizer, FrameParser};
use tpt_protocol_nmea::{NmeaNormalizer, NmeaParser};

let frame = NmeaParser::new().parse(&wire)?;
let points = NmeaNormalizer::new().normalize_all(&frame)?;
// points include "GGA.longitude" as signed degrees
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data (`Vec`, `String`,
`format!`). It works in embedded environments that provide an allocator.

## Crate layout

- `src/lib.rs` — `Talker`, `SentenceType`, `NmeaFrame`, `NmeaParser` (`FrameParser`),
  `NmeaEncoder` (`FrameEncoder`), `NmeaState` + `NmeaValidator` (`StateValidator`),
  `NmeaNormalizer` (`DataNormalizer`), unit tests, and the `proptest` `mod fuzz`.
- `src/nmea2000.rs` — the NMEA 2000 CAN frame parser and encoder, the fast-packet
  reassembler, and the PGN decoders.
- `examples/parse_nmea.rs` — runnable parse-and-normalize example.
- `examples/diff_decode.rs` — used by the differential test against pynmea2 in CI.

## Status / Conformance

- **Differential test.** `scripts/differential/nmea_pynmea2.py` checks the decoded fields
  against pynmea2 in CI.
- **Field coverage is partial.** Only the sentences listed above are decoded. Other
  sentences keep their raw fields.
- **NMEA 2000 field layouts are not checked against a bus capture.** They follow the
  published definitions. A fast-packet PGN given as a single frame is refused, so a partial
  message is never normalized. The date and time in 129029 are not decoded.
- Normalised `timestamp` is `0`; callers must supply a real timestamp.

## Related crates

- [`tpt-protocol-core`](../tpt-protocol-core) — the `FrameParser` / `FrameEncoder` /
  `StateValidator` / `DataNormalizer` traits this crate implements.
- [`tpt-protocol-tup`](../tpt-protocol-tup) — the normalized envelope this crate emits.

## License

Licensed under either of [MIT](../../LICENSE-MIT) or [Apache-2.0](../../LICENSE-APACHE)
at your option.
