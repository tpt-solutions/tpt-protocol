# tpt-protocol-ocpp

OCPP 2.0.1 EV charging station protocol parser.

**Pattern B** — Implements `FrameParser`, `StateValidator`, and `DataNormalizer`.

## Frame Format

Binary: message_type(1) + unique_id + action + payload.

### Message Types
Call (2), CallResult (3), CallError (4)

### Actions
BootNotification, StatusNotification, MeterValues, StartTransaction, StopTransaction, Heartbeat, Authorize

## Tests

Run `cargo test -p tpt-protocol-ocpp`.
