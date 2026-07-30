# tpt-protocol-isobus

ISO 11783 (ISOBUS) agricultural vehicle protocol parser for the TPT telemetry pipeline.

## Implementation

- **Frame**: 29-bit CAN ID (J1939-compatible) with PGN decoding, implement_id and device_class extraction
- **PGNs**: RequestForRep (60416), PTO, WorkingSetMaster, TaskController
- **Parser**: Validates minimum 4 bytes, max 8 bytes payload
- **Validator**: Checks connection, source address range, non-zero implement_id, error thresholds
- **Normalizer**: Maps to DataPoints keyed by source address and implement_id
