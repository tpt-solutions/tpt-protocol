#!/usr/bin/env python3
"""Endpoint test: the daemon's CoAP decoding against messages made by aiocoap.

aiocoap, the reference Python CoAP library, encodes a non-confirmable Content message
with a payload and an acknowledgement with a code and no payload. They are sent as UDP
datagrams to the daemon (`--simulate coap --udp-bind`). The normalizer gives one point
per message: the device is `coap/<type>`, the point is `mid:<message id>`, and the value
is the payload as bytes, or the code when there is no payload. The test checks both.

Usage (from the repository root):

    cargo build -p tpt-protocol-daemon
    pip install aiocoap==0.4.17
    python scripts/endpoints/coap_aiocoap.py target/debug/tpt-protocol-daemon.exe
"""

import sys

from common import Daemon, report, send_datagram

PORT = 14563
WS_PORT = 19083


def main(binary):
    import warnings

    import aiocoap

    # aiocoap 0.4 deprecates building messages with an mtype and mid; the test builds
    # them that way on purpose, because it needs exact message IDs on the wire.
    warnings.filterwarnings("ignore", category=DeprecationWarning, module="__main__")

    content = aiocoap.Message(
        mtype=aiocoap.NON,
        code=aiocoap.CONTENT,
        mid=0x2A,
        payload=b'{"t":21.5}',
    )
    acknowledgement = aiocoap.Message(
        mtype=aiocoap.ACK,
        code=aiocoap.CHANGED,
        mid=0x2B,
    )
    expected = [
        ("coap/NON", "mid:42", {"bytes": "0x" + b'{"t":21.5}'.hex()}, None),
        ("coap/ACK", "mid:43", 68, None),  # code 2.04 Changed = (2 << 5) | 4
    ]

    daemon = Daemon(binary, "coap", PORT, WS_PORT)
    try:
        for message in (content, acknowledgement):
            send_datagram(PORT, message.encode())
        recorded = daemon.points()
    finally:
        daemon.stop()
    return report("coap (aiocoap 0.4.17)", expected, recorded)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
