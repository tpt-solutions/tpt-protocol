# tpt-protocol-hl7v2

HL7 v2.x hospital data messaging protocol parser.

**Pattern A** — Implements `FrameParser`, `StateValidator`, and `DataNormalizer`.

## Frame Format

Pipe-delimited text: segments separated by `\r`, fields by `|`, components by `^`.

### Message Types
ADT (admit/discharge/transfer), ORM (order), ORU (observation result), MDM (document)

## Tests

Run `cargo test -p tpt-protocol-hl7v2`.
