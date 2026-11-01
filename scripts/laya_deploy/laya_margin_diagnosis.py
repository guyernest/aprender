#!/usr/bin/env python3
"""Name the CLASS of the SC4 margin miss from evidence already on disk (plan 15-09; read by 15-10, 15-11, 15-12).

    python3 -I scripts/laya_deploy/laya_margin_diagnosis.py snapshot --out FILE [--local-root DIR]
    python3 -I scripts/laya_deploy/laya_margin_diagnosis.py verify-inputs --inputs FILE
    python3 -I scripts/laya_deploy/laya_margin_diagnosis.py --selftest

`snapshot` is the ONLY step that reads gitignored files (the fetched run directories and the data directory, both
only in the main checkout). It keeps class indices and floats and nothing else: no eval text, no text hash. Every
other subcommand is PURE over tracked files: no network, no subprocess, no clock, no environment.

`verify-inputs` recomputes all nine recorded seed-run margins, macro-F1s and post-calibration ECEs from the fixture
with first-principles transcriptions of scripts/laya_train/metrics.py and requires each to equal the tracked gate-run
evidence at an absolute difference of at most 1e-12.

Exit codes: 0 ok, 1 selftest failure, 2 refusal printed as exactly one `REFUSED <rule>: <reason>` line.
stdlib only. This file grows across plan 15-09 tasks; each section says which task added it.
"""

import hashlib
import json
import math
import os
import subprocess
import sys
import unicodedata

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

MAX_FILE = 8 * 1024 * 1024
MAX_STRING = 200
TOL = 1e-12
ECE_BINS = 15
EVAL_N = 100
SEEDS = (13, 17, 23)
RUN_IDS = ("p12", "p14", "p15")
INPUTS_SCHEMA = "laya-margin-inputs-v1"
DATA_DIR = "data/decide/motivation-v1"

_PH12 = ".planning/phases/12-motivation-v1-decide-server-for-experiment-coach-laya-pilot"
_PH14 = ".planning/phases/14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out"
_PH15 = ".planning/phases/15-laya-pack-bound-amendment-per-study-d-15-and-the-motivation"

# The three runs, one table. The local run directory is NEVER written here: it is read from the tracked evidence
# (`run_dir` for phase 12, `attempts[0].run_dir` for phases 14 and 15).
RUNS = {
    "p12": {"phase": "12", "epochs": 12, "evidence": _PH12 + "/12-GATE-RUN-EVIDENCE.json", "run_dir_path": ("run_dir",)},
    "p14": {"phase": "14", "epochs": 12, "evidence": _PH14 + "/14-GATE-RUN-EVIDENCE.json", "run_dir_path": ("attempts", 0, "run_dir")},
    "p15": {"phase": "15", "epochs": 4, "evidence": _PH15 + "/15-GATE-RUN-EVIDENCE.json", "run_dir_path": ("attempts", 0, "run_dir")},
}


class Refused(Exception):
    def __init__(self, rule, reason):
        Exception.__init__(self, "%s: %s" % (rule, reason))
        self.rule = rule
        self.reason = reason


# ---- reading and writing JSON ------------------------------------------------------------------------------------

def _pairs(pairs):
    d = {}
    for k, v in pairs:
        if k in d:
            raise ValueError("duplicate key %r" % k)
        d[k] = v
    return d


def _const(name):
    raise ValueError("non-finite number %s" % name)


def parse_json_bytes(raw, what="input"):
    """A duplicate-key-refusing, NaN-refusing parser; anything over 8 MiB is refused."""
    if len(raw) > MAX_FILE:
        raise Refused("json", "%s is larger than %d bytes" % (what, MAX_FILE))
    try:
        return json.loads(raw.decode("utf-8"), object_pairs_hook=_pairs, parse_constant=_const)
    except (ValueError, UnicodeDecodeError) as ex:
        raise Refused("json", "%s is not valid JSON (%s)" % (what, str(ex)[:80]))


