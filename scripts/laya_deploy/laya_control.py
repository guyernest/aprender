#!/usr/bin/env python3
"""The Phase 16 pipeline-equivalence control's reference, rehearsal and paired report (plan 16-01; used by 16-02, 16-03).

    python3 -I scripts/laya_deploy/laya_control.py snapshot-reference --evidence FILE --out FILE
    python3 -I scripts/laya_deploy/laya_control.py check-reference --fixture FILE --evidence FILE [--draft FILE]
    python3 -I scripts/laya_deploy/laya_control.py rehearse --declaration FILE --reference FILE [--json]
    python3 -I scripts/laya_deploy/laya_control.py --selftest

`snapshot-reference` copies the Phase 10 Mac (mps:0) per-seed numbers into a text-free fixture (numbers and hashes only).
`check-reference` re-derives that fixture byte for byte from the tracked evidence, pins the evidence sha256 and
cross-checks the owner-accepted declaration draft. `rehearse` builds synthetic fetched CUDA runs from a declaration and
judges each by the COMMITTED evaluator laya_repro_eval.py (a subprocess, never an import, never edited), so the
declaration is shown to be both passable and failable before any paid step.

No network, no AWS, no environment reads, no write outside --out and mktemp directories (the selftest and rehearse
remove every directory they make). Exit codes: 0 ok, 1 selftest or rehearsal failure, 2 refusal printed as exactly one
`REFUSED <rule>: <reason>` line on stderr. stdlib only.
"""

import hashlib
import json
import math
import os
import shutil
import subprocess
import sys
import tempfile


class Refused(Exception):
    def __init__(self, rule, reason):
        Exception.__init__(self, "%s: %s" % (rule, reason))
        self.rule = rule
        self.reason = reason


HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
EVALUATOR = os.path.join(HERE, "laya_repro_eval.py")
DIAGNOSIS = os.path.join(HERE, "laya_margin_diagnosis.py")
FIXTURES = os.path.join(REPO, "crates", "aprender-mcp-laya-train", "fixtures")

MAX_FILE = 8 * 1024 * 1024
REF_SCHEMA = "laya-control-reference-v1"
SEEDS = (13, 17, 23)
INPUT_KEYS = (("task_json", "task.json"), ("train_jsonl", "train.jsonl"), ("eval_jsonl", "eval.jsonl"))
THRESHOLD_KEYS = ("min_macro_f1_margin", "max_ece", "ece_bins")
ID_CHARS = frozenset("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789:._+-")
LABEL_CHARS = ID_CHARS | frozenset(" ")
HEX = frozenset("0123456789abcdef")


# ---- reading JSON ------------------------------------------------------------------------------------------------

def _pairs(pairs):
    d = {}
    for k, v in pairs:
        if k in d:
            raise ValueError("duplicate key %r" % k)
        d[k] = v
    return d


def _const(name):
    raise ValueError("non-finite number %s" % name)


def parse_json_bytes(raw, rule, what):
    if len(raw) > MAX_FILE:
        raise Refused(rule, "%s is larger than %d bytes" % (what, MAX_FILE))
    try:
        return json.loads(raw.decode("utf-8"), object_pairs_hook=_pairs, parse_constant=_const)
    except (ValueError, UnicodeDecodeError) as ex:
        raise Refused(rule, "%s is not valid JSON (%s)" % (what, str(ex)[:80]))


def read_bytes(path, rule):
    if os.path.islink(path) or not os.path.isfile(path):
        raise Refused(rule, "%s is missing" % path)
    if os.path.getsize(path) > MAX_FILE:
        raise Refused(rule, "%s is larger than %d bytes" % (path, MAX_FILE))
    with open(path, "rb") as f:
        return f.read()


def sha256_hex(raw):
    return hashlib.sha256(raw).hexdigest()


def is_num(x):
    return isinstance(x, (int, float)) and not isinstance(x, bool) and math.isfinite(x)


def is_int(x):
    return isinstance(x, int) and not isinstance(x, bool)


def is_hex64(x):
    return isinstance(x, str) and len(x) == 64 and set(x) <= HEX


def clone(o):
    return json.loads(json.dumps(o))


def load_evidence_bytes(raw, what):
    return parse_json_bytes(raw, "reference-evidence", what)


# ---- the reference fixture -----------------------------------------------------------------------------------------

