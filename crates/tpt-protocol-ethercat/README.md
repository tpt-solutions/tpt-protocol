# tpt-protocol-ethercat

EtherCAT high-performance factory automation protocol parser for the TPT telemetry pipeline.

## Implementation

- **Ethernet**: EtherCAT frames use EtherType 0x88A4; header contains length + type
- **Datagram**: command (1 byte) + index + addressing (4 bytes) + length + reserved + data + working counter (WKC)
- **Commands**: APRD (1), APWR (5), FPRD (4), FPWR (8), BRD (7), BWR (11), LRD (12), LWR (13)
- **Parser**: Validates minimum 12 bytes for EtherCAT header + 10 bytes for datagram header; extracts data and WKC
- **Validator**: Checks connection, OPERATIONAL state, slave count, error thresholds
- **Normalizer**: Maps to DataPoints keyed by EtherCAT command
