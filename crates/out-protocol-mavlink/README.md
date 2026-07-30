# out-protocol-mavlink

MAVLink drone telemetry protocol parser (v1 and v2) for the TPT telemetry pipeline.

## Implementation

- **v1**: magic=0xFE, length+seq+sys+comp+msg_id(1 byte)+payload+CRC-16-IBM
- **v2**: magic=0xFD, length+incompat+compat+seq+sys+comp+msg_id(3 bytes)+payload+CRC-16-IBM
- **Messages**: HEARTBEAT (0), SYS_STATUS (1), GPS_RAW_INT (24), ATTITUDE (30)
- **Parser**: Auto-detects v1/v2 by magic byte; validates CRC-16-IBM over header+payload
- **Validator**: Checks connection, heartbeat received flag, system ID range, error thresholds
- **Normalizer**: Maps to DataPoints keyed by system:component and message ID
