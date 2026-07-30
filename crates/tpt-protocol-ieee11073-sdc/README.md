# tpt-protocol-ieee11073-sdc

IEEE 11073 SDC bedside medical device protocol parser.

**Pattern A** — Implements `FrameParser`, `StateValidator`, and `DataNormalizer`.

## Frame Format

Binary: message type(1) + timestamp(8) + body. Supports numeric metrics, waveforms, alerts, and context.

### Message Types
MDIB (0x01), Waveform (0x02), Alert (0x03), Context (0x04)

## Tests

Run `cargo test -p tpt-protocol-ieee11073-sdc`.
