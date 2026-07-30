# tpt-protocol-matter-thread

Matter/Thread smart home mesh protocol parser.

**Pattern B** — Implements `FrameParser`, `StateValidator`, and `DataNormalizer`.

## Frame Format

Binary: protocol(1) + cluster_id(2) + command_id(2) + endpoint(2) + data_version(4) + interaction_type(1) + TLV data.

### Clusters
OnOff (0x0006), LevelControl (0x0008), TemperatureMeasurement (0x0402), OccupancySensing (0x0406)

## Tests

Run `cargo test -p tpt-protocol-matter-thread`.
