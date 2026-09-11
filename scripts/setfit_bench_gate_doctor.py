#!/usr/bin/env python3
"""Doctor a SCRATCH copy of the benchmark directory into verifier spot-check E.

Called only by ``scripts/setfit_bench_gate_door_probe.sh``. It is a separate file
rather than an inline heredoc because ``bashrs`` — the shell linter this repo uses
instead of shellcheck — does not skip a quoted heredoc body, and reports a dozen
phantom shell parse errors against the Python inside one. A probe whose own lint
gate is red teaches a reader to ignore lint output.

WHAT IT DOES, and why each step is load-bearing:

1. Re-derives the COMMITTED digests first and refuses to continue if the scheme
   does not reproduce. Without this the script could write wrong digests, the
   gate would refuse at the digest step, and the probe would report success
   having proven nothing about path resolution.
2. MOVES the committed lock record out of the benchmark directory, so the escape
   target holds the very bytes the row attests and nothing legitimate inside the
   tree can satisfy the row. A target holding OTHER bytes would be refused as a
   provenance mismatch — still red, but red for the wrong reason, proving the
   escape was DETECTED rather than that it SUCCEEDED.
3. Repairs the row's ``semantic_hash``, the manifest's ``row_sha256`` for that
   cell, and the manifest's envelope digest. ``lock_record_path`` sits INSIDE the
   hashed payload, so an unrepaired edit is refused before provenance is reached.

The digest scheme is ``sha256`` over the payload's COMPACT, SERDE-ORDER JSON —
key order as the file carries it, not sorted. That was measured against the
committed tree, not assumed: sorting the keys reproduces neither the row nor the
manifest digest.

This is fixture doctoring of throwaway JSON inside a scratch directory, the same
method ``05-VERIFICATION.md`` used to produce spot-checks B through G. It is not
the ML-stack substitution that ``crates/aprender-train/CLAUDE.md``'s Python
prohibition targets, and it never touches the checkout.

Usage: setfit_bench_gate_doctor.py <scratch-bench-dir> <outside-dir> <cell>
Prints the absolute escape path on stdout.
"""

import hashlib
import json
import os
import shutil
import sys


def digest(payload):
    """The committed scheme: sha256 over the payload's compact serde-order JSON."""
    return hashlib.sha256(
        json.dumps(payload, separators=(",", ":")).encode("utf-8")
    ).hexdigest()


def write_pretty(path, value):
    """Write pretty JSON with the trailing newline the Rust writer emits.

    The FILE's whitespace is free: the digest is over the payload's own canonical
    compact bytes, so rows stay reviewable in diffs without weakening anything.
    """
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(value, handle, indent=2)
        handle.write("\n")


def load(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


def main(argv):
    if len(argv) != 4:
        raise SystemExit(
            "usage: setfit_bench_gate_doctor.py <scratch-bench-dir> <outside-dir> <cell>"
        )
    bench_dir, outside_dir, cell = argv[1], argv[2], argv[3]

    row_path = os.path.join(bench_dir, "rows", cell + ".json")
    lock_path = os.path.join(bench_dir, "locks", cell + ".lock.json")
    manifest_path = os.path.join(bench_dir, "run-manifest.json")
    escape_path = os.path.join(outside_dir, "anywhere.json")

    # Refuse to doctor the real tree, whatever a caller passes.
    if os.path.realpath(bench_dir).startswith(os.path.realpath("benchmarks")):
        raise SystemExit("refusing to doctor the committed benchmark directory")

    row = load(row_path)
    manifest = load(manifest_path)

    # PROVE THE REPAIR SCHEME BEFORE USING IT.
    if digest(row["payload"]) != row["semantic_hash"]:
        raise SystemExit("the row digest scheme no longer reproduces the committed digest")
    if digest(manifest["payload"]) != manifest["semantic_hash"]:
        raise SystemExit("the manifest digest scheme no longer reproduces the committed digest")

    shutil.move(lock_path, escape_path)

    row["payload"]["evidence"]["setfit"]["lock"]["lock_record_path"] = escape_path
    row["semantic_hash"] = digest(row["payload"])
    write_pretty(row_path, row)

    matched = 0
    for entry in manifest["payload"]["cells"]:
        rendered = "{0}-s{1}-seed{2}".format(entry["method"], entry["shots"], entry["seed"])
        if rendered == cell:
            entry["row_sha256"] = row["semantic_hash"]
            matched += 1
    if matched != 1:
        raise SystemExit(
            "expected exactly one manifest entry for {0}, found {1}".format(cell, matched)
        )
    manifest["semantic_hash"] = digest(manifest["payload"])
    write_pretty(manifest_path, manifest)

    print(escape_path)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
