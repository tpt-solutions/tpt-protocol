"""Shared helpers for the endpoint tests: they run the daemon with a UDP listener,
send datagrams made by a third-party library, and compare what the daemon records.

The daemon is started with `--record`, which writes one NDJSON line per data point.
Each endpoint test passes the daemon binary path on the command line.
"""

import json
import math
import os
import socket
import subprocess
import tempfile
import time

LOOPBACK = "127.0.0.1"


class Daemon:
    """A daemon process listening for UDP datagrams on `port` and recording to `record`."""

    def __init__(self, binary, protocol, port, ws_port, extra=(), transport=None):
        self.record = os.path.join(tempfile.mkdtemp(prefix="tpt-endpoint-"), "points.ndjson")
        self.process = subprocess.Popen(
            [
                binary,
                "--simulate", protocol,
                *(transport or ("--udp-bind", f"{LOOPBACK}:{port}")),
                "--port", str(ws_port),
                "--record", self.record,
                *extra,
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )
        time.sleep(2.0)  # let the listener bind
        if self.process.poll() is not None:
            raise RuntimeError(f"daemon exited at start: {self.process.stderr.read()}")

    def points(self, settle=1.5):
        """The recorded points, after giving the daemon time to write them."""
        time.sleep(settle)
        if not os.path.exists(self.record):
            return []
        with open(self.record, encoding="utf-8") as f:
            return [json.loads(line) for line in f if line.strip()]

    def stop(self):
        self.process.terminate()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()


def send_datagram(port, data):
    """Sends one UDP datagram to the daemon on the loopback interface."""
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.sendto(data, (LOOPBACK, port))


def compare(expected, recorded, tolerance=1e-6):
    """Checks that every expected (device, point, value, unit) appears in `recorded`.

    Returns a list of problems. Floats are compared within `tolerance`; integers and
    text exactly. A missing point is a problem; an extra point is not, because the
    daemon may decode more fields than a test checks.
    """
    problems = []
    index = {}
    for point in recorded:
        index.setdefault((point["device_id"], point["point_id"]), []).append(point)
    for device, point_id, value, unit in expected:
        candidates = index.get((device, point_id), [])
        if not candidates:
            problems.append(f"missing {device} {point_id}")
            continue
        got = candidates[-1]
        if unit is not None and got.get("unit") != unit:
            problems.append(f"{point_id}: unit {got.get('unit')!r}, expected {unit!r}")
        actual = got["value"]
        if isinstance(value, float):
            if not isinstance(actual, (int, float)) or not math.isclose(
                actual, value, rel_tol=0, abs_tol=tolerance
            ):
                problems.append(f"{point_id}: {actual!r}, expected {value!r}")
        elif actual != value:
            problems.append(f"{point_id}: {actual!r}, expected {value!r}")
    return problems


def report(name, expected, recorded):
    problems = compare(expected, recorded)
    print(f"{name}: {len(expected)} expected points, {len(recorded)} recorded, "
          f"{len(problems)} problems")
    for problem in problems[:20]:
        print("  " + problem)
    return 1 if problems else 0
