#!/usr/bin/env python3
"""The ONE implementation of the Laya deploy/upload eligibility decision (Phase 15, plan 15-03; D-08, D-09, D-04).

    python3 -I scripts/laya_deploy/laya_eligibility.py role
    python3 -I scripts/laya_deploy/laya_eligibility.py record-path --run RUN
    python3 -I scripts/laya_deploy/laya_eligibility.py check-record --record PATH --apr-sha256 H
    python3 -I scripts/laya_deploy/laya_eligibility.py judge --apr-sha256 H --verify-rc RC --verify-log LOG
                                                    [--record PATH --finding-log FILE --server S]
    python3 -I scripts/laya_deploy/laya_eligibility.py --selftest

A run packed by the training server's Graviton pack task carries logs/pack-outcome.json (schema
laya-train-pack-outcome-v1): the pack task's own recorded `pack_laya verify`. That record, bound to the EXACT local
.apr bytes by apr_sha256, is the eligibility authority for such a run; the local `just laya-verify` on this machine
is advisory in the way ROLE says (this Mac and Graviton disagree on the same bytes: smoke-stance-64 fine_tuned is
1.055x the A1 bound here and 0.843x on Graviton).

ROLE is the owner's signed role (15-BOUND-DECISION.json verify_role, decision commit 91969e11e, signed 2026-10-31 as
`sb-recommended role-drift-advisory`). The consequence table implemented here is that file's twin,
15-BOUND-CANDIDATES.json sign_off.verify_role_consequences:

    role-drift-advisory: rescore_drift logged | other_local_refusal blocks | gate_failed_exit_3 blocks
    role-all-advisory:   rescore_drift logged | other_local_refusal logged | gate_failed_exit_3 logged
    both:                record_invalid_or_sha_mismatch blocks | explicit_record_absent blocks
                         convention_record_absent -> local gate | no_record -> local gate

A missing record is never a pass: no record keeps today's local gate (the local verify's own verdict decides), and an
EXPLICITLY named record (LAYA_PACK_OUTCOME) that does not exist is refused.

Every subcommand prints exactly ONE verdict line on stdout. PURE: no network, no subprocess, no environment reads;
every input is an argument. Only the record, the log and the finding log are files. stdlib only.
"""

import datetime
import json
import os
import re
import sys
import tempfile
import shutil

RECORD_SCHEMA = "laya-train-pack-outcome-v1"
MAX_RECORD_BYTES = 1048576
SHA_RE = re.compile(r"[0-9a-f]{64}")
# The exact line pack_laya prints for a re-score drift refusal (examples/pack_laya.rs `refused`, verify.rs
# RescoreDrift): anchored at both ends, so no other variant (RescoreBoundCeiling, RescoreNoiseMismatch,
# RescoreNoiseInvalid, GateFailed, ...) and no indented or suffix-less line can be demoted.
DRIFT_RE = re.compile(
    r"^REFUSED RescoreDrift which=(fine_tuned|zero_shot) row=[0-9]+ "
    r"max_abs=[0-9.eE+-]+ bound=[0-9.eE+-]+ \(nothing written\)$"
)
ROLES = ("role-drift-advisory", "role-all-advisory")
ROLE = "role-drift-advisory"
DIGEST_RE = re.compile(r"sha256:[0-9a-f]{64}")
SERVER_RE = re.compile(r"[A-Za-z0-9._-]{1,128}")


class Refused(Exception):
    """A refusal: str(self) is the reason, after the fixed prefix."""


# ---- the record --------------------------------------------------------------------------------------------

def _pairs(pairs):
    d = {}
    for k, v in pairs:
        if k in d:
            raise ValueError("duplicate key %r" % k)
        d[k] = v
    return d


