# tpt-protocol-bacnet

BACnet building automation protocol parser for the TPT telemetry pipeline.

## Implementation

- **BVLC**: type (0x81 for IP) + function + length header
- **APDU**: High nibble of first APDU byte identifies type:
  ConfirmedReq (0), UnconfirmedReq (1), SimpleAck (2), ComplexAck (3), Error (5)
- **Objects**: Object type (2 bytes) + instance (4 bytes) + optional property ID (2 bytes)
- **Parser**: Validates BVLC type (0x81), decodes BVLC function, APDU type, and object identifier
- **Validator**: Checks connection, device instance range (≤ 4,194,303), error thresholds
- **Normalizer**: Maps to DataPoints keyed by BVLC function or object type:instance