def read_bytes(path, rule):
    if os.path.islink(path) or not os.path.isfile(path):
        raise Refused(rule, "%s is missing" % path)
    if os.path.getsize(path) > MAX_FILE:
        raise Refused(rule, "%s is larger than %d bytes" % (path, MAX_FILE))
    with open(path, "rb") as f:
        return f.read()


def read_json(path, rule):
    return parse_json_bytes(read_bytes(path, rule), path)


def sha256_hex(raw):
    return hashlib.sha256(raw).hexdigest()


def dumps(obj):
    return json.dumps(obj, sort_keys=True, indent=2) + "\n"


def write_text(path, text):
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)


def is_num(x):
    return isinstance(x, (int, float)) and not isinstance(x, bool) and math.isfinite(x)


def is_int(x):
    return isinstance(x, int) and not isinstance(x, bool)


# ---- metrics, transcribed from scripts/laya_train/metrics.py in the same operation order -----------------------------

def first_max(row):
    """Index of the first maximum (numpy argmax: the lowest index wins a tie)."""
    best = 0
    for i in range(1, len(row)):
        if row[i] > row[best]:
            best = i
    return best


def _f1(pred, y, label):
    tp = sum(1 for p, t in zip(pred, y) if p == label and t == label)
    fp = sum(1 for p, t in zip(pred, y) if p == label and t != label)
    fn = sum(1 for p, t in zip(pred, y) if p != label and t == label)
    den = 2 * tp + fp + fn
    return 0.0 if den == 0 else 2.0 * tp / den


def macro_f1(y, pred):
    """math.fsum of per-label F1 over sorted(set(y) | set(pred)), divided by their count."""
    labels = sorted(set(y) | set(pred))
    return math.fsum(_f1(pred, y, c) for c in labels) / len(labels)


def ece_top_label(y, pred, conf, bins=ECE_BINS):
    """ece_top_label: bin = min(floor(conf * bins), bins - 1); sum_b (n_b / n) * |acc_b - conf_b| via math.fsum."""
    n = len(y)
    idx = [min(int(math.floor(c * bins)), bins - 1) for c in conf]
    terms = []
    for b in range(bins):
        sel = [i for i in range(n) if idx[i] == b]
        nb = len(sel)
        if nb:
            acc = math.fsum(1.0 if pred[i] == y[i] else 0.0 for i in sel) / nb
            mean_conf = math.fsum(conf[i] for i in sel) / nb
            terms.append((nb / n) * abs(acc - mean_conf))
    return math.fsum(terms)


def margin_of(y, pred, zs_pred):
    return macro_f1(y, pred) - macro_f1(y, zs_pred)


# ---- the fixture -------------------------------------------------------------------------------------------------

def walk_leaks(o, where="$"):
    """The first leak in a fixture: a key named text or texts, or a string over MAX_STRING characters."""
    if isinstance(o, dict):
        for k, v in o.items():
            if k in ("text", "texts"):
                return "key named %r at %s" % (k, where)
            if isinstance(k, str) and len(k) > MAX_STRING:
                return "a key over %d characters at %s" % (MAX_STRING, where)
            found = walk_leaks(v, where + "." + str(k))
            if found:
                return found
    elif isinstance(o, list):
        for i, v in enumerate(o):
            found = walk_leaks(v, "%s[%d]" % (where, i))
            if found:
                return found
    elif isinstance(o, str) and len(o) > MAX_STRING:
        return "a string over %d characters at %s" % (MAX_STRING, where)
    return None


def _bad(rule, reason):
    raise Refused(rule, reason)


def _evidence_row(doc, seed):
    for r in ((doc.get("seeds") or {}).get("per_seed") or []):
        if isinstance(r, dict) and r.get("seed") == seed:
            return r
    return None


