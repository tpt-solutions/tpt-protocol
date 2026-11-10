#!/usr/bin/env python3
"""Endpoint test: the daemon's J1939 and CANopen decoding over SocketCAN, with frames
encoded by cantools from DBC definitions.

cantools, the reference Python CAN library, encodes the signals of an EEC1, ET1, CCVS and
fuel-economy message (J1939) and of a CANopen TPDO1, using 29-bit and 11-bit identifiers.
The frames are sent on a SocketCAN interface, and the daemon reads them with `--can` and
records a point per signal. The test checks each recorded value against the signal value
that was encoded.

Linux only: SocketCAN is Linux. Create a virtual interface first, as root:

    ip link add dev vcan0 type vcan && ip link set up vcan0

Then, with the daemon built with `--features socketcan`:

    pip install cantools==45.1.2
    python scripts/endpoints/can_cantools.py /path/to/tpt-protocol-daemon vcan0
"""

import os
import socket
import struct
import sys
import tempfile

import cantools

from common import Daemon, report

CAN_EFF_FLAG = 0x80000000  # extended (29-bit) identifier
PORT = 0  # unused: the transport is SocketCAN
WS_PORT = 19087

# DBC identifiers: extended identifiers carry bit 31 in DBC files.
DBC_EXTENDED = 0x80000000


def dbc_text():
    def bo(identifier, extended, name, signal):
        value = identifier | (DBC_EXTENDED if extended else 0)
        return f"BO_ {value} {name}: 8 ECU\n {signal}\n"

    return "\n".join(
        [
            'VERSION ""',
            "",
            "NS_ :",
            "",
            "BS_:",
            "",
            "BU_: ECU",
            "",
            bo(0x0CF00400, True, "EEC1",
               'SG_ engine_speed : 24|16@1+ (0.125,0) [0|8031.875] "rpm" Vector__XXX'),
            bo(0x18FEEE00, True, "ET1",
               'SG_ coolant_temp : 0|8@1+ (1,-40) [-40|210] "Cel" Vector__XXX'),
            bo(0x18FEF100, True, "CCVS",
               'SG_ wheel_speed : 8|16@1+ (0.00390625,0) [0|255.99609375] "km/h" Vector__XXX'),
            bo(0x18FEE900, True, "FuelEconomy",
               'SG_ fuel_rate : 0|16@1+ (0.05,0) [0|3212.75] "L/h" Vector__XXX'),
            bo(0x0CF00430, True, "EEC1_ISOBUS",
               'SG_ engine_speed : 24|16@1+ (0.125,0) [0|8031.875] "rpm" Vector__XXX'),
            bo(0x190, False, "TPDO1",
               'SG_ value : 0|16@1+ (1,0) [0|65535] "" Vector__XXX'),
        ]
    )


def send(sock, can_id, data, extended):
    frame_id = can_id | (CAN_EFF_FLAG if extended else 0)
    sock.send(struct.pack("=IB3x8s", frame_id, len(data), bytes(data).ljust(8, b"\x00")))


def main(binary, channel):
    database = cantools.database.load_string(dbc_text(), database_format="dbc")
    by_name = {m.name: m for m in database.messages}

    # The source address of the J1939 messages is 0x00, so the device is j1939:0x00.
    expected = [
        ("j1939:0x00", "pgn:EEC1.engine_speed", 400.0, "rpm"),
        ("j1939:0x00", "pgn:EngineTemp1.coolant_temp", 70.0, "Cel"),
        ("j1939:0x00", "pgn:CCVS.wheel_speed", 10.0, "km/h"),
        ("j1939:0x00", "pgn:FuelEconomy.fuel_rate", 5.0, "L/h"),
    ]
    j1939_frames = [
        # (message, signals, J1939 identifier)
        ("EEC1", {"engine_speed": 400.0}, 0x0CF00400),
        ("ET1", {"coolant_temp": 70.0}, 0x18FEEE00),
        ("CCVS", {"wheel_speed": 10.0}, 0x18FEF100),
        ("FuelEconomy", {"fuel_rate": 5.0}, 0x18FEE900),
    ]

    # The daemon's SocketCAN transport takes the frames from the interface. The
    # identifiers in the DBC are the J1939 identifiers with the source address 0x00.
    def run(protocol, frames, expected_points):
        daemon = Daemon(
            binary,
            protocol,
            PORT,
            WS_PORT,
            transport=("--can", channel),
        )
        try:
            with socket.socket(socket.AF_CAN, socket.SOCK_RAW, socket.CAN_RAW) as sock:
                sock.bind((channel,))
                for message_id, data, extended in frames:
                    send(sock, message_id, data, extended)
            recorded = daemon.points()
        finally:
            daemon.stop()
        return report(f"{protocol} over SocketCAN ({channel}, cantools 45.1.2)", expected_points, recorded)

    j1939_encoded = []
    for name, signals, identifier in j1939_frames:
        message = by_name[name]
        data = message.encode(signals)
        j1939_encoded.append((identifier, data, True))
    status = run("j1939", j1939_encoded, expected)

    # The same EEC1 layout on an ISOBUS source address (0x30).
    isobus_data = by_name["EEC1_ISOBUS"].encode({"engine_speed": 400.0})
    isobus_expected = [("isobus:0x30", "pgn:0x0F004.engine_speed", 400.0, "rpm")]
    status |= run("isobus", [(0x0CF00430, isobus_data, True)], isobus_expected)

    canopen_message = by_name["TPDO1"]
    canopen_data = canopen_message.encode({"value": 513})
    canopen_expected = [("canopen:16", "fc:PDO1-TX", 513, None)]
    status |= run("canopen", [(0x190, canopen_data, False)], canopen_expected)
    return status


if __name__ == "__main__":
    if not sys.platform.startswith("linux"):
        print("skipped: SocketCAN needs Linux")
        sys.exit(0)
    sys.exit(main(os.path.abspath(sys.argv[1]), sys.argv[2]))
