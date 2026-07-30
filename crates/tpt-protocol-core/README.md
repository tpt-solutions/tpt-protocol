# tpt-protocol-core

**Core traits for TPT Protocol — FrameParser, StateValidator, DataNormalizer.**

This crate defines the three universal traits that every protocol crate implements,
providing a consistent interface for frame parsing, state validation, and
normalization into the TUP envelope (`tpt-protocol-tup`).

## Traits

- **`FrameParser`**: Parse raw bytes into a protocol-specific frame.
- **`StateValidator`**: Validate the health/state of a protocol connection.
- **`DataNormalizer`**: Convert a parsed frame into a `tup::DataPoint`.

## Usage

```rust
use tpt_protocol_core::{FrameParser, StateValidator, DataNormalizer, Error};
use tpt_protocol_tup::{DataPoint, SourceProtocol, Quality, TupValue};

struct MyFrame { value: f64, point: &'static str };

struct MyParser;
impl FrameParser for MyParser {
    type Frame = MyFrame;
    fn parse(&self, bytes: &[u8]) -> Result<MyFrame, Error> {
        Ok(MyFrame { value: 42.0, point: "test" })
    }
}
```

## no_std

This crate is `#![no_std]` by default and only requires `alloc`.
