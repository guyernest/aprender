#!/usr/bin/env python3
"""Measure the cross-CPU re-score drift of the Rust pack and verify step (plan 14-20; 14-VERIFICATION gap 2; D-14, D-15).

    python3 -I scripts/laya_deploy/laya_drift.py local [--only LEG[,LEG...]] [--repeats N] [--out DIR]
    python3 -I scripts/laya_deploy/laya_drift.py aws-plan --source smoke:TASK_ID [--source motivation:TASK_ID]
                                                  [--state-dir DIR] [--script FILE] [--results DIR]
    python3 -I scripts/laya_deploy/laya_drift.py collect [--local FILE] [--graviton DIR] [--out FILE] [--draft FILE]
    python3 -I scripts/laya_deploy/laya_drift.py bashrs-gate FILE [FILE...]
    python3 -I scripts/laya_deploy/laya_drift.py --selftest

The pack bound of laya-parity-v1 A1 is max(1e-5, 4 x a noise figure measured on the TRAINING device). Two live
observations say it does not hold across CPUs; this harness turns that into measurements. It never amends a
contract and never edits a decide crate (D-15): it produces numbers. It makes NO aws call itself.

`aws-plan` WRITES the owner-run Graviton script (default models/laya-trainer/drift-graviton.sh, gitignored: it holds
the account id) from outputs-dev.json and prints the owner's commands; the legs are the script's arguments
(LEG:REPEATS). Every object the script writes under runs/drift-LEG/UUID/ is written by `s3api copy-object
--tagging-directive REPLACE --tagging laya-retention=expire` or `s3api put-object --tagging laya-retention=expire`
(the lifecycle rule only removes expire-tagged objects under runs/, and a default copy inherits the SOURCE's tag, so
a copy of the kept smoke run would otherwise persist forever), and every key's tag is read back before the pack task
starts. The script is judged before it is written: the static policy table (static_violations) and bashrs-gate.

`bashrs-gate` is the repository's bashrs judgement for a file git does not list (a gitignored, generated script):
the first command must be exactly `set -euo pipefail`, then scripts/check_bashrs_gate.sh runs over a temporary git
repository holding a copy (that gate enumerates git ls-files). Exit 0 pass, 1 findings, 2 environment (no bashrs,
an empty or unreadable file, the gate's own vacuity). A bare `bashrs lint` exit is not the verdict.

`collect` merges the local evidence, the Graviton result directories (pack-outcome.json packed_line and verify_line,
read by the same parser) and the recorded references into 14-DRIFT-EVIDENCE.json and a table for the 14-21 brief;
it scrubs twelve-digit runs and ARNs from every string and refuses to write when one remains.

`local` runs legs on THIS machine. A leg is one retained run, measured by the pack_laya example binary:

    kind verify   pack_laya verify APR --run RUN --data DATA --base BASE    (a retained, accepted .apr)
    kind pack     pack_laya pack --run RUN --data DATA --base BASE --out S  (no .apr exists; S is a scratch file)

The legs it knows are LEGS below: the CUDA-trained smoke-stance-64 (verify) and motivation-v1-sm (pack), and the
MPS-trained laya-motivation-v1, laya-stance-64 and laya-antisemitism-v1 (verify). An MPS run's data directory is
FOUND, never guessed: the one whose task.json, train.jsonl, eval.jsonl (and shift.jsonl when the run has one) hash
to the run's gate-report.json inputs_sha256; a leg without such a directory is recorded not measurable.

Verification Discipline (CLAUDE.md 1, 2, 3, 6), applied:
  * the binary is PINNED: built with `cargo build --release -p aprender-decide --example pack_laya
    --message-format=json-render-diagnostics`, its path taken from cargo's compiler-artifact message (a gitignored
    cargo config redirects the target dir, so a guessed path could name a stale file), its sha256 and the commit
    recorded. A dirty source closure (every in-tree crate `cargo tree` lists for aprender-decide, the two contracts
    the binary reads at run time, Cargo.lock) is refused before anything is built (`REFUSED drift-dirty`, exit 2);
  * the child runs from an explicit argv (no shell), stdout and stderr go to files, and the return code is read
    from the child object, never through a pipe;
  * the positive control: the smoke-stance-64 leg must reproduce the refusal recorded in 14-SMOKE-EVIDENCE.json
    (RescoreDrift fine_tuned row 199 max_abs 0.00003439188003540039 bound 0.00003259039634251337) exactly, or the
    run aborts `REFUSED drift-control` and no number is recorded.

THE PARSER reads the last relevant line of a child's stdout (one starting PACKED, REFUSED or `{`):

    PACKED ... rescore_max_abs=.. zs_rescore_max_abs=.. rescore_bound=.. zs_rescore_bound=.. noise=.. zs_noise=..
    {"artifact_sha256":..,"deploy_eligible":..,"rescore_max_abs":..,...}          (the verify JSON line)
    REFUSED RescoreDrift which=SET row=N max_abs=X bound=B (nothing written)       (the FIRST drifted set only)
    REFUSED <OtherRule> ...                                                         -> another_rule, NOT a measurement
    anything else, a truncated line, a non-numeric value, an empty log              -> unparsed

and the class must agree with the return code (accepted 0, drift 2, another_rule 2 or 3), or it is unparsed.
Numbers are kept as the exact decimal text printed and as floats.

Refusals are one `REFUSED drift-<rule>: <detail>` line on stderr, exit 2. stdlib only.
"""

import hashlib
import json
import math
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PHASE_DIR = ".planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out"
SMOKE_EVIDENCE = PHASE_DIR + "/14-SMOKE-EVIDENCE.json"
RULES = ("usage", "leg", "out", "dirty", "build", "base", "control", "evidence", "plan")
EVIDENCE_SCHEMA = "laya-drift-evidence-v1"
GATE_CONTRACTS = ("contracts/laya-finetune-gate-v1.yaml", "contracts/laya-parity-v1.yaml")
CHILD_TIMEOUT_S = 3 * 3600
# The positive control, as the plan states it. The 14-SMOKE-EVIDENCE.json line must parse to exactly this too.
POSITIVE = {"which": "fine_tuned", "row": 199, "max_abs": "0.00003439188003540039", "bound": "0.00003259039634251337"}

# kind, trained_on, run dir, data dir (None: found by input hash), apr (None: pack into a scratch file), repeats
LEGS = {
    "smoke-stance-64": dict(kind="verify", trained_on="cuda:0 (A10G, SageMaker ml.g5.xlarge)",
                            run="models/decide/smoke-stance-64/run", data="models/decide/smoke-stance-64/data",
                            apr="models/decide/smoke-stance-64/smoke-stance-64.apr", repeats=3),
    "motivation-v1-sm": dict(kind="pack", trained_on="cuda:0 (A10G, SageMaker ml.g5.xlarge)",
                             run="models/decide/motivation-v1-sm/run", data="models/decide/motivation-v1-sm/data",
                             apr=None, repeats=3),
    "laya-motivation-v1": dict(kind="verify", trained_on="mps:0", run="models/decide/laya-motivation-v1", data=None,
                               apr="models/decide/laya-motivation-v1.apr", repeats=1),
    "laya-stance-64": dict(kind="verify", trained_on="mps:0", run="models/decide/laya-stance-64", data=None,
                           apr="models/decide/laya-stance-64.apr", repeats=1),
    "laya-antisemitism-v1": dict(kind="verify", trained_on="mps:0", run="models/decide/laya-antisemitism-v1", data=None,
                                 apr="models/decide/laya-antisemitism-v1.apr", repeats=1),
}
DATA_CANDIDATE_GLOBS = ("data/decide/*", "models/decide/*/data")
INPUT_FILES = (("task_json", "task.json"), ("train_jsonl", "train.jsonl"), ("eval_jsonl", "eval.jsonl"),
               ("shift_jsonl", "shift.jsonl"))


class Refused(Exception):
    def __init__(self, rule, msg):
        super().__init__(rule)
        self.rule = rule
        self.msg = msg


def refuse(rule, msg):
    assert rule in RULES, rule
    raise Refused(rule, msg)


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def tidy_path(p):
    """A path as recorded: repo-relative when under the checkout, the home directory written ~, else as given."""
    s = str(p)
    root = str(ROOT) + os.sep
    if s.startswith(root):
        return s[len(root):]
    home = os.path.expanduser("~")
    if home and home != "/" and (s == home or s.startswith(home + os.sep)):
        return "~" + s[len(home):]
    return s


# ---- the parser ---------------------------------------------------------------------------------------------------

NUM = r"-?[0-9]+(?:\.[0-9]+)?(?:[eE][-+]?[0-9]+)?"
SETS = r"fine_tuned|zero_shot|shift_fine_tuned|shift_zero_shot|seed_[0-9]+"
NUM_RE = re.compile(NUM + r"\Z")
PACKED_RE = re.compile(
    r"PACKED (?P<path>\S(?:.*\S)?) sha256=(?P<sha>[0-9a-f]{64}) rescore_max_abs=(?P<ft>%(n)s) zs_rescore_max_abs=(?P<zs>%(n)s)"
    r" rescore_bound=(?P<ftb>%(n)s) zs_rescore_bound=(?P<zsb>%(n)s) noise=(?P<ftn>%(n)s|null) zs_noise=(?P<zsn>%(n)s|null)"
    r" shipped_seed=(?P<seed>-?[0-9]+|null) argmax=(?P<agree>[0-9]+)/(?P<n>[0-9]+) waived=(?P<waived>\S+)\Z" % {"n": NUM})
DRIFT_RE = re.compile(r"REFUSED RescoreDrift which=(?P<which>%s) row=(?P<row>[0-9]+) max_abs=(?P<max>%s) bound=(?P<bound>%s)"
                      r" \(nothing written\)\Z" % (SETS, NUM, NUM))
OTHER_RE = re.compile(r"REFUSED (?P<rule>[A-Z][A-Za-z0-9]*) (?P<detail>\S.*)\Z")
VERIFY_KEYS = ("artifact_sha256", "deploy_eligible", "rescore_max_abs", "zs_rescore_max_abs", "rescore_bound",
               "zs_rescore_bound", "noise", "zs_noise", "argmax")


class Unparsed(Exception):
    pass


def num(text):
    """{"text": the exact decimal printed, "value": its float}. Anything not a finite decimal is Unparsed."""
    if not isinstance(text, str) or not NUM_RE.match(text):
        raise Unparsed("not a decimal number: %r" % (str(text)[:40],))
    v = float(text)
    if not math.isfinite(v):
        raise Unparsed("not finite: %r" % text[:40])
    return {"text": text, "value": v}


def opt_num(text):
    return None if text in (None, "null") else num(text)


def strict_json_object(line):
    def pairs(items):
        out = {}
        for k, v in items:
            if k in out:
                raise Unparsed("duplicate key %r" % k)
            out[k] = v
        return out

    def no_constant(name):
        raise Unparsed("non-numeric constant %s" % name)

    try:
        obj = json.loads(line, object_pairs_hook=pairs, parse_float=str, parse_int=str, parse_constant=no_constant)
    except ValueError as e:
        raise Unparsed("not JSON: %s" % str(e)[:60])
    if not isinstance(obj, dict):
        raise Unparsed("not a JSON object")
    return obj


