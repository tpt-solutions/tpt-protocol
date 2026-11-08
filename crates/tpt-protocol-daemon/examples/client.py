"""Minimal consumer for the TPT Protocol daemon's TUP WebSocket stream.

No Rust required. Install deps:  pip install websocket-client

The daemon streams JSON arrays of TUP DataPoints, e.g.
  [{"schema_version":1,"source_protocol":"modbus","device_id":"modbus-sim-01",
    "point_id":"modbus-point-1","timestamp":...,"value":68.5,"quality":"good"}]
"""

import json
import websocket  # pip install websocket-client


def on_message(ws, message):
    payload = json.loads(message)
    if isinstance(payload, dict):
        # {"type": "lagged", "dropped": n}: this client fell behind the stream.
        if payload.get("type") == "lagged":
            print(f"warning: daemon dropped {payload.get('dropped')} messages")
        return
    # Otherwise the daemon sends a JSON array of DataPoints per tick.
    for dp in payload:
        print(
            f"{dp['source_protocol']:>12} {dp['device_id']:<16} "
            f"{dp['point_id']:<16} {dp['value']!s:<10} {dp['quality']}"
        )


def on_error(ws, error):
    print("error:", error)


def on_close(ws, *args):
    print("connection closed")


def on_open(ws):
    print("connected to daemon")


if __name__ == "__main__":
    ws = websocket.WebSocketApp(
        "ws://127.0.0.1:9080",
        on_open=on_open,
        on_message=on_message,
        on_error=on_error,
        on_close=on_close,
    )
    ws.run_forever()
