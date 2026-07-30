# tpt-protocol-canopen

CANopen automation protocol parser for the TPT telemetry pipeline.

## Implementation

- **Frame**: 11-bit COB-ID split into function code (4 bits) and node ID (7 bits)
- **Function Codes**: NMT (0), SYNC (1), PDO1-TX (3), PDO1-RX (4), PDO2-TX (5),
  PDO2-RX (6), SDO-TX (11), SDO-RX (12), Heartbeat (14), Emergency, Timestamp
- **Parser**: Validates minimum 2 bytes, max 8 bytes payload
- **Validator**: Checks connection, node ID range (1-127), error thresholds
- **Normalizer**: Maps to DataPoints keyed by node ID and function code