def parse_line(line):
    """One relevant line -> a record dict with class accepted | drift | another_rule (or raises Unparsed)."""
    if line.startswith("PACKED "):
        m = PACKED_RE.match(line)
        if not m:
            raise Unparsed("malformed PACKED line")
        return {"class": "accepted", "mode": "pack", "artifact_sha256": m["sha"],
                "argmax": "%s/%s" % (m["agree"], m["n"]), "deploy_eligible": None,
                "sets": {"fine_tuned": {"max_abs": num(m["ft"]), "bound": num(m["ftb"]), "noise": opt_num(m["ftn"])},
                         "zero_shot": {"max_abs": num(m["zs"]), "bound": num(m["zsb"]), "noise": opt_num(m["zsn"])}}}
    if line.startswith("{"):
        obj = strict_json_object(line)
        missing = [k for k in VERIFY_KEYS if k not in obj]
        if missing:
            raise Unparsed("verify line lacks %s" % ",".join(missing))
        if not isinstance(obj["artifact_sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", obj["artifact_sha256"]):
            raise Unparsed("verify line artifact_sha256 is malformed")
        if not isinstance(obj["deploy_eligible"], bool):
            raise Unparsed("verify line deploy_eligible is not a boolean")
        if not isinstance(obj["argmax"], str) or not re.fullmatch(r"[0-9]+/[0-9]+", obj["argmax"]):
            raise Unparsed("verify line argmax is malformed")
        return {"class": "accepted", "mode": "verify", "artifact_sha256": obj["artifact_sha256"],
                "argmax": obj["argmax"], "deploy_eligible": obj["deploy_eligible"],
                "sets": {"fine_tuned": {"max_abs": num(obj["rescore_max_abs"]), "bound": num(obj["rescore_bound"]),
                                        "noise": opt_num(obj["noise"])},
                         "zero_shot": {"max_abs": num(obj["zs_rescore_max_abs"]), "bound": num(obj["zs_rescore_bound"]),
                                       "noise": opt_num(obj["zs_noise"])}}}
    if line.startswith("REFUSED RescoreDrift"):
        m = DRIFT_RE.match(line)
        if not m:
            raise Unparsed("malformed RescoreDrift refusal")
        return {"class": "drift", "which": m["which"], "row": int(m["row"]),
                "sets": {m["which"]: {"max_abs": num(m["max"]), "bound": num(m["bound"]), "noise": None}}}
    if line.startswith("REFUSED "):
        m = OTHER_RE.match(line)
        if not m:
            raise Unparsed("malformed REFUSED line")
        return {"class": "another_rule", "rule": m["rule"], "text": line[:300]}
    raise Unparsed("not a PACKED, REFUSED or JSON line")


def parse_output(text, rc):
    """A child's whole stdout and its return code -> a record. A refused or unparsed child is never a number."""
    lines = [ln.rstrip("\r") for ln in (text or "").splitlines()]
    relevant = [ln for ln in lines if ln.startswith(("PACKED ", "REFUSED ", "{"))]
    if not relevant:
        return {"class": "unparsed", "why": "no PACKED, REFUSED or JSON line", "rc": rc}
    line = relevant[-1]
    try:
        rec = parse_line(line)
    except Unparsed as e:
        return {"class": "unparsed", "why": str(e), "rc": rc, "line": line[:300]}
    want = {"accepted": (0,), "drift": (2,), "another_rule": (2, 3)}[rec["class"]]
    if rc not in want:
        return {"class": "unparsed", "why": "return code %s does not agree with a %s line" % (rc, rec["class"]),
                "rc": rc, "line": line[:300]}
    rec["rc"] = rc
    rec["line"] = line[:600]
    return rec


def measured_key(rec):
    """What two repeats must share to be identical: the class, the set, the row and every printed number."""
    sets = tuple(sorted((k, v["max_abs"]["text"], v["bound"]["text"], (v.get("noise") or {}).get("text"))
                        for k, v in (rec.get("sets") or {}).items()))
    return (rec["class"], rec.get("rule"), rec.get("which"), rec.get("row"), rec.get("artifact_sha256"), sets)


PARSER_CASES = [
    # (label, stdout, rc, expected class, extra check)
    ("a PACKED line", "loading\nPACKED /s/x.apr sha256=" + "a" * 64 + " rescore_max_abs=0.000027477741241455078 "
     "zs_rescore_max_abs=0.000015079975128173828 rescore_bound=0.00003259039634251337 zs_rescore_bound=0.00001881216214005832 "
     "noise=0.000008147599085628343 zs_noise=null shipped_seed=23 argmax=459/459 waived=none\n", 0, "accepted",
     lambda r: r["mode"] == "pack" and r["sets"]["fine_tuned"]["max_abs"]["text"] == "0.000027477741241455078"
     and r["sets"]["zero_shot"]["noise"] is None and abs(r["sets"]["fine_tuned"]["bound"]["value"] - 3.259039634251337e-05) < 1e-20),
    ("the recorded verify JSON line", None, 0, "accepted",
     lambda r: r["mode"] == "verify" and r["deploy_eligible"] is True and r["sets"]["zero_shot"]["max_abs"]["text"]
     == "0.000015079975128173828" and r["sets"]["fine_tuned"]["noise"]["text"] == "8.147599085628343e-6"),
    ("a drift refusal, fine_tuned", "REFUSED RescoreDrift which=fine_tuned row=199 max_abs=0.00003439188003540039 "
     "bound=0.00003259039634251337 (nothing written)\n", 2, "drift",
     lambda r: r["which"] == "fine_tuned" and r["row"] == 199 and r["sets"]["fine_tuned"]["max_abs"]["text"] == POSITIVE["max_abs"]),
    ("a drift refusal, zero_shot", "REFUSED RescoreDrift which=zero_shot row=17 max_abs=0.00001728534698486328 bound=0.00001 "
     "(nothing written)\n", 2, "drift", lambda r: r["which"] == "zero_shot" and r["sets"]["zero_shot"]["bound"]["text"] == "0.00001"),
    ("a drift refusal, shift_fine_tuned", "REFUSED RescoreDrift which=shift_fine_tuned row=0 max_abs=1e-4 bound=0.00001 "
     "(nothing written)", 2, "drift", lambda r: r["which"] == "shift_fine_tuned"),
    ("a drift refusal, shift_zero_shot", "REFUSED RescoreDrift which=shift_zero_shot row=3 max_abs=0.5 bound=0.00001 "
     "(nothing written)", 2, "drift", lambda r: r["which"] == "shift_zero_shot"),
    ("a drift refusal, seed_13", "REFUSED RescoreDrift which=seed_13 row=3 max_abs=0.5 bound=0.00001 (nothing written)",
     2, "drift", lambda r: r["which"] == "seed_13"),
    ("another refusal", "REFUSED InputHash data/task.json: sha256 differs from the gate report (nothing written)\n",
     2, "another_rule", lambda r: r["rule"] == "InputHash"),
    ("a gate refusal at exit 3", "REFUSED GateFailed margin below 0.05 (nothing written)", 3, "another_rule",
     lambda r: r["rule"] == "GateFailed"),
    ("the LAST relevant line wins", "PACKED junk\nREFUSED RescoreDrift which=zero_shot row=1 max_abs=0.1 bound=0.01 "
     "(nothing written)\nsome trailing log\n", 2, "drift", lambda r: r["which"] == "zero_shot"),
    # must-not-match
    ("a truncated drift line", "REFUSED RescoreDrift which=fine_tuned row=199 max_abs=0.0000343", 2, "unparsed", None),
    ("a drift line with a non-numeric max_abs", "REFUSED RescoreDrift which=fine_tuned row=199 max_abs=NaN bound=0.00001 "
     "(nothing written)", 2, "unparsed", None),
    ("a drift line naming an unknown set", "REFUSED RescoreDrift which=everything row=1 max_abs=0.1 bound=0.01 "
     "(nothing written)", 2, "unparsed", None),
    ("a truncated PACKED line", "PACKED /s/x.apr sha256=" + "a" * 64 + " rescore_max_abs=0.1", 0, "unparsed", None),
    ("a verify line with a NaN constant", '{"artifact_sha256":"%s","deploy_eligible":true,"rescore_max_abs":NaN,'
     '"zs_rescore_max_abs":0.1,"rescore_bound":0.1,"zs_rescore_bound":0.1,"noise":null,"zs_noise":null,"argmax":"1/1"}'
     % ("b" * 64), 0, "unparsed", None),
    ("a verify line with a string value", '{"artifact_sha256":"%s","deploy_eligible":true,"rescore_max_abs":"abc",'
     '"zs_rescore_max_abs":0.1,"rescore_bound":0.1,"zs_rescore_bound":0.1,"noise":null,"zs_noise":null,"argmax":"1/1"}'
     % ("b" * 64), 0, "unparsed", None),
    ("a verify line missing a key", '{"artifact_sha256":"%s","deploy_eligible":true}' % ("b" * 64), 0, "unparsed", None),
    ("a verify line with a duplicate key", '{"artifact_sha256":"%s","artifact_sha256":"%s"}' % ("b" * 64, "c" * 64),
     0, "unparsed", None),
    ("a truncated verify line", '{"artifact_sha256":"' + "b" * 30, 0, "unparsed", None),
    ("an empty log", "", 2, "unparsed", None),
    ("a log with no relevant line (a panic)", "thread 'main' panicked at src/verify.rs:1:1\n", 101, "unparsed", None),
    ("a PACKED line at exit 1", None, 1, "unparsed", None),
    ("a drift line at exit 0", "REFUSED RescoreDrift which=fine_tuned row=1 max_abs=0.1 bound=0.01 (nothing written)",
     0, "unparsed", None),
    ("a refusal with no detail", "REFUSED InputHash", 2, "unparsed", None),
]


def parser_cases(recorded_verify_line):
    """-> (n_ok, failures) for PARSER_CASES (None stdout = the recorded verify line; the 'exit 1' case reuses row 0)."""
    failures = []
    for label, text, rc, want, check in PARSER_CASES:
        if text is None:
            text = recorded_verify_line if want == "accepted" else PARSER_CASES[0][1]
        rec = parse_output(text, rc)
        ok = rec["class"] == want and (check is None or check(rec))
        if not ok:
            failures.append("%s -> %s (%s)" % (label, rec["class"], rec.get("why", "")))
    return len(PARSER_CASES) - len(failures), failures


# ---- the machine, the binary, the base ---------------------------------------------------------------------------

class Ctx:
    """The ONE place commands run. The selftest swaps `cmd` (captured commands) and `spawn` (the measured child)."""

    def __init__(self, root=ROOT, cmd=None, spawn=None, emit=print, err=None):
        self.root = Path(root)
        self.cmd = cmd or self._cmd
        self.spawn = spawn or self._spawn
        self.emit = emit
        self.err = err or (lambda s: print(s, file=sys.stderr))

    def _cmd(self, argv, timeout=1800):
        p = subprocess.run(argv, cwd=self.root, stdin=subprocess.DEVNULL, capture_output=True, timeout=timeout)
        return p.returncode, p.stdout.decode("utf-8", "replace"), p.stderr.decode("utf-8", "replace")

    def _spawn(self, argv, stdout_path, stderr_path, env, timeout):
        """Run the measured child: explicit argv, no shell, output to files, rc from the child object."""
        with open(stdout_path, "wb") as fo, open(stderr_path, "wb") as fe:
            t0 = time.monotonic()
            child = subprocess.Popen(argv, cwd=self.root, stdin=subprocess.DEVNULL, stdout=fo, stderr=fe, env=env)
            try:
                child.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
            wall = time.monotonic() - t0
        return child.returncode, wall


def host_info(ctx):
    info = {"os": platform.system(), "os_release": platform.release(), "arch": platform.machine(),
            "logical_cpus": os.cpu_count()}
    if platform.system() == "Darwin":
        def sysctl(key):
            rc, out, _ = ctx.cmd(["sysctl", "-n", key], timeout=30)
            return out.strip() if rc == 0 and out.strip() else None
        info["cpu_model"] = sysctl("machdep.cpu.brand_string")
        info["physical_cpus"] = sysctl("hw.physicalcpu")
        info["performance_cores"] = sysctl("hw.perflevel0.physicalcpu")
        info["efficiency_cores"] = sysctl("hw.perflevel1.physicalcpu")
        info["os_version"] = platform.mac_ver()[0]
    else:
        model = None
        try:
            for ln in open("/proc/cpuinfo", encoding="utf-8", errors="replace"):
                key = ln.split(":", 1)[0].strip().lower()
                if key in ("model name", "cpu part", "hardware") and ":" in ln:
                    model = model or ln.split(":", 1)[1].strip()
        except OSError:
            pass
        info["cpu_model"] = model
    return info


def closure_paths(ctx):
    """The binary's in-tree source closure: every path crate `cargo tree` lists for aprender-decide, the contracts
    the binary reads at run time and the lock file."""
    rc, out, err = ctx.cmd(["cargo", "tree", "-p", "aprender-decide", "-e", "normal", "--prefix", "none",
                            "--format", "{p}"], timeout=600)
    if rc != 0:
        refuse("build", "cargo tree -p aprender-decide failed (exit %s): %s" % (rc, err.strip()[-200:]))
    paths = set()
    root = str(ctx.root.resolve())
    for ln in out.splitlines():
        for m in re.finditer(r"\((/[^()]+)\)", ln):
            p = m.group(1).rstrip("/")
            if not (p == root or p.startswith(root + "/")):
                refuse("dirty", "the closure names a path crate outside the checkout (%s): git cannot attest it" % p)
            paths.add(os.path.relpath(p, root))
    if "crates/aprender-decide" not in paths:
        refuse("build", "cargo tree did not list crates/aprender-decide (the closure parse is broken)")
    return sorted(paths) + list(GATE_CONTRACTS) + ["Cargo.lock"]


def binary_identity(ctx, logdir):
    """Refuse a dirty closure, build the example, take its path from cargo's own message, hash it."""
    paths = closure_paths(ctx)
    rc, out, err = ctx.cmd(["git", "status", "--porcelain", "--untracked-files=all", "--"] + paths, timeout=120)
    if rc != 0:
        refuse("dirty", "git status failed (exit %s): %s" % (rc, err.strip()[-200:]))
    dirty = [ln[3:] for ln in out.splitlines() if ln.strip()]
    if dirty:
        refuse("dirty", "the pack_laya source closure is dirty (%s); commit or restore it so the binary measured is "
                        "the one the commit names" % ", ".join(dirty[:5]))
    rc, head, _ = ctx.cmd(["git", "rev-parse", "HEAD"], timeout=60)
    head = head.strip()
    if rc != 0 or not re.fullmatch(r"[0-9a-f]{40}", head):
        refuse("build", "git rev-parse HEAD failed")
    argv = ["cargo", "build", "--release", "-p", "aprender-decide", "--example", "pack_laya",
            "--message-format=json-render-diagnostics"]
    rc, out, err = ctx.cmd(argv, timeout=3600)
    (Path(logdir) / "cargo-build.jsonl").write_text(out)
    (Path(logdir) / "cargo-build.stderr").write_text(err)
    if rc != 0:
        refuse("build", "cargo build failed (exit %s): %s" % (rc, err.strip()[-300:]))
    exe, fresh = None, None
    for ln in out.splitlines():
        try:
            msg = json.loads(ln)
        except ValueError:
            continue
        t = msg.get("target") or {}
        if (msg.get("reason") == "compiler-artifact" and t.get("name") == "pack_laya" and "example" in (t.get("kind") or [])
                and msg.get("executable")):
            exe, fresh = msg["executable"], msg.get("fresh")
    if not exe or not os.path.isfile(exe):
        refuse("build", "cargo printed no compiler-artifact message with an executable for the pack_laya example")
    return {"path": tidy_path(exe), "abs_path": exe, "sha256": sha256_file(exe), "commit": head,
            "closure_dirty": False, "closure_checked": paths, "cargo_fresh": fresh, "build_argv": argv}


def base_dir(ctx):
    rc, out, err = ctx.cmd(["just", "--evaluate", "laya_model_dir"], timeout=60)
    base = out.strip()
    if rc != 0 or not base:
        refuse("base", "just --evaluate laya_model_dir failed (exit %s): %s" % (rc, err.strip()[-200:]))
    if not os.path.isfile(os.path.join(base, "model.safetensors")):
        refuse("base", "the base snapshot %s holds no model.safetensors" % tidy_path(base))
    return base


# ---- legs --------------------------------------------------------------------------------------------------------

def input_hashes(run_dir):
    rep = json.loads((Path(run_dir) / "gate-report.json").read_text(encoding="utf-8"))
    ins = rep.get("inputs_sha256")
    if not isinstance(ins, dict):
        raise ValueError("gate-report.json has no inputs_sha256")
    return ins, rep.get("device_used")


def data_matches(data_dir, ins):
    for key, fname in INPUT_FILES:
        want = ins.get(key)
        p = Path(data_dir) / fname
        if want is None:
            continue
        if not p.is_file() or sha256_file(p) != want:
            return False
    return True


def resolve_leg(ctx, name, base_sha):
    """-> (spec dict, None) or (None, reason): the leg's paths, its data directory proven by hash, the base check."""
    spec = dict(LEGS[name])
    spec["leg"] = name
    run = ctx.root / spec["run"]
    if not (run / "gate-report.json").is_file():
        return None, "the run directory %s has no gate-report.json" % spec["run"]
    try:
        ins, device = input_hashes(run)
    except (OSError, ValueError) as e:
        return None, "the run's gate-report.json cannot be read: %s" % e
    spec["device_used"] = device
    if base_sha is not None and ins.get("base_model") not in (None, base_sha):
        return None, "the run was trained on another base (base_model %s, this base %s)" % (ins.get("base_model"), base_sha)
    if spec["apr"] is not None and not (ctx.root / spec["apr"]).is_file():
        return None, "the artifact %s does not exist" % spec["apr"]
    if spec["data"] is not None:
        if not data_matches(ctx.root / spec["data"], ins):
            return None, "the data directory %s does not hash to the run's inputs_sha256" % spec["data"]
        spec["data_found_by"] = "fixed path, hashes verified against gate-report.json inputs_sha256"
        spec["data_candidates"] = [spec["data"]]
    else:
        found = []
        for g in DATA_CANDIDATE_GLOBS:
            for d in sorted(ctx.root.glob(g)):
                if d.is_dir() and data_matches(d, ins):
                    found.append(os.path.relpath(d, ctx.root))
        if not found:
            return None, "no data directory hashes to the run's inputs_sha256 (searched %s)" % ", ".join(DATA_CANDIDATE_GLOBS)
        spec["data"] = found[0]
        spec["data_found_by"] = "the first directory (sorted) whose input files hash to gate-report.json inputs_sha256"
        spec["data_candidates"] = found
    spec["inputs_sha256"] = ins
    return spec, None


def leg_argv(binary, spec, base, scratch_apr=None, data=None):
    run, data = str(ROOT / spec["run"]), str(ROOT / (data or spec["data"]))
    if spec["kind"] == "verify":
        return [binary, "verify", str(ROOT / spec["apr"]), "--run", run, "--data", data, "--base", base]
    return [binary, "pack", "--run", run, "--data", data, "--base", base, "--out", str(scratch_apr)]


def run_once(ctx, binary, spec, base, logdir, label, threads=None, data=None):
    """One measured child -> {n, rc, wall_s, threads, record}. A refused or unparsed child stays a non-number."""
    scratch = None
    if spec["kind"] == "pack":
        scratch = Path(logdir) / (label + ".apr")
        if scratch.exists():
            refuse("out", "the scratch output %s already exists" % tidy_path(scratch))
    env = dict(os.environ)
    env.pop("RAYON_NUM_THREADS", None)
    if threads is not None:
        env["RAYON_NUM_THREADS"] = str(threads)
    so, se = Path(logdir) / (label + ".stdout"), Path(logdir) / (label + ".stderr")
    argv = leg_argv(binary, spec, base, scratch, data)
    rc, wall = ctx.spawn(argv, so, se, env, CHILD_TIMEOUT_S)
    rec = parse_output(so.read_text(encoding="utf-8", errors="replace"), rc)
    out = {"label": label, "rc": rc, "wall_s": round(wall, 2), "threads": "default" if threads is None else threads,
           "record": rec, "stdout": tidy_path(so)}
    if scratch is not None:
        if rec["class"] == "accepted":
            got = sha256_file(scratch) if scratch.is_file() else None
            out["packed_file_sha256_matches"] = got is not None and got == rec.get("artifact_sha256")
            if not out["packed_file_sha256_matches"]:
                out["record"] = {"class": "unparsed", "why": "PACKED names a sha256 the written file does not have",
                                 "rc": rc, "line": rec.get("line")}
        if scratch.is_file():
            scratch.unlink()                    # 807 MB per repeat; the sha256 above is the record
            out["packed_file_removed"] = True
    return out


def leg_summary(repeats):
    recs = [r["record"] for r in repeats]
    keys = {measured_key(r) for r in recs}
    measured = [r for r in recs if r["class"] in ("accepted", "drift")]
    return {"repeats_identical": len(keys) == 1, "distinct_results": len(keys),
            "classes": sorted({r["class"] for r in recs}), "measured_repeats": len(measured),
            "wall_s": [r["wall_s"] for r in repeats]}


def check_positive(rec, reference_line):
    """The positive control: the smoke leg must reproduce the recorded Mac refusal exactly."""
    ref = parse_output(reference_line, 2)
    want = (POSITIVE["which"], POSITIVE["row"], POSITIVE["max_abs"], POSITIVE["bound"])
    got_ref = (ref.get("which"), ref.get("row"), *(ref["sets"][ref["which"]][k]["text"] for k in ("max_abs", "bound"))) \
        if ref["class"] == "drift" else None
    if got_ref != want:
        refuse("control", "14-SMOKE-EVIDENCE.json local_verify.line no longer parses to the recorded refusal %s" % (want,))
    got = (rec.get("which"), rec.get("row"), *(rec["sets"][rec["which"]][k]["text"] for k in ("max_abs", "bound"))) \
        if rec["class"] == "drift" else None
    return got == want, {"expected": dict(POSITIVE), "got": None if got is None else dict(zip(("which", "row", "max_abs", "bound"), got)),
                         "got_class": rec["class"], "reference": SMOKE_EVIDENCE + " local_verify.line"}


def describe(rep):
    rec = rep["record"]
    head = "LEG %s rc=%s wall=%.1fs threads=%s %s" % (rep["label"], rep["rc"], rep["wall_s"], rep["threads"], rec["class"])
    if rec["class"] == "drift":
        s = rec["sets"][rec["which"]]
        return head + " %s row=%s max_abs=%s bound=%s" % (rec["which"], rec["row"], s["max_abs"]["text"], s["bound"]["text"])
    if rec["class"] == "accepted":
        return head + " " + " ".join("%s max_abs=%s bound=%s" % (k, v["max_abs"]["text"], v["bound"]["text"])
                                     for k, v in sorted(rec["sets"].items()))
    if rec["class"] == "another_rule":
        return head + " " + rec["text"][:160]
    return head + " (%s)" % rec.get("why")


def prepare_out(out):
    p = (ROOT / out) if not os.path.isabs(out) else Path(out)
    if p.exists() and (not p.is_dir() or any(p.iterdir())):
        refuse("out", "%s exists and is not an empty directory; choose a new --out" % tidy_path(p))
    p.mkdir(parents=True, exist_ok=True)
    return p


def cmd_local(argv, ctx):
    only, repeats, out = None, None, "models/decide/drift-local"
    it = iter(argv)
    for a in it:
        if a == "--only":
            only = next(it, None)
            if not only:
                refuse("usage", "--only needs LEG[,LEG...]")
        elif a == "--repeats":
            v = next(it, "")
            if not re.fullmatch(r"[1-9]", v):
                refuse("usage", "--repeats must be 1 to 9")
            repeats = int(v)
        elif a == "--out":
            out = next(it, None)
            if not out:
                refuse("usage", "--out needs a directory")
        else:
            refuse("usage", "unknown argument %r" % a[:40])
    names = list(LEGS) if only is None else only.split(",")
    unknown = [n for n in names if n not in LEGS]
    if unknown or len(set(names)) != len(names):
        refuse("leg", "unknown or repeated leg %s (known: %s)" % (",".join(unknown) or "?", ", ".join(LEGS)))
    smoke_ev = json.loads((ctx.root / SMOKE_EVIDENCE).read_text(encoding="utf-8"))
    reference_line = smoke_ev["local_verify"]["line"]
    outdir = prepare_out(out)
    logdir = outdir / "legs"
    logdir.mkdir()
    binary = binary_identity(ctx, outdir)
    base = base_dir(ctx)
    base_sha = sha256_file(os.path.join(base, "model.safetensors"))
    host = host_info(ctx)
    ctx.emit("DRIFT HOST %s %s %s logical=%s; binary %s sha256 %s commit %s"
             % (host.get("cpu_model"), host["os"], host["arch"], host["logical_cpus"], binary["path"], binary["sha256"],
                binary["commit"][:12]))
    legs, not_measurable, positive = [], [], None
    for name in names:
        spec, why = resolve_leg(ctx, name, base_sha)
        if spec is None:
            not_measurable.append({"leg": name, "reason": why})
            ctx.emit("LEG %s NOT MEASURABLE: %s" % (name, why))
            continue
        reps = []
        for i in range(1, (repeats or spec["repeats"]) + 1):
            rep = run_once(ctx, binary["abs_path"], spec, base, logdir, "%s-%d" % (name, i))
            rep["n"] = i
            ctx.emit(describe(rep))
            if name == "smoke-stance-64" and i == 1:
                ok, positive = check_positive(rep["record"], reference_line)
                positive["reproduced"] = ok
                if not ok:
                    refuse("control", "the smoke-stance-64 leg did not reproduce the recorded Mac refusal (expected %s, got "
                                      "%s %s): the harness or the binary is not the one that produced the recorded figure; "
                                      "no number is recorded" % (POSITIVE, rep["record"]["class"], positive["got"]))
                ctx.emit("POSITIVE CONTROL reproduced: fine_tuned row 199 max_abs %s bound %s" % (POSITIVE["max_abs"], POSITIVE["bound"]))
            reps.append(rep)
        legs.append({"leg": name, "kind": spec["kind"], "trained_on": spec["trained_on"], "device_used": spec["device_used"],
                     "run": spec["run"], "data": spec["data"], "data_found_by": spec["data_found_by"],
                     "data_candidates": spec["data_candidates"], "apr": spec["apr"], "repeats": reps,
                     "summary": leg_summary(reps)})
    result = {"host": host, "binary": {k: v for k, v in binary.items() if k != "abs_path"},
              "base": {"path": tidy_path(base), "model_safetensors_sha256": base_sha}, "legs": legs,
              "not_measurable": not_measurable, "positive": positive}
    (outdir / "local-legs.json").write_text(json.dumps(result, indent=2) + "\n")
    ctx.emit("DRIFT LOCAL OK legs=%d (records %s)" % (len(legs), tidy_path(outdir / "local-legs.json")))
    return 0, result


# ---- aws-plan: the owner-run Graviton script ---------------------------------------------------------------------

STATE_DIR_DEFAULT = "models/laya-trainer"
PROFILE, REGION = "ze-kasher-dev", "us-east-1"
SOURCE_RUNS = {"smoke": "smoke-stance-64", "motivation": "motivation-v1-sm"}
OUTPUTS_SCHEMA = "laya-trainer-outputs-v1"
EXPIRE = "laya-retention=expire"
UUID_RE = re.compile(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\Z")
BUCKET_RE = re.compile(r"[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]\Z")
CLUSTER_RE = re.compile(r"arn:aws:ecs:us-east-1:(?P<acct>[0-9]{12}):cluster/[A-Za-z0-9_-]{1,255}\Z")
TASKDEF_RE = re.compile(r"arn:aws:ecs:us-east-1:(?P<acct>[0-9]{12}):task-definition/(?P<famrev>[A-Za-z0-9_-]{1,255}:[0-9]{1,6})\Z")
SUBNETS_RE = re.compile(r"subnet-[0-9a-f]{8,17}(,subnet-[0-9a-f]{8,17}){0,15}\Z")
SG_RE = re.compile(r"sg-[0-9a-f]{8,17}\Z")
SAFE_PATH_RE = re.compile(r"/[A-Za-z0-9._/-]+\Z")
S3API_ALLOWED = ("list-objects-v2", "get-object", "get-object-tagging", "copy-object", "put-object")
AWS_ALLOWED = {("sts", "get-caller-identity"), ("ecs", "describe-task-definition"), ("ecs", "run-task"),
               ("ecs", "describe-tasks")} | {("s3api", v) for v in S3API_ALLOWED}

GRAVITON_TEMPLATE = r'''#!/usr/bin/env bash
set -euo pipefail
# drift-graviton.sh: the owner-run Graviton legs of the cross-CPU re-score drift study (plan 14-20; run in 14-21).
# Generated by `python3 -I scripts/laya_deploy/laya_drift.py aws-plan`; regenerate rather than edit.
#
#   LAYA_DRIFT_APPROVED=graviton bash THIS_FILE LEG:REPEATS [LEG:REPEATS ...]
#
# LEG is one of: @@LEGS@@. REPEATS is 1 to 5. One script serves a tracer stage (smoke:1) and a full stage
# (smoke:2 motivation:3). Each repeat is one paid arm64 Fargate pack task (8 vCPU, about 10 minutes).
#
# Per leg and repeat: a NEW measurement prefix runs/drift-LEG/UUID/ (a fresh uuid each repeat) receives
# server-side copies of only the source run's run/ and data/ training outputs (the entries of its
# training-manifest.json) and the source's outcome.json and training-manifest.json with the task id
# rewritten to the new uuid. The source run is only read. The tag of every key written is read back
# before the pack task starts. The pack task definition then runs once and its results are downloaded
# to RESULTS/LEG-N/ (pack-outcome.json, pack.log, verify.log, describe-tasks.json, task-summary.json).
#
# Retention: this script deletes nothing. Every object it writes AND every object the pack task writes
# under a measurement prefix carries the tag laya-retention=expire, and the bucket's 30 day lifecycle
# rule (expire-unkept-runs) removes them.
#
# Every aws call names the profile and the region below; the caller's account is checked first.
readonly PROFILE='ze-kasher-dev' REGION='us-east-1'
readonly ACCOUNT='@@ACCOUNT@@'
readonly BUCKET='@@BUCKET@@'
readonly CLUSTER='@@CLUSTER@@'
readonly TASKDEF='@@TASKDEF@@'
readonly SUBNETS='@@SUBNETS@@'
readonly SECURITY_GROUP='@@SG@@'
readonly RESULTS='@@RESULTS@@'
readonly CAP_S=2700
POLL_S="${LAYA_DRIFT_POLL_S:-30}"
readonly POLL_S
TAB="$(printf '\t')"
readonly TAB

readonly PY_UUID='import uuid; print(uuid.uuid4())'
readonly PY_ENTRIES='
import json, re, sys
m = json.load(open(sys.argv[1], encoding="utf-8"))
if m.get("schema") != "laya-train-training-manifest-v1" or m.get("task_id") != sys.argv[2] or m.get("complete") is not True:
    sys.exit("REFUSED manifest: not a complete training manifest of task " + sys.argv[2])
ok = re.compile(r"(run|data)(/[A-Za-z0-9][A-Za-z0-9._-]*)+")
paths = []
for e in m.get("entries") or []:
    p = e.get("path") if isinstance(e, dict) else None
    if isinstance(p, str) and p.split("/", 1)[0] in ("run", "data") and "/" in p:
        if not ok.fullmatch(p) or ".." in p.split("/"):
            sys.exit("REFUSED manifest: unsafe entry path " + repr(p[:80]))
        paths.append(p)
if not any(p.startswith("run/") for p in paths) or not any(p.startswith("data/") for p in paths):
    sys.exit("REFUSED manifest: no run/ or no data/ entries")
print("\n".join(sorted(paths)))
'
readonly PY_REWRITE='
import json, sys
src, dst, old, new, schema = sys.argv[1:6]
o = json.load(open(src, encoding="utf-8"))
if not isinstance(o, dict) or o.get("schema") != schema or o.get("task_id") != old:
    sys.exit("REFUSED rewrite: " + src + " is not a " + schema + " of task " + old)
o["task_id"] = new
open(dst, "w", encoding="utf-8").write(json.dumps(o, indent=2) + "\n")
'
readonly PY_OVERRIDES='
import json, sys
task, name, bucket, prefix, digest = sys.argv[1:6]
env = [{"name": "LAYA_TASK_ID", "value": task}, {"name": "LAYA_NAME", "value": name},
       {"name": "LAYA_BUCKET", "value": bucket}, {"name": "LAYA_RUN_PREFIX", "value": prefix},
       {"name": "LAYA_PACK_IMAGE_DIGEST", "value": digest}]
print(json.dumps({"containerOverrides": [{"name": "pack", "environment": env}]}))
'
readonly PY_SUMMARY='
import json, sys
d = json.load(open(sys.argv[1], encoding="utf-8"))
t = (d.get("tasks") or [{}])[0]
c = [x for x in t.get("containers") or [] if x.get("name") == "pack"]
c = c[0] if c else {}
keep = ("lastStatus", "stopCode", "stoppedReason", "createdAt", "startedAt", "stoppingAt", "stoppedAt", "cpu", "memory")
s = {k: t.get(k) for k in keep}
s["container_exit_code"] = c.get("exitCode")
s["container_reason"] = c.get("reason")
open(sys.argv[2], "w", encoding="utf-8").write(json.dumps(s, indent=2) + "\n")
print(c.get("exitCode"))
'

refuse() {
    printf 'REFUSED %s\n' "$*" >&2
    exit 2
}

src_run() {
    case "$1" in
@@SRC_RUN_ARMS@@
        *) return 1 ;;
    esac
}

src_task() {
    case "$1" in
@@SRC_TASK_ARMS@@
        *) return 1 ;;
    esac
}

run_leg() {
    local leg="$1" n="$2" run task uuid name src dst out work rel key got arn status start code f
    run="$(src_run "$leg")"
    task="$(src_task "$leg")"
    uuid="$(python3 -I -c "$PY_UUID")"
    case "$uuid" in [0-9a-f]*-*-*-*-*) ;; *) refuse "uuid: python3 printed no uuid" ;; esac
    name="drift-${leg}"
    src="runs/${run}/${task}/"
    dst="runs/drift-${leg}/${uuid}/"
    out="${RESULTS}/${leg}-${n}"
    work="${out}/work"
    [[ "$work" == /* ]] || refuse "results: ${work} is not an absolute path"
    [[ "$work" != *..* ]] || refuse "results: ${work} contains .."
    mkdir -p "$work"
    printf 'leg %s\nrepeat %s\nsource %s\nprefix %s\ntask_id %s\n' "$leg" "$n" "$src" "$dst" "$uuid" > "${out}/leg.txt"
    : > "${work}/written.txt"
    got="$(aws s3api list-objects-v2 --bucket "$BUCKET" --prefix "$dst" --query 'KeyCount' --output text --profile "$PROFILE" --region "$REGION")"
    [ "$got" = "0" ] || refuse "prefix: ${dst} is not empty"
    aws s3api get-object --bucket "$BUCKET" --key "${src}training-manifest.json" "${work}/training-manifest.json" --profile "$PROFILE" --region "$REGION" > /dev/null
    aws s3api get-object --bucket "$BUCKET" --key "${src}outcome.json" "${work}/outcome.json" --profile "$PROFILE" --region "$REGION" > /dev/null
    python3 -I -c "$PY_ENTRIES" "${work}/training-manifest.json" "$task" > "${work}/entries.txt"
    while IFS= read -r rel; do
        aws s3api copy-object --bucket "$BUCKET" --copy-source "${BUCKET}/${src}${rel}" --key "${dst}${rel}" --tagging-directive REPLACE --tagging 'laya-retention=expire' --profile "$PROFILE" --region "$REGION" > /dev/null
        printf '%s\n' "${dst}${rel}" >> "${work}/written.txt"
    done < "${work}/entries.txt"
    python3 -I -c "$PY_REWRITE" "${work}/training-manifest.json" "${work}/training-manifest.drift.json" "$task" "$uuid" laya-train-training-manifest-v1
    python3 -I -c "$PY_REWRITE" "${work}/outcome.json" "${work}/outcome.drift.json" "$task" "$uuid" laya-train-outcome-v1
    aws s3api put-object --bucket "$BUCKET" --key "${dst}training-manifest.json" --body "${work}/training-manifest.drift.json" --tagging 'laya-retention=expire' --profile "$PROFILE" --region "$REGION" > /dev/null
    printf '%s\n' "${dst}training-manifest.json" >> "${work}/written.txt"
    aws s3api put-object --bucket "$BUCKET" --key "${dst}outcome.json" --body "${work}/outcome.drift.json" --tagging 'laya-retention=expire' --profile "$PROFILE" --region "$REGION" > /dev/null
    printf '%s\n' "${dst}outcome.json" >> "${work}/written.txt"
    while IFS= read -r key; do
        got="$(aws s3api get-object-tagging --bucket "$BUCKET" --key "$key" --query 'TagSet[].[Key,Value]' --output text --profile "$PROFILE" --region "$REGION")"
        [ "$got" = "laya-retention${TAB}expire" ] || refuse "tag: ${key} is not tagged laya-retention=expire (read back: ${got:-no tag}); no task was started"
    done < "${work}/written.txt"
    printf 'TAGS OK %s-%s: %s keys under %s read back laya-retention=expire\n' "$leg" "$n" "$(wc -l < "${work}/written.txt" | tr -d ' ')" "$dst"
    python3 -I -c "$PY_OVERRIDES" "$uuid" "$name" "$BUCKET" "$dst" "$DIGEST" > "${work}/overrides.json"
    arn="$(aws ecs run-task --cluster "$CLUSTER" --task-definition "$TASKDEF" --launch-type FARGATE --platform-version LATEST --network-configuration "awsvpcConfiguration={subnets=[${SUBNETS}],securityGroups=[${SECURITY_GROUP}],assignPublicIp=ENABLED}" --overrides "file://${work}/overrides.json" --query 'tasks[0].taskArn' --output text --profile "$PROFILE" --region "$REGION")"
    case "$arn" in arn:aws:ecs:*) ;; *) refuse "run-task: no task was started (${arn:-empty answer})" ;; esac
    printf '%s\n' "$arn" > "${work}/task-arn.txt"
    start="$SECONDS"
    while :; do
        status="$(aws ecs describe-tasks --cluster "$CLUSTER" --tasks "$arn" --query 'tasks[0].lastStatus' --output text --profile "$PROFILE" --region "$REGION")"
        [ "$status" != "STOPPED" ] || break
        if [ $((SECONDS - start)) -ge "$CAP_S" ]; then
            printf 'TIMEOUT %s-%s: the pack task did not stop within 45 minutes (last status %s); it is still running (its id is in %s)\n' "$leg" "$n" "$status" "${work}/task-arn.txt" >&2
            exit 1
        fi
        sleep "$POLL_S"
    done
    aws ecs describe-tasks --cluster "$CLUSTER" --tasks "$arn" --output json --profile "$PROFILE" --region "$REGION" > "${work}/describe-tasks.json"
    code="$(python3 -I -c "$PY_SUMMARY" "${work}/describe-tasks.json" "${out}/task-summary.json")"
    for f in pack-outcome.json pack.log verify.log pack-manifest.json; do
        if ! aws s3api get-object --bucket "$BUCKET" --key "${dst}${f}" "${out}/${f}" --profile "$PROFILE" --region "$REGION" > /dev/null; then
            printf '  absent: %s%s\n' "$dst" "$f"
        fi
    done
    printf 'LEG DONE %s-%s prefix %s container_exit %s results %s\n' "$leg" "$n" "$dst" "$code" "$out"
}

[ "${LAYA_DRIFT_APPROVED:-}" = "graviton" ] || refuse "approval: set LAYA_DRIFT_APPROVED=graviton (each repeat starts one paid Fargate pack task)"
[ "$#" -ge 1 ] || refuse "usage: LAYA_DRIFT_APPROVED=graviton bash THIS_FILE LEG:REPEATS [LEG:REPEATS ...]"
case "$POLL_S" in '' | *[!0-9]*) refuse "usage: LAYA_DRIFT_POLL_S must be a whole number of seconds" ;; esac
for spec in "$@"; do
    case "${spec%%:*}" in @@LEG_PATTERN@@) ;; *) refuse "usage: unknown leg in ${spec} (known: @@LEGS@@)" ;; esac
    case "${spec#*:}" in [1-5]) ;; *) refuse "usage: repeats must be 1 to 5 in ${spec}" ;; esac
done
caller="$(aws sts get-caller-identity --query Account --output text --profile "$PROFILE" --region "$REGION")"
[ "$caller" = "$ACCOUNT" ] || refuse "account: the caller is not the account that owns the cluster; nothing was read or written"
image="$(aws ecs describe-task-definition --task-definition "$TASKDEF" --query 'taskDefinition.containerDefinitions[?name==`pack`].image | [0]' --output text --profile "$PROFILE" --region "$REGION")"
DIGEST="${image##*@}"
readonly DIGEST
[[ "$DIGEST" =~ ^sha256:[0-9a-f]{64}$ ]] || refuse "image: the deployed pack container image is not digest-pinned"
for spec in "$@"; do
    leg="${spec%%:*}"
    reps="${spec#*:}"
    i=1
    while [ "$i" -le "$reps" ]; do
        n=1
        while [ -e "${RESULTS}/${leg}-${n}" ]; do n=$((n + 1)); done
        run_leg "$leg" "$n"
        i=$((i + 1))
    done
done
printf 'DRIFT GRAVITON DONE %s\n' "$*"
'''


def render_script(cfg):
    """The generated script for validated values (every value is matched by a strict grammar before it is placed
    between single quotes)."""
    legs = sorted(cfg["sources"])
    run_arms = "\n".join("        %s) printf '%%s' '%s' ;;" % (leg, SOURCE_RUNS[leg]) for leg in legs)
    task_arms = "\n".join("        %s) printf '%%s' '%s' ;;" % (leg, cfg["sources"][leg]) for leg in legs)
    rep = {"@@LEGS@@": ", ".join(legs), "@@LEG_PATTERN@@": " | ".join(legs), "@@ACCOUNT@@": cfg["account"],
           "@@BUCKET@@": cfg["bucket"], "@@CLUSTER@@": cfg["cluster"], "@@TASKDEF@@": cfg["taskdef"],
           "@@SUBNETS@@": cfg["subnets"], "@@SG@@": cfg["sg"], "@@RESULTS@@": cfg["results"],
           "@@SRC_RUN_ARMS@@": run_arms, "@@SRC_TASK_ARMS@@": task_arms}
    text = GRAVITON_TEMPLATE
    for k, v in rep.items():
        text = text.replace(k, v)
    assert "@@" not in text, "an unfilled template slot"
    return text


def logical_lines(text):
    """(line number, text) of every non-comment, non-blank line, continuation lines joined."""
    out, buf, start = [], "", 0
    for i, ln in enumerate(text.splitlines(), 1):
        if not buf:
            start = i
        if ln.rstrip().endswith("\\"):
            buf += ln.rstrip()[:-1] + " "
            continue
        full = buf + ln
        buf = ""
        s = full.strip()
        if s and not s.startswith("#"):
            out.append((start, s))
    return out


FORBIDDEN_RE = re.compile(r"\bdelete-object|\bdelete-bucket|--acl\b|put-object-acl|put-object-tagging|put-bucket"
                          r"|(^|[\s;&|(])(rm|mv)\s")


def static_violations(text):
    """The static policy table over the generated text (non-comment lines). -> [(rule, line, detail)]."""
    v = []
    dst_assign = []
    for no, s in logical_lines(text):
        if re.match(r"(local\s+)?dst=", s):
            dst_assign.append((no, s))
        if FORBIDDEN_RE.search(s):
            v.append(("forbidden-command", no, s[:120]))
        if re.search(r"\baws\s+s3\s", s):
            v.append(("s3-high-level", no, s[:120]))
        for m in re.finditer(r"(?:^|[\s(\"$])aws\s+([a-z0-9-]+)\s+([a-z0-9-]+)", s):
            if (m.group(1), m.group(2)) not in AWS_ALLOWED:
                v.append(("aws-verb", no, "%s %s" % (m.group(1), m.group(2))))
            if '--profile "$PROFILE" --region "$REGION"' not in s:
                v.append(("profile-region", no, s[:120]))
        for m in re.finditer(r"--tagging\s+'([^']*)'", s):
            if m.group(1) != EXPIRE:
                v.append(("tag-value", no, "--tagging is %s, not %s" % (m.group(1), EXPIRE)))
        verbs = re.findall(r"\bs3api\s+([a-z0-9-]+)", s)
        if any(x in ("copy-object", "put-object") for x in verbs):
            if not re.search(r'--key\s+"\$\{dst\}', s):
                v.append(("write-prefix", no, "an s3 write whose --key is not under ${dst}: " + s[:100]))
            if "--tagging '" not in s:
                v.append(("put-tag" if "put-object" in verbs else "copy-tag", no, "a write without --tagging"))
            if "copy-object" in verbs and "--tagging-directive REPLACE" not in s:
                v.append(("copy-tag", no, "copy-object without --tagging-directive REPLACE"))
    if [s for _, s in dst_assign] != ['dst="runs/drift-${leg}/${uuid}/"']:
        v.append(("write-prefix", dst_assign[0][0] if dst_assign else 0,
                  "dst must be assigned exactly once, to runs/drift-${leg}/${uuid}/ (found %d)" % len(dst_assign)))
    if not text.startswith("#!/usr/bin/env bash\nset -euo pipefail\n"):
        v.append(("strict-options", 1, "the first command is not set -euo pipefail"))
    return v


def load_outputs(state_dir):
    p = Path(state_dir) / "outputs-dev.json"
    try:
        d = json.loads(p.read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        refuse("plan", "%s cannot be read (%s); run just laya-trainer-infra outputs dev first" % (tidy_path(p), type(e).__name__))
    if not isinstance(d, dict) or d.get("schema") != OUTPUTS_SCHEMA or d.get("env") != "dev" or not isinstance(d.get("trainer"), dict):
        refuse("plan", "%s is not a %s file of env dev" % (tidy_path(p), OUTPUTS_SCHEMA))
    t = d["trainer"]
    vals = {k: t.get(k) for k in ("ClusterArn", "PackTaskDefinitionArn", "SubnetIds", "SecurityGroupId", "BucketName")}
    mc, mt = CLUSTER_RE.match(vals["ClusterArn"] or ""), TASKDEF_RE.match(vals["PackTaskDefinitionArn"] or "")
    if not mc or not mt or mc["acct"] != mt["acct"]:
        refuse("plan", "ClusterArn and PackTaskDefinitionArn must be us-east-1 ECS ARNs of one account")
    if not SUBNETS_RE.match(vals["SubnetIds"] or "") or not SG_RE.match(vals["SecurityGroupId"] or "") \
            or not BUCKET_RE.match(vals["BucketName"] or ""):
        refuse("plan", "SubnetIds, SecurityGroupId or BucketName is malformed")
    return {"account": mc["acct"], "cluster": vals["ClusterArn"], "taskdef": mt["famrev"], "subnets": vals["SubnetIds"],
            "sg": vals["SecurityGroupId"], "bucket": vals["BucketName"]}


def cmd_aws_plan(argv, ctx):
    state, script, results, sources = None, None, None, {}
    it = iter(argv)
    for a in it:
        if a in ("--state-dir", "--script", "--results", "--source"):
            v = next(it, None)
            if not v:
                refuse("usage", "%s needs a value" % a)
            if a == "--source":
                leg, _, task = v.partition(":")
                if leg not in SOURCE_RUNS or not UUID_RE.match(task) or leg in sources:
                    refuse("usage", "--source is LEG:TASK_ID with LEG one of %s (once each) and a lowercase uuid"
                           % ", ".join(SOURCE_RUNS))
                sources[leg] = task
            elif a == "--state-dir":
                state = v
            elif a == "--script":
                script = v
            else:
                results = v
        else:
            refuse("usage", "unknown argument %r" % a[:40])
    if not sources:
        refuse("usage", "at least one --source LEG:TASK_ID is required (legs %s)" % ", ".join(SOURCE_RUNS))
    sd = Path(state or STATE_DIR_DEFAULT)
    sd = sd if sd.is_absolute() else ctx.root / sd
    script = Path(script) if script else sd / "drift-graviton.sh"
    script = script if script.is_absolute() else ctx.root / script
    results = Path(results) if results else sd / "drift-graviton"
    results = results if results.is_absolute() else ctx.root / results
    results = Path(os.path.realpath(results)) if results.exists() else Path(os.path.realpath(results.parent)) / results.name
    if not SAFE_PATH_RE.match(str(results)) or ".." in str(results).split("/"):
        refuse("plan", "the results directory %s must be an absolute path of letters, digits, . _ / -" % tidy_path(results))
    for p in (script, results):
        rp = os.path.realpath(p.parent if not p.exists() else p)
        root = str(ctx.root.resolve())
        if rp == root or rp.startswith(root + os.sep):
            rc, _, _ = ctx.cmd(["git", "check-ignore", "-q", "--no-index", os.path.relpath(str(p), str(ctx.root))], timeout=60)
            if rc != 0:
                refuse("plan", "%s is inside the repository and not gitignored (the script holds account ids)" % tidy_path(p))
    cfg = load_outputs(sd)
    cfg.update(results=str(results), sources=sources)
    text = render_script(cfg)
    bad = static_violations(text)
    if bad:
        refuse("plan", "the generated script fails its own static policy table: %s" % bad[:3])
    script.parent.mkdir(parents=True, exist_ok=True)
    tmp = script.parent / (".%s.tmp-%d" % (script.name, os.getpid()))
    tmp.write_text(text)
    os.chmod(tmp, 0o700)
    rc, lines = bashrs_gate([str(tmp)], ctx=ctx, names={str(tmp): script.name})
    if rc != 0:
        tmp.unlink()
        for ln in lines:
            ctx.err(ln)
        refuse("plan", "the generated script is not bashrs-clean under the repository gate (exit %d); nothing written" % rc)
    os.replace(tmp, script)
    legs = sorted(sources)
    ctx.emit("AWS PLAN OK script %s legs %s (no aws call was made; bashrs-gate PASS, static policy table clean)"
             % (tidy_path(script), ",".join(legs)))
    ctx.emit("OWNER (tracer stage):  LAYA_DRIFT_APPROVED=graviton bash %s %s:1" % (tidy_path(script), legs[-1] if "smoke" not in legs else "smoke"))
    ctx.emit("OWNER (full stage):    LAYA_DRIFT_APPROVED=graviton bash %s %s" % (
        tidy_path(script), " ".join("%s:%d" % (leg, 2 if leg == "smoke" else 3) for leg in sorted(legs, key=lambda x: x != "smoke"))))
    return 0


# ---- bashrs-gate: the repository's bashrs judgement over any shell file -------------------------------------------

def first_command(text):
    """The first line after the shebang that is neither blank nor a comment (None when there is none)."""
    for i, ln in enumerate(text.splitlines()):
        if i == 0 and ln.startswith("#!"):
            continue
        s = ln.strip()
        if s and not s.startswith("#"):
            return ln
    return None


def bashrs_gate(files, ctx=None, env=None, names=None):
    """-> (rc, lines). 0 pass, 1 findings (strict options or SEC/DET/IDEM), 2 environment or vacuity. The verdict is
    scripts/check_bashrs_gate.sh run over a temporary git repository holding a copy of each file (the gate
    enumerates git ls-files, which never lists a gitignored file); its exit code is read from the child."""
    ctx = ctx or Ctx()
    env = dict(env or os.environ)
    names = names or {}
    worst, lines = 0, []
    gate = ctx.root / "scripts" / "check_bashrs_gate.sh"
    for f in files:
        shown = names.get(f, f)
        if not os.path.isfile(f):
            lines.append("REFUSED bashrs-gate ENV: %s: cannot read the file (a gate that cannot read its input never passes)" % shown)
            worst = max(worst, 2)
            continue
        text = open(f, encoding="utf-8", errors="replace").read()
        if not text.strip():
            lines.append("REFUSED bashrs-gate ENV: %s: the file is empty (nothing to judge)" % shown)
            worst = max(worst, 2)
            continue
        first = first_command(text)
        if first != "set -euo pipefail":
            lines.append("REFUSED bashrs-gate strict-options: %s: the first command is %r, not exactly set -euo pipefail"
                         % (shown, (first or "")[:60]))
            worst = max(worst, 1)
            continue
        if shutil.which("bashrs", path=env.get("PATH", "")) is None:
            lines.append("REFUSED bashrs-gate ENV: %s: bashrs is not on PATH, so the gate cannot run" % shown)
            worst = max(worst, 2)
            continue
        tmp = os.path.realpath(tempfile.mkdtemp(prefix="laya-bashrs-gate-"))
        try:
            if not os.path.isabs(tmp) or ".." in tmp.split(os.sep):
                lines.append("REFUSED bashrs-gate ENV: %s: unusable temporary directory" % shown)
                worst = max(worst, 2)
                continue
            base = os.path.basename(f)
            copy = base if base.endswith(".sh") else base + ".sh"
            os.makedirs(os.path.join(tmp, "s"))
            shutil.copyfile(f, os.path.join(tmp, "s", copy))
            for argv in (["git", "init", "-q", tmp], ["git", "-C", tmp, "add", "s/" + copy]):
                if subprocess.run(argv, stdin=subprocess.DEVNULL, capture_output=True, env=env).returncode != 0:
                    lines.append("REFUSED bashrs-gate ENV: %s: %s failed" % (shown, " ".join(argv[:3])))
                    worst = max(worst, 2)
                    break
            else:
                so, se = os.path.join(tmp, "gate.out"), os.path.join(tmp, "gate.err")
                with open(so, "wb") as fo, open(se, "wb") as fe:
                    child = subprocess.Popen(["bash", str(gate), "--root", tmp], cwd=str(ctx.root), stdin=subprocess.DEVNULL,
                                             stdout=fo, stderr=fe, env=env)
                    child.wait(timeout=600)
                rc = child.returncode
                out = open(so, encoding="utf-8", errors="replace").read() + open(se, encoding="utf-8", errors="replace").read()
                out = out.replace(tmp + "/", "")
                if rc == 0:
                    lines.append("BASHRS GATE PASS %s (%s)" % (shown, out.strip().splitlines()[-1][:160] if out.strip() else ""))
                elif rc == 1:
                    for ln in out.splitlines():
                        m = re.match(r"s/(\S+?):([0-9]+) ([A-Z]+[0-9]+)\Z", ln)
                        if m:
                            lines.append("%s:%s %s" % (shown, m.group(2), m.group(3)))
                    lines.append("REFUSED bashrs-gate findings: %s" % shown)
                    worst = max(worst, 1)
                else:
                    lines.append("REFUSED bashrs-gate ENV: %s: the gate exited %s: %s" % (shown, rc, " | ".join(out.strip().splitlines()[-2:])[:300]))
                    worst = max(worst, 2)
        finally:
            shutil.rmtree(tmp, ignore_errors=True)
    return worst, lines


def cmd_bashrs_gate(argv, ctx):
    if not argv or any(a.startswith("-") for a in argv):
        refuse("usage", "bashrs-gate FILE [FILE...]")
    rc, lines = bashrs_gate(argv, ctx=ctx)
    for ln in lines:
        (ctx.emit if ln.startswith("BASHRS GATE PASS") or not ln.startswith("REFUSED") else ctx.err)(ln)
    return rc


# ---- collect: merge the local and Graviton legs with the recorded references --------------------------------------

GATE_RUN_EVIDENCE = PHASE_DIR + "/14-GATE-RUN-EVIDENCE.json"
LOCAL_EVIDENCE = PHASE_DIR + "/14-DRIFT-LOCAL-EVIDENCE.json"
MERGED_EVIDENCE = PHASE_DIR + "/14-DRIFT-EVIDENCE.json"
MERGED_DRAFT = PHASE_DIR + "/14-DRIFT-TABLE.md"
GRAVITON_CPU = "AWS Graviton (ECS Fargate arm64, 8 vCPU, the pack image)"
ID_RE = re.compile(r"\b[0-9]{12}\b")
ARN_RE = re.compile(r"arn:aws[a-zA-Z-]*:[^\s\"',]*")


def scrub(obj):
    """Every string value with a twelve-digit run or an ARN rewritten; keys are left as they are (a key that holds an
    identifier makes the result unwritable, see assert_clean)."""
    if isinstance(obj, dict):
        return {k: scrub(v) for k, v in obj.items()}
    if isinstance(obj, list):
        return [scrub(v) for v in obj]
    if isinstance(obj, str):
        return ID_RE.sub("<account-id>", ARN_RE.sub("<arn>", obj))
    return obj


def assert_clean(text, what):
    if ID_RE.search(text) or "arn:aws" in text:
        refuse("evidence", "%s would still hold an account id or an ARN after the scrub; nothing written" % what)


def ratio(a, b):
    return None if a is None or b in (None, 0) else float("%.6g" % (a / b))


def rows_of(rec, artifact, cpu, step, noise_ref, source, extra=None):
    out = []
    for which, s in sorted((rec.get("sets") or {}).items()):
        noise = (s.get("noise") or {}).get("value")
        noise_from = "printed by pack_laya" if noise is not None else "rescore-noise.json of the run (training device)"
        if noise is None:
            noise = (noise_ref or {}).get(which)
        mx, bd = s["max_abs"]["value"], s["bound"]["value"]
        row = {"artifact": artifact, "cpu": cpu, "step": step, "set": which, "max_abs": s["max_abs"]["text"],
               "bound": s["bound"]["text"], "noise": noise, "noise_from": noise_from, "max_abs_over_bound": ratio(mx, bd),
               "max_abs_over_noise": ratio(mx, noise), "within_bound": mx <= bd, "cell": source,
               "outcome": rec["class"]}
        row.update(extra or {})
        out.append(row)
    return out


def local_rows(local):
    rows = []
    cpu = "%s (%s)" % ((local.get("host") or {}).get("cpu_model"), (local.get("host") or {}).get("arch"))
    for leg in local.get("legs", []):
        reps = [r for r in leg["repeats"] if r["record"]["class"] in ("accepted", "drift")]
        if not reps:
            continue
        rec = reps[0]["record"]
        step = "local %s" % leg["kind"]
        rows += rows_of(rec, leg["leg"], cpu, step, leg.get("noise_reference"), "measured",
                        {"repeats": len(leg["repeats"]), "repeats_identical": leg["summary"]["repeats_identical"],
                         "trained_on": leg["trained_on"]})
    return rows


def graviton_dirs(gdir):
    out = []
    if not gdir.is_dir():
        return out
    for d in sorted(gdir.iterdir()):
        m = re.fullmatch(r"(?P<leg>%s)-(?P<n>[0-9]+)" % "|".join(SOURCE_RUNS), d.name)
        if m and d.is_dir():
            out.append((m["leg"], int(m["n"]), d))
    return out


def graviton_legs(gdir):
    """-> [{leg, n, artifact, pack, verify, exit_code, seconds, task_summary}] from LEG-N result directories."""
    legs = []
    for leg, n, d in graviton_dirs(gdir):
        po_path = d / "pack-outcome.json"
        entry = {"leg": leg, "n": n, "artifact": SOURCE_RUNS[leg], "dir": tidy_path(d)}
        try:
            po = json.loads(po_path.read_text(encoding="utf-8"))
        except (OSError, ValueError) as e:
            entry["error"] = "pack-outcome.json unreadable: %s" % type(e).__name__
            legs.append(entry)
            continue
        ec = po.get("exit_code")
        pl, vl = po.get("packed_line"), po.get("verify_line")
        entry["pack"] = parse_output(pl, 0 if (pl or "").startswith("PACKED") else ec) if pl else None
        entry["verify"] = parse_output(vl, 0 if (vl or "").startswith("{") else ec) if vl else None
        entry.update(exit_code=ec, accepted=po.get("accepted"), pack_seconds=po.get("pack_seconds"),
                     verify_seconds=po.get("verify_seconds"), pack_image_digest=po.get("pack_image_digest"))
        try:
            entry["task_summary"] = json.loads((d / "task-summary.json").read_text(encoding="utf-8"))
        except (OSError, ValueError):
            entry["task_summary"] = None
        legs.append(entry)
    return legs


def references(root):
    smoke = json.loads((root / SMOKE_EVIDENCE).read_text(encoding="utf-8"))
    gate = json.loads((root / GATE_RUN_EVIDENCE).read_text(encoding="utf-8"))
    sv = smoke["smoke"]["verdict"]["pack"]
    lv = smoke["local_verify"]
    rows = []
    rows += rows_of(parse_output(sv["line"], 0), "smoke-stance-64", GRAVITON_CPU, "pack-task verify (14-14)", None,
                    "reference", {"from": SMOKE_EVIDENCE + " smoke.verdict.pack.line"})
    rows += rows_of(parse_output(lv["line"], lv["exit_code"]), "smoke-stance-64", "Apple M4 Pro (arm64)",
                    "local verify (14-14)", None, "reference", {"from": SMOKE_EVIDENCE + " local_verify.line"})
    gp = gate["pack"]
    rows += rows_of(parse_output(gp["line"], gp["rc"]), "motivation-v1-sm", GRAVITON_CPU, "pack-task pack (14-15)",
                    {k: (gate.get("noise") or {}).get(k, {}).get("max_abs") for k in ("fine_tuned", "zero_shot")},
                    "reference", {"from": GATE_RUN_EVIDENCE + " pack.line"})
    return rows


def graviton_rows(legs, noise_refs):
    rows = []
    for g in legs:
        for step, rec in (("pack-task pack", g.get("pack")), ("pack-task verify", g.get("verify"))):
            if rec and rec["class"] in ("accepted", "drift"):
                rows += rows_of(rec, g["artifact"], GRAVITON_CPU, step, noise_refs.get(g["artifact"]), "measured",
                                {"repeat": g["n"]})
    return rows


def determinism(legs):
    out = {}
    for g in legs:
        for step in ("pack", "verify"):
            rec = g.get(step)
            if rec is None:
                continue
            out.setdefault("%s %s" % (g["artifact"], step), set()).add(measured_key(rec) if rec["class"] != "unparsed"
                                                                         else ("unparsed", rec.get("why")))
    return {k: {"distinct_results": len(v), "identical": len(v) == 1} for k, v in sorted(out.items())}


def draft_table(rows):
    head = ("| artifact | cpu | step | set | max_abs | bound | max_abs / bound | noise | max_abs / noise | within | cell |\n"
            "|---|---|---|---|---|---|---|---|---|---|---|\n")
    body = "".join("| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |\n" % (
        r["artifact"], r["cpu"], r["step"], r["set"], r["max_abs"], r["bound"], r["max_abs_over_bound"],
        "%.4g" % r["noise"] if isinstance(r["noise"], float) else r["noise"], r["max_abs_over_noise"],
        "yes" if r["within_bound"] else "NO", r["cell"]) for r in rows)
    return ("# Cross-CPU re-score drift: the table for the 14-21 brief\n\nGenerated by `laya_drift.py collect`. A "
            "`measured` cell was produced by this harness's legs; a `reference` cell is copied from earlier evidence "
            "(its `from` field in 14-DRIFT-EVIDENCE.json names the file). A refusal reports only the FIRST drifted "
            "set, so a refused leg has one row.\n\n" + head + body)


def cmd_collect(argv, ctx):
    local, gdir, out, draft = LOCAL_EVIDENCE, STATE_DIR_DEFAULT + "/drift-graviton", MERGED_EVIDENCE, MERGED_DRAFT
    it = iter(argv)
    for a in it:
        v = next(it, None) if a in ("--local", "--graviton", "--out", "--draft") else None
        if a == "--local" and v:
            local = v
        elif a == "--graviton" and v:
            gdir = v
        elif a == "--out" and v:
            out = v
        elif a == "--draft" and v:
            draft = v
        else:
            refuse("usage", "collect [--local FILE] [--graviton DIR] [--out FILE] [--draft FILE]")
    absp = lambda p: Path(p) if os.path.isabs(p) else ctx.root / p
    try:
        loc = json.loads(absp(local).read_text(encoding="utf-8"))
    except (OSError, ValueError) as e:
        refuse("evidence", "the local evidence %s cannot be read (%s)" % (local, type(e).__name__))
    if loc.get("schema") != EVIDENCE_SCHEMA:
        refuse("evidence", "%s is not a %s file" % (local, EVIDENCE_SCHEMA))
    noise_refs = {leg["leg"]: leg.get("noise_reference") for leg in loc.get("legs", [])}
    glegs = graviton_legs(absp(gdir))
    rows = local_rows(loc) + graviton_rows(glegs, noise_refs) + references(ctx.root)
    merged = {"schema": EVIDENCE_SCHEMA, "kind": "merged", "plan": "14-20 collect",
              "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
              "cells": "measured = produced by this harness (local legs from the local evidence, Graviton legs from the "
                       "result directories); reference = copied from 14-SMOKE-EVIDENCE.json or 14-GATE-RUN-EVIDENCE.json",
              "local_evidence": tidy_path(absp(local)), "host": loc.get("host"), "binary": loc.get("binary"),
              "rows": rows, "graviton_legs": glegs, "graviton_determinism": determinism(glegs),
              "local_controls": loc.get("controls"), "a10g_noise": loc.get("references", {}).get("a10g_noise"),
              "not_measured": ["x86_64 CPU: there is no x86_64 pack binary or image (new infrastructure, not planned)"]
              + [n.get("leg") + ": " + n.get("reason", "") for n in loc.get("not_measurable", [])]}
    merged = scrub(merged)
    text = json.dumps(merged, indent=2, default=lambda o: sorted(o) if isinstance(o, set) else str(o)) + "\n"
    table = draft_table(merged["rows"])
    assert_clean(text, out)
    assert_clean(table, draft)
    absp(out).write_text(text)
    absp(draft).write_text(table)
    ctx.emit("DRIFT COLLECT OK rows=%d graviton_legs=%d -> %s, %s" % (len(rows), len(glegs), out, draft))
    return 0


# ---- the fake aws (selftest only): canned answers and a key -> tags store with S3 copy semantics -------------------

FAKE_AWS = r'''
import json, os, sys
st_path = os.environ["FAKE_AWS_STATE"]
with open(st_path) as fh:
    st = json.load(fh)
argv = sys.argv[1:]
st["calls"].append(argv)


def save():
    with open(st_path, "w") as fh:
        json.dump(st, fh)


def opt(name, default=None):
    if name in argv:
        i = argv.index(name)
        return argv[i + 1] if i + 1 < len(argv) else default
    return default


def die(msg, rc=254):
    save()
    sys.stderr.write(msg + "\n")
    sys.exit(rc)


def tags_of(spec):
    out = {}
    for kv in (spec or "").split("&"):
        if kv:
            k, _, v = kv.partition("=")
            out[k] = v
    return out


def done(text=""):
    save()
    if text != "":
        print(text)
    sys.exit(0)


if opt("--profile") != "ze-kasher-dev" or opt("--region") != "us-east-1":
    die("fake aws: --profile ze-kasher-dev and --region us-east-1 are required", 97)
svc, verb = (argv + ["", ""])[:2]
store = st["store"]
if (svc, verb) == ("sts", "get-caller-identity"):
    done(st["account"])
if (svc, verb) == ("ecs", "describe-task-definition"):
    done(st["image"])
if svc == "s3api" and verb in ("list-objects-v2", "get-object", "copy-object", "put-object", "get-object-tagging"):
    if opt("--bucket") != st["bucket"]:
        die("An error occurred (NoSuchBucket)")
    key = opt("--key")
    if verb == "list-objects-v2":
        done(str(sum(1 for k in store if k.startswith(opt("--prefix")))))
    if verb == "get-object":
        if key not in store:
            die("An error occurred (NoSuchKey) when calling the GetObject operation")
        with open(argv[-5] if argv[-4] == "--profile" else argv[-1], "w") as fh:
            fh.write(store[key]["body"])
        done("{}")
    if verb == "copy-object":
        b, _, sk = opt("--copy-source", "").partition("/")
        if b != st["bucket"] or sk not in store:
            die("An error occurred (NoSuchKey) when calling the CopyObject operation")
        tags = tags_of(opt("--tagging")) if opt("--tagging-directive", "COPY") == "REPLACE" else dict(store[sk]["tags"])
        store[key] = {"body": store[sk]["body"], "tags": tags}
        done("{}")
    if verb == "put-object":
        with open(opt("--body")) as fh:
            store[key] = {"body": fh.read(), "tags": tags_of(opt("--tagging"))}
        done("{}")
    if key not in store:
        die("An error occurred (NoSuchKey) when calling the GetObjectTagging operation")
    done("\n".join("%s\t%s" % kv for kv in sorted(store[key]["tags"].items())))
if (svc, verb) == ("ecs", "run-task"):
    ov = opt("--overrides", "")
    if ov.startswith("file://"):
        with open(ov[7:]) as fh:
            ov = fh.read()
    env = {e["name"]: e["value"] for e in json.loads(ov)["containerOverrides"][0]["environment"]}
    prefix, name = env["LAYA_RUN_PREFIX"], env["LAYA_NAME"]
    oc = json.loads(store.get(prefix + "outcome.json", {"body": "{}"})["body"])
    ok = oc.get("task_id") == env["LAYA_TASK_ID"] and prefix == "runs/%s/%s/" % (name, env["LAYA_TASK_ID"])
    line = st["canned"].get(name) if ok else "REFUSED env: the run prefix does not hold this task"
    po = {"schema": "laya-train-pack-outcome-v1", "task_id": env["LAYA_TASK_ID"], "exit_code": 0 if ok and line.startswith("PACKED") else 2,
          "accepted": False, "packed_line": line, "verify_line": None, "pack_seconds": 120.5, "verify_seconds": None,
          "pack_image_digest": env["LAYA_PACK_IMAGE_DIGEST"]}
    for rel, body in (("pack-outcome.json", json.dumps(po)), ("pack.log", line + "\n"), ("pack-manifest.json", "[]")):
        store[prefix + rel] = {"body": body, "tags": {"laya-retention": "expire"}}   # run_step.py tags its uploads
    arn = "arn:aws:ecs:us-east-1:%s:task/c/%032x" % (st["account"], len(st["tasks"]) + 1)
    st["tasks"][arn] = {"polls": 0}
    done(arn)
if (svc, verb) == ("ecs", "describe-tasks"):
    arn = opt("--tasks")
    t = st["tasks"][arn]
    if opt("--query"):
        t["polls"] += 1
        done("RUNNING" if t["polls"] == 1 else "STOPPED")
    done(json.dumps({"tasks": [{"taskArn": arn, "lastStatus": "STOPPED", "stopCode": "EssentialContainerExited",
                                "stoppedReason": "Essential container in task exited", "createdAt": "2026-10-30T10:00:00Z",
                                "stoppedAt": "2026-10-30T10:05:00Z", "containers": [{"name": "pack", "exitCode": 2}]}]}))
die("fake aws: unexpected call: " + " ".join(argv), 97)
'''


def tag_violations(calls, store):
    """THE tag oracle, used by every case: (a) the arguments of every write into runs/drift-, (b) the end-state tags of
    every key under runs/drift- (state, not arguments), (c) a tag read-back of every written key before the next
    run-task. -> [violation text]."""
    v = []

    def opt(argv, name):
        return argv[argv.index(name) + 1] if name in argv and argv.index(name) + 1 < len(argv) else None

    writes = []
    for i, c in enumerate(calls):
        if c[:2] in (["s3api", "copy-object"], ["s3api", "put-object"]) and (opt(c, "--key") or "").startswith("runs/drift-"):
            key, tag = opt(c, "--key"), opt(c, "--tagging")
            writes.append((i, key))
            if tag is None:
                v.append("%s without --tagging %s: %s" % (c[1], EXPIRE, key))
            elif tag != EXPIRE:
                v.append("%s --tagging is %s, not %s: %s" % (c[1], tag, EXPIRE, key))
            if c[1] == "copy-object" and opt(c, "--tagging-directive") != "REPLACE":
                v.append("copy-object without --tagging-directive REPLACE: %s" % key)
    for k in sorted(store):
        if k.startswith("runs/drift-") and store[k]["tags"] != {"laya-retention": "expire"}:
            v.append("end-state tag of %s is %s" % (k, store[k]["tags"] or "none"))
    runs = [i for i, c in enumerate(calls) if c[:2] == ["ecs", "run-task"]]
    for i, key in writes:
        nxt = next((j for j in runs if j > i), None)
        if nxt is None:
            continue
        if not any(calls[j][:2] == ["s3api", "get-object-tagging"] and opt(calls[j], "--key") == key for j in range(i + 1, nxt)):
            v.append("no tag read-back of %s before the run-task that follows it" % key)
    return v


# ---- entry -------------------------------------------------------------------------------------------------------

def main(argv, ctx=None):
    ctx = ctx or Ctx()
    try:
        if not argv:
            refuse("usage", "a subcommand is required: local, aws-plan, collect, bashrs-gate")
        if argv[0] == "local":
            return cmd_local(argv[1:], ctx)[0]
        if argv[0] == "aws-plan":
            return cmd_aws_plan(argv[1:], ctx)
        if argv[0] == "collect":
            return cmd_collect(argv[1:], ctx)
        if argv[0] == "bashrs-gate":
            return cmd_bashrs_gate(argv[1:], ctx)
        refuse("usage", "unknown subcommand %r" % argv[0][:40])
    except Refused as e:
        ctx.err("REFUSED drift-%s: %s" % (e.rule, e.msg))
        return 2


# ---- selftest ----------------------------------------------------------------------------------------------------

FAKE_PACK = r'''#!/bin/sh
# a fake pack_laya: prints $FAKE_LINE (when set) and exits $FAKE_RC
[ -n "${FAKE_LINE:-}" ] && printf '%s\n' "noise before the verdict" "$FAKE_LINE"
printf '%s\n' "${RAYON_NUM_THREADS:-unset}" > "${FAKE_THREADS_FILE:-/dev/null}"
exit "${FAKE_RC:-0}"
'''


T_SMOKE = "00000000-0000-4000-8000-00000000a001"
T_MOT = "00000000-0000-4000-8000-00000000a002"
FIX_ACCOUNT = "111122223333"
SEC010_LINES = ['#!/usr/bin/env bash', 'set -euo pipefail', 'REPO="${REPO_ROOT:-}"',
                'while [ $# -gt 0 ]; do case "$1" in --repo) REPO="$2"; shift;; esac; shift; done',
                'rc=0; ( cd "$REPO" && timeout 60 cargo test --no-run ) > out.log 2>&1 || rc=$?', 'echo "$rc"']
DET002_GATE_LINES = ('STAMP=$(date +%%s%%N)', 'cp build.log "out/report_$STAMP.log"')   # as printf writes them in the gate


def fixture_state(work):
    sd = work / "state"
    sd.mkdir()
    (sd / "outputs-dev.json").write_text(json.dumps({"schema": OUTPUTS_SCHEMA, "env": "dev", "trainer": {
        "ClusterArn": "arn:aws:ecs:us-east-1:%s:cluster/aprender-laya-train-dev" % FIX_ACCOUNT,
        "PackTaskDefinitionArn": "arn:aws:ecs:us-east-1:%s:task-definition/aprender-laya-pack-dev:3" % FIX_ACCOUNT,
        "SubnetIds": "subnet-0aaaaaaaaaaaaaaa1,subnet-0bbbbbbbbbbbbbbb2", "SecurityGroupId": "sg-0ccccccccccccccc3",
        "BucketName": "fixture-laya-bucket"}}))
    return sd


def fixture_store():
    """Two retained source runs: smoke tagged KEEP (a tag-inheriting copy is observable), motivation tagged expire."""
    store = {}
    for leg, task, tag in (("smoke", T_SMOKE, "keep"), ("motivation", T_MOT, "expire")):
        run = SOURCE_RUNS[leg]
        pre = "runs/%s/%s/" % (run, task)
        files = {"run/gate-report.json": '{"pass": true}', "run/checkpoint/model.safetensors": "weights-" + leg,
                 "data/task.json": '{"labels": ["a", "b"]}', "data/eval.jsonl": '{"text": "x"}\n',
                 "verdict.json": '{"phase": "completed"}', run + ".apr": "apr-bytes", "train.log": "epoch 1\n"}
        entries = [{"path": p, "size": len(b), "sha256": hashlib.sha256(b.encode()).hexdigest()} for p, b in sorted(files.items())]
        files["training-manifest.json"] = json.dumps({"schema": "laya-train-training-manifest-v1", "task_id": task,
                                                      "complete": True, "entries": entries})
        files["outcome.json"] = json.dumps({"schema": "laya-train-outcome-v1", "task_id": task, "exit_class": "pass",
                                            "device_ok": True})
        for p, b in files.items():
            store[pre + p] = {"body": b, "tags": {"laya-retention": tag}}
    return store


CANNED = {"drift-smoke": "PACKED /opt/ml/work/out/drift-smoke.apr sha256=" + "e" * 64 + " rescore_max_abs=0.000027477741241455078 "
                         "zs_rescore_max_abs=0.000015079975128173828 rescore_bound=0.00003259039634251337 "
                         "zs_rescore_bound=0.00001881216214005832 noise=0.000008147599085628343 zs_noise=0.00000470304053501458 "
                         "shipped_seed=23 argmax=459/459 waived=none",
          "drift-motivation": "REFUSED RescoreDrift which=zero_shot row=17 max_abs=0.00001728534698486328 bound=0.00001 (nothing written)"}


def run_fake(work, script, args, approved=True, account=FIX_ACCOUNT, tag="r"):
    stf = work / ("fake-state-%s.json" % tag)
    stf.write_text(json.dumps({"calls": [], "store": fixture_store(), "account": account, "bucket": "fixture-laya-bucket",
                               "image": "%s.dkr.ecr.us-east-1.amazonaws.com/aprender-laya-pack-dev@sha256:%s" % (FIX_ACCOUNT, "5" * 64),
                               "canned": CANNED, "tasks": {}}))
    results = work / "results"
    shutil.rmtree(results, ignore_errors=True)
    env = dict(os.environ, PATH="%s:%s" % (work / "fakebin", os.environ.get("PATH", "")), FAKE_AWS_STATE=str(stf),
               LAYA_DRIFT_POLL_S="0")
    env.pop("LAYA_DRIFT_APPROVED", None)
    if approved:
        env["LAYA_DRIFT_APPROVED"] = "graviton"
    so, se = work / ("run-%s.out" % tag), work / ("run-%s.err" % tag)
    with open(so, "wb") as fo, open(se, "wb") as fe:
        child = subprocess.Popen(["bash", str(script)] + list(args), cwd=str(work), stdin=subprocess.DEVNULL, stdout=fo,
                                 stderr=fe, env=env)
        child.wait(timeout=300)
    st = json.loads(stf.read_text())
    return child.returncode, so.read_text(), se.read_text(), st


def selftest_aws(case, work):
    sd = fixture_state(work)
    (work / "fakebin").mkdir()
    (work / "fake_aws.py").write_text(FAKE_AWS)
    (work / "fakebin" / "aws").write_text('#!/bin/sh\nexec python3 -I "%s" "$@"\n' % (work / "fake_aws.py"))
    (work / "fakebin" / "aws").chmod(0o755)
    script, results = work / "state" / "drift-graviton.sh", work / "results"
    out_lines, err_lines = [], []
    ctx = Ctx(emit=out_lines.append, err=err_lines.append)
    rc = main(["aws-plan", "--state-dir", str(sd), "--results", str(results), "--source", "smoke:" + T_SMOKE,
               "--source", "motivation:" + T_MOT], ctx)
    case("aws-plan writes the script for two fixture runs (bashrs-gate PASS inside, no aws call)",
         rc == 0 and script.is_file() and any(ln.startswith("AWS PLAN OK") for ln in out_lines), "; ".join(err_lines)[-300:])
    if rc != 0:
        return
    text = script.read_text()
    case("the generated script's first two lines are the shebang and exactly set -euo pipefail",
         text.split("\n")[:2] == ["#!/usr/bin/env bash", "set -euo pipefail"])
    case("the header says the script deletes nothing and names the 30 day lifecycle rule",
         "this script deletes nothing" in text and "30 day lifecycle" in text)
    case("the generated script contains no rm command and no date command",
         not re.search(r"(^|[\s;&|(])rm\s", "\n".join(s for _, s in logical_lines(text))) and "date " not in text)
    case("the 45 minute cap uses the SECONDS builtin", "SECONDS" in text and "CAP_S=2700" in text)

    # --- the clean run against the fake aws ---
    rc, out, err, st = run_fake(work, script, ["smoke:1", "motivation:1"], tag="clean")
    calls, store = st["calls"], st["store"]
    case("clean run: exit 0 and DRIFT GRAVITON DONE", rc == 0 and "DRIFT GRAVITON DONE" in out, (err or out)[-300:])
    case("clean run: the account check is the first aws call, the image digest the second",
         calls[:2] and calls[0][:2] == ["sts", "get-caller-identity"] and calls[1][:2] == ["ecs", "describe-task-definition"])
    case("clean run: every aws call carries --profile ze-kasher-dev and --region us-east-1",
         all("--profile" in c and c[c.index("--profile") + 1] == PROFILE and "--region" in c and c[c.index("--region") + 1] == REGION
             for c in calls))
    case("clean run: no delete, acl, iam or other call outside the allowed verbs",
         all(tuple(c[:2]) in AWS_ALLOWED for c in calls), str([c[:2] for c in calls if tuple(c[:2]) not in AWS_ALLOWED]))
    writes = [c for c in calls if c[:2] in (["s3api", "copy-object"], ["s3api", "put-object"])]
    keys = [c[c.index("--key") + 1] for c in writes]
    case("clean run: every s3 write targets runs/drift- and none a source prefix (%d writes)" % len(writes),
         writes and all(k.startswith("runs/drift-") for k in keys)
         and not any(k.startswith(("runs/smoke-stance-64/", "runs/motivation-v1-sm/")) for k in keys))
    seed = fixture_store()
    case("clean run: every source object is byte- and tag-identical afterwards",
         all(store.get(k) == v for k, v in seed.items()))
    copies = [c for c in writes if c[1] == "copy-object"]
    case("clean run: only the run/ and data/ entries are copied (4 per leg; no verdict, .apr, log or outcome)",
         len(copies) == 8 and all(c[c.index("--key") + 1].split("/", 3)[3].split("/")[0] in ("run", "data") for c in copies))
    tv = tag_violations(calls, store)
    case("clean run: the tag oracle finds no violation (arguments, end state, read-back before run-task)", tv == [], "; ".join(tv[:3]))
    drift_keys = [k for k in store if k.startswith("runs/drift-")]
    case("clean run: the end-state store holds the copies, the rewritten files and the pack task's outputs, all expire",
         len(drift_keys) == 2 * (4 + 2 + 3) and all(store[k]["tags"] == {"laya-retention": "expire"} for k in drift_keys))
    oc = [k for k in drift_keys if k.endswith("/outcome.json")]
    case("clean run: each rewritten outcome.json carries the new uuid of its own prefix",
         len(oc) == 2 and all(json.loads(store[k]["body"])["task_id"] == k.split("/")[2] for k in oc))
    case("clean run: the results were fetched (pack-outcome.json, pack.log, task-summary.json per leg)",
         all((results / d / f).is_file() for d in ("smoke-1", "motivation-1") for f in ("pack-outcome.json", "pack.log", "task-summary.json")))
    case("clean run: task-summary.json holds the exit code and no ARN",
         all("arn:" not in (results / d / "task-summary.json").read_text()
             and json.loads((results / d / "task-summary.json").read_text())["container_exit_code"] == 2 for d in ("smoke-1", "motivation-1")))
    case("clean run: the poll loop saw RUNNING then STOPPED for each task",
         sum(1 for c in calls if c[:2] == ["ecs", "describe-tasks"] and "--query" in c) == 4)

    # --- the gates ---
    rc, out, err, st = run_fake(work, script, ["smoke:1"], approved=False, tag="unapproved")
    case("LAYA_DRIFT_APPROVED unset: refused, exit 2, no aws call", rc == 2 and "REFUSED approval" in err and st["calls"] == [])
    rc, out, err, st = run_fake(work, script, ["smoke:1"], account="999988887777", tag="account")
    case("a wrong account is refused before any other call (exit 2, only the sts call)",
         rc == 2 and "REFUSED account" in err and [c[:2] for c in st["calls"]] == [["sts", "get-caller-identity"]])
    rc, out, err, st = run_fake(work, script, ["nope:1"], tag="leg")
    case("an unknown leg is refused before any aws call", rc == 2 and st["calls"] == [])
    rc, out, err, st = run_fake(work, script, ["smoke:9"], tag="reps")
    case("repeats outside 1 to 5 are refused before any aws call", rc == 2 and st["calls"] == [])

    # --- red side: three tag mutants, each refused by THE oracle for its own reason ---
    put_lines = [ln for ln in text.splitlines() if "s3api put-object" in ln]
    m2 = text
    for ln in put_lines:
        m2 = m2.replace(ln, ln.replace(" --tagging 'laya-retention=expire'", ""))
    mutants = [("directive dropped from the copy", text.replace(" --tagging-directive REPLACE", ""),
                "copy-object without --tagging-directive REPLACE", "copy-tag"),
               ("tag dropped from the upload", m2, "put-object without --tagging laya-retention=expire", "put-tag"),
               ("tag value keep", text.replace("--tagging 'laya-retention=expire'", "--tagging 'laya-retention=keep'"),
                "--tagging is laya-retention=keep, not laya-retention=expire", "tag-value")]
    for i, (label, mtext, want, static_rule) in enumerate(mutants):
        mp = work / ("mutant-%d.sh" % i)
        mp.write_text(mtext)
        rc, out, err, st = run_fake(work, mp, ["smoke:1"], tag="m%d" % i)
        tv = tag_violations(st["calls"], st["store"])
        wrote = any(c[:2] in (["s3api", "copy-object"], ["s3api", "put-object"]) for c in st["calls"])
        case("mutant (%s): the oracle refuses it naming %r (it ran to its writes, not a crash)" % (label, want),
             mtext != text and wrote and any(want in x for x in tv), "; ".join(tv[:2]) or "no violation")
        case("mutant (%s): the script's own read-back stopped it before any run-task" % label,
             rc == 2 and "REFUSED tag" in err and not any(c[:2] == ["ecs", "run-task"] for c in st["calls"]), err[-200:])
        case("mutant (%s): the static table refuses it (%s)" % (label, static_rule),
             any(r == static_rule for r, _, _ in static_violations(mtext)))
    case("the unmutated script yields no violation from the static table", static_violations(text) == [], str(static_violations(text)[:2]))

    # --- the static policy table over the generated text ---
    copy_line = next(ln for ln in text.splitlines() if "s3api copy-object" in ln)
    add = '\n    aws %s --profile "$PROFILE" --region "$REGION"\n'
    static_cases = [
        ("a copy whose key is the source prefix", text.replace('--key "${dst}${rel}"', '--key "${src}${rel}"'), "write-prefix"),
        ("a second dst assignment", text.replace('    out="${RESULTS}', '    dst="runs/x/"\n    out="${RESULTS}'), "write-prefix"),
        ("an s3 high-level copy", text.replace(copy_line, copy_line + add % 's3 cp x "s3://${BUCKET}/runs/drift-x/y"'), "s3-high-level"),
        ("an s3api delete-object", text.replace(copy_line, copy_line + add % 's3api delete-object --bucket "$BUCKET" --key "${dst}x"'), "aws-verb"),
        ("an s3api put-object-tagging", text.replace(copy_line, copy_line + add % 's3api put-object-tagging --bucket "$BUCKET" --key "${dst}x"'), "aws-verb"),
        ("an iam call", text.replace(copy_line, copy_line + add % 'iam list-roles'), "aws-verb"),
        ("a call without the profile and region", text.replace(copy_line, copy_line + '\n    aws sts get-caller-identity\n'), "profile-region"),
        ("an rm of a path", text.replace(copy_line, copy_line + '\n    rm -f "${work}/entries.txt"\n'), "forbidden-command"),
        ("a copy without any --tagging", text.replace(copy_line, copy_line.replace(" --tagging 'laya-retention=expire'", "")), "copy-tag"),
        ("set -e only", text.replace("set -euo pipefail\n", "set -e\n", 1), "strict-options"),
    ]
    for label, mtext, rule in static_cases:
        got = static_violations(mtext)
        case("static table refuses %s (%s)" % (label, rule), mtext != text and any(r == rule for r, _, _ in got), str(got[:2]))
    commented = text.replace(copy_line, copy_line + "\n    # aws s3 rm s3://x --recursive is never run here\n")
    case("static table ignores a comment that spells a refused shape", static_violations(commented) == [])

    # --- the lint: the generated script passes bashrs-gate; an unguarded path-removing mutant is refused ---
    rc, lines = bashrs_gate([str(script)])
    case("bashrs-gate passes the generated fixture script", rc == 0 and lines[-1].startswith("BASHRS GATE PASS"), " | ".join(lines)[-200:])
    rmm = work / "rm-mutant.sh"
    rmm.write_text(text + 'rm -rf "$work"\n')
    rc, lines = bashrs_gate([str(rmm)])
    case("bashrs-gate refuses the generated script with an appended unguarded rm -rf \"$work\"",
         rc == 1 and lines[-1].startswith("REFUSED bashrs-gate findings") and any(re.search(r":[0-9]+ SEC0", ln) for ln in lines),
         " | ".join(lines)[-200:])


def selftest_bashrs(case, work):
    gate_src = (ROOT / "scripts" / "check_bashrs_gate.sh").read_text()
    try:
        pv = subprocess.run(["bashrs", "--version"], capture_output=True, text=True)
        rcv, outv = pv.returncode, pv.stdout.strip()
    except OSError:
        rcv, outv = 127, ""
    print("     bashrs-gate case table runs under: %s" % (outv or "NO bashrs ON PATH"))
    case("bashrs is installed (the selftest fails, never skips, without it)", rcv == 0 and bool(outv))
    case("the SEC010 and DET002 fixtures are the gate's own (self_test case 4, positive_control), verbatim",
         all("'%s'" % ln in gate_src for ln in SEC010_LINES[2:]) and all(ln in gate_src for ln in DET002_GATE_LINES))
    d = work / "bg"
    d.mkdir()
    det002 = ["#!/usr/bin/env bash", "set -euo pipefail"] + [ln.replace("%%", "%") for ln in DET002_GATE_LINES]
    fixtures = {
        "clean.sh": ("#!/usr/bin/env bash\nset -euo pipefail\n# an idiomatic script: arithmetic and quoted expansions\n"
                     "n=0\nfor f in \"$@\"; do\n    n=$((n + 1))\n    printf '%s\\n' \"$f\"\ndone\necho \"$n\"\n"),
        "guarded.sh": ("#!/usr/bin/env bash\nset -euo pipefail\nd=\"$(mktemp -d)\"\nprintf 'x\\n' > \"${d}/f\"\n"
                       "[ -n \"$d\" ] || exit 2\nrm -rf \"${d:?}\"\n"),
        "sec010.sh": "\n".join(SEC010_LINES) + "\n",
        "det002.sh": "\n".join(det002) + "\n",
        "set-e-only.sh": "#!/usr/bin/env bash\nset -e\necho hi\n",
        "late-strict.sh": "#!/usr/bin/env bash\necho early\nset -euo pipefail\n",
        "empty.sh": "",
    }
    for name, body in fixtures.items():
        (d / name).write_text(body)
    table = [("clean.sh", 0, None), ("guarded.sh", 0, None), ("sec010.sh", 1, "SEC010"), ("det002.sh", 1, "DET002"),
             ("set-e-only.sh", 1, "strict-options"), ("late-strict.sh", 1, "strict-options"), ("empty.sh", 2, "ENV"),
             ("does-not-exist.sh", 2, "ENV")]
    for name, want_rc, want in table:
        rc, lines = bashrs_gate([str(d / name)])
        txt = " | ".join(lines)
        case("bashrs-gate %s -> exit %d%s" % (name, want_rc, (" naming " + want) if want else ""),
             rc == want_rc and (want is None or want in txt) and (want_rc != 0 or lines[-1].startswith("BASHRS GATE PASS")), txt[-200:])
    rc, lines = bashrs_gate([str(d / "clean.sh")], env=dict(os.environ, PATH="/usr/bin:/bin"))
    case("bashrs-gate with PATH lacking bashrs -> exit 2 (fails closed)", rc == 2 and "bashrs is not on PATH" in " ".join(lines))
    errs = []
    rc = main(["bashrs-gate", str(d / "sec010.sh"), str(d / "clean.sh")], Ctx(emit=errs.append, err=errs.append))
    case("bashrs-gate over two files exits with the worst (1) and names the finding by the original file",
         rc == 1 and any(e.endswith("sec010.sh:5 SEC010") for e in errs),
         " | ".join(errs)[-200:])


def selftest_collect(case, work):
    d = work / "collect"
    g = d / "graviton"
    (g / "smoke-1").mkdir(parents=True)
    smoke_ev = json.loads((ROOT / SMOKE_EVIDENCE).read_text(encoding="utf-8"))
    rec_drift = parse_output(smoke_ev["local_verify"]["line"], 2)
    local = {"schema": EVIDENCE_SCHEMA, "host": {"cpu_model": "Apple M4 Pro", "arch": "arm64"}, "binary": {"sha256": "0" * 64},
             "legs": [{"leg": "smoke-stance-64", "kind": "verify", "trained_on": "cuda:0", "noise_reference": {"fine_tuned": 8.1e-6},
                       "repeats": [{"record": rec_drift}], "summary": {"repeats_identical": True}}],
             "controls": {}, "references": {}, "not_measurable": []}
    (d / "local.json").write_text(json.dumps(local))
    po = {"exit_code": 0, "accepted": True, "packed_line": CANNED["drift-smoke"], "verify_line": smoke_ev["smoke"]["verdict"]["pack"]["line"],
          "pack_seconds": 300.1, "verify_seconds": 299.2, "pack_image_digest": "sha256:" + "5" * 64}
    (g / "smoke-1" / "pack-outcome.json").write_text(json.dumps(po))
    (g / "smoke-1" / "task-summary.json").write_text(json.dumps({
        "stoppedReason": "task arn:aws:ecs:us-east-1:%s:task/c/abc exited" % FIX_ACCOUNT,
        "container_reason": "account %s" % FIX_ACCOUNT, "container_exit_code": 0}))
    args = ["collect", "--local", str(d / "local.json"), "--graviton", str(g), "--out", str(d / "merged.json"), "--draft", str(d / "table.md")]
    out_lines, errs = [], []
    rc = main(args, Ctx(emit=out_lines.append, err=errs.append))
    merged = (d / "merged.json").read_text() if (d / "merged.json").is_file() else ""
    case("collect merges local, Graviton and reference cells and scrubs a planted account id and ARN",
         rc == 0 and merged and not ID_RE.search(merged) and "arn:aws" not in merged and "<arn>" in merged
         and "<account-id>" in merged, "; ".join(errs)[-200:])
    if merged:
        m = json.loads(merged)
        cells = {(r["cpu"].split(" ")[0], r["step"], r["set"], r["cell"]) for r in m["rows"]}
        case("collect rows: measured Graviton pack and verify cells for both sets, plus the 14-14 / 14-15 references",
             ("AWS", "pack-task pack", "zero_shot", "measured") in cells and ("AWS", "pack-task verify", "fine_tuned", "measured") in cells
             and ("AWS", "pack-task pack (14-15)", "zero_shot", "reference") in cells, str(sorted(cells))[:300])
        case("collect states which cells are measured and which are references", "reference = copied" in m["cells"]
             and "measured" in (d / "table.md").read_text())
    (g / "smoke-1" / "task-summary.json").write_text(json.dumps({FIX_ACCOUNT: "an identifier planted as a key"}))
    (d / "merged.json").unlink()
    errs.clear()
    rc = main(args, Ctx(emit=lambda s: None, err=errs.append))
    case("collect refuses to write when an identifier survives the scrub (planted as a key), nothing written",
         rc == 2 and errs and errs[-1].startswith("REFUSED drift-evidence") and not (d / "merged.json").exists())


def selftest():
    failures, n_ok = [], [0]

    def case(label, ok, detail=""):
        if ok:
            n_ok[0] += 1
        else:
            failures.append("%s%s" % (label, (": " + detail) if detail else ""))
        print("%s %s%s" % ("ok  " if ok else "FAIL", label, ("" if ok else "  [%s]" % detail)))

    smoke_ev = json.loads((ROOT / SMOKE_EVIDENCE).read_text(encoding="utf-8"))
    verify_line = smoke_ev["smoke"]["verdict"]["pack"]["line"]
    good, bad = parser_cases(verify_line)
    case("parser case table: %d of %d rows classified as expected" % (good, len(PARSER_CASES)), not bad, "; ".join(bad))

    # the recorded refusal parses to the positive control the plan states
    rec = parse_output(smoke_ev["local_verify"]["line"], 2)
    case("the recorded Mac refusal in 14-SMOKE-EVIDENCE.json parses to the positive control",
         rec["class"] == "drift" and check_positive(rec, smoke_ev["local_verify"]["line"])[0])
    other = parse_output("REFUSED RescoreDrift which=fine_tuned row=199 max_abs=0.00003439188003540038 bound=0.00003259039634251337 "
                         "(nothing written)", 2)
    case("a refusal one digit away does NOT reproduce the positive control", not check_positive(other, smoke_ev["local_verify"]["line"])[0])

    work = Path(tempfile.mkdtemp(prefix="laya-drift-selftest-"))
    try:
        fake = work / "pack_laya"
        fake.write_text(FAKE_PACK)
        fake.chmod(0o755)
        spec = dict(LEGS["smoke-stance-64"], leg="smoke-stance-64")
        ctx = Ctx(emit=lambda s: None)

        def leg_with(line, rc, threads=None, kind="verify"):
            os.environ["FAKE_LINE"], os.environ["FAKE_RC"] = line or "", str(rc)
            os.environ["FAKE_THREADS_FILE"] = str(work / "threads.txt")
            d = work / ("l%d" % len(list(work.iterdir())))
            d.mkdir()
            s = dict(spec, kind=kind)
            try:
                return run_once(ctx, str(fake), s, "/nonexistent-base", d, "x", threads=threads)
            finally:
                for k in ("FAKE_LINE", "FAKE_RC", "FAKE_THREADS_FILE"):
                    os.environ.pop(k, None)

        r = leg_with(smoke_ev["local_verify"]["line"], 2)
        case("fake binary: a drift refusal at exit 2 is recorded as drift with the child's rc and a wall time",
             r["rc"] == 2 and r["record"]["class"] == "drift" and r["wall_s"] >= 0 and r["threads"] == "default"
             and check_positive(r["record"], smoke_ev["local_verify"]["line"])[0])
        r = leg_with(verify_line, 0)
        case("fake binary: the verify JSON line at exit 0 is recorded accepted with both sets",
             r["record"]["class"] == "accepted" and sorted(r["record"]["sets"]) == ["fine_tuned", "zero_shot"])
        r = leg_with(verify_line, 1)
        case("fake binary: an accepted line at exit 1 is unparsed, never a number", r["record"]["class"] == "unparsed" and r["rc"] == 1)
        r = leg_with(None, 101)
        case("fake binary: a crash with no line is unparsed (rc 101 read from the child)", r["record"]["class"] == "unparsed" and r["rc"] == 101)
        r = leg_with("REFUSED InputHash data/task.json differs (nothing written)", 2)
        case("fake binary: another rule is recorded another_rule, not a drift leg", r["record"]["class"] == "another_rule")
        r = leg_with(smoke_ev["local_verify"]["line"], 2, threads=4)
        case("fake binary: RAYON_NUM_THREADS reaches the child", (work / "threads.txt").read_text().strip() == "4" and r["threads"] == 4)
        os.environ["RAYON_NUM_THREADS"] = "7"
        try:
            leg_with(smoke_ev["local_verify"]["line"], 2)
            ambient = (work / "threads.txt").read_text().strip()
        finally:
            os.environ.pop("RAYON_NUM_THREADS", None)
        case("fake binary: an ambient RAYON_NUM_THREADS never reaches a default leg", ambient == "unset", ambient)
        r = leg_with("PACKED %s sha256=%s rescore_max_abs=0.1 zs_rescore_max_abs=0.1 rescore_bound=0.2 zs_rescore_bound=0.2 "
                     "noise=null zs_noise=null shipped_seed=17 argmax=1/1 waived=none" % (work / "never.apr", "d" * 64), 0, kind="pack")
        case("fake binary: a PACKED line whose file was never written is unparsed", r["record"]["class"] == "unparsed")

        # the dirty closure is refused before anything is built
        calls = []
        tree = "aprender-decide v0.69.3 (%s/crates/aprender-decide)\naprender-core v0.69.3 (%s/crates/aprender-core)\nserde v1\n" % (ROOT, ROOT)

        def fake_cmd(dirty):
            def run(argv, timeout=0):
                calls.append(argv)
                if argv[:2] == ["cargo", "tree"]:
                    return 0, tree, ""
                if argv[:2] == ["git", "status"]:
                    return 0, (" M crates/aprender-decide/src/verify.rs\n" if dirty else ""), ""
                if argv[:2] == ["git", "rev-parse"]:
                    return 0, "f" * 40 + "\n", ""
                if argv[:2] == ["cargo", "build"]:
                    msg = {"reason": "compiler-artifact", "target": {"name": "pack_laya", "kind": ["example"]},
                           "executable": str(fake), "fresh": True}
                    return 0, json.dumps({"reason": "build-script-executed"}) + "\n" + json.dumps(msg) + "\n", ""
                return 1, "", "unexpected"
            return run

        ctx_d = Ctx(cmd=fake_cmd(True), emit=lambda s: None)
        d = work / "dirty"
        d.mkdir()
        try:
            binary_identity(ctx_d, d)
            got = None
        except Refused as e:
            got = e.rule
        case("a dirty decide crate is refused (drift-dirty) before cargo build runs",
             got == "dirty" and not any(c[:2] == ["cargo", "build"] for c in calls))
        status_call = [c for c in calls if c[:2] == ["git", "status"]]
        case("the dirty check covers the closure crates, both contracts and Cargo.lock",
             bool(status_call) and {"crates/aprender-decide", "crates/aprender-core", "Cargo.lock"} <= set(status_call[0])
             and set(GATE_CONTRACTS) <= set(status_call[0]))
        calls.clear()
        b = binary_identity(Ctx(cmd=fake_cmd(False), emit=lambda s: None), d)
        case("a clean closure: the executable is cargo's compiler-artifact path, hashed, with the commit",
             b["abs_path"] == str(fake) and b["sha256"] == sha256_file(fake) and b["commit"] == "f" * 40 and b["closure_dirty"] is False)

        def fake_build_cmd(argv, timeout=0):
            if argv[:2] == ["cargo", "build"]:
                return 0, json.dumps({"reason": "compiler-artifact", "target": {"name": "aprender_decide", "kind": ["lib"]},
                                      "executable": None}) + "\n", ""
            return fake_cmd(False)(argv, timeout)
        try:
            binary_identity(Ctx(cmd=fake_build_cmd, emit=lambda s: None), d)
            got = None
        except Refused as e:
            got = e.rule
        case("no compiler-artifact message for the example is refused (never a guessed target path)", got == "build")
        outside = Ctx(cmd=lambda argv, timeout=0: (0, "x v1 (/elsewhere/crate)\n", "") if argv[:2] == ["cargo", "tree"] else (0, "", ""),
                      emit=lambda s: None)
        try:
            closure_paths(outside)
            got = None
        except Refused as e:
            got = e.rule
        case("a path crate outside the checkout is refused (git cannot attest it)", got == "dirty")

        # usage and leg refusals reach no build
        errs = []
        calls.clear()
        rc = main(["local", "--only", "no-such-leg"], Ctx(cmd=fake_cmd(False), emit=lambda s: None, err=errs.append))
        case("local with an unknown leg is refused drift-leg, exit 2, before any command",
             rc == 2 and errs and errs[-1].startswith("REFUSED drift-leg:") and calls == [])
        rc = main(["local", "--repeats", "0"], Ctx(cmd=fake_cmd(False), emit=lambda s: None, err=errs.append))
        case("--repeats 0 is refused drift-usage", rc == 2 and errs[-1].startswith("REFUSED drift-usage:"))
        (work / "nonempty").mkdir()
        (work / "nonempty" / "x").write_text("x")
        rc = main(["local", "--out", str(work / "nonempty")], Ctx(cmd=fake_cmd(False), emit=lambda s: None, err=errs.append))
        case("a non-empty --out is refused drift-out (old logs are never mixed in)", rc == 2 and errs[-1].startswith("REFUSED drift-out:"))

        aws_work = work / "aws"
        aws_work.mkdir()
        selftest_aws(case, aws_work)
        selftest_bashrs(case, work)
        selftest_collect(case, work)
    finally:
        shutil.rmtree(work, ignore_errors=True)

    if failures:
        print("DRIFT SELFTEST FAILED: %d case(s): %s" % (len(failures), "; ".join(failures)))
        return 1
    print("DRIFT SELFTEST OK (%d cases)" % n_ok[0])
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--selftest"]:
        sys.exit(selftest())
    sys.exit(main(sys.argv[1:]))
