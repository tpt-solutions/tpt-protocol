#!/usr/bin/env python3
"""Differential test: tpt-protocol-uds against udsoncan.

udsoncan, the reference Python UDS (ISO 14229) library, reads random request,
response and negative-response payloads. The Rust example `diff_decode` reads the same
bytes and prints a canonical line. This script compares the two: service ID,
sub-function (with the suppress bit removed), suppressPosRsp, data, and for negative
responses the requested SID and NRC.

Payloads that udsoncan refuses are counted and reported, not compared. They are
usually services whose payload it interprets further (data identifiers) and that this
test does not generate.

Usage (from the repository root, with udsoncan installed):

    pip install udsoncan==1.26.1
    python scripts/differential/uds_udsoncan.py
"""

import random
import subprocess
import sys

import udsoncan
from udsoncan import Request, Response

SEED = 20261110
CASES = 400
RUST_CMD = [
    "cargo",
    "run",
    "-q",
    "-p",
    "tpt-protocol-uds",
    "--example",
    "diff_decode",
]
# Services with a sub-function byte and a payload that udsoncan reads generically.
SUB_SERVICES = [0x10, 0x11, 0x27, 0x3E]
# Services whose request payload is a raw DID (two bytes) or nothing.
PLAIN_SERVICES = [0x14, 0x22, 0x19]


def hex_bytes(data: bytes) -> str:
    return data.hex()


def request_case(rng):
    if rng.random() < 0.6:
        sid = rng.choice(SUB_SERVICES)
        sub = rng.randrange(0, 0x80)
        if rng.random() < 0.3:
            sub |= 0x80  # suppressPosRsp
        data = bytes(rng.randrange(256) for _ in range(rng.randint(0, 3)))
        return bytes([sid, sub]) + data
    sid = rng.choice(PLAIN_SERVICES)
    data = bytes(rng.randrange(256) for _ in range(rng.randint(0, 4)))
    return bytes([sid]) + data


def response_case(rng):
    if rng.random() < 0.6:
        sid = rng.choice(SUB_SERVICES) | 0x40
        sub = rng.randrange(0, 0x80)
        data = bytes(rng.randrange(256) for _ in range(rng.randint(0, 4)))
        return bytes([sid, sub]) + data
    return bytes([0x7F, rng.choice(PLAIN_SERVICES), rng.randrange(0x10, 0x80)])


def reference(payload: bytes):
    """udsoncan's canonical reading of a payload, or None when it refuses it."""
    try:
        if payload[0] == 0x7F:
            resp = Response.from_payload(payload)
            return f"neg sid={resp.service.request_id():02x} nrc={resp.code:02x}"
        if payload[0] & 0x40:
            resp = Response.from_payload(payload)
            sid = resp.service.response_id()
            if resp.service.use_subfunction:
                return f"resp sid={sid:02x} sub={payload[1]:02x} data={payload[2:].hex()}"
            return f"resp sid={sid:02x} sub=- data={payload[1:].hex()}"
        req = Request.from_payload(payload)
        sub = "-" if req.subfunction is None else f"{req.subfunction:02x}"
        return (
            f"req sid={req.service.request_id():02x} sub={sub} "
            f"suppress={str(bool(req.suppress_positive_response)).lower()} "
            f"data={(req.data or b'').hex()}"
        )
    except Exception:  # udsoncan refuses the payload
        return None


def rust_output(payloads):
    text = "\n".join(p.hex() for p in payloads) + "\n"
    result = subprocess.run(RUST_CMD, input=text, capture_output=True, text=True, check=True)
    blocks = {}
    current = None
    for line in result.stdout.splitlines():
        if line.startswith("#"):
            current = int(line[1:])
            blocks[current] = []
        else:
            blocks[current].append(line)
    return blocks


def main() -> int:
    rng = random.Random(SEED)
    payloads = []
    for i in range(CASES):
        payloads.append(request_case(rng) if i % 2 == 0 else response_case(rng))

    blocks = rust_output(payloads)
    compared = 0
    refused = 0
    failures = []
    for index, payload in enumerate(payloads):
        expected = reference(payload)
        got = blocks.get(index, ["error"])[0]
        if expected is None:
            refused += 1
            continue
        compared += 1
        if got != expected:
            failures.append(f"{payload.hex()}: rust {got!r}, udsoncan {expected!r}")

    print(
        f"udsoncan {udsoncan.__version__}: {compared} payloads compared, "
        f"{refused} refused by udsoncan, {len(failures)} mismatches"
    )
    for problem in failures[:20]:
        print("  " + problem)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
