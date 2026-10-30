#!/usr/bin/env python3
"""Measure the cross-CPU re-score drift of the Rust pack and verify step (plan 14-20; 14-VERIFICATION gap 2; D-14, D-15).

    python3 -I scripts/laya_deploy/laya_drift.py local [--only LEG[,LEG...]] [--repeats N] [--out DIR]
    python3 -I scripts/laya_deploy/laya_drift.py --selftest

The pack bound of laya-parity-v1 A1 is max(1e-5, 4 x a noise figure measured on the TRAINING device). Two live
observations say it does not hold across CPUs; this harness turns that into measurements. It never amends a
contract and never edits a decide crate (D-15): it produces numbers.

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
RULES = ("usage", "leg", "out", "dirty", "build", "base", "control", "evidence")
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


# ---- entry -------------------------------------------------------------------------------------------------------

def main(argv, ctx=None):
    ctx = ctx or Ctx()
    try:
        if not argv:
            refuse("usage", "a subcommand is required: local")
        if argv[0] == "local":
            return cmd_local(argv[1:], ctx)[0]
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
