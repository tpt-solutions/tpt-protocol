# tpt-protocol-doip-someip

DoIP (ISO 13400) and SOME/IP automotive Ethernet protocol parser for the TPT telemetry pipeline.

## Implementation

- **DoIP**: protocol_version + inverse_version + payload_type + payload_length + payload.
  Inverse version must be !protocol_version.
- **SOME/IP**: service_id + method_id + length + client_id + session_id + protocol_version +
  interface_version + message_type + return_code + payload.
- **Parser**: Auto-detects DoIP (first byte = 0x02) vs SOME/IP (minimum 16 bytes)
- **Payload Types**: VehicleIdentification, RoutingActivation, AliveCheck, DiagnosticMessage, etc.
- **Message Types**: Request, RequestNoReturn, Notification, Response, Error
- **Validator**: Checks TCP connection, routing activation, error thresholds
- **Normalizer**: Maps to DataPoints keyed by payload type or service/method ID