def verify_inputs_obj(fx, evidence_loader):
    """Pure over the fixture and the evidence. evidence_loader(run_id, path) -> raw bytes. Returns a summary dict or
    raises Refused('inputs', reason)."""
    if not isinstance(fx, dict) or fx.get("schema") != INPUTS_SCHEMA:
        _bad("inputs", "SCHEMA the fixture schema is not %r" % INPUTS_SCHEMA)
    leak = walk_leaks(fx)
    if leak:
        _bad("inputs", "LEAK %s" % leak)
    ev, zs, runs, srs = fx.get("eval"), fx.get("zero_shot"), fx.get("runs"), fx.get("seed_runs")
    if not (isinstance(ev, dict) and isinstance(zs, dict) and isinstance(runs, dict) and isinstance(srs, list)):
        _bad("inputs", "COUNT the fixture lacks eval, zero_shot, runs or seed_runs")
    if ev.get("n") != EVAL_N:
        _bad("inputs", "COUNT eval.n is %r, expected %d" % (ev.get("n"), EVAL_N))
    y, zpred = ev.get("y"), zs.get("pred")
    for name, arr in (("eval.y", y), ("zero_shot.pred", zpred)):
        if not (isinstance(arr, list) and len(arr) == EVAL_N and all(is_int(v) and 0 <= v <= 2 for v in arr)):
            _bad("inputs", "COUNT %s is not %d class indices in 0..2" % (name, EVAL_N))
    counts = [y.count(c) for c in range(3)]
    if ev.get("class_counts") != counts:
        _bad("inputs", "COUNT eval.class_counts %r differ from the labels' %r" % (ev.get("class_counts"), counts))
    if sorted(runs) != sorted(RUN_IDS):
        _bad("inputs", "COUNT runs are %r, expected %r" % (sorted(runs), sorted(RUN_IDS)))
    want = [(r, s) for r in RUN_IDS for s in SEEDS]
    have = [(e.get("run"), e.get("seed")) if isinstance(e, dict) else None for e in srs]
    if sorted(have, key=repr) != sorted(want, key=repr) or len(srs) != 9:
        _bad("inputs", "COUNT seed_runs are not exactly the nine (run, seed) pairs of %s x %s" % (list(RUN_IDS), list(SEEDS)))
    for e in srs:
        for name in ("pred", "conf"):
            if not isinstance(e.get(name), list) or len(e[name]) != EVAL_N:
                _bad("inputs", "COUNT run=%s seed=%s %s is not %d long" % (e["run"], e["seed"], name, EVAL_N))
        if not all(is_int(v) and 0 <= v <= 2 for v in e["pred"]):
            _bad("inputs", "COUNT run=%s seed=%s pred holds a value outside 0..2" % (e["run"], e["seed"]))
        if not all(is_num(c) and 0.0 < c <= 1.0 for c in e["conf"]):
            _bad("inputs", "COUNT run=%s seed=%s conf holds a value outside (0, 1]" % (e["run"], e["seed"]))
    # Evidence digests, then the context each run claims.
    evid = {}
    for rid in RUN_IDS:
        r = runs[rid]
        if not isinstance(r, dict) or r.get("evidence_path") != RUNS[rid]["evidence"]:
            _bad("inputs", "COUNT runs.%s.evidence_path is not %s" % (rid, RUNS[rid]["evidence"]))
        raw = evidence_loader(rid, r["evidence_path"])
        if sha256_hex(raw) != r.get("evidence_sha256"):
            _bad("inputs", "EVIDENCE-DIGEST run=%s the tracked evidence digest differs from the recorded one" % rid)
        doc = parse_json_bytes(raw, r["evidence_path"])
        evid[rid] = doc
        ctx = (("epochs", RUNS[rid]["epochs"], doc.get("epochs")), ("epochs", r.get("epochs"), doc.get("epochs")),
               ("device", r.get("device"), doc.get("device_used")), ("recipe_id", r.get("recipe_id"), doc.get("recipe_id_written")),
               ("shipped_seed", r.get("shipped_seed"), (doc.get("seeds") or {}).get("shipped")))
        for name, a, b in ctx:
            if a != b:
                _bad("inputs", "EVIDENCE-CONTEXT run=%s %s is %r in the fixture and %r in the evidence" % (rid, name, a, b))
    # Zero-shot, then every seed-run.
    zs_f1 = macro_f1(y, zpred)
    if not is_num(zs.get("macro_f1")) or abs(zs_f1 - zs["macro_f1"]) > TOL:
        _bad("inputs", "ZERO-SHOT MISMATCH recomputed %r, recorded %r" % (zs_f1, zs.get("macro_f1")))
    for rid in RUN_IDS:
        rec = (evid[rid].get("gate") or {}).get("zero_shot_macro_f1")
        if not is_num(rec) or abs(zs_f1 - rec) > TOL:
            _bad("inputs", "ZERO-SHOT MISMATCH run=%s recomputed %r, evidence gate.zero_shot_macro_f1 %r" % (rid, zs_f1, rec))
    margins_exact = ece_exact = 0
    for e in sorted(srs, key=lambda e: (RUN_IDS.index(e["run"]), e["seed"])):
        row = _evidence_row(evid[e["run"]], e["seed"])
        if row is None:
            _bad("inputs", "EVIDENCE-CONTEXT run=%s the evidence has no per_seed row for seed %s" % (e["run"], e["seed"]))
        f1 = macro_f1(y, e["pred"])
        margin = f1 - zs_f1
        ece = ece_top_label(y, e["pred"], e["conf"])
        for name, got, rec in (("MARGIN", margin, row.get("margin")), ("MACRO-F1", f1, row.get("macro_f1"))):
            if not is_num(rec) or abs(got - rec) > TOL:
                _bad("inputs", "%s MISMATCH run=%s seed=%s recomputed %r, evidence %r" % (name, e["run"], e["seed"], got, rec))
        margins_exact += 1
        if not is_num(row.get("ece_post")) or abs(ece - row["ece_post"]) > TOL:
            _bad("inputs", "ECE MISMATCH run=%s seed=%s recomputed %r, evidence ece_post %r" % (e["run"], e["seed"], ece, row.get("ece_post")))
        ece_exact += 1
    return {"seed_runs": len(srs), "margins_exact": margins_exact, "ece_exact": ece_exact, "zero_shot_f1": zs_f1}