def build_snapshot(ev, path, evidence_sha):
    """The text-free reference fixture for evidence object `ev` read from `path` (numbers and hashes only)."""
    def need(ok, msg):
        if not ok:
            raise Refused("reference-evidence", msg)

    def dig(obj, *keys):
        cur = obj
        for k in keys:
            need(isinstance(cur, dict) and k in cur, "the evidence lacks %s" % ".".join(keys))
            cur = cur[k]
        return cur

    def ident(v, name, chars=ID_CHARS, limit=40):
        need(isinstance(v, str) and 0 < len(v) <= limit and set(v) <= chars, "%s is not a short identifier" % name)
        return v

    need(isinstance(ev, dict), "the evidence is not a JSON object")
    need(isinstance(path, str) and 0 < len(path) <= 200 and path.isprintable(), "the evidence path is not a short printable string")
    plan = ident(dig(ev, "plan"), "plan", ID_CHARS, 12)
    device = ident(dig(ev, "device_used"), "device_used")
    torch_version = ident(dig(ev, "torch_version"), "torch_version")
    epochs = dig(ev, "epochs_declared")
    need(is_int(epochs) and epochs > 0, "epochs_declared is not a positive integer")
    recipe_id = dig(ev, "recipe_id")
    need(is_hex64(recipe_id), "recipe_id is not a 64-character hex id")
    labels = dig(ev, "inspect", "labels")
    need(isinstance(labels, list) and len(labels) >= 2, "inspect.labels is not a list of at least two labels")
    labels = [ident(x, "a label", LABEL_CHARS, 80) for x in labels]
    data = dig(ev, "data_sha256")
    inputs = {}
    for key, fname in INPUT_KEYS:
        need(fname in data and is_hex64(data[fname]), "data_sha256 lacks a 64-character hex %s" % fname)
        inputs[key] = data[fname]
    eval_n = dig(ev, "eval_n")
    need(is_int(eval_n) and eval_n > 0, "eval_n is not a positive integer")
    wall = dig(ev, "wall_seconds")
    need(is_num(wall), "wall_seconds is not a finite number")
    shipped = dig(ev, "seeds", "shipped")
    need(is_int(shipped) and shipped in SEEDS, "seeds.shipped is not one of the seeds 13, 17, 23")
    rows = dig(ev, "seeds", "per_seed")
    need(isinstance(rows, list) and all(isinstance(r, dict) and is_int(r.get("seed")) for r in rows)
         and sorted(r["seed"] for r in rows) == list(SEEDS), "seeds.per_seed is not exactly the seeds 13, 17, 23")
    per_seed = []
    for r in sorted(rows, key=lambda r: r["seed"]):
        row = {}
        for k in ("seed", "margin", "ece_post", "macro_f1", "t_applied", "clamp_hit", "best_epoch", "epochs_run", "train_seconds"):
            need(k in r, "a per-seed row lacks %s" % k)
            v = r[k]
            if k == "clamp_hit":
                need(isinstance(v, bool), "clamp_hit is not a boolean")
            elif k in ("seed", "best_epoch", "epochs_run"):
                need(is_int(v), "%s is not an integer" % k)
            else:
                need(is_num(v), "%s is not a finite number" % k)
            row[k] = v
        per_seed.append(row)
    gate_ece = dig(ev, "gate", "ece_post")
    need(is_num(gate_ece), "gate.ece_post is not a finite number")
    thr = dig(ev, "gate", "thresholds")
    need(all(k in thr and is_num(thr[k]) for k in THRESHOLD_KEYS), "gate.thresholds lacks the three numeric thresholds")
    return {
        "schema": REF_SCHEMA,
        "source": {"path": path, "sha256": evidence_sha, "plan": plan},
        "device": device,
        "torch_version": torch_version,
        "epochs": epochs,
        "recipe_id": recipe_id,
        "labels": labels,
        "inputs_sha256": inputs,
        "eval_n": eval_n,
        "seeds": list(SEEDS),
        "shipped_seed": shipped,
        "shipped_ece_post": gate_ece,
        "thresholds": {k: thr[k] for k in THRESHOLD_KEYS},
        "per_seed": per_seed,
        "three_seed_mean_margin": math.fsum(r["margin"] for r in per_seed) / len(per_seed),
        "wall_seconds": wall,
        "note": "numbers and hashes only: no tweet text, no account id, no free-text field of the evidence is copied",
    }