def check_record(path, apr_sha256):
    """Return (None, info) when the record accepts exactly these bytes, else (reason, None)."""
    if not isinstance(apr_sha256, str) or not SHA_RE.fullmatch(apr_sha256):
        return "the local sha256 is not 64 lowercase hex", None
    try:
        size = os.path.getsize(path)
    except OSError:
        return "%s does not exist" % path, None
    if size > MAX_RECORD_BYTES:
        return "%s is %d bytes, over the %d byte limit" % (path, size, MAX_RECORD_BYTES), None
    try:
        with open(path, "rb") as f:
            raw = f.read(MAX_RECORD_BYTES + 1)
        rec = json.loads(raw.decode("utf-8"), object_pairs_hook=_pairs)
    except (OSError, ValueError) as e:
        return "%s is not readable JSON (%s)" % (path, str(e)[:80]), None
    if not isinstance(rec, dict):
        return "%s is not a JSON object" % path, None
    if rec.get("schema") != RECORD_SCHEMA:
        return "schema is not %s" % RECORD_SCHEMA, None
    if rec.get("accepted") is not True:
        return "accepted is not true", None
    if rec.get("deploy_eligible") is not True:
        return "deploy_eligible is not true", None
    rsha = rec.get("apr_sha256")
    if not isinstance(rsha, str) or not SHA_RE.fullmatch(rsha):
        return "apr_sha256 is not 64 lowercase hex", None
    if rsha != apr_sha256:
        return "apr_sha256 %s differs from the local file's sha256 %s" % (rsha, apr_sha256), None
    digest = rec.get("pack_image_digest")
    parity = rec.get("parity_contract_sha256")
    return None, {
        "pack_image": digest if isinstance(digest, str) and DIGEST_RE.fullmatch(digest) else "unrecorded",
        "parity_contract": parity if isinstance(parity, str) and SHA_RE.fullmatch(parity) else "unrecorded",
    }


def record_path(run):
    """The convention record of a laya_fetch layout <dir>/run, only when it exists; else None."""
    if os.path.basename(run.rstrip("/")) != "run":
        return None
    rec = os.path.join(os.path.dirname(run.rstrip("/")), "logs", "pack-outcome.json")
    return rec if os.path.isfile(rec) else None


# ---- the local verify's outcome ----------------------------------------------------------------------------

def refused_lines(log_text):
    return [l for l in log_text.splitlines() if l.startswith("REFUSED ")]


def local_accepts(log_text, h):
    """RC 0 only counts when the last JSON line says deploy_eligible true for exactly these bytes."""
    lines = [l for l in log_text.splitlines() if l.startswith("{")]
    if not lines:
        return False
    try:
        v = json.loads(lines[-1])
    except ValueError:
        return False
    return isinstance(v, dict) and v.get("deploy_eligible") is True and v.get("artifact_sha256") == h


def _exit_code(rc):
    return rc if isinstance(rc, int) and 0 < rc < 256 else 2


def judge(h, rc, log_text, log_path, record=None, finding_log=None, server=None, role=ROLE, now=None):
    """Return (exit_code, verdict_line). `role` is a parameter only so the selftest can exercise both tables."""
    first = (refused_lines(log_text) or [None])[0]

    def local_refusal():
        return _exit_code(rc), "REFUSED eligibility: %s" % (first or "laya-verify exited %d (log: %s)" % (rc, log_path))

    def accepted_or_refused(ok_line):
        if local_accepts(log_text, h):
            return 0, ok_line
        return 2, "REFUSED eligibility: laya-verify exited 0 without deploy_eligible true for sha256 %s (log: %s)" % (h, log_path)

    if record is None:
        if rc != 0:
            return local_refusal()
        return accepted_or_refused("ELIGIBLE local-verify sha256 %s" % h)
    reason, info = check_record(record, h)
    if reason is not None:
        return 2, "REFUSED eligibility: pack record %s" % reason
    if rc == 0:
        return accepted_or_refused("ELIGIBLE pack-record sha256 %s (local verify accepted)" % h)
    demote = False
    if first is not None:
        if role == "role-drift-advisory":
            demote = rc == 2 and DRIFT_RE.search(first) is not None
        elif role == "role-all-advisory":
            # A local REFUSED line is a local refusal; a non-zero exit without one is a build or process failure,
            # which no role turns into a finding.
            demote = rc in (2, 3)
        else:
            return 2, "REFUSED eligibility: unknown role %s" % role
    if not demote:
        return local_refusal()
    stamp = (now or datetime.datetime.now(datetime.timezone.utc)).strftime("%Y-%m-%dT%H:%M:%SZ")
    srv = server if isinstance(server, str) and SERVER_RE.fullmatch(server) else "unknown"
    line = "CROSS-CPU FINDING %s server=%s sha256=%s pack_image=%s parity_contract=%s local=%s" % (
        stamp, srv, h, info["pack_image"], info["parity_contract"], first)
    if finding_log:
        os.makedirs(os.path.dirname(finding_log) or ".", exist_ok=True)
        with open(finding_log, "a", encoding="utf-8") as f:
            f.write(line + "\n")
    return 0, "ELIGIBLE pack-record sha256 %s (CROSS-CPU FINDING: %s)" % (h, first)


# ---- the selftest ------------------------------------------------------------------------------------------

