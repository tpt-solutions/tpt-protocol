# tpt-protocol-j1939

SAE J1939 heavy-duty vehicle protocol parser for the TPT telemetry pipeline.

## Implementation

- **Frame**: 29-bit CAN ID with priority/EDP/DP/PF/PS/SA fields, PGN decoding, up to 8 bytes payload
- **PGNs**: Recognises TC1 (65281), EEC1 (61444), EngineTemp1, FuelEconomy, CabTemp
- **Parser**: Validates minimum length (4 bytes for CAN ID), maximum payload (8 bytes)
- **Validator**: Checks connection state, source address validity, error thresholds
- **Normalizer**: Produces DataPoints keyed by source address and PGN