def cmd_snapshot_reference(opts, out):
    raw = read_bytes(opts["--evidence"], "reference-evidence")
    snap = build_snapshot(load_evidence_bytes(raw, opts["--evidence"]), opts["--evidence"], sha256_hex(raw))
    parent = os.path.dirname(opts["--out"])
    if parent and not os.path.isdir(parent):
        raise Refused("usage", "the directory of --out does not exist")
    _write(opts["--out"], _dumps(snap))
    out("SNAPSHOT OK fixture_sha256=%s" % sha256_hex(_dumps(snap).encode())[:12])
    return 0


def check_reference(fixture_path, evidence_path, draft_path):
    """The REFERENCE OK line, or Refused reference-source / reference-drift / reference-draft."""
    raw_f = read_bytes(fixture_path, "reference-drift")
    fx = parse_json_bytes(raw_f, "reference-drift", fixture_path)
    raw_e = read_bytes(evidence_path, "reference-evidence")
    ev = load_evidence_bytes(raw_e, evidence_path)
    e_sha = sha256_hex(raw_e)
    src = fx.get("source") if isinstance(fx, dict) else None
    if not isinstance(src, dict) or not isinstance(src.get("path"), str) or src.get("sha256") != e_sha:
        raise Refused("reference-source", "the fixture's source.sha256 is not the sha256 of the evidence file")
    if not os.path.normpath(os.path.abspath(evidence_path)).endswith(os.path.normpath(src["path"])):
        raise Refused("reference-source", "the evidence file is not the one the fixture names")
    fresh = _dumps(build_snapshot(ev, src["path"], e_sha)).encode("utf-8")
    if fresh != raw_f:
        want = json.loads(fresh.decode("utf-8"))
        first = next((k for k in sorted(set(want) | set(fx)) if want.get(k) != fx.get(k)), "?")
        raise Refused("reference-drift", "the fixture differs from a fresh snapshot of the evidence (first key: %s)" % first)
    if draft_path is not None:
        draft = parse_json_bytes(read_bytes(draft_path, "reference-draft"), "reference-draft", draft_path)
        ref = draft.get("reference") if isinstance(draft, dict) else None
        if not isinstance(ref, dict):
            raise Refused("reference-draft", "the draft has no reference block")
        exp = {
            "device": fx["device"], "epochs": fx["epochs"], "recipe_id": fx["recipe_id"],
            "per_seed_margin": {str(r["seed"]): r["margin"] for r in fx["per_seed"]},
            "per_seed_ece_post": {str(r["seed"]): r["ece_post"] for r in fx["per_seed"]},
            "shipped_seed": fx["shipped_seed"], "shipped_ece_post": fx["shipped_ece_post"],
            "clamp_hit_seeds": [r["seed"] for r in fx["per_seed"] if r["clamp_hit"]],
            "three_seed_mean_margin": round(fx["three_seed_mean_margin"], 4),
        }
        for k, v in exp.items():
            if ref.get(k) != v:
                raise Refused("reference-draft", "the draft's reference.%s differs from the fixture" % k)
        must = draft.get("must")
        if not isinstance(must, dict) or must.get("inputs_sha256") != fx["inputs_sha256"]:
            raise Refused("reference-draft", "the draft's must.inputs_sha256 differs from the fixture's")
    return "REFERENCE OK source_sha256=%s fixture_sha256=%s" % (e_sha[:12], sha256_hex(raw_f)[:12])


def cmd_check_reference(opts, out):
    out(check_reference(opts["--fixture"], opts["--evidence"], opts.get("--draft")))
    return 0


# ---- synthetic fetched runs and the rehearsal ------------------------------------------------------------------

def load_base():
    base = {}
    for key, name in (("report", "gate-report-v1.json"), ("pack", "pack-outcome-v1.json"), ("outcome", "outcome-v1.json")):
        base[key] = parse_json_bytes(read_bytes(os.path.join(FIXTURES, name), "rehearse-fixture"), "rehearse-fixture", name)
    return base


