# tpt-protocol-knx

KNX smart building protocol parser.

**Pattern A** — Implements `FrameParser`, `StateValidator`, and `DataNormalizer`.

## Frame Format

cEMI-like binary: 7-byte header + DPT data. Group address: main(5) + middle(3) + sub(8).

### Service Types
GroupValueRead (0x00), GroupValueResponse (0x40), GroupValueWrite (0x80)

## Tests

Run `cargo test -p tpt-protocol-knx`.
