# tpt-protocol-opcua

OPC UA Industry 4.0 protocol parser for the TPT telemetry pipeline.

## Implementation

- **Header**: msg_type (3 ASCII bytes: HEL/ACK/ERR/MSG/OPN/CLO) + chunk_type + message_size + channel_id + token_id + sequence_num + request_id
- **MSG payload**: service_id (4 bytes) + node_id (4 bytes) + value
- **Parser**: Validates minimum 8 bytes; decodes all header fields; extracts service/node IDs for MSG frames
- **Validator**: Checks connection, non-zero channel_id and token_id, error thresholds
- **Normalizer**: Maps to DataPoints keyed by channel_id and service/node ID