H1 = "31ace8ea95ca0c750b6d007d0f4c68e4322d20363117f837673a936d6f696c85"
H0 = "0" * 64
DRIFT_LINE = ("REFUSED RescoreDrift which=fine_tuned row=199 max_abs=0.00003439188003540039 "
              "bound=0.00003259039634251337 (nothing written)")
SYN_LINE = "REFUSED SyntheticNotDeployable the artifact carries a synthetic marker (nothing written)"
GATE_LINE = "REFUSED GateFailed margin 0.01 below 0.05 (nothing written)"


def _local_json(h):
    return json.dumps({"deploy_eligible": True, "artifact_sha256": h})


def _record(**over):
    r = {"schema": RECORD_SCHEMA, "accepted": True, "deploy_eligible": True, "apr_sha256": H1,
         "pack_image_digest": "sha256:" + "5" * 8 + "a" * 56, "parity_contract_sha256": "7" * 64}
    r.update(over)
    return r


def selftest():
    results = []
    must_match = [0]
    must_not = [0]

    def case(label, ok):
        results.append((label, ok))
        print("%s %s" % ("PASS" if ok else "FAIL", label))

    # DRIFT_RE case table (discipline 7)
    matches = [
        ("drift-mac-line", DRIFT_LINE),
        ("drift-zero-shot-e-notation", "REFUSED RescoreDrift which=zero_shot row=3 max_abs=2.5e-05 bound=1e-5 (nothing written)"),
    ]
    not_matches = [
        ("ceiling", "REFUSED RescoreBoundCeiling set=fine_tuned derived bound=0.002 exceeds the ceiling 0.001 (nothing written)"),
        ("noise-mismatch", "REFUSED RescoreNoiseMismatch which=fine_tuned recorded=1e-6 recomputed=2e-6 (nothing written)"),
        ("noise-invalid", "REFUSED RescoreNoiseInvalid which=fine_tuned (nothing written)"),
        ("variant-suffix", "REFUSED RescoreDriftX which=fine_tuned row=199 max_abs=0.1 bound=0.2 (nothing written)"),
        ("leading-space", " " + DRIFT_LINE),
        ("no-suffix", DRIFT_LINE[: -len(" (nothing written)")]),
        ("gate-failed", GATE_LINE),
        ("trailing-text", DRIFT_LINE + " extra"),
        ("unknown-which", DRIFT_LINE.replace("fine_tuned", "base")),
    ]
    for label, line in matches:
        must_match[0] += 1
        case("must-match %s" % label, DRIFT_RE.search(line) is not None)
    for label, line in not_matches:
        must_not[0] += 1
        case("must-not-match %s" % label, DRIFT_RE.search(line) is None)

    tmp = tempfile.mkdtemp(prefix="laya-elig-selftest-")
    try:
        def put(name, obj, raw=None):
            p = os.path.join(tmp, name)
            with open(p, "w", encoding="utf-8") as f:
                f.write(raw if raw is not None else json.dumps(obj))
            return p

        good = put("good.json", _record())
        # check-record
        for label, path, h, want in [
            ("record-valid", good, H1, None),
            ("record-absent", os.path.join(tmp, "absent.json"), H1, "does not exist"),
            ("record-wrong-schema", put("schema.json", _record(schema="other")), H1, "schema"),
            ("record-accepted-false", put("acc.json", _record(accepted=False)), H1, "accepted"),
            ("record-eligible-false", put("elig.json", _record(deploy_eligible=False)), H1, "deploy_eligible"),
            ("record-sha-uppercase", put("up.json", _record(apr_sha256=H1.upper())), H1, "64 lowercase hex"),
            ("record-sha-63", put("s63.json", _record(apr_sha256=H1[:63])), H1, "64 lowercase hex"),
            ("record-sha-mismatch", good, H0, "differs from the local file"),
            ("record-not-object", put("arr.json", None, raw="[1]"), H1, "not a JSON object"),
            ("record-not-json", put("bad.json", None, raw="{"), H1, "not readable JSON"),
        ]:
            reason, _ = check_record(path, h)
            case("check-record %s" % label, (reason is None) if want is None else (reason is not None and want in reason))
        dup = put("dup.json", None, raw='{"schema": "%s", "accepted": true, "accepted": true}' % RECORD_SCHEMA)
        reason, _ = check_record(dup, H1)
        case("check-record duplicate-key", reason is not None and "not readable JSON" in reason)
        big = put("big.json", None, raw=" " * (MAX_RECORD_BYTES + 1))
        reason, _ = check_record(big, H1)
        case("check-record oversize", reason is not None and "byte limit" in reason)
        # record-path
        d1 = os.path.join(tmp, "p1")
        os.makedirs(os.path.join(d1, "run"))
        os.makedirs(os.path.join(d1, "logs"))
        open(os.path.join(d1, "logs", "pack-outcome.json"), "w").close()
        want = os.path.join(d1, "logs", "pack-outcome.json")
        case("record-path run-with-record", record_path(os.path.join(d1, "run")) == want)
        case("record-path trailing-slash", record_path(os.path.join(d1, "run") + "/") == want)
        d2 = os.path.join(tmp, "p2")
        os.makedirs(os.path.join(d2, "run"))
        case("record-path run-without-record", record_path(os.path.join(d2, "run")) is None)
        d3 = os.path.join(tmp, "p3")
        os.makedirs(os.path.join(d3, "other"))
        os.makedirs(os.path.join(d3, "logs"))
        open(os.path.join(d3, "logs", "pack-outcome.json"), "w").close()
        case("record-path other-basename", record_path(os.path.join(d3, "other")) is None)
        # role
        case("role is one of ROLES", ROLE in ROLES)
        # judge without a record: today's behaviour
        for label, rc, log, want_rc, want_pref in [
            ("norecord-rc0-accept", 0, _local_json(H1) + "\n", 0, "ELIGIBLE local-verify sha256 " + H1),
            ("norecord-rc0-wrong-sha", 0, _local_json(H0) + "\n", 2, "REFUSED eligibility: laya-verify exited 0 without"),
            ("norecord-rc0-no-json", 0, "hello\n", 2, "REFUSED eligibility: laya-verify exited 0 without"),
            ("norecord-rc2-refused", 2, "noise\n" + SYN_LINE + "\n", 2, "REFUSED eligibility: " + SYN_LINE),
            ("norecord-rc3", 3, GATE_LINE + "\n", 3, "REFUSED eligibility: " + GATE_LINE),
            ("norecord-rc101-no-refused", 101, "cargo failed\n", 101, "REFUSED eligibility: laya-verify exited 101"),
        ]:
            code, line = judge(H1, rc, log, "LOG")
            case("judge %s" % label, code == want_rc and line.startswith(want_pref))
        # judge with a valid record, under BOTH roles
        fl = os.path.join(tmp, "findings", "f.log")
        for role in ROLES:
            allrole = role == "role-all-advisory"

            def run(label, rc, log, want_code, want_pref, finding):
                before = open(fl).read() if os.path.isfile(fl) else ""
                code, line = judge(H1, rc, log, "LOG", record=good, finding_log=fl, server="srv-a", role=role)
                after = open(fl).read() if os.path.isfile(fl) else ""
                gained = after[len(before):] if after.startswith(before) else after
                ok = code == want_code and line.startswith(want_pref)
                if finding:
                    ok = ok and gained.startswith("CROSS-CPU FINDING ") and " server=srv-a " in gained \
                        and ("sha256=" + H1) in gained and "local=" + finding in gained and gained.count("\n") == 1
                else:
                    ok = ok and gained == ""
                case("[%s] judge %s" % (role, label), ok)

            drift_pref = "ELIGIBLE pack-record sha256 %s (CROSS-CPU FINDING: %s" % (H1, DRIFT_LINE)
            run("record-drift-first", 2, DRIFT_LINE + "\n", 0, drift_pref, DRIFT_LINE)
            run("record-drift-after-cargo-noise", 2, "Compiling x\n" + DRIFT_LINE + "\n", 0, drift_pref, DRIFT_LINE)
            run("record-drift-second-refused-line", 2, SYN_LINE + "\n" + DRIFT_LINE + "\n",
                0 if allrole else 2,
                ("ELIGIBLE pack-record sha256 %s (CROSS-CPU FINDING: %s" % (H1, SYN_LINE)) if allrole else "REFUSED eligibility: " + SYN_LINE,
                SYN_LINE if allrole else None)
            run("record-synthetic", 2, SYN_LINE + "\n", 0 if allrole else 2,
                ("ELIGIBLE pack-record sha256 %s (CROSS-CPU FINDING: %s" % (H1, SYN_LINE)) if allrole else "REFUSED eligibility: " + SYN_LINE,
                SYN_LINE if allrole else None)
            run("record-rc3", 3, GATE_LINE + "\n", 0 if allrole else 3,
                ("ELIGIBLE pack-record sha256 %s (CROSS-CPU FINDING: %s" % (H1, GATE_LINE)) if allrole else "REFUSED eligibility: " + GATE_LINE,
                GATE_LINE if allrole else None)
            run("record-rc101-no-refused-line", 101, "cargo failed\n", 101, "REFUSED eligibility: laya-verify exited 101", None)
            run("record-ceiling", 2, "REFUSED RescoreBoundCeiling set=fine_tuned x (nothing written)\n", 0 if allrole else 2,
                ("ELIGIBLE pack-record sha256 %s (CROSS-CPU FINDING: REFUSED RescoreBoundCeiling" % H1) if allrole else "REFUSED eligibility: REFUSED RescoreBoundCeiling",
                "REFUSED RescoreBoundCeiling set=fine_tuned x (nothing written)" if allrole else None)
            run("record-rc0-accept", 0, _local_json(H1) + "\n", 0, "ELIGIBLE pack-record sha256 %s (local verify accepted)" % H1, None)
            run("record-rc0-wrong-sha", 0, _local_json(H0) + "\n", 2, "REFUSED eligibility: laya-verify exited 0 without", None)
            # an invalid record refuses before any local outcome is considered
            for label, rec, h in [
                ("record-sha-mismatch", good, H0),
                ("record-accepted-false", put("acc2.json", _record(accepted=False)), H1),
                ("record-explicit-absent", os.path.join(tmp, "absent2.json"), H1),
            ]:
                before = open(fl).read() if os.path.isfile(fl) else ""
                code, line = judge(h, 2, DRIFT_LINE + "\n", "LOG", record=rec, finding_log=fl, server="srv-a", role=role)
                after = open(fl).read() if os.path.isfile(fl) else ""
                case("[%s] judge %s refuses before any local outcome" % (role, label),
                     code == 2 and line.startswith("REFUSED eligibility: pack record ") and after == before)
                code, line = judge(h, 0, _local_json(h) + "\n", "LOG", record=rec, finding_log=fl, server="srv-a", role=role)
                case("[%s] judge %s refuses even when the local verify accepted" % (role, label),
                     code == 2 and line.startswith("REFUSED eligibility: pack record "))
        code, line = judge(H1, 2, DRIFT_LINE + "\n", "LOG", record=good, finding_log=fl, server="s", role="role-bogus")
        case("judge unknown role refuses", code == 2 and "unknown role" in line)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    bad = [l for l, ok in results if not ok]
    if bad:
        print("ELIGIBILITY SELFTEST FAILED %d of %d cases: %s" % (len(bad), len(results), ", ".join(bad)))
        return 1
    print("ELIGIBILITY SELFTEST OK %d cases (%d must-match, %d must-not-match, roles %s)" % (
        len(results), must_match[0], must_not[0], ",".join(ROLES)))
    return 0


