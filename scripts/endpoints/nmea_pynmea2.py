#!/usr/bin/env python3
"""Endpoint test: the daemon's NMEA 0183 decoding against sentences made by pynmea2.

pynmea2, the reference Python NMEA library, builds GGA and RMC sentences from their
fields and writes them with a checksum. They are sent as UDP datagrams (CRLF ended, as
a GPS receiver sends them) to the daemon (`--simulate nmea --udp-bind`). The test
checks each recorded field against the field pynmea2 was given.

Usage (from the repository root):

    cargo build -p tpt-protocol-daemon
    pip install pynmea2==1.19.0
    python scripts/endpoints/nmea_pynmea2.py target/debug/tpt-protocol-daemon.exe
"""

import sys

from common import Daemon, report, send_datagram

PORT = 14562
WS_PORT = 19082
DEVICE = "nmea:GP"


def main(binary):
    import pynmea2

    gga = pynmea2.GGA(
        "GP", "GGA",
        ("123519", "4807.038", "N", "01131.000", "E", "1", "08", "0.9", "545.4", "M", "46.9", "M", "", ""),
    )
    rmc = pynmea2.RMC(
        "GP", "RMC",
        ("123519", "A", "4807.038", "N", "01131.000", "E", "022.4", "084.4", "230394", "003.1", "W"),
    )
    expected = [
        (DEVICE, "GGA.latitude", 48.117300, None),
        (DEVICE, "GGA.longitude", 11.516667, None),
        (DEVICE, "GGA.fix_quality", 1, None),
        (DEVICE, "GGA.satellites", 8, None),
        (DEVICE, "GGA.altitude_m", 545.4, None),
        (DEVICE, "RMC.latitude", 48.117300, None),
        (DEVICE, "RMC.longitude", 11.516667, None),
        (DEVICE, "RMC.speed_knots", 22.4, None),
        (DEVICE, "RMC.course_deg", 84.4, None),
    ]

    daemon = Daemon(binary, "nmea", PORT, WS_PORT)
    try:
        for sentence in (gga, rmc):
            send_datagram(PORT, (str(sentence) + "\r\n").encode("ascii"))
        recorded = daemon.points()
    finally:
        daemon.stop()
    return report("nmea (pynmea2 1.19.0)", expected, recorded)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
