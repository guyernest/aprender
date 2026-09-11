#!/usr/bin/env bash
# setfit_bench_gate_door_probe.sh - replay verifier spot-check E through the SHIPPED door.
#
# WHY THIS EXISTS. Phase 5's verification found that `verify_provenance` built
# `bench_dir.join(row.evidence.setfit.lock.lock_record_path)` from a row-supplied
# string with no validation. `Path::join` DISCARDS its base when the argument is
# absolute and never resolves `..`, so with the committed lock record deleted and
# the row pointing at a file outside the benchmark directory,
# `apr setfit bench report` exited 0 AND printed its own attestation that
# "provenance was recomputed from the committed lock bytes rather than read off
# the rows". That sentence was FALSE on the run that produced it.
#
# The unit suite proves the refusal at the library boundary. This probe proves it
# at the door a user actually runs, on the exact tree shape that produced the
# finding - because a guard that does not scan the surface where the DECISION is
# made is theater (CLAUDE.md Verification Discipline rule 5).
#
# TWO RUNS, AND THE ORDER IS THE PROPERTY:
#   1. POSITIVE CONTROL on the undoctored slim copy. It must exit 0. Without it,
#      a probe cannot distinguish "the gate refused the attack" from "the scratch
#      copy was broken", and would report success for the wrong reason.
#   2. THE ATTACK on the doctored copy. It must exit non-zero AND name the
#      escaping path.
#
# Status is captured as `cmd > "$log" 2>&1; rc=$?` and NEVER through a pipe.
# `$?` after a pipeline is the LAST command's status; this repo has shipped that
# defect twice (#2336, #2360), so it is spelled out rather than assumed.
#
# Run:  bash scripts/setfit_bench_gate_door_probe.sh
# Make: make setfit-bench-door-probe
# Needs: an apr built from HEAD **and carrying the `setfit` surface**, which is
#        NOT a default feature:
#            cargo build --release --bin apr --features setfit
#        Measured: a default `cargo build --release --bin apr` produces a binary
#        whose `apr setfit` is "unrecognized subcommand". The probe checks for
#        the surface explicitly rather than letting that failure surface as a
#        broken positive control, which would read as "the evidence tree is bad".

set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT" || exit 1

SOURCE_DIR="$REPO_ROOT/benchmarks/tweeteval-stance"
# The cell verifier spot-check E doctored. Named once.
TARGET_CELL="setfit-s8-seed13"

fail() {
    printf 'FAIL: %s\n' "$*" >&2
    exit 1
}

# ---- The binary pin -------------------------------------------------------
#
# NEVER a bare `apr` and never a hardcoded absolute path: four `apr` binaries
# were once found coexisting on one machine, and the stale one won. If the pin
# refuses, say so and name the remedy rather than degrading to whatever is on
# PATH - a probe that silently tested a different binary is worse than no probe.
if ! . scripts/apr_bin.sh; then
    printf 'FAIL: scripts/apr_bin.sh refused to resolve an apr binary built from HEAD.\n' >&2
    printf '      This probe drives the SHIPPED door, so a stale binary would prove\n' >&2
    printf '      nothing about this checkout.\n' >&2
    printf '      Remedy: cargo build --release --bin apr --features setfit\n' >&2
    exit 1
fi

[ -d "$SOURCE_DIR" ] || fail "$SOURCE_DIR does not exist; there is no committed evidence to probe"

# ---- Scratch, removed on every exit path ----------------------------------
SCRATCH=$(mktemp -d "${TMPDIR:-/tmp}/setfit-bench-door-probe.XXXXXX")
cleanup() {
    # `${SCRATCH:?}` so an unset or empty variable aborts rather than expanding
    # to `rm -rf /` (bashrs SEC011/SC2114). `$SCRATCH` is mktemp's own output.
    rm -rf "${SCRATCH:?}"
}
trap cleanup EXIT

# ---- The FEATURE pin ------------------------------------------------------
#
# `setfit` is not a default feature, so a binary built from HEAD can still be
# fresh AND have no `apr setfit` at all. Checked here, with its own remedy,
# because the alternative is a rc!=0 positive control that reads as "the
# committed evidence is broken" when the evidence is fine and the binary is not.
SURFACE_LOG="$SCRATCH/surface.log"
set +e
"$APR" setfit bench report --help > "$SURFACE_LOG" 2>&1
surface_rc=$?
set -e
if [ "$surface_rc" -ne 0 ]; then
    printf 'FAIL: %s has no "apr setfit bench report" surface (rc=%s).\n' "$APR" "$surface_rc" >&2
    printf '      The setfit feature is NOT a default feature.\n' >&2
    printf '      Remedy: cargo build --release --bin apr --features setfit\n' >&2
    exit 1
