# tpt-protocol-sparkplugb

Sparkplug B MQTT-based IIoT protocol parser.

**Pattern A** — Implements `FrameParser`, `StateValidator`, and `DataNormalizer`.

## Frame Format

Binary protobuf-ish: 14-byte header + metric TLV (Int32/Int64/Float/Double/Bool/String).

### Message Types
NBIRTH, DBIRTH, NCMD, DCMD, NDATA, DDATA, NDEATH, DDEATH

## Tests

Run `cargo test -p tpt-protocol-sparkplugb`.
