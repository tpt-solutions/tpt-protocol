#!/usr/bin/env python3
"""Endpoint test: the daemon's BACnet decoding against ReadProperty-ACK messages made by bacpypes3.

bacpypes3, the reference Python BACnet library, encodes the application body of two
ReadProperty-ACK messages (the present value of an analog input and an analog value).
The test adds the three-byte APDU header (type, invoke id, service choice), an NPDU with
no network layer, and the BVLC header (Original-Unicast-NPDU), then sends the frames as UDP
datagrams to the daemon (`--simulate bacnet --udp-bind`). The daemon's normalizer gives
one point per object and property, named `obj:<type>:<instance>`. The test checks the
values that were encoded.

Usage (from the repository root):

    cargo build -p tpt-protocol-daemon
    pip install bacpypes3==0.0.110
    python scripts/endpoints/bacnet_bacpypes3.py target/debug/tpt-protocol-daemon.exe
"""

import struct
import sys

from common import Daemon, report, send_datagram

PORT = 47809
WS_PORT = 19086
DEVICE = "bacnet:bvlc:UnicastNPDU"

# BACnet object type numbers (ASHRAE 135 Table 12-6).
ANALOG_INPUT = 0
ANALOG_VALUE = 2


def read_property_ack(object_type, instance, value, invoke_id):
    """A BACnet/IP frame carrying a ReadProperty-ACK for present-value."""
    from bacpypes3.apdu import ReadPropertyACK
    from bacpypes3.primitivedata import Real

    ack = ReadPropertyACK(
        objectIdentifier=(object_type, instance),
        propertyIdentifier="present-value",
        propertyValue=Real(value),
    )
    ack.apduInvokeID = invoke_id
    body = ack.encode().pduData  # object id, property id and value, without the APDU header
    apdu = bytes([0x30, invoke_id, 0x0C]) + bytes(body)  # ComplexACK, invoke id, ReadProperty
    npdu = bytes([0x01, 0x00])  # version 1, no network layer
    length = 4 + len(npdu) + len(apdu)
    bvlc = bytes([0x81, 0x0A]) + struct.pack(">H", length)  # Original-Unicast-NPDU
    return bvlc + npdu + apdu


def main(binary):
    messages = [
        read_property_ack(ANALOG_INPUT, 7, 33.25, 5),
        read_property_ack(ANALOG_VALUE, 3, -4.5, 6),
    ]
    expected = [
        (DEVICE, "obj:0:7", 33.25, None),
        (DEVICE, "obj:2:3", -4.5, None),
    ]

    daemon = Daemon(binary, "bacnet", PORT, WS_PORT)
    try:
        for message in messages:
            send_datagram(PORT, message)
        recorded = daemon.points()
    finally:
        daemon.stop()
    return report("bacnet (bacpypes3 0.0.110)", expected, recorded)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