def tracked_evidence_loader(rid, path):
    return read_bytes(os.path.join(REPO, path), "inputs")


def cmd_verify_inputs(opts):
    raw = read_bytes(opts["--inputs"], "inputs")
    fx = parse_json_bytes(raw, opts["--inputs"])
    res = verify_inputs_obj(fx, tracked_evidence_loader)
    print("INPUTS OK seed_runs=%d margins_exact=%d ece_exact=%d zero_shot_f1=%r fixture_sha256=%s"
          % (res["seed_runs"], res["margins_exact"], res["ece_exact"], res["zero_shot_f1"], sha256_hex(raw)[:12]))
    return 0


# ---- snapshot: the ONLY step that reads gitignored files ---------------------------------------------------------

def _git(*args):
    return subprocess.run(("git",) + args, cwd=REPO, check=True, capture_output=True, text=True).stdout.strip()


def default_local_root():
    """The main checkout: the parent of the git common dir; else the repository root."""
    try:
        common = _git("rev-parse", "--path-format=absolute", "--git-common-dir")
        return os.path.dirname(common.rstrip("/")) if common else REPO
    except (OSError, subprocess.CalledProcessError):
        return REPO


def _dig(doc, path):
    cur = doc
    for k in path:
        try:
            cur = cur[k]
        except (KeyError, IndexError, TypeError):
            raise Refused("snapshot", "the tracked evidence lacks %s" % "/".join(str(p) for p in path))
    return cur


def _resolve(run_dir, rel):
    cands = [os.path.join(run_dir, "run", rel), os.path.join(run_dir, rel)]
    for c in cands:
        if os.path.isfile(c) and not os.path.islink(c):
            return c
    raise Refused("snapshot", "%s is missing (also tried %s)" % (cands[0], cands[1]))


def _norm_digests(text):
    return {hashlib.sha256(text.encode("utf-8")).hexdigest(),
            hashlib.sha256(unicodedata.normalize("NFC", text).strip().encode("utf-8")).hexdigest()}