def synth_parts(decl, base):
    """The files of an in-band, passing, deploy-eligible fetched CUDA run for `decl` (every value derived from it)."""
    must, band = decl["must"], decl["band"]
    seeds = list(must["seeds"])
    mid = (band["margin"][0] + band["margin"][1]) / 2
    emid = (band["ece_post"][0] + band["ece_post"][1]) / 2
    shipped = sorted(seeds)[len(seeds) // 2]
    rep = clone(base["report"])
    tmpl = rep["seeds"]["per_seed"][0]
    rows = []
    for s in seeds:
        r = clone(tmpl)
        r.update({"seed": s, "margin": mid, "ece_post": emid, "macro_f1": 0.8, "pass": True, "t_applied": 1.4,
                  "clamp_hit": False, "best_epoch": 1, "epochs_run": 4, "train_seconds": 100.0})
        rows.append(r)
    rep["seeds"].update({"declared": seeds[0], "shipped": shipped, "n": len(seeds), "per_seed": rows})
    rep["margin"] = mid
    rep["fine_tuned"]["ece_post"] = emid
    rep["pass"] = True
    rep["device_used"] = must["device_prefix"] + ":0"
    rep["device_is_cpu"] = False
    rep["recipe_id"] = must["recipe_id"]
    ins = clone(rep["inputs_sha256"])
    ins.update(must["inputs_sha256"])
    rep["inputs_sha256"] = ins
    return {"report": rep, "pack": clone(base["pack"]), "accepted_pack": clone(base["pack"]),
            "verdict": {"phase": "completed", "verdict": {"pass": True, "waived": None}},
            "outcome": clone(base["outcome"]), "train_log": "GATE PASS\n", "drop": set(), "shipped": shipped,
            "band": band, "mid": mid, "emid": emid}


def write_run(root, name, p):
    d = os.path.join(root, name)
    files = {"run/gate-report.json": p["report"], "verdict.json": p["verdict"], "logs/pack-outcome.json": p["pack"],
             "logs/outcome.json": p["outcome"], "manifest.json": {}}
    for rel, obj in files.items():
        if rel not in p["drop"]:
            _write(os.path.join(d, *rel.split("/")), _dumps(obj))
    if "logs/train.log" not in p["drop"]:
        _write(os.path.join(d, "logs", "train.log"), p["train_log"])
    return d


def _run_tool(argv, rule):
    cp = subprocess.run(argv, capture_output=True, text=True, timeout=120, stdin=subprocess.DEVNULL)
    return cp.returncode, cp.stdout, cp.stderr


def run_evaluator(decl_path, run_dir):
    rc, so, se = _run_tool([sys.executable, "-I", EVALUATOR, "evaluate", "--declaration", decl_path, "--run-dir", run_dir,
                            "--attempt", "1"], "rehearse-evaluator")
    lines = [ln for ln in so.splitlines() if ln.strip()]
    if rc != 0 or len(lines) != 1:
        raise Refused("rehearse-evaluator", "evaluate exited %d: %s" % (rc, (se or so)[:100]))
    return parse_json_bytes(lines[0].encode("utf-8"), "rehearse-evaluator", "the evaluator output")


def run_classify(run_dir):
    rc, so, se = _run_tool([sys.executable, "-I", DIAGNOSIS, "classify-run", "--run-dir", run_dir], "rehearse-classify")
    lines = [ln for ln in so.splitlines() if ln.strip()]
    if rc != 0 or len(lines) != 1:
        raise Refused("rehearse-classify", "classify-run exited %d: %s" % (rc, (se or so)[:100]))
    return parse_json_bytes(lines[0].encode("utf-8"), "rehearse-classify", "the classify-run output")


def _expect_pass(ev, rule):
    ok = (ev["class"] == "pass" and ev["verdict_if_final"] == "reproduced" and all(ev["band"].values())
          and all(ev["must"].values()) and ev["defect_reasons"] == [])
    return ok, "class=%s verdict=%s" % (ev["class"], ev["verdict_if_final"])


# Each case: name, build(parts) mutating the in-band parts, expect(evaluation, classify-or-None) -> (ok, detail).
# Plan 16-01 Task 1 holds the tracer's one row; Task 3 completes the table.
CASES = [
    {"name": "in-band", "build": lambda p: None, "expect": _expect_pass, "classify": False},
]


def rehearse(decl_path, ref_path):
    """Judge every synthetic run of CASES by the committed evaluator; a list of result dicts."""
    decl = parse_json_bytes(read_bytes(decl_path, "rehearse-declaration"), "rehearse-declaration", decl_path)
    rc, so, se = _run_tool([sys.executable, "-I", EVALUATOR, "check-declaration", decl_path], "rehearse-declaration")
    if rc != 0:
        raise Refused("rehearse-declaration", "the committed evaluator refuses the declaration: %s" % (se or so).strip()[:120])
    fx = parse_json_bytes(read_bytes(ref_path, "rehearse-reference"), "rehearse-reference", ref_path)
    must = decl["must"]
    if (must["recipe_id"] != fx.get("recipe_id") or must["inputs_sha256"] != fx.get("inputs_sha256")
            or sorted(must["seeds"]) != sorted(fx.get("seeds", []))):
        raise Refused("rehearse-reference", "the declaration's recipe, inputs or seeds differ from the reference fixture's")
    rf = decl.get("reference_fixture")
    if isinstance(rf, dict) and rf.get("sha256") != sha256_hex(read_bytes(ref_path, "rehearse-reference")):
        raise Refused("rehearse-reference", "the declaration's reference_fixture.sha256 is not the fixture's sha256")
    base = load_base()
    work = tempfile.mkdtemp(prefix="laya-control-rehearse-")
    results = []
    try:
        for case in CASES:
            p = synth_parts(decl, base)
            case["build"](p)
            run = write_run(work, case["name"], p)
            ev = run_evaluator(decl_path, run)
            cl = run_classify(run) if case["classify"] else None
            ok, detail = case["expect"](ev, cl)
            results.append({"name": case["name"], "ok": bool(ok), "detail": detail, "class": ev["class"],
                            "verdict_if_final": ev["verdict_if_final"], "band": ev["band"], "must": ev["must"],
                            "defect_reasons": ev["defect_reasons"],
                            "rule": cl["class"] if cl else None, "rule_reason": cl["reason"] if cl else None})
    finally:
        shutil.rmtree(work, ignore_errors=True)
    return results


def cmd_rehearse(opts, out):
    results = rehearse(opts["--declaration"], opts["--reference"])
    bad = [r for r in results if not r["ok"]]
    if "--json" in opts:
        out(json.dumps({"cases": [{k: r[k] for k in ("name", "class", "verdict_if_final", "band", "must", "defect_reasons",
                                                      "rule", "rule_reason")} for r in results]},
                       sort_keys=True, separators=(",", ":")))
    else:
        for r in results:
            out("  %-4s rehearse:%s  %s" % ("ok" if r["ok"] else "FAIL", r["name"], r["detail"]))
        out("REHEARSAL FAILED" if bad else "REHEARSAL OK cases=%d" % len(results))
    return 1 if bad else 0


# ---- the selftest ------------------------------------------------------------------------------------------------
# RED FIRST (plan 16-01 Task 1): this table was written before any subcommand existed.

class Cases:
    def __init__(self):
        self.n = 0
        self.failed = []

    def check(self, label, fn):
        """fn returns bool or (bool, detail); an exception (a missing function included) is a FAIL, never a crash."""
        try:
            r = fn()
        except Exception as ex:
            ok, detail = False, "crashed with %r" % (ex,)
        else:
            ok, detail = (r if isinstance(r, tuple) else (bool(r), ""))
        self.n += 1
        print("  %-4s %s  %s" % ("ok" if ok else "FAIL", label, detail))
        if not ok:
            self.failed.append(label)

    def refused(self, label, fn, rule):
        def run():
            try:
                fn()
            except Refused as ex:
                return (ex.rule == rule, "refused with %s: %s" % (ex.rule, ex.reason[:90]))
            except Exception as ex:
                return (False, "crashed with %r" % (ex,))
            return (False, "was accepted")
        self.check(label, run)


def _dumps(obj):
    return json.dumps(obj, sort_keys=True, indent=2) + "\n"


def _write(path, text):
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)


