# tpt-protocol-modbus

Modbus RTU and Modbus TCP frame parsing, validation, and TUP normalisation
for the [TPT Protocol](https://github.com/tpt-protocol/tpt-protocol) suite.

Supports the most common Modbus function codes:

| FC | Name                     |
|----|--------------------------|
| 01 | Read Coils               |
| 02 | Read Discrete Inputs     |
| 03 | Read Holding Registers   |
| 04 | Read Input Registers     |
| 05 | Write Single Coil        |
| 06 | Write Single Register    |
| 07 | Read Exception Status    |
| 08 | Diagnostics              |
| 16 | Write Multiple Registers |

Unrecognised function codes are captured as `ModbusFrame::Raw`.

## Usage

```rust
use tpt_protocol_core::{FrameParser, DataNormalizer};
use tpt_protocol_modbus::{ModbusParser, ModbusNormalizer};

fn main() {
    // RTU request: slave=1, FC=3, addr=0, qty=10
    let raw: [u8; 8] = [
        0x01, 0x03, 0x00, 0x00, 0x00, 0x0A,
        0xC5, 0xCD,  // CRC-16/Modbus
    ];

    let parser = ModbusParser::new();
    let frame = parser.parse(&raw).unwrap();
    assert_eq!(frame.slave_address, 0x01);

    let normalizer = ModbusNormalizer::new();
    let datapoint = normalizer.normalize(&frame).unwrap();

    assert_eq!(datapoint.source_protocol, tpt_protocol_tup::SourceProtocol::Modbus);
    assert_eq!(datapoint.device_id, "modbus:1");
    assert_eq!(datapoint.point_id, "holding_register:0");
}
```

## `no_std`

This crate is `#![no_std]` and uses `alloc` for heap-allocated data
(`Vec`, `String`, `format!`). It works in embedded environments that
provide an allocator.
