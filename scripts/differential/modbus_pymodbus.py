#!/usr/bin/env python3
"""Differential test: tpt-protocol-modbus against pymodbus.

pymodbus, the reference Python Modbus library, encodes random Modbus PDUs and
decodes them again. The Rust example `diff_decode` parses the same frame bytes.
Both sides reduce each frame to one canonical line, and this script fails on any
difference.

Covered: read holding and input registers, write single register, write multiple
registers, their responses, and exception responses. Each case is framed as Modbus
TCP (MBAP header) and as RTU (CRC-16), so both framings are checked. Negative cases
(an RTU frame with a wrong CRC, a TCP frame with a trailing byte) must be rejected.

The CRC-16 here is written independently of the Rust code.

Usage (from the repository root, with pymodbus installed):

    pip install pymodbus==3.6.9
    python scripts/differential/modbus_pymodbus.py
"""

import random
import subprocess
import sys

from pymodbus.pdu import ExceptionResponse
from pymodbus.register_read_message import (
    ReadHoldingRegistersRequest,
    ReadHoldingRegistersResponse,
    ReadInputRegistersRequest,
    ReadInputRegistersResponse,
)
from pymodbus.register_write_message import (
    WriteMultipleRegistersRequest,
    WriteMultipleRegistersResponse,
    WriteSingleRegisterRequest,
)

SEED = 20261110
CASES = 600
RUST_CMD = [
    "cargo",
    "run",
    "-q",
    "-p",
    "tpt-protocol-modbus",
    "--example",
    "diff_decode",
]


def crc16(data: bytes) -> int:
    """Modbus CRC-16 (polynomial 0xA001 reflected, initial value 0xFFFF)."""
    crc = 0xFFFF
    for byte in data:
        crc ^= byte
        for _ in range(8):
            crc = (crc >> 1) ^ 0xA001 if crc & 1 else crc >> 1
    return crc


def tcp_adu(unit: int, function_code: int, data: bytes, transaction: int) -> bytes:
    pdu = bytes([function_code]) + data
    length = len(pdu) + 1  # unit id and PDU
    header = transaction.to_bytes(2, "big") + b"\x00\x00" + length.to_bytes(2, "big")
    return header + bytes([unit]) + pdu


def rtu_adu(unit: int, function_code: int, data: bytes) -> bytes:
    body = bytes([unit, function_code]) + data
    return body + crc16(body).to_bytes(2, "little")


def values_text(values) -> str:
    return ",".join(str(int(v)) for v in values)


def read_request_case(rng, unit, function_code):
    address = rng.randrange(0, 65536 - 125)
    count = rng.randint(1, 125)
    cls = ReadHoldingRegistersRequest if function_code == 3 else ReadInputRegistersRequest
    data = cls(address=address, count=count, slave=unit).encode()
    decoded = cls(0, 0)
    decoded.decode(data)
    expected = (
        f"req unit={unit} fc={function_code} addr={decoded.address} qty={decoded.count}"
    )
    return "req", function_code, data, expected


def write_single_case(rng, unit):
    address = rng.randrange(0, 65536)
    value = rng.randrange(0, 65536)
    data = WriteSingleRegisterRequest(address=address, value=value, slave=unit).encode()
    decoded = WriteSingleRegisterRequest(0, 0)
    decoded.decode(data)
    expected = f"req unit={unit} fc=6 addr={decoded.address} value={decoded.value}"
    return "req", 6, data, expected


def write_multiple_case(rng, unit):
    address = rng.randrange(0, 65536 - 123)
    values = [rng.randrange(0, 65536) for _ in range(rng.randint(1, 123))]
    data = WriteMultipleRegistersRequest(address=address, values=values, slave=unit).encode()
    decoded = WriteMultipleRegistersRequest(0, [])
    decoded.decode(data)
    expected = (
        f"req unit={unit} fc=16 addr={decoded.address} qty={decoded.count} "
        f"values={values_text(decoded.values)}"
    )
    return "req", 16, data, expected


