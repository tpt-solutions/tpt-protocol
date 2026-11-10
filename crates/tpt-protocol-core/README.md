# tpt-protocol-core

**Core traits for the [TPT Protocol](https://github.com/tpt-solutions/tpt-protocol) — `FrameParser`, `StateValidator`, `DataNormalizer`, and the shared `Error` type.**

This crate defines the universal traits that every protocol crate in the
suite implements. They provide a single, consistent interface for frame
parsing, connection-state validation, and normalization into the TUP
envelope (`tpt-protocol-tup`), so that any protocol can be driven by the
daemon, the CLI, and the UI tools through one code path.

## Status / Conformance

- **Version:** `0.1.0` (initial release).
- **Memory safety:** `#![no_std]`. The only heap allocations are the `Vec`s returned by
  `DataNormalizer::normalize_all`, `normalize_batch` and `FrameEncoder::encode`, which use
  `alloc`.
- **Conformance:** implements the three core traits specified in
  **spec §7** of the project specification. The `ProtocolPlugin` trait (used
  by the daemon's plugin architecture) is provided behind the `std` feature.
- The crate ships with unit tests covering each trait and is part of the
  workspace-wide `proptest` fuzz harness that asserts `FrameParser::parse`
  never panics on arbitrary input.

## Traits

All three parser/normalizer traits carry an associated `Frame` type that
must be `Debug + Clone + PartialEq`, so parsed frames can be inspected,
compared, and batched.

### `FrameParser`

Parses raw bytes into a protocol-specific frame.

```rust
pub trait FrameParser {
    type Frame: core::fmt::Debug + Clone + PartialEq;

    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error>;

    fn try_parse(&self, bytes: &[u8]) -> Result<Option<(Self::Frame, usize)>, Error> {
        self.parse(bytes).map(|f| Some((f, bytes.len())))
    }
}
```

- `parse` is the only required method — it converts a byte slice into `Frame`
  or returns an `Error`.
- `try_parse` has a default implementation that returns the whole frame and
  the number of bytes consumed (`bytes.len()`). Protocol crates may override
  it to report how many bytes of a larger buffer were actually consumed,
  enabling incremental/streaming decoding.

### `StateValidator`

Validates the health/state of a protocol connection.

```rust
pub trait StateValidator {
    type State: core::fmt::Debug + Clone + PartialEq;

    fn validate(&self, state: &Self::State) -> Result<(), Error>;
}
```

`validate` returns `Ok(())` when the connection is healthy and an `Error`
otherwise. `State` is protocol-defined (e.g. an online/offline flag or a
link-status word).

### `DataNormalizer`

Normalizes a parsed frame into a TUP `DataPoint`.

```rust
pub trait DataNormalizer {
    type Frame: core::fmt::Debug + Clone + PartialEq;

    fn normalize(&self, frame: &Self::Frame) -> Result<tpt_protocol_tup::DataPoint, Error>;

    fn normalize_batch(
        &self,
        frames: &[Self::Frame],
    ) -> Result<alloc::vec::Vec<tpt_protocol_tup::DataPoint>, Error> {
        frames.iter().map(|f| self.normalize(f)).collect()
    }
}
```

- `normalize` is required — it maps a `Frame` into a TUP `DataPoint`
  (`tpt-protocol-tup`).
- `normalize_all` returns every point a frame carries. Its default implementation
  returns the single point from `normalize`. Protocols whose frames carry several
  values override it: Sparkplug B (one per metric), Matter (one per attribute), NMEA
  (one per decoded field) and IEEE 11073 SDC (one per numeric observation).
- `normalize_batch` concatenates `normalize_all` for each frame, so a batch fans out.
  A frame with several values contributes all of them.

### `FrameEncoder`

The inverse of `FrameParser`: it writes a frame as wire bytes.

```rust
pub trait FrameEncoder {
    type Frame;

    fn encode(&self, frame: &Self::Frame) -> Result<alloc::vec::Vec<u8>, Error>;
}
```

Most protocol crates implement it, and their tests check that `parse(encode(frame))`
gives the frame back. Not every crate does: BACnet, Matter and MAVLink do not yet.

### `ProtocolPlugin` (behind the `std` feature)

The plugin trait used by `tpt-protocol-daemon`. Implementations produce a
futures stream of TUP `DataPoint` values (from simulated or real hardware).

```rust
#[cfg(feature = "std")]
pub trait ProtocolPlugin: Send + 'static {
    fn simulate(
        self: alloc::boxed::Box<Self>,
    ) -> core::pin::Pin<
        alloc::boxed::Box<
            dyn futures_util::stream::Stream<Item = tpt_protocol_tup::DataPoint> + Send,
        >,
    >;
}
```

`ProtocolPlugin` is only available when the `std` feature is enabled, which
pulls in `futures-util`. The daemon wires one `ProtocolPlugin` per protocol
crate (see `tpt-protocol-daemon/src/plugins/mod.rs`).

## The `Error` type

Every trait method returns `Result<T, Error>`. The enum is `#[non_exhaustive]`
so new variants can be added in a semver-compatible way:

```rust
#[non_exhaustive]
pub enum Error {
    Parse(String),
    State(String),
    Normalize(String),
    Cbor(String),
    Transport(String),
    Config(String),
    Checksum { expected: u32, actual: u32, message: String },
    Overflow  { max: usize, actual: usize, message: String },
    InvalidFrame { reason: String, offset: usize },
}
```

Each string variant has a matching constructor for ergonomic construction
without importing the enum variant:

| Constructor | Produces |
| --- | --- |
| `Error::parse(msg)` | `Error::Parse` |
| `Error::state(msg)` | `Error::State` |
| `Error::normalize(msg)` | `Error::Normalize` |
| `Error::cbor(msg)` | `Error::Cbor` |
| `Error::transport(msg)` | `Error::Transport` |
| `Error::config(msg)` | `Error::Config` |
| `Error::checksum(expected, actual, msg)` | `Error::Checksum` |
| `Error::overflow(max, actual, msg)` | `Error::Overflow` |
| `Error::invalid_frame(reason, offset)` | `Error::InvalidFrame` |

`Error` implements `Display` (and `std::error::Error` under the `std`
feature), and converts from `core::str::Utf8Error`. The structured variants
(`Checksum`, `Overflow`, `InvalidFrame`) carry machine-readable detail that
protocol crates use to report exactly where and why a frame was rejected.

## `no_std` story

The crate is `#![no_std]`, and its default features are empty. It requires only `alloc`
(no full `std`). The parser, validator, normalizer and encoder traits and the `Error` type
are all available in a bare-metal/`no_std` context. The `std` feature (off by default)
additionally enables `tpt-protocol-tup/std` and `futures-util`,
which brings in the `ProtocolPlugin` trait for the daemon's async streaming
model. To use the crate without `std`:

```toml
tpt-protocol-core = { version = "0.1", default-features = false }
```

(Note: `tpt-protocol-tup`, which `DataNormalizer`/`ProtocolPlugin` depend
on for `DataPoint`, also has a `no_std` story of its own — see its README.)

## Writing a custom protocol

To add a new protocol to the ecosystem, implement the three core traits for
your wire format. A complete, runnable example lives at
`examples/custom_protocol.rs` (`cargo run -p tpt-protocol-core --example
custom_protocol`) — it defines a hypothetical "SensorX" protocol:

```rust
use tpt_protocol_core::{DataNormalizer, Error, FrameParser, StateValidator};
use tpt_protocol_tup::{DataPoint, Quality, SourceProtocol, TupValue};

#[derive(Debug, Clone, PartialEq)]
struct SensorXFrame { sensor_id: u8, reading: f32 }

struct SensorXParser;
impl FrameParser for SensorXParser {
    type Frame = SensorXFrame;
    fn parse(&self, bytes: &[u8]) -> Result<Self::Frame, Error> {
        if bytes.len() < 6 {
            return Err(Error::parse("sensorx frame too short"));
        }
        if bytes[0] != 0x5A {
            return Err(Error::parse("sensorx bad magic byte"));
        }
        let sensor_id = bytes[1];
        let reading = f32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]);
        Ok(SensorXFrame { sensor_id, reading })
    }
}

struct SensorXValidator;
impl StateValidator for SensorXValidator {
    type State = u8;
    fn validate(&self, state: &Self::State) -> Result<(), Error> {
        if *state == 0 {
            return Err(Error::state("sensorx sensor offline"));
        }
        Ok(())
    }
}

struct SensorXNormalizer;
impl DataNormalizer for SensorXNormalizer {
    type Frame = SensorXFrame;
    fn normalize(&self, frame: &Self::Frame) -> Result<DataPoint, Error> {
        Ok(DataPoint::new(
            SourceProtocol::Modbus,
            format!("sensorx/{}", frame.sensor_id),
            "temperature".to_string(),
            0,
            TupValue::Float(frame.reading as f64),
            Quality::Good,
        ).with_unit("degC"))
    }
}
```

Once a protocol implements these traits, the daemon can drive it by adding a
`ProtocolPlugin` entry (behind the `std` feature) — see
`tpt-protocol-daemon/src/plugins/mod.rs`. `cargo xtask new-protocol <name>`
scaffolds the crate and does that wiring; see
[`docs/bring-your-own-protocol.md`](../../docs/bring-your-own-protocol.md).

## Crate layout

```
crates/tpt-protocol-core/
├── Cargo.toml
├── README.md
├── CHANGELOG.md
├── src/
│   ├── lib.rs        # FrameParser, StateValidator, DataNormalizer, ProtocolPlugin
│   └── error.rs      # Error enum + constructors + Display/From impls
└── examples/
    └── custom_protocol.rs  # Run a full custom-protocol example
```

## Related crates

- [`tpt-protocol-tup`](https://github.com/tpt-solutions/tpt-protocol) — the
  TPT Unified Protocol envelope (`DataPoint`, `TupValue`, `Quality`,
  `SourceProtocol`) that `DataNormalizer` produces.
- `tpt-protocol-daemon` — background service that drives every protocol
  crate through the `ProtocolPlugin` trait.
- The 19 protocol crates (`tpt-protocol-modbus`, `tpt-protocol-j1939`, …)
  each implement the three core traits and register a `ProtocolPlugin`.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
