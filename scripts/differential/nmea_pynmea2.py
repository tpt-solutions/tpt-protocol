#!/usr/bin/env python3
"""Differential test: tpt-protocol-nmea against pynmea2.

pynmea2, the reference Python NMEA 0183 library, parses random GGA and RMC sentences
(and some with a corrupted checksum). The Rust example `diff_decode` parses the same
text and prints the points its normalizer gives. This script compares the two field
by field: floats to 2e-6 degrees (or units), integers exactly, and a corrupted
sentence must fail on both sides.

Covered: GGA (latitude, longitude, fix quality, satellites, altitude) and RMC
(latitude, longitude, speed over ground, true course, with an active fix). Latitude
and longitude are generated in both hemispheres.

The checksum is written here, independently of the Rust code.

Usage (from the repository root, with pynmea2 installed):

    pip install pynmea2==1.19.0
    python scripts/differential/nmea_pynmea2.py
"""

import random
import subprocess
import sys

import pynmea2

SEED = 20261110
CASES_PER_TYPE = 150
TOLERANCE = 2e-6
RUST_CMD = [
    "cargo",
    "run",
    "-q",
    "-p",
    "tpt-protocol-nmea",
    "--example",
    "diff_decode",
]


def checksum(body: str) -> str:
    value = 0
    for ch in body:
        value ^= ord(ch)
    return f"{value:02X}"


def wrap(body: str, corrupt: bool = False) -> str:
    digits = checksum(body)
    if corrupt:
        digits = "00" if digits != "00" else "FF"
    return f"${body}*{digits}"


def coordinate(rng, degrees_width: int, positive_hemisphere: str, negative_hemisphere: str):
    """A random coordinate, as (text, signed decimal degrees)."""
    degrees = rng.randrange(0, 10 ** degrees_width if degrees_width == 2 else 180)
    minutes = rng.randrange(0, 6000) / 100.0
    hemisphere = rng.choice([positive_hemisphere, negative_hemisphere])
    text = f"{degrees:0{degrees_width}d}{minutes:07.4f}"
    value = degrees + minutes / 60.0
    if hemisphere == negative_hemisphere:
        value = -value
    return text, hemisphere, value


def gga_case(rng):
    time = f"{rng.randrange(24):02d}{rng.randrange(60):02d}{rng.randrange(60):02d}.00"
    lat_text, lat_h, lat = coordinate(rng, 2, "N", "S")
    lon_text, lon_h, lon = coordinate(rng, 3, "E", "W")
    quality = rng.randrange(0, 9)
    sats = rng.randrange(0, 13)
    hdop = f"{rng.randrange(1, 200) / 10:.1f}"
    altitude = rng.randrange(-200, 9000) / 10.0
    body = (
        f"GPGGA,{time},{lat_text},{lat_h},{lon_text},{lon_h},{quality},{sats:02d},"
        f"{hdop},{altitude:.1f},M,46.9,M,,"
    )
    expected = [
        ("GGA.latitude", lat),
        ("GGA.longitude", lon),
        ("GGA.fix_quality", quality),
        ("GGA.satellites", sats),
        ("GGA.altitude_m", altitude),
    ]
    return body, expected


def rmc_case(rng):
    time = f"{rng.randrange(24):02d}{rng.randrange(60):02d}{rng.randrange(60):02d}.00"
    lat_text, lat_h, lat = coordinate(rng, 2, "N", "S")
    lon_text, lon_h, lon = coordinate(rng, 3, "E", "W")
    speed = rng.randrange(0, 5000) / 100.0
    course = rng.randrange(0, 36000) / 100.0
    body = (
        f"GPRMC,{time},A,{lat_text},{lat_h},{lon_text},{lon_h},"
        f"{speed:.2f},{course:.2f},230394,003.1,W"
    )
    expected = [
        ("RMC.latitude", lat),
        ("RMC.longitude", lon),
        ("RMC.speed_knots", speed),
        ("RMC.course_deg", course),
    ]
    return body, expected


def reference(sentence: str):
    """pynmea2's reading of the sentence, or None if it rejects it."""
    try:
        msg = pynmea2.parse(sentence, check=True)
    except pynmea2.ParseError:
        return None
    return msg


def rust_output(sentences):
    text = "\n".join(sentences) + "\n"
    result = subprocess.run(
        RUST_CMD,
        input=text,
        capture_output=True,
        text=True,
        check=True,
    )
    blocks = {}
    current = None
    for line in result.stdout.splitlines():
        if line.startswith("#"):
            current = int(line[1:])
            blocks[current] = []
        else:
            blocks[current].append(line)
    return blocks


def compare(index, sentence, expected, lines):
    """Returns a description of the mismatch, or None when they agree."""
    msg = reference(sentence)
    if expected is None:
        if msg is None and lines == ["error"]:
            return None
        return f"{sentence}: expected an error, got {lines} (pynmea2: {msg})"
    if msg is None:
        return f"{sentence}: pynmea2 rejected a sentence it should accept"
    if lines == ["error"]:
        return f"{sentence}: tpt-protocol rejected a sentence pynmea2 accepts"
    if len(lines) != len(expected):
        return f"{sentence}: {len(lines)} points from Rust, {len(expected)} expected: {lines}"
    for line, (name, value) in zip(lines, expected):
        point_id, text = line.rsplit(" ", 1)
        if point_id != name:
            return f"{sentence}: point {point_id} where {name} was expected"
        if isinstance(value, int):
            if text != str(value):
                return f"{sentence}: {name} is {text}, expected {value}"
        else:
            if abs(float(text) - value) > TOLERANCE:
                return f"{sentence}: {name} is {text}, expected {value:.6f}"
    return None


def main() -> int:
    rng = random.Random(SEED)
    cases = []  # (sentence, expected points or None for "must fail")
    for _ in range(CASES_PER_TYPE):
        body, expected = gga_case(rng)
        cases.append((wrap(body), expected))
        body, expected = rmc_case(rng)
        cases.append((wrap(body), expected))
    # Corrupted checksums: both sides must reject them.
    for _ in range(CASES_PER_TYPE // 3):
        body, _ = gga_case(rng)
        cases.append((wrap(body, corrupt=True), None))

    sentences = [sentence for sentence, _ in cases]
    blocks = rust_output(sentences)
    failures = []
    for index, (sentence, expected) in enumerate(cases):
        lines = blocks.get(index, [])
        problem = compare(index, sentence, expected, lines)
        if problem:
            failures.append(problem)

    print(f"pynmea2 {pynmea2.__version__}: {len(cases)} sentences, {len(failures)} mismatches")
    for problem in failures[:20]:
        print("  " + problem)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
