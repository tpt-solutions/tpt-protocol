# tpt-protocol-coap

CoAP (RFC 7252) constrained IoT protocol parser.

Implements `FrameParser`, `StateValidator`, and `DataNormalizer`.

## Frame Format

Binary header: version(2) + type(2) + token_len(4) + code(8) + message_id(16) + token + options + payload.

### Methods
GET(1), POST(2), PUT(3), DELETE(4)

## Tests

Run `cargo test -p tpt-protocol-coap`.
