#!/usr/bin/env python3
"""Runs every endpoint test against one daemon binary and exits non-zero on any failure.

Usage (from the repository root):

    python scripts/endpoints/run_all.py target/debug/tpt-protocol-daemon
"""

import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
TESTS = [
    "mavlink_pymavlink.py",
    "nmea_pynmea2.py",
    "coap_aiocoap.py",
    "opcua_asyncua.py",
    "bacnet_bacpypes3.py",
]


def main(binary):
    binary = os.path.abspath(binary)
    failed = []
    for test in TESTS:
        result = subprocess.run([sys.executable, os.path.join(HERE, test), binary])
        if result.returncode != 0:
            failed.append(test)
    if failed:
        print("failed: " + ", ".join(failed))
        return 1
    print("all endpoint tests passed")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