def synth_evidence(margins=(0.20, 0.22, 0.21), plan="10-04"):
    """A synthetic Phase 10-shaped evidence object, free text included on purpose (it must never be copied)."""
    rows = []
    for s, m, e, ch in zip((13, 17, 23), margins, (0.011, 0.046, 0.042), (False, True, False)):
        rows.append({"seed": s, "macro_f1": 0.8 + s / 1e4, "ece_post": e, "margin": m, "t_applied": 1.5, "clamp_hit": ch,
                     "pass": True, "epochs_run": 4, "best_epoch": 1 if s != 17 else 3, "train_seconds": 100.5 + s,
                     "eval_probs_sha256": "00" * 32, "text": "FREE TEXT THAT MUST NOT BE COPIED"})
    return {"schema": "laya-gate-run-evidence-v1", "plan": plan, "device_used": "mps:0", "torch_version": "2.14.0",
            "epochs_declared": 4, "recipe_id": "ab" * 32, "inspect": {"labels": ["neg", "pos"], "variant": "production"},
            "data_sha256": {"task.json": "11" * 32, "train.jsonl": "22" * 32, "eval.jsonl": "33" * 32},
            "eval_n": 200, "wall_seconds": 1234.5,
            "seeds": {"policy": "median", "shipped": 23, "per_seed": rows},
            "gate": {"pass": True, "ece_post": 0.042, "margin": margins[2],
                     "thresholds": {"min_macro_f1_margin": 0.05, "max_ece": 0.1, "ece_bins": 15}},
            "claim_note": "x" * 500}


