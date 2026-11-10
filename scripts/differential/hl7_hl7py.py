#!/usr/bin/env python3
"""Differential test: tpt-protocol-hl7v2 against the `hl7` package.

The `hl7` package is the reference Python HL7 v2 library. This script builds random
ADT and OBX messages, parses each with `hl7.parse`, and compares the fields with what
the Rust example `diff_decode` reads from the same text: the MSH fields the parser
keeps (MSH-3 to MSH-12 except MSH-8), and every non-empty field of the other segments.
A message whose first segment is not MSH must be rejected on both sides.

Components (`^`) are compared as the full field text, which is what both sides keep.
Escape characters and repetitions (`&`, `~`) are not generated.

Usage (from the repository root, with the hl7 package installed):

    pip install hl7==0.4.5
    python scripts/differential/hl7_hl7py.py
"""

import random
import subprocess
import sys

import hl7

SEED = 20261110
MESSAGES = 300
SEGMENT_TERMINATOR = "\r"
RUST_CMD = [
    "cargo",
    "run",
    "-q",
    "-p",
    "tpt-protocol-hl7v2",
    "--example",
    "diff_decode",
]
MSH_FIELDS = [3, 4, 5, 6, 7, 9, 10, 11, 12]


def word(rng, low=0, high=8):
    alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
    return "".join(rng.choice(alphabet) for _ in range(rng.randint(low, high)))


def text(rng, high=12):
    alphabet = "abcdefghijklmnopqrstuvwxyz0123456789 .-"
    return "".join(rng.choice(alphabet) for _ in range(rng.randint(0, high)))


def message(rng):
    """A valid ADT-style message: MSH, PID, OBX and sometimes NTE."""
    event = rng.randint(1, 8)
    msh = (
        f"MSH|^~\\&|{word(rng, 1)}|{word(rng)}|{word(rng)}|{word(rng)}|"
        f"20260101{rng.randrange(24):02d}{rng.randrange(60):02d}00||"
        f"ADT^A{event:02d}|{word(rng, 1, 10)}|P|2.5"
    )
    pid = (
        f"PID|1||{word(rng, 1)}||{word(rng)}^{word(rng)}^^||"
        f"19{rng.randrange(40, 99)}0101|{rng.choice(['F', 'M', ''])}"
    )
    obx = (
        f"OBX|1|ST|{word(rng, 1)}^{text(rng, 6)}||{text(rng)}^{word(rng)}|||||F"
    )
    segments = [msh, pid, obx]
    if rng.random() < 0.5:
        segments.append(f"NTE|1||{text(rng, 20)}")
    return segments


def expected_fields(segments):
    """The fields the reference library reads, in the Rust example's format."""
    parsed = hl7.parse(SEGMENT_TERMINATOR.join(segments))
    out = []
    msh = parsed[0]
    for number in MSH_FIELDS:
        value = str(msh[number])
        if value:
            out.append(f"MSH-{number} {value}")
    for seg in list(parsed)[1:]:
        name = str(seg[0])
        for i in range(1, len(seg)):
            value = str(seg[i])
            if value:
                out.append(f"{name}-{i} {value}")
    return out


def reference_rejects(segments):
    """True when the reference library refuses the text (first segment not MSH)."""
    return not segments[0].startswith("MSH")


def rust_output(lines):
    result = subprocess.run(
        RUST_CMD,
        input="\n".join(lines) + "\n",
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


def main() -> int:
    rng = random.Random(SEED)
    cases = []  # (escaped one-line text, expected lines or None for "must fail")
    for _ in range(MESSAGES):
        segments = message(rng)
        escaped = "\\r".join(segments)
        cases.append((escaped, expected_fields(segments)))
    # Negative cases: no MSH first. The Rust parser must refuse them.
    for _ in range(MESSAGES // 10):
        segments = ["PID|1||" + word(rng, 1), "OBX|1|ST|X||" + text(rng)]
        cases.append(("\\r".join(segments), None))

    blocks = rust_output([escaped for escaped, _ in cases])
    failures = []
    for index, (escaped, expected) in enumerate(cases):
        got = blocks.get(index, [])
        if expected is None:
            if got != ["error"]:
                failures.append(f"expected an error for: {escaped!r}; got {got[:3]}")
            continue
        if got == ["error"]:
            failures.append(f"rejected a message the reference reads: {escaped!r}")
            continue
        if got != expected:
            diff = next(
                (i for i, (a, b) in enumerate(zip(got, expected)) if a != b),
                min(len(got), len(expected)),
            )
            failures.append(
                f"fields differ in {escaped!r} at line {diff}: "
                f"rust {got[diff:diff + 1]} reference {expected[diff:diff + 1]}"
            )

    print(f"hl7 {hl7.__version__}: {len(cases)} messages, {len(failures)} mismatches")
    for problem in failures[:20]:
        print("  " + problem)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