def _read_probs(path, labels, eval_rows):
    """(pred, conf, sha256) of one probabilities file; proves row alignment 100 of 100 by text hash."""
    raw = read_bytes(path, "snapshot")
    doc = parse_json_bytes(raw, path)
    if not isinstance(doc, dict) or doc.get("labels") != labels:
        _bad("snapshot", "%s labels are not %r" % (path, labels))
    rows = doc.get("rows")
    if not isinstance(rows, list) or len(rows) != EVAL_N:
        _bad("snapshot", "%s does not hold %d rows" % (path, EVAL_N))
    pred, conf = [], []
    for i, r in enumerate(rows):
        probs = r.get("probabilities") if isinstance(r, dict) else None
        if r.get("row") != i or not isinstance(probs, list) or len(probs) != len(labels) or not all(is_num(p) for p in probs):
            _bad("snapshot", "%s row %d is malformed" % (path, i))
        if r.get("text_sha256") not in _norm_digests(eval_rows[i]["text"]):
            _bad("snapshot", "%s row %d is not aligned with the eval row (text_sha256 differs)" % (path, i))
        k = first_max(probs)
        pred.append(k)
        conf.append(probs[k])
    return pred, conf, sha256_hex(raw)


def build_snapshot(local_root, evidence_loader=tracked_evidence_loader):
    task = read_json(os.path.join(local_root, DATA_DIR, "task.json"), "snapshot")
    labels = list((task.get("criteria") or {}).keys())
    if labels != ["social_affiliative", "conspiratorial", "other"]:
        _bad("snapshot", "the task's criteria order is %r" % labels)
    eval_raw = read_bytes(os.path.join(local_root, DATA_DIR, "eval.jsonl"), "snapshot")
    eval_sha = sha256_hex(eval_raw)
    eval_rows = []
    for line in eval_raw.decode("utf-8").splitlines():
        if line.strip():
            eval_rows.append(parse_json_bytes(line.encode("utf-8"), "an eval row"))
    if len(eval_rows) != EVAL_N:
        _bad("snapshot", "eval.jsonl holds %d rows, expected %d" % (len(eval_rows), EVAL_N))
    y = []
    for r in eval_rows:
        if r.get("label") not in labels:
            _bad("snapshot", "an eval row has a label outside the task's criteria")
        y.append(labels.index(r["label"]))
    runs, seed_runs, zero_pred, zero_shas = {}, [], None, {}
    for rid in RUN_IDS:
        spec = RUNS[rid]
        raw = evidence_loader(rid, spec["evidence"])
        doc = parse_json_bytes(raw, spec["evidence"])
        if (doc.get("inputs_sha256") or {}).get("eval_jsonl") != eval_sha:
            _bad("snapshot", "the local eval.jsonl sha256 differs from run %s inputs_sha256.eval_jsonl" % rid)
        run_dir = os.path.join(local_root, _dig(doc, spec["run_dir_path"]))
        runs[rid] = {"phase": spec["phase"], "device": doc.get("device_used"), "epochs": spec["epochs"],
                     "recipe_id": doc.get("recipe_id_written"), "shipped_seed": (doc.get("seeds") or {}).get("shipped"),
                     "evidence_path": spec["evidence"], "evidence_sha256": sha256_hex(raw)}
        zp, _zc, zsha = _read_probs(_resolve(run_dir, "zero-shot-probs.json"), labels, eval_rows)
        zero_shas[rid] = zsha
        if zero_pred is None:
            zero_pred = zp
        elif zp != zero_pred:
            _bad("snapshot", "the zero-shot predictions of run %s differ from the other runs'" % rid)
        zf1 = macro_f1(y, zp)
        rec = (doc.get("gate") or {}).get("zero_shot_macro_f1")
        if not is_num(rec) or abs(zf1 - rec) > TOL:
            _bad("snapshot", "run %s zero-shot macro-F1 %r differs from the evidence's %r" % (rid, zf1, rec))
        var = read_json(_resolve(run_dir, "variance-report.json"), "snapshot")
        var_rows = {r.get("seed"): r for r in (var.get("per_seed") or []) if isinstance(r, dict)}
        for seed in SEEDS:
            path = _resolve(run_dir, os.path.join("seeds", "seed-%d" % seed, "eval-probs.json"))
            pred, conf, sha = _read_probs(path, labels, eval_rows)
            if rid == "p12":
                want = (_evidence_row(doc, seed) or {}).get("eval_probs_sha256")
                if want != sha:
                    _bad("snapshot", "phase 12 seed %d: the local eval-probs.json sha256 differs from the evidence's" % seed)
            vr = var_rows.get(seed)
            if vr is None:
                _bad("snapshot", "variance-report.json of run %s has no row for seed %d" % (rid, seed))
            seed_runs.append({"run": rid, "seed": seed, "pred": pred, "conf": conf,
                              "local": {"best_epoch": vr.get("best_epoch"), "epochs_run": vr.get("epochs_run"),
                                        "clamp_hit": vr.get("clamp_hit")},
                              "source_sha256": sha})
    local_kind = "main" if os.path.abspath(local_root) == os.path.abspath(REPO) else "worktree"
    counts = [y.count(c) for c in range(3)]
    return {"schema": INPUTS_SCHEMA,
            "eval": {"n": EVAL_N, "labels": labels, "y": y, "eval_jsonl_sha256": eval_sha, "class_counts": counts},
            "zero_shot": {"pred": zero_pred, "macro_f1": macro_f1(y, zero_pred), "source_sha256": zero_shas},
            "runs": runs, "seed_runs": seed_runs, "alignment_checked_rows": EVAL_N, "local_root_kind": local_kind}