def synth_draft(fx):
    return {"reference": {"device": fx["device"], "epochs": fx["epochs"], "recipe_id": fx["recipe_id"],
                          "per_seed_margin": {str(r["seed"]): r["margin"] for r in fx["per_seed"]},
                          "per_seed_ece_post": {str(r["seed"]): r["ece_post"] for r in fx["per_seed"]},
                          "three_seed_mean_margin": round(fx["three_seed_mean_margin"], 4),
                          "shipped_seed": fx["shipped_seed"], "shipped_ece_post": fx["shipped_ece_post"],
                          "clamp_hit_seeds": [r["seed"] for r in fx["per_seed"] if r["clamp_hit"]]},
            "must": {"inputs_sha256": dict(fx["inputs_sha256"])}}


def synth_decl(fx, fixture_path=None, fixture_sha=None):
    mean = fx["three_seed_mean_margin"]
    d = {"schema": "laya-reproduction-declaration-v1", "plan": "selftest",
         "must": {"deploy_eligible": True, "device_prefix": "cuda", "recipe_id": fx["recipe_id"],
                  "inputs_sha256": dict(fx["inputs_sha256"]), "seeds": list(fx["seeds"])},
         "band": {"margin": [round(mean - 0.03, 3), round(mean + 0.03, 3)], "ece_post": [0.0, 0.10]},
         "judging": {"margin_statistic": "seed_mean", "note": "selftest"},
         "retry": {"attempt_cap": 1, "retry_classes": [], "rule": "selftest"},
         "waiver_rule": "inconclusive"}
    if fixture_path is not None:
        d["reference_fixture"] = {"path": fixture_path, "sha256": fixture_sha, "source_evidence_sha256": fx["source"]["sha256"]}
    return d


def _walk_text_free(x, path=""):
    """None when x holds no text-bearing key and no string over 200 characters, else the offending path."""
    if isinstance(x, dict):
        for k, v in x.items():
            if k.lower() in ("text", "tweet", "content", "row", "rows_text", "claim_note", "notes", "note_text"):
                return path + k
            bad = _walk_text_free(v, path + k + ".")
            if bad:
                return bad
    elif isinstance(x, list):
        for i, v in enumerate(x):
            bad = _walk_text_free(v, path + "[%d]." % i)
            if bad:
                return bad
    elif isinstance(x, str) and len(x) > 200:
        return path
    return None