# ---- the command line --------------------------------------------------------------------------------------

def _arg(argv, name, required=True):
    if name in argv:
        i = argv.index(name)
        if i + 1 < len(argv):
            return argv[i + 1]
    if required:
        raise Refused("usage: %s is required" % name)
    return None


def main(argv):
    if argv == ["--selftest"]:
        return selftest()
    if not argv:
        print("REFUSED usage: laya_eligibility.py role|record-path|check-record|judge|--selftest")
        return 2
    cmd, rest = argv[0], argv[1:]
    try:
        if cmd == "role":
            print(ROLE)
            return 0
        if cmd == "record-path":
            p = record_path(_arg(rest, "--run"))
            if p:
                print(p)
            return 0
        if cmd == "check-record":
            h = _arg(rest, "--apr-sha256")
            rec = _arg(rest, "--record")
            reason, info = check_record(rec, h)
            if reason is not None:
                print("REFUSED eligibility: pack record %s" % reason)
                return 2
            print("PACK RECORD OK sha256 %s pack_image %s parity_contract %s" % (h, info["pack_image"], info["parity_contract"]))
            return 0
        if cmd == "judge":
            h = _arg(rest, "--apr-sha256")
            try:
                rc = int(_arg(rest, "--verify-rc"))
            except ValueError:
                raise Refused("usage: --verify-rc must be an integer")
            log = _arg(rest, "--verify-log")
            try:
                with open(log, "r", encoding="utf-8", errors="replace") as f:
                    text = f.read()
            except OSError:
                print("REFUSED eligibility: laya-verify log %s is unreadable" % log)
                return 2
            code, line = judge(h, rc, text, log, record=_arg(rest, "--record", False),
                               finding_log=_arg(rest, "--finding-log", False), server=_arg(rest, "--server", False))
            print(line)
            return code
        raise Refused("usage: unknown subcommand %s" % cmd)
    except Refused as e:
        print("REFUSED %s" % e)
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
