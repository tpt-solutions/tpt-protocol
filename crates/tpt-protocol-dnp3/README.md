# tpt-protocol-dnp3

DNP3 electrical grid SCADA protocol parser for the TPT telemetry pipeline.

## Implementation

- **Data Link**: start=0x0564, length, control, dest (2 bytes LE), src (2 bytes LE), CRC-16/DNP over bytes 2-7
- **Transport**: final flag (bit 7) + sequence (bits 0-5)
- **Application**: function code (1=Read, 2=Write, 3=Select, 4=Operate, 5=DirectOperate, etc.) + payload
- **Parser**: Validates start bytes, CRC-16/DNP, minimum frame length
- **Validator**: Checks connection, source address validity, error thresholds
- **Normalizer**: Maps to DataPoints keyed by source address and function code