def selftest():
    c = Cases()
    work = tempfile.mkdtemp(prefix="laya-control-selftest-")
    try:
        ev = synth_evidence()
        evp = os.path.join(work, "evidence", "10-GATE-RUN-EVIDENCE.json")
        _write(evp, _dumps(ev))
        ev_raw = open(evp, "rb").read()
        ev_sha = hashlib.sha256(ev_raw).hexdigest()
        src_path = "evidence/10-GATE-RUN-EVIDENCE.json"

        # --- snapshot ---------------------------------------------------------------------------------------
        snap = lambda e=ev, sha=ev_sha: build_snapshot(e, src_path, sha)
        c.check("snapshot: deterministic bytes on two runs", lambda: _dumps(snap()) == _dumps(snap()) and _dumps(snap()).endswith("}\n"))
        c.check("snapshot: no text-bearing key and no string over 200 characters",
                lambda: (_walk_text_free(snap()) is None and "MUST NOT BE COPIED" not in _dumps(snap()), str(_walk_text_free(snap()))))
        c.check("snapshot: per-seed numbers copied unrounded and the mean is fsum/3",
                lambda: [r["margin"] for r in snap()["per_seed"]] == [0.20, 0.22, 0.21]
                and snap()["three_seed_mean_margin"] == math.fsum([0.20, 0.22, 0.21]) / 3
                and snap()["shipped_ece_post"] == 0.042 and snap()["inputs_sha256"]["train_jsonl"] == "22" * 32)

        def two_seed():
            e = synth_evidence()
            e["seeds"]["per_seed"].pop()
            return build_snapshot(e, src_path, ev_sha)
        c.refused("snapshot: a two-seed evidence is refused", two_seed, "reference-evidence")

        def other_seeds():
            e = synth_evidence()
            e["seeds"]["per_seed"][0]["seed"] = 14
            return build_snapshot(e, src_path, ev_sha)
        c.refused("snapshot: seeds other than 13, 17, 23 are refused", other_seeds, "reference-evidence")

        def lacks_field():
            e = synth_evidence()
            del e["seeds"]["per_seed"][1]["best_epoch"]
            return build_snapshot(e, src_path, ev_sha)
        c.refused("snapshot: a per-seed row lacking best_epoch is refused", lacks_field, "reference-evidence")

        def dup_key():
            return load_evidence_bytes(b'{"a": 1, "a": 2}', "x.json")
        c.refused("snapshot: a duplicate JSON key is refused", dup_key, "reference-evidence")
        c.refused("snapshot: a non-finite number is refused", lambda: load_evidence_bytes(b'{"a": NaN}', "x.json"), "reference-evidence")

        # --- check-reference --------------------------------------------------------------------------------
        fxp = os.path.join(work, "fixture", "16-REFERENCE-MPS.json")
        _write(fxp, _dumps(snap()))
        fx = json.loads(open(fxp, "rb").read().decode())
        drp = os.path.join(work, "draft.json")
        _write(drp, _dumps(synth_draft(fx)))

        def mutated(edit, name):
            d = json.loads(json.dumps(fx))
            edit(d)
            p = os.path.join(work, "fixture", name)
            _write(p, _dumps(d))
            return p

        c.check("check-reference: the fixture agrees with its evidence (accept)",
                lambda: check_reference(fxp, evp, None).startswith("REFERENCE OK source_sha256=%s " % ev_sha[:12]))
        c.check("check-reference: and with the draft (accept)", lambda: check_reference(fxp, evp, drp).startswith("REFERENCE OK "))
        c.refused("check-reference: one changed margin digit is drift",
                  lambda: check_reference(mutated(lambda d: d["per_seed"][0].__setitem__("margin", 0.2000000001), "m1.json"), evp, None),
                  "reference-drift")
        c.refused("check-reference: a changed source sha256 is refused",
                  lambda: check_reference(mutated(lambda d: d["source"].__setitem__("sha256", "0" * 64), "m2.json"), evp, None),
                  "reference-source")
        c.refused("check-reference: a missing seed is drift",
                  lambda: check_reference(mutated(lambda d: d["per_seed"].pop(), "m3.json"), evp, None), "reference-drift")
        c.refused("check-reference: a changed device is drift",
                  lambda: check_reference(mutated(lambda d: d.__setitem__("device", "cuda:0"), "m4.json"), evp, None), "reference-drift")

        def bad_draft(edit):
            dr = synth_draft(fx)
            edit(dr)
            p = os.path.join(work, "draft-bad.json")
            _write(p, _dumps(dr))
            return check_reference(fxp, evp, p)
        c.refused("check-reference: a draft whose margin differs is refused",
                  lambda: bad_draft(lambda dr: dr["reference"]["per_seed_margin"].__setitem__("17", 0.5)), "reference-draft")
        c.refused("check-reference: a draft whose input digest differs is refused",
                  lambda: bad_draft(lambda dr: dr["must"]["inputs_sha256"].__setitem__("eval_jsonl", "0" * 64)), "reference-draft")
        c.refused("check-reference: a draft whose clamp seeds differ is refused",
                  lambda: bad_draft(lambda dr: dr["reference"].__setitem__("clamp_hit_seeds", [])), "reference-draft")

        def other_evidence():
            e = synth_evidence(margins=(0.20, 0.22, 0.2100001))
            p = os.path.join(work, "evidence", "other.json")
            _write(p, _dumps(e))
            return check_reference(fxp, p, None)
        c.refused("check-reference: an evidence file other than the pinned one is refused", other_evidence, "reference-source")

        # --- rehearse ---------------------------------------------------------------------------------------
        dcp = os.path.join(work, "decl.json")
        _write(dcp, _dumps(synth_decl(fx, "fixture/16-REFERENCE-MPS.json", hashlib.sha256(open(fxp, "rb").read()).hexdigest())))
        try:
            results = rehearse(dcp, fxp)
        except Exception as ex:
            results = []
            c.check("rehearse: the table runs", lambda: (False, "crashed with %r" % (ex,)))
        for r in results:
            c.check("rehearse:" + r["name"], lambda r=r: (r["ok"], r["detail"]))
        c.check("rehearse: at least the in-band row ran", lambda: len(results) >= 1)

        def unknown_clause():
            d = synth_decl(fx)
            d["must"]["gate_pass_unwaived"] = True
            p = os.path.join(work, "decl-unknown.json")
            _write(p, _dumps(d))
            return rehearse(p, fxp)
        c.refused("rehearse: a declaration with an unknown must clause is refused", unknown_clause, "rehearse-declaration")

        def other_recipe():
            d = synth_decl(fx)
            d["must"]["recipe_id"] = "cd" * 32
            p = os.path.join(work, "decl-recipe.json")
            _write(p, _dumps(d))
            return rehearse(p, fxp)
        c.refused("rehearse: a declaration whose recipe differs from the reference is refused", other_recipe, "rehearse-reference")

        # --- the tracer: main() end to end over a temp directory ------------------------------------------------
        def drive(argv):
            o, e = [], []
            rc = main(argv, out=o.append, err=e.append)
            return rc, o, e
        os.makedirs(os.path.join(work, "tracer"))
        tfx = os.path.join(work, "tracer", "ref.json")
        rc1, o1, e1 = drive(["snapshot-reference", "--evidence", evp, "--out", tfx])
        c.check("tracer:snapshot-reference writes the fixture through main()",
                lambda: (rc1 == 0 and os.path.isfile(tfx) and open(tfx, "rb").read().decode() == _dumps(build_snapshot(ev, evp, ev_sha)), str(e1)))
        rc2, o2, e2 = drive(["check-reference", "--fixture", tfx, "--evidence", evp])
        c.check("tracer:check-reference accepts that fixture through main()",
                lambda: (rc2 == 0 and len(o2) == 1 and o2[0].startswith("REFERENCE OK source_sha256="), str(e2)))
        tdc = os.path.join(work, "tracer", "decl.json")
        _write(tdc, _dumps(synth_decl(json.loads(open(tfx, "rb").read().decode()))))
        rc3, o3, e3 = drive(["rehearse", "--declaration", tdc, "--reference", tfx])
        c.check("tracer:rehearse judges a synthetic CUDA run by the committed evaluator through main()",
                lambda: (rc3 == 0 and bool(o3) and o3[-1].startswith("REHEARSAL OK cases="), "%s %s" % (o3[-1:], e3)))
        os.remove(tfx)
        rc4, o4, e4 = drive(["check-reference", "--fixture", tfx, "--evidence", evp])
        c.check("tracer:a cut link (the fixture removed) is refused, never passed", lambda: rc4 == 2 and bool(e4) and e4[0].startswith("REFUSED "))
    finally:
        shutil.rmtree(work, ignore_errors=True)

    if c.failed:
        print("LAYA CONTROL SELFTEST FAILED")
        return 1
    print("LAYA CONTROL SELFTEST OK cases=%d" % c.n)
    return 0


