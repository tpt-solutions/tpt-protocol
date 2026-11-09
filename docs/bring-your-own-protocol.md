# Bring your own protocol

TPT Protocol turns any byte-level industrial protocol into the same TUP data
point that every other protocol produces. To add one, you implement three
traits and wire the crate into the workspace. This guide shows the smallest
complete version, then the scaffolder that does the wiring for you.

## The three traits

A protocol is a parser, an optional validator, and a normalizer. They live in
`tpt-protocol-core`:

```rust
use tpt_protocol_core::{DataNormalizer, Error, FrameParser};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

/// The sensor frame: `0x5A`, a sensor id, then a little-endian f32 reading.
struct Frame { sensor_id: u8, reading: f32 }

struct SensorParser;
impl FrameParser for SensorParser {
    type Frame = Frame;
    fn parse(&self, bytes: &[u8]) -> Result<Frame, Error> {
        match bytes {
            [0x5A, id, a, b, c, d, ..] => Ok(Frame {
                sensor_id: *id,
                reading: f32::from_le_bytes([*a, *b, *c, *d]),
            }),
            _ => Err(Error::Parse("not a sensor frame".into())),
        }
    }
}

struct SensorNormalizer;
impl DataNormalizer for SensorNormalizer {
    type Frame = Frame;
    fn normalize(&self, f: &Frame) -> Result<DataPoint, Error> {
        Ok(DataPoint::new(
            SourceProtocol::Modbus, // until you add a variant; see below
            format!("sensor/{}", f.sensor_id),
            "temperature",
            0,
            TupValue::Float(f.reading as f64),
            Quality::Good,
        )
        .with_unit("degC"))
    }
}
```

The full runnable version, with a validator and a `main`, is
[`crates/tpt-protocol-core/examples/custom_protocol.rs`](../crates/tpt-protocol-core/examples/custom_protocol.rs).
Run it with `cargo run -p tpt-protocol-core --example custom_protocol`.

Two rules follow from the design:

- **A parser must never panic.** Every length check returns `Err`. The
  protocol crates are fuzzed and property-tested against random input.
- **No value means no point.** A frame that carries no measurement (a read
  request, an empty payload) should normalize to an `Err`, not to a zero. A zero
  reads as a measurement.

## Let the scaffolder wire it

Writing the three traits is the part only you can do. The rest is bookkeeping:
a crate, a workspace entry, a `SourceProtocol` variant, the wire schema, the
daemon plugin, a fuzz target and a test. `cargo xtask` does that bookkeeping:

```bash
cargo xtask --dry-run new-protocol sensorx   # show every change, write nothing
cargo xtask new-protocol sensorx
cargo test -p tpt-protocol-sensorx
cargo run -p tpt-protocol-sensorx --example parse_sensorx
```

The scaffold creates `crates/tpt-protocol-sensorx/` with:

| File | What it is |
|---|---|
| `Cargo.toml` | `#![no_std]` crate; `publish = false` until you decide otherwise |
| `src/lib.rs` | placeholder parser, validator and normalizer, plus unit and property tests |
| `examples/parse_sensorx.rs` | parses and normalizes a sample frame |
| `fuzz/fuzz_targets/sensorx.rs` | libFuzzer target (Linux) |
| `README.md` | status and how to run it |

And it edits, in one step:

- the root `Cargo.toml` (workspace member),
- `tpt-protocol-tup/src/source_protocol.rs` (a `SourceProtocol::Sensorx` variant),
- `schema/tup-v1.cddl` and `schema/tup-v1.schema.json` (the wire names),
- `tpt-protocol-tup/src/cddl_conformance.rs` (the protocol list the test checks),
- `fuzz/Cargo.toml`,
- the daemon's `Cargo.toml` (a dependency and a default-on feature) and
  `src/plugins/mod.rs` (the plugin table).

Every edit is computed in memory first. If an anchor in an existing file is
missing, nothing is written. The scaffold does not touch `README.md`,
`CHANGELOG.md` or `TODO.md`; add those entries yourself.

## After scaffolding

1. Replace the placeholder framing in `src/lib.rs` with the protocol's real
   layout: its length fields, checksums and state. Keep the "never panic" rule.
2. Give `SensorxNormalizer` real fields. Use the protocol's own names for
   `device_id` and `point_id`, real units, and a `Quality` that reflects the
   protocol's status bits.
3. Replace the sample bytes in the example with a real capture.
4. Add the sample to `crates/tpt-protocol-daemon/samples/` and to the `SAMPLES`
   table in the daemon's tests, so `cargo test` replays it.
5. Run `cargo test --workspace`. The conformance test fails if the wire schema
   and the Rust enum disagree, which catches a mistake in the scaffold or in your
   edits.

## Replaying a capture

Once the crate is wired into the daemon, a capture replays through it:

```bash
# one frame, hex text
tpt-protocol-daemon --simulate sensorx --replay frame.hex

# a stream of frames from a file, or piped in
tpt-protocol-daemon --simulate sensorx --stdin --record out.ndjson < capture.raw

# which protocol is this capture? rank them all
tpt-protocol-daemon --detect capture.raw --replay-format raw
```

## Limits to know about

- **`SourceProtocol` is part of the wire format.** A new protocol adds a name
  to the schema. Once data has been recorded with a name, renaming it breaks old
  recordings. Choose the name before the first release.
- **Detection is a heuristic.** `--detect` ranks protocols by how much of a
  capture they decode. A framer without a length field can accept almost any
  byte string, so read the frame and point counts, not just the order.
- **A scaffold is not a decoder.** The placeholder accepts any non-empty frame.
  Until you write the framing, the protocol decodes nothing useful.