def cmd_snapshot(opts):
    root = opts.get("--local-root") or default_local_root()
    fx = build_snapshot(root)
    write_text(opts["--out"], dumps(fx))
    print("SNAPSHOT OK seed_runs=%d alignment_checked_rows=%d local_root_kind=%s"
          % (len(fx["seed_runs"]), fx["alignment_checked_rows"], fx["local_root_kind"]))
    return 0


# ---- the selftest ------------------------------------------------------------------------------------------------

class Cases:
    def __init__(self):
        self.n = 0
        self.failed = []

    def check(self, label, ok, detail=""):
        self.n += 1
        print("  %-4s %s%s" % ("ok" if ok else "FAIL", label, ("  " + detail) if (detail and not ok) else ""))
        if not ok:
            self.failed.append(label)

    def refused(self, label, fn, needle):
        """The call must raise Refused whose text contains needle."""
        try:
            fn()
        except Refused as ex:
            ok = needle in str(ex)
            self.check(label, ok, "refused with %r, wanted %r" % (str(ex)[:100], needle))
            return
        except Exception as ex:  # a crash is not a refusal
            self.check(label, False, "crashed with %r" % (ex,))
            return
        self.check(label, False, "was accepted")


def synth_fixture(seed=7):
    """A synthetic nine-seed-run fixture plus the evidence bytes that agree with it, built with the same functions
    verify-inputs uses (no gitignored read)."""
    import random
    rng = random.Random(seed)
    labels = ["social_affiliative", "conspiratorial", "other"]
    y = [rng.choice([0, 1, 2, 2]) for _ in range(EVAL_N)]
    zpred = [rng.choice([0, 1, 2]) for _ in range(EVAL_N)]
    zs_f1 = macro_f1(y, zpred)
    fx = {"schema": INPUTS_SCHEMA,
          "eval": {"n": EVAL_N, "labels": labels, "y": y, "eval_jsonl_sha256": "0" * 64,
                   "class_counts": [y.count(c) for c in range(3)]},
          "zero_shot": {"pred": zpred, "macro_f1": zs_f1, "source_sha256": {r: "1" * 64 for r in RUN_IDS}},
          "runs": {}, "seed_runs": [], "alignment_checked_rows": EVAL_N, "local_root_kind": "main"}
    evidence = {}
    for rid in RUN_IDS:
        per_seed = []
        for s in SEEDS:
            pred = [t if rng.random() < 0.7 else rng.choice([0, 1, 2]) for t in y]
            conf = [round(rng.uniform(0.34, 0.99), 6) for _ in y]
            fx["seed_runs"].append({"run": rid, "seed": s, "pred": pred, "conf": conf,
                                    "local": {"best_epoch": 1, "epochs_run": 4, "clamp_hit": False}, "source_sha256": "2" * 64})
            f1 = macro_f1(y, pred)
            per_seed.append({"seed": s, "macro_f1": f1, "margin": f1 - zs_f1, "ece_post": ece_top_label(y, pred, conf)})
        doc = {"epochs": RUNS[rid]["epochs"], "device_used": "mps:0" if rid == "p12" else "cuda:0",
               "recipe_id_written": "ab" * 32, "seeds": {"shipped": 13, "per_seed": per_seed},
               "gate": {"zero_shot_macro_f1": zs_f1}}
        raw = json.dumps(doc, sort_keys=True).encode("utf-8")
        evidence[rid] = raw
        fx["runs"][rid] = {"phase": RUNS[rid]["phase"], "device": doc["device_used"], "epochs": RUNS[rid]["epochs"],
                           "recipe_id": "ab" * 32, "shipped_seed": 13, "evidence_path": RUNS[rid]["evidence"],
                           "evidence_sha256": sha256_hex(raw)}
    return fx, evidence