# ---- command line ------------------------------------------------------------------------------------------------

COMMANDS = {
    "snapshot-reference": (cmd_snapshot_reference, {"--evidence": True, "--out": True}),
    "check-reference": (cmd_check_reference, {"--fixture": True, "--evidence": True, "--draft": False}),
    "rehearse": (cmd_rehearse, {"--declaration": True, "--reference": True, "--json": "flag"}),
}


def parse_opts(args, spec):
    opts = {}
    i = 0
    while i < len(args):
        k = args[i]
        if k not in spec:
            raise Refused("usage", "unknown option %r" % k)
        if k in opts:
            raise Refused("usage", "%s given twice" % k)
        if spec[k] == "flag":
            opts[k] = True
            i += 1
            continue
        if i + 1 >= len(args):
            raise Refused("usage", "%s needs a value" % k)
        opts[k] = args[i + 1]
        i += 2
    for k, required in spec.items():
        if required is True and k not in opts:
            raise Refused("usage", "%s is required" % k)
    return opts


def main(argv, out=None, err=None):
    out = out or (lambda s: print(s, flush=True))
    err = err or (lambda s: print(s, file=sys.stderr, flush=True))
    try:
        if argv == ["--selftest"]:
            return selftest()
        if not argv or argv[0] not in COMMANDS:
            raise Refused("usage", "expected --selftest or one of %s" % ", ".join(sorted(COMMANDS)))
        fn, spec = COMMANDS[argv[0]]
        return fn(parse_opts(argv[1:], spec), out)
    except Refused as ex:
        err("REFUSED %s: %s" % (ex.rule, ex.reason))
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
