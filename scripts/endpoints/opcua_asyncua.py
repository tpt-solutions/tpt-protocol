#!/usr/bin/env python3
"""Endpoint test: the daemon's OPC UA server, read by the asyncua client.

asyncua, the reference Python OPC UA library, is an independent client. The test:
1. starts the daemon with an NMEA UDP listener and the OPC UA server on loopback,
2. sends a GGA sentence made by pynmea2 over UDP,
3. connects with asyncua (anonymous, no security), browses the TPT folder, and reads
   every variable, checking the names, the types, the values, and the status.

Usage (from the repository root):

    cargo build -p tpt-protocol-daemon
    pip install asyncua==2.1.0 pynmea2==1.19.0
    python scripts/endpoints/opcua_asyncua.py target/debug/tpt-protocol-daemon.exe
"""

import asyncio
import sys

from common import Daemon, send_datagram

PORT = 14564
WS_PORT = 19084
OPCUA_PORT = 14840
ENDPOINT = f"opc.tcp://127.0.0.1:{OPCUA_PORT}/"

EXPECTED = {
    "nmea:GP/GGA.latitude": 48.117300,
    "nmea:GP/GGA.longitude": 11.516667,
    "nmea:GP/GGA.fix_quality": 1,
    "nmea:GP/GGA.satellites": 8,
    "nmea:GP/GGA.altitude_m": 545.4,
}


async def read_all():
    from asyncua import Client, ua

    problems = []
    async with Client(url=ENDPOINT) as client:
        namespace = await client.get_namespace_index("urn:tpt-protocol")
        if namespace != 1:
            problems.append(f"namespace index {namespace}, expected 1")

        tpt = client.get_node("ns=1;i=0")
        children = await tpt.get_children()
        names = {}
        for node in children:
            browse = await node.read_browse_name()
            names[browse.Name] = node
        missing = set(EXPECTED) - set(names)
        for name in sorted(missing):
            problems.append(f"not browsed: {name}")

        for name, expected in EXPECTED.items():
            if name not in names:
                continue
            node = names[name]
            data_value = await node.read_data_value()
            value = data_value.Value.Value
            status = data_value.StatusCode.value
            if status != 0:
                problems.append(f"{name}: status 0x{status:08X}, expected good")
            if isinstance(expected, float):
                if not isinstance(value, float) or abs(value - expected) > 1e-6:
                    problems.append(f"{name}: {value!r}, expected {expected!r}")
            elif value != expected or type(value) is not int:
                problems.append(f"{name}: {value!r}, expected {expected!r}")
    return problems, len(names)


def main(binary):
    daemon = Daemon(
        binary,
        "nmea",
        PORT,
        WS_PORT,
        extra=["--opcua-listen", f"127.0.0.1:{OPCUA_PORT}"],
    )
    try:
        import pynmea2

        gga = pynmea2.GGA(
            "GP", "GGA",
            ("123519", "4807.038", "N", "01131.000", "E", "1", "08", "0.9", "545.4", "M", "46.9", "M", "", ""),
        )
        send_datagram(PORT, (str(gga) + "\r\n").encode("ascii"))
        # The OPC UA server takes its values from the points, so let them arrive first.
        import time

        time.sleep(2.0)
        problems, browsed = asyncio.run(read_all())
    finally:
        daemon.stop()
    print(f"opcua (asyncua 2.1.0): {browsed} variables browsed, {len(problems)} problems")
    for problem in problems[:20]:
        print("  " + problem)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