def read_response_case(rng, unit, function_code):
    values = [rng.randrange(0, 65536) for _ in range(rng.randint(1, 125))]
    cls = ReadHoldingRegistersResponse if function_code == 3 else ReadInputRegistersResponse
    data = cls(values=values, slave=unit).encode()
    decoded = cls([])
    decoded.decode(data)
    expected = f"resp unit={unit} fc={function_code} values={values_text(decoded.registers)}"
    return "resp", function_code, data, expected


def write_single_response_case(rng, unit):
    address = rng.randrange(0, 65536)
    value = rng.randrange(0, 65536)
    data = WriteSingleRegisterRequest(address=address, value=value, slave=unit).encode()
    decoded = WriteSingleRegisterRequest(0, 0)
    decoded.decode(data)
    expected = f"resp unit={unit} fc=6 addr={decoded.address} value={decoded.value}"
    return "resp", 6, data, expected


def write_multiple_response_case(rng, unit):
    address = rng.randrange(0, 65536 - 123)
    count = rng.randint(1, 123)
    data = WriteMultipleRegistersResponse(address=address, count=count, slave=unit).encode()
    decoded = WriteMultipleRegistersResponse(0, 0)
    decoded.decode(data)
    expected = f"resp unit={unit} fc=16 addr={decoded.address} qty={decoded.count}"
    return "resp", 16, data, expected


def exception_case(rng, unit):
    function_code = rng.choice([1, 2, 3, 4, 5, 6, 15, 16])
    code = rng.randint(1, 11)
    exc = ExceptionResponse(function_code, code)
    # pymodbus stores the function code with bit 7 set; the wire PDU carries it that way.
    expected = (
        f"resp unit={unit} fc={function_code} exception={exc.exception_code}"
    )
    return "resp", exc.function_code, exc.encode(), expected


def generate(rng):
    """Yields (direction, framing, frame bytes, expected canonical line, negative?)."""
    makers = [
        lambda u: read_request_case(rng, u, 3),
        lambda u: read_request_case(rng, u, 4),
        lambda u: write_single_case(rng, u),
        lambda u: write_multiple_case(rng, u),
        lambda u: read_response_case(rng, u, 3),
        lambda u: read_response_case(rng, u, 4),
        lambda u: write_single_response_case(rng, u),
        lambda u: write_multiple_response_case(rng, u),
        lambda u: exception_case(rng, u),
    ]
    for i in range(CASES):
        unit = rng.randint(1, 247)
        direction, function_code, data, expected = makers[i % len(makers)](unit)
        tcp = (i // len(makers)) % 2 == 0
        if tcp:
            frame = tcp_adu(unit, function_code, data, transaction=rng.randrange(0, 65536))
        else:
            frame = rtu_adu(unit, function_code, data)
        yield direction, frame, expected, False

        # Negative cases: the parser must reject these, whatever pymodbus would do.
        if not tcp and i % 10 == 0:
            broken = bytearray(frame)
            broken[-1] ^= 0x5A  # damage the CRC so it no longer matches
            yield direction, bytes(broken), "error", True
        if tcp and i % 10 == 1:
            yield direction, frame + b"\x00", "error", True


def hex_text(data: bytes) -> str:
    return data.hex()


def main() -> int:
    rng = random.Random(SEED)
    cases = list(generate(rng))
    stdin_lines = [f"{d} {hex_text(f)}" for d, f, _, _ in cases]
    expected = [e for _, _, e, _ in cases]

    result = subprocess.run(
        RUST_CMD,
        input="\n".join(stdin_lines) + "\n",
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        print("the Rust example failed to build or run", file=sys.stderr)
        return 2

    actual = result.stdout.splitlines()
    if len(actual) != len(expected):
        print(
            f"expected {len(expected)} output lines, got {len(actual)}", file=sys.stderr
        )
        return 1

    failures = 0
    for (direction, frame, _, negative), want, got in zip(cases, expected, actual):
        if want != got:
            failures += 1
            if failures <= 20:
                print(
                    f"MISMATCH ({'negative' if negative else 'valid'}) {direction} "
                    f"{hex_text(frame)}\n  pymodbus: {want}\n  tpt:      {got}",
                    file=sys.stderr,
                )
    positives = sum(1 for c in cases if not c[3])
    negatives = len(cases) - positives
    print(
        f"{len(cases)} frames ({positives} valid, {negatives} negative), "
        f"{failures} mismatches"
    )
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