def _clone(o):
    return json.loads(json.dumps(o))


def _verify(fx, evidence):
    return verify_inputs_obj(fx, lambda rid, path: evidence[rid])


def selftest_inputs(c):
    """Task 1 cases: hand-computed anchors, a good synthetic fixture, and the red side."""
    ident = list(range(3)) * 2
    c.check("anchor: perfect predictions give macro-F1 1.0", macro_f1(ident, ident) == 1.0)
    c.check("anchor: one class everywhere on six rows, two per class, gives macro-F1 0.5/3",
            macro_f1(ident, [0] * 6) == math.fsum([0.5, 0.0, 0.0]) / 3, "got %r" % macro_f1(ident, [0] * 6))
    c.check("anchor: confidence 1.0 with half correct gives ECE 0.5",
            abs(ece_top_label([0, 0, 0, 0], [0, 0, 1, 1], [1.0] * 4) - 0.5) < TOL)
    c.check("anchor: four lone bins give ECE 1.53/4 (metrics.py case 3)",
            abs(ece_top_label([0, 1, 0, 1], [0, 0, 0, 1], [0.9, 0.75, 0.62, 0.7]) - 1.53 / 4) < TOL)
    c.check("anchor: the same four rows give macro-F1 (0.8 + 2/3)/2",
            abs(macro_f1([0, 1, 0, 1], [0, 0, 0, 1]) - (0.8 + 2.0 / 3.0) / 2) < 1e-15)
    c.check("first_max: the lowest index wins a tie", first_max([0.5, 0.5, 0.0]) == 0 and first_max([0.1, 0.3, 0.3]) == 1)
    fx, evidence = synth_fixture()
    try:
        res = _verify(fx, evidence)
        c.check("a synthetic fixture built from the same functions verifies OK (9 margins, 9 ECEs exact)",
                res["seed_runs"] == 9 and res["margins_exact"] == 9 and res["ece_exact"] == 9)
    except Refused as ex:
        c.check("a synthetic fixture built from the same functions verifies OK (9 margins, 9 ECEs exact)", False, str(ex))
    c.check("dumps is deterministic and re-parses to the same object",
            dumps(fx) == dumps(parse_json_bytes(dumps(fx).encode("utf-8"))))

    def mutated(fn):
        m = _clone(fx)
        fn(m)
        return m

    def flip_y(m):
        m["eval"]["y"][0] = (m["eval"]["y"][0] + 1) % 3
        m["eval"]["class_counts"] = [m["eval"]["y"].count(k) for k in range(3)]

    def flip_pred(m):
        e = m["seed_runs"][4]
        for i, (p, t) in enumerate(zip(e["pred"], m["eval"]["y"])):
            if p == t:
                e["pred"][i] = (p + 1) % 3
                return

    def nudge_conf(m):
        m["seed_runs"][5]["conf"][0] = m["seed_runs"][5]["conf"][0] + 1e-6

    def flip_zero_shot(m):
        m["zero_shot"]["pred"][0] = (m["zero_shot"]["pred"][0] + 1) % 3

    c.refused("red: one flipped y is refused (MISMATCH)", lambda: _verify(mutated(flip_y), evidence), "MISMATCH")
    c.refused("red: one flipped pred in one seed-run is refused (MISMATCH)", lambda: _verify(mutated(flip_pred), evidence), "MISMATCH")
    c.refused("red: one conf moved by 1e-6 is refused (ECE MISMATCH)", lambda: _verify(mutated(nudge_conf), evidence), "ECE MISMATCH")
    c.refused("red: a dropped seed-run is refused (COUNT)", lambda: _verify(mutated(lambda m: m["seed_runs"].pop()), evidence), "COUNT")
    c.refused("red: an added key named text is refused (LEAK)",
              lambda: _verify(mutated(lambda m: m["eval"].update({"text": ["a"]})), evidence), "LEAK")
    c.refused("red: a 300-character string is refused (LEAK)",
              lambda: _verify(mutated(lambda m: m["runs"]["p12"].update({"note": "x" * 300})), evidence), "LEAK")
    c.refused("red: an edited evidence digest is refused (EVIDENCE-DIGEST)",
              lambda: _verify(mutated(lambda m: m["runs"]["p14"].update({"evidence_sha256": "f" * 64})), evidence), "EVIDENCE-DIGEST")
    c.refused("red: a duplicate JSON key is refused", lambda: parse_json_bytes(b'{"a": 1, "a": 2}'), "duplicate key")
    c.refused("red: a zero-shot mismatch is refused (ZERO-SHOT MISMATCH)",
              lambda: _verify(mutated(flip_zero_shot), evidence), "ZERO-SHOT MISMATCH")
    c.refused("red: a non-finite number is refused", lambda: parse_json_bytes(b'{"a": NaN}'), "non-finite")
    c.refused("red: a file over 8 MiB is refused", lambda: parse_json_bytes(b" " * (MAX_FILE + 1)), "larger than")
    c.refused("red: a wrong schema is refused", lambda: _verify(mutated(lambda m: m.update({"schema": "x"})), evidence), "SCHEMA")