fi

BENCH_DIR="$SCRATCH/bench"
OUTSIDE_DIR="$SCRATCH/outside"
mkdir -p "$BENCH_DIR" "$OUTSIDE_DIR"

# Copy everything EXCEPT `artifacts/` (40 x ~90 MB of .apr) and `logs/`. The gate
# reads neither; the positive control below is what proves the slim copy is
# sufficient, rather than this comment asserting it.
# Both operands are derived here, never from user input: $SOURCE_DIR is
# $REPO_ROOT/benchmarks/tweeteval-stance and $BENCH_DIR is under mktemp's own
# output, so neither can carry a traversal a caller supplied.
# bashrs:allow SEC014
find "$SOURCE_DIR" -mindepth 1 -maxdepth 1 ! -name artifacts ! -name logs \
    -exec cp -R {} "$BENCH_DIR/" \;

# ---- 1. POSITIVE CONTROL --------------------------------------------------
CONTROL_LOG="$SCRATCH/control.log"
set +e
"$APR" setfit bench report --bench-dir "$BENCH_DIR" > "$CONTROL_LOG" 2>&1
control_rc=$?
set -e
if [ "$control_rc" -ne 0 ]; then
    printf 'CONTROL: rc=%s on the UNDOCTORED slim copy - the probe cannot proceed.\n' \
        "$control_rc" >&2
    tail -20 "$CONTROL_LOG" >&2
    fail "the undoctored benchmark directory must verify before an attack on it means anything"
fi
printf 'CONTROL: undoctored slim copy of %s verifies (rc=0)\n' "$SOURCE_DIR"

# ---- 2. DOCTOR THE COPY INTO SPOT-CHECK E ---------------------------------
#
# The digest repair is LOAD-BEARING. `lock_record_path` sits inside the hashed
# payload, so an unrepaired edit is refused at the row-digest step BEFORE
# provenance is ever reached, and the probe would go green having proven nothing
# about path resolution. The python block re-derives the committed digest FIRST
# and refuses to continue if it does not reproduce, so a change to the digest
# scheme fails this probe loudly instead of quietly making it vacuous.
#
# This is fixture doctoring of throwaway JSON inside a scratch directory - the
# same method 05-VERIFICATION.md used to produce spot-checks B through G - and
# not the ML-stack substitution that crates/aprender-train/CLAUDE.md's Python
# prohibition targets. Nothing here touches the checkout.
ESCAPE_PATH=$(python3 scripts/setfit_bench_gate_doctor.py \
    "$BENCH_DIR" "$OUTSIDE_DIR" "$TARGET_CELL")
[ -n "$ESCAPE_PATH" ] || fail "the doctoring step produced no escape path"
[ -f "$ESCAPE_PATH" ] || fail "the escape target $ESCAPE_PATH does not hold the attested lock bytes"
[ ! -e "$BENCH_DIR/locks/$TARGET_CELL.lock.json" ] \
    || fail "the committed lock record is still present; this is not the shape spot-check E had"
printf 'DOCTORED: %s now points at %s, and the committed lock record is gone\n' \
    "$TARGET_CELL" "$ESCAPE_PATH"

# ---- 3. THE ATTACK --------------------------------------------------------
ATTACK_LOG="$SCRATCH/attack.log"
set +e
"$APR" setfit bench report --bench-dir "$BENCH_DIR" > "$ATTACK_LOG" 2>&1
attack_rc=$?
set -e

if [ "$attack_rc" -eq 0 ]; then
    tail -20 "$ATTACK_LOG" >&2
    fail "the doctored tree was ACCEPTED (rc=0) - verifier gap 1 is open again"
fi

# The refusal must be the PATH one, not a digest one. A digest refusal would mean
# the repair above was skipped and provenance was never reached.
if ! grep -q -- "$ESCAPE_PATH" "$ATTACK_LOG"; then
    tail -20 "$ATTACK_LOG" >&2
    fail "the refusal does not name the escaping path, so it is not the path-escape refusal"
fi
if ! grep -q "leaves the benchmark directory" "$ATTACK_LOG"; then
    tail -20 "$ATTACK_LOG" >&2
    fail "the refusal is not the path-escape one; the gate refused for some other reason"
fi
if grep -q "not the bytes that were attested" "$ATTACK_LOG"; then
    tail -20 "$ATTACK_LOG" >&2
    fail "the gate refused at the DIGEST step: the digest repair was skipped, so provenance was never reached"
fi

printf 'ATTACK: rc=%s, refused as a path escape naming %s\n' "$attack_rc" "$ESCAPE_PATH"
printf 'PASS: %s refuses a row-supplied evidence path that leaves the benchmark directory, having first verified the undoctored tree\n' "$APR"
exit 0
