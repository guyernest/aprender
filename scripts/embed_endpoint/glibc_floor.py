#!/usr/bin/env python3
"""Glibc floor check for Lambda arm64 binaries (Phase 11, plan 11-01).

usage: glibc_floor.py <ceiling-minor> <binary> [<binary> ...]

Prints `glibc floor: <file> needs <= 2.N` for each binary and exits 1, naming the
first file whose highest GLIBC_2.N symbol exceeds the ceiling. The ceiling is the
minor version of the Lambda runtime's glibc (provided.al2023 ships 2.34).

The regex is the one the justfile recipe build-trainer-asset uses; that recipe is
not edited to call this script, so this file is the phase-11 copy of the check.
Standard library only; imports no sibling module.
"""

import re
import sys


def glibc_floor(path):
    """Highest minor version N of any GLIBC_2.N string in the binary (0 if none)."""
    with open(path, "rb") as handle:
        data = handle.read()
    need = [int(v) for v in re.findall(rb"GLIBC_2\.(\d+)", data)]
    return max(need, default=0)


def main(argv):
    if len(argv) < 3:
        print("usage: glibc_floor.py <ceiling-minor> <binary> [<binary> ...]", file=sys.stderr)
        return 2
    try:
        ceiling = int(argv[1])
    except ValueError:
        print(f"ERROR: ceiling must be a minor version number, got {argv[1]!r}", file=sys.stderr)
        return 2
    offender = None
    for path in argv[2:]:
        name = path.split("/")[-1]
        top = glibc_floor(path)
        print(f"glibc floor: {name} needs <= 2.{top}")
        if top > ceiling and offender is None:
            offender = (name, top)
    if offender is not None:
        name, top = offender
        print(
            f"ERROR: {name} needs GLIBC_2.{top}, above the ceiling 2.{ceiling}.\n"
            f"       Pin the target to the runtime's glibc (cargo zigbuild --target "
            f"aarch64-unknown-linux-gnu.{ceiling})",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