SELFTEST_SECTIONS = [selftest_inputs]


def selftest():
    c = Cases()
    for section in SELFTEST_SECTIONS:
        section(c)
    if c.failed:
        print("MARGIN DIAGNOSIS SELFTEST FAILED n=%d" % len(c.failed))
        return 1
    print("MARGIN DIAGNOSIS SELFTEST OK cases=%d" % c.n)
    return 0


# ---- command line ------------------------------------------------------------------------------------------------

COMMANDS = {
    "snapshot": (cmd_snapshot, {"--out": True, "--local-root": False}),
    "verify-inputs": (cmd_verify_inputs, {"--inputs": True}),
}


def parse_opts(args, spec):
    opts = {}
    i = 0
    while i < len(args):
        k = args[i]
        if k not in spec:
            raise Refused("usage", "unknown option %r" % k)
        if i + 1 >= len(args):
            raise Refused("usage", "%s needs a value" % k)
        if k in opts:
            raise Refused("usage", "%s given twice" % k)
        opts[k] = args[i + 1]
        i += 2
    for k, required in spec.items():
        if required and k not in opts:
            raise Refused("usage", "%s is required" % k)
    return opts


def main(argv):
    try:
        if argv == ["--selftest"]:
            return selftest()
        if not argv or argv[0] not in COMMANDS:
            raise Refused("usage", "expected --selftest or one of %s" % ", ".join(sorted(COMMANDS)))
        fn, spec = COMMANDS[argv[0]]
        return fn(parse_opts(argv[1:], spec))
    except Refused as ex:
        print("REFUSED %s: %s" % (ex.rule, ex.reason))
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
