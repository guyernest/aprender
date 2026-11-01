#!/usr/bin/env python3
"""Name the CLASS of the SC4 margin miss from evidence already on disk (plan 15-09; read by 15-10, 15-11, 15-12).

    python3 -I scripts/laya_deploy/laya_margin_diagnosis.py snapshot --out FILE [--local-root DIR]
    python3 -I scripts/laya_deploy/laya_margin_diagnosis.py verify-inputs --inputs FILE
    python3 -I scripts/laya_deploy/laya_margin_diagnosis.py scan-identifiers FILE...     (plan 15-11)
    python3 -I scripts/laya_deploy/laya_margin_diagnosis.py classify-run --run-dir DIR   (plan 15-11)
    python3 -I scripts/laya_deploy/laya_margin_diagnosis.py check-run-go --run-go FILE --blob FILE --commit SHA   (plan 15-12)
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

import datetime
import hashlib
import itertools
import json
import math
import os
import random
import re
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


# ---- the pre-registered class rule and its statistics (plan 15-09 task 2) --------------------------------------------
#
# RULE_SPEC is committed BEFORE `analyze` is ever run on the real fixture and is never edited after the result is seen.
# Its canonical JSON (sort_keys, compact separators) is hashed into rule_sha256, which the diagnosis carries and
# check-diagnosis re-derives, so a changed constant is a changed hash.

RULE_SPEC = {
    "version": 1,
    "floor": 0.30,
    "ceiling": 0.45,
    "ece_band": [0.07, 0.12],
    "gate": {"min_margin": 0.05, "max_ece": 0.10},
    "bootstrap": {
        "B": 2000,
        "seed": 20261031,
        "design": "paired row bootstrap: B resamples of 100 row indices drawn with replacement by random.Random(seed).randrange(100), "
                  "the SAME indices for the fine-tuned and the zero-shot macro-F1 of one resample; the same index sets serve every seed-run",
        "percentiles": "p05 = sorted[int(0.05 * B)], p95 = sorted[int(0.95 * B)]",
    },
    "z_one_sided_90": 1.2815515655446004,
    "rank_scale": 10000,
    "floor_rule": "the CUDA seed-run mean minus 2 sd (ddof 1), rounded DOWN to hundredths",
    "device_anchoring": "supported iff the three paired MPS-minus-CUDA differences share a sign and the df 2 two-sided p "
                        "(1 - |t| / sqrt(t^2 + 2)) is below 0.05, else unproven; informational, never selects the class",
    "lever": {"detectable_abs_z": 2.0, "mde_multiplier": 2.8},
    "class_rule": "deficiency is supported iff cuda_mean + z * sem < floor, refuted iff cuda_mean - z * sem >= floor, else "
                  "undetermined (z = z_one_sided_90, sem = sd / sqrt(n), sd with ddof 1); single_draw_acceptance holds iff "
                  "(shortfall <= cuda_sd) AND (shortfall <= median row-resampling se) AND (declared-criterion pass "
                  "probability < 0.5) AND (deficiency is not supported on the full set nor on any leave-one-out set); "
                  "class = recipe-deficiency if deficiency is supported on the full set, else single-draw-acceptance if "
                  "that predicate holds, else undetermined",
}

CLASS_NAMES = ("recipe-deficiency", "single-draw-acceptance", "undetermined")
GATE_MAX_ECE = RULE_SPEC["gate"]["max_ece"]
GATE_MIN_MARGIN = RULE_SPEC["gate"]["min_margin"]
FLOOR = RULE_SPEC["floor"]
CEILING = RULE_SPEC["ceiling"]
ECE_LO, ECE_HI = RULE_SPEC["ece_band"]
Z90 = RULE_SPEC["z_one_sided_90"]
RANK_SCALE = RULE_SPEC["rank_scale"]


def rule_sha256(spec=None):
    spec = RULE_SPEC if spec is None else spec
    return sha256_hex(json.dumps(spec, sort_keys=True, separators=(",", ":")).encode("utf-8"))


def mean_of(xs):
    return math.fsum(xs) / len(xs)


def median_of(xs):
    v = sorted(xs)
    m = len(v) // 2
    return v[m] if len(v) % 2 else (v[m - 1] + v[m]) / 2.0


def sd_of(xs):
    """Sample standard deviation, ddof 1."""
    m = mean_of(xs)
    return math.sqrt(math.fsum((x - m) ** 2 for x in xs) / (len(xs) - 1))


def pearson(xs, ys):
    mx, my = mean_of(xs), mean_of(ys)
    sxy = math.fsum((a - mx) * (b - my) for a, b in zip(xs, ys))
    sxx = math.fsum((a - mx) ** 2 for a in xs)
    syy = math.fsum((b - my) ** 2 for b in ys)
    return sxy / math.sqrt(sxx * syy)


def deficiency_status(mean, sd, n):
    """supported / refuted / undetermined, from the one-sided 90 percent bounds of the mean against the floor."""
    sem = sd / math.sqrt(n)
    if mean + Z90 * sem < FLOOR:
        return "supported"
    if mean - Z90 * sem >= FLOOR:
        return "refuted"
    return "undetermined"


def class_rule(full_status, loo_statuses, shortfall, cuda_sd, median_se, p_declared):
    """(name, predicates). The rule text is RULE_SPEC['class_rule']."""
    no_loo_support = "supported" not in loo_statuses
    preds = {"shortfall_le_cuda_sd": shortfall <= cuda_sd,
             "shortfall_le_median_se": shortfall <= median_se,
             "declared_p_pass_lt_half": p_declared < 0.5,
             "deficiency_not_supported_full": full_status != "supported",
             "deficiency_not_supported_any_loo": no_loo_support}
    holds = all(preds.values())
    if full_status == "supported":
        return "recipe-deficiency", dict(preds, single_draw_acceptance=holds)
    return ("single-draw-acceptance" if holds else "undetermined"), dict(preds, single_draw_acceptance=holds)


def floor_rule_value(mean, sd):
    return math.floor((mean - 2.0 * sd) * 100.0) / 100.0


def paired_t_df2(diffs):
    """(mean, sd, t, p) of three paired differences; p is the df 2 two-sided p, 1 - |t| / sqrt(t^2 + 2)."""
    m, s = mean_of(diffs), sd_of(diffs)
    if s == 0.0:
        return m, s, None, (1.0 if m == 0.0 else 0.0)
    t = m / (s / math.sqrt(len(diffs)))
    return m, s, t, p_df2(t)


def p_df2(t):
    return 1.0 - abs(t) / math.sqrt(t * t + 2.0)


def device_status(diffs):
    m, s, t, p = paired_t_df2(diffs)
    same_sign = all(d > 0 for d in diffs) or all(d < 0 for d in diffs)
    return ("supported" if (same_sign and p < 0.05) else "unproven"), same_sign


def rank_key(ece):
    return int(math.floor(float(ece) * RANK_SCALE))


def ship(units):
    """The median-ECE seed of three units: rank key floor(ece x rank_scale), ties to the smaller seed, then to the
    earlier unit. units are dicts with seed, margin, ece."""
    ranked = sorted(enumerate(units), key=lambda iu: (rank_key(iu[1]["ece"]), iu[1]["seed"], iu[0]))
    return ranked[1][1]


def judge_triple(triple, mode, lo):
    """(class, gate_pass) of one triple of drawn seed-runs. mode: shipped (the shipped seed's margin) or mean (the mean of
    the three drawn margins). The ECE clause and the gate are on the shipped seed in both modes."""
    shipped = ship(triple)
    judged = shipped["margin"] if mode == "shipped" else mean_of([u["margin"] for u in triple])
    gate = shipped["ece"] <= GATE_MAX_ECE and shipped["margin"] >= GATE_MIN_MARGIN
    in_band = lo <= judged <= CEILING and ECE_LO <= shipped["ece"] <= ECE_HI
    if not in_band:
        return "defect", gate
    return ("pass" if gate else "in-band-gate-fail"), gate


def resample_rule(units, mode, lo):
    """Every ordered triple with replacement of the given seed-runs (len(units) ** 3), judged by `mode` with the margin
    band [lo, ceiling]. Exact fractions of the triples, as floats."""
    n = 0
    tally = {"pass": 0, "in-band-gate-fail": 0, "defect": 0, "gate": 0}
    for idx in itertools.product(range(len(units)), repeat=3):
        cls, gate = judge_triple([units[i] for i in idx], mode, lo)
        tally[cls] += 1
        tally["gate"] += 1 if gate else 0
        n += 1
    p_pass, p_ibgf = tally["pass"] / n, tally["in-band-gate-fail"] / n
    return {"p_gate": tally["gate"] / n, "p_pass": p_pass, "p_in_band_gate_fail": p_ibgf,
            "p_defect": tally["defect"] / n, "within_cap2": p_pass + p_ibgf * p_pass, "triples": n}


def resample_with_loo(units, mode, lo):
    """resample_rule plus the leave-one-out list (each unit dropped in turn, 125 triples of the other five)."""
    out = resample_rule(units, mode, lo)
    loo = []
    for d in range(len(units)):
        r = resample_rule(units[:d] + units[d + 1:], mode, lo)
        loo.append({"dropped": "%s:%s" % (units[d]["run"], units[d]["seed"]) if "run" in units[d] else str(d),
                    "p_pass": r["p_pass"], "within_cap2": r["within_cap2"]})
    out["loo"] = loo
    out["loo_min"] = min(e["p_pass"] for e in loo)
    out["loo_max"] = max(e["p_pass"] for e in loo)
    out["loo_within_cap2_min"] = min(e["within_cap2"] for e in loo)
    out["loo_within_cap2_max"] = max(e["within_cap2"] for e in loo)
    return out


def _counts_f1(y, pred):
    """macro-F1 from one pass over the rows; identical to macro_f1 (same label set, same fsum order)."""
    tp, fp, fn, seen = [0, 0, 0], [0, 0, 0], [0, 0, 0], [False] * 3
    for t, p in zip(y, pred):
        seen[t] = seen[p] = True
        if t == p:
            tp[t] += 1
        else:
            fp[p] += 1
            fn[t] += 1
    f1s = []
    for c in range(3):
        if seen[c]:
            den = 2 * tp[c] + fp[c] + fn[c]
            f1s.append(0.0 if den == 0 else 2.0 * tp[c] / den)
    return math.fsum(f1s) / len(f1s)


def bootstrap_index_sets(n, B, seed):
    rng = random.Random(seed)
    return [[rng.randrange(n) for _ in range(n)] for _ in range(B)]


def bootstrap_margins(y, pred, zs_pred, index_sets):
    """The paired row-bootstrap margins of one seed-run: macro-F1(pred) - macro-F1(zs_pred) on each resample."""
    out = []
    for idx in index_sets:
        ys = [y[i] for i in idx]
        out.append(_counts_f1(ys, [pred[i] for i in idx]) - _counts_f1(ys, [zs_pred[i] for i in idx]))
    return out


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


# ---- analyze: the class diagnosis (plan 15-09 task 2, after RULE_SPEC was committed) ---------------------------------

DIAGNOSIS_SCHEMA = "laya-class-diagnosis-v1"
DRAFTS_PATH = _PH15 + "/15-DECLARATION-DRAFTS.json"
OPTION_IDS = ("stop-accept", "redraw", "mean-rule", "mean-rule-retry", "floor-rule")
RUN_NAME = "motivation-v1-p15b"
DATASET_PREFIX = "datasets/motivation-v1/"
CLAIM_PREFIX = "gate outcome and band only; not an identical reproduction of attempt 1 or of Phase 12: "
PACK_IMAGE = "sha256:dd38865c43aba2f3819e50addcf6db5e9da94ef84e762d93cf6b2c132f1d17e2"
RESAMPLE_BASIS = ("a resampling estimate over the six CUDA seed-runs (all 216 ordered triples with replacement, median-ECE "
                  "ship rule); it is not a confidence interval")

# (tracked path, exact single-line needle): the provenance of the band, each found by exact-string count.
CITATIONS = (
    (_PH14 + "/14-RESEARCH.md", "the researcher's proposal, [ASSUMED]"),
    (_PH14 + "/14-RESEARCH.md", "around 0.3780 / 0.0936"),
    (_PH14 + "/14-RESEARCH.md", "the median-of-three on a different device is a fresh draw"),
    (_PH15 + "/15-RESEARCH.md", "D-16 outcome: LOW (single draw)"),
    (_PH15 + "/15-REVIEWS.md", "The paid-run outcome is a draw by construction"),
    (".claude/skills/spike-findings-aprender/references/laya-calibration-gate-and-pack-tolerance.md",
     "report pass rates with a CI, not a verdict"),
    (".claude/skills/spike-findings-aprender/references/laya-calibration-gate-and-pack-tolerance.md", "MPS replicates moved"),
    ("contracts/laya-finetune-gate-v1.yaml", "measured MPS training as not bitwise reproducible"),
)


def f4(x):
    return "%.4f" % x


def p3(x):
    return "%.3f" % x


def _stats(xs):
    return {"n": len(xs), "mean": mean_of(xs), "sd": sd_of(xs)}


def build_options(res, floor_value, recipe_id, billable, pack_seconds):
    """The five owner options. Probabilities come from the resample block; nothing is typed by hand."""
    cost = ("report-only: about %d s of ml.g5.xlarge billed on attempt 1 plus about %d s of Fargate pack"
            % (billable, int(round(pack_seconds / 10.0)) * 10))
    common = {"epochs": 12, "recipe_id": recipe_id, "run_name": RUN_NAME, "dataset_prefix": DATASET_PREFIX,
              "ece_band": [ECE_LO, ECE_HI], "cost_report_only": cost}

    def paid(judging, margin_lo, cap, retry, supersedes, block, rule_change):
        within = block["within_cap2"] if cap > 1 else block["p_pass"]
        lo_k, hi_k = ("loo_within_cap2_min", "loo_within_cap2_max") if cap > 1 else ("loo_min", "loo_max")
        o = dict(common)
        o.update({"paid_run": True, "judging": judging, "margin_band": [margin_lo, CEILING], "attempt_cap": cap,
                  "retry_classes": retry, "supersedes": supersedes, "p_pass": block["p_pass"], "within_cap": within,
                  "loo_min": block[lo_k], "loo_max": block[hi_k], "claim_scope": CLAIM_PREFIX + rule_change})
        return o

    opts = {
        "stop-accept": {k: None for k in ("judging", "margin_band", "ece_band", "attempt_cap", "retry_classes", "epochs",
                                          "recipe_id", "run_name", "dataset_prefix", "p_pass", "within_cap", "loo_min",
                                          "loo_max", "cost_report_only", "claim_scope")},
        "redraw": paid("shipped_seed", FLOOR, 1, [], [], res["declared"],
                       "no rule change; the Phase 12 and attempt 1 recipe (12 epochs) is drawn again, the shipped seed's margin "
                       "judged in [0.3000, 0.4500] as before"),
        "mean-rule": paid("seed_mean", FLOOR, 1, [], ["D-11"], res["mean_rule"],
                          "the margin clause is judged on the mean of the three seed margins in [0.3000, 0.4500] instead of "
                          "the shipped seed's margin (D-11 superseded); the ECE clause and the gate stay on the shipped seed"),
        "mean-rule-retry": paid("seed_mean", FLOOR, 2, ["in-band-gate-fail"], ["D-11", "D-12"], res["mean_rule"],
                                "the mean-of-three margin rule (D-11 superseded) and one second paid run allowed after an "
                                "in-band-gate-fail (D-12 superseded); a defect is never retried"),
        "floor-rule": paid("shipped_seed", floor_value, 1, [], ["D-11"], res["floor_rule"],
                           "the shipped seed's margin floor moves from 0.3000 to %s (D-11 superseded); ceiling 0.4500 and "
                           "the ECE clause unchanged" % f4(floor_value)),
    }
    opts["stop-accept"]["paid_run"] = False
    opts["stop-accept"]["supersedes"] = []
    return opts


def citation_rows(reader):
    rows = []
    for path, needle in CITATIONS:
        try:
            text = reader(path).decode("utf-8")
            count = text.count(needle)
        except (Refused, UnicodeDecodeError):
            count = 0
        rows.append({"path": path, "needle": needle, "count": count, "found": count >= 1})
    return rows


def analyze_obj(fx, raw_inputs, reader, B=None):
    """Pure and deterministic: the same fixture and tracked files give the same object. `reader(path)` returns the
    bytes of a tracked file (raises Refused if missing). B overrides the bootstrap size for the selftest only."""
    verify_inputs_obj(fx, lambda rid, path: reader(path))
    S = RULE_SPEC
    B = S["bootstrap"]["B"] if B is None else B
    y, zpred, zs_f1 = fx["eval"]["y"], fx["zero_shot"]["pred"], macro_f1(fx["eval"]["y"], fx["zero_shot"]["pred"])
    evid = {rid: parse_json_bytes(reader(RUNS[rid]["evidence"]), RUNS[rid]["evidence"]) for rid in RUN_IDS}
    thr = (evid["p15"].get("gate") or {}).get("thresholds") or {}
    if thr.get("min_macro_f1_margin") != GATE_MIN_MARGIN or thr.get("max_ece") != GATE_MAX_ECE:
        raise Refused("analyze", "RULE_SPEC gate thresholds differ from 15-GATE-RUN-EVIDENCE.json gate.thresholds %r" % (thr,))
    drafts = parse_json_bytes(reader(DRAFTS_PATH), DRAFTS_PATH)
    redraw = ((drafts.get("drafts") or {}).get("epochs-12-redraw") or {})
    recipe_id = (redraw.get("must") or {}).get("recipe_id")
    billable = ((redraw.get("report_only") or {}).get("references") or {}).get("attempt_1_billable_seconds")
    if not isinstance(recipe_id, str) or not is_num(billable):
        raise Refused("analyze", "15-DECLARATION-DRAFTS.json lacks drafts.epochs-12-redraw must.recipe_id or report_only references")
    pack_seconds = evid["p15"].get("pack_task_seconds")
    if not is_num(pack_seconds):
        raise Refused("analyze", "15-GATE-RUN-EVIDENCE.json lacks pack_task_seconds")

    # Per seed-run values, in the fixed order p12, p14, p15 x 13, 17, 23.
    sr = {}
    recompute_rows = []
    for e in sorted(fx["seed_runs"], key=lambda e: (RUN_IDS.index(e["run"]), e["seed"])):
        row = _evidence_row(evid[e["run"]], e["seed"])
        margin = macro_f1(y, e["pred"]) - zs_f1
        ece = ece_top_label(y, e["pred"], e["conf"])
        exact = abs(margin - row["margin"]) <= TOL and abs(ece - row["ece_post"]) <= TOL
        sr[(e["run"], e["seed"])] = {"run": e["run"], "seed": e["seed"], "margin": margin, "ece": ece, "pred": e["pred"]}
        recompute_rows.append({"run": e["run"], "seed": e["seed"], "device": fx["runs"][e["run"]]["device"],
                               "recorded_margin": row["margin"], "recomputed_margin": margin,
                               "recorded_ece_post": row["ece_post"], "recomputed_ece_post": ece, "exact": exact})
    all_exact = all(r["exact"] for r in recompute_rows)

    def margins(rid):
        return [sr[(rid, s)]["margin"] for s in SEEDS]

    cuda_units = [dict(sr[(rid, s)]) for rid in ("p14", "p15") for s in SEEDS]
    for u in cuda_units:
        u.pop("pred")
    cuda_margins = [u["margin"] for u in cuda_units]
    cm, csd = mean_of(cuda_margins), sd_of(cuda_margins)
    sem = csd / math.sqrt(len(cuda_margins))
    spread = {rid: dict(_stats(margins(rid)), sd_ddof=1) for rid in RUN_IDS}
    spread["cuda"] = {"n": 6, "mean": cm, "sd": csd, "sem": sem, "lower90": cm - Z90 * sem, "upper90": cm + Z90 * sem,
                      "mean_minus_floor_in_sd": (cm - FLOOR) / csd, "runs": ["p14", "p15"]}
    spread["mps"] = dict(spread["p12"], run="p12")

    shipped15 = fx["runs"]["p15"]["shipped_seed"]
    shipped_margin = sr[("p15", shipped15)]["margin"]
    shortfall = {"run": "p15", "shipped_seed": shipped15, "shipped_margin": shipped_margin, "floor": FLOOR,
                 "shortfall": FLOOR - shipped_margin}

    sets = bootstrap_index_sets(len(y), B, S["bootstrap"]["seed"])
    per_run = []
    for key in sorted(sr, key=lambda k: (RUN_IDS.index(k[0]), k[1])):
        bm = sorted(bootstrap_margins(y, sr[key]["pred"], zpred, sets))
        per_run.append({"run": key[0], "seed": key[1], "margin": sr[key]["margin"], "se": sd_of(bm),
                        "p05": bm[int(0.05 * B)], "p95": bm[int(0.95 * B)],
                        "p_ge_floor": sum(1 for v in bm if v >= FLOOR) / B})
    ses = sorted(r["se"] for r in per_run)
    median_se = median_of(ses)
    measurement = {"B": B, "seed": S["bootstrap"]["seed"], "per_seed_run": per_run, "median_se": median_se,
                   "min_se": ses[0], "max_se": ses[-1], "shortfall_over_median_se": shortfall["shortfall"] / median_se}

    by_run = {}
    for rid in RUN_IDS:
        sh = fx["runs"][rid]["shipped_seed"]
        ms = margins(rid)
        by_run[rid] = {"shipped_seed": sh, "shipped_margin": sr[(rid, sh)]["margin"], "run_mean_margin": mean_of(ms),
                       "delta": sr[(rid, sh)]["margin"] - mean_of(ms), "shipped_is_lowest_margin": sr[(rid, sh)]["margin"] == min(ms)}
    all_keys = sorted(sr, key=lambda k: (RUN_IDS.index(k[0]), k[1]))
    cuda_keys = [k for k in all_keys if k[0] in ("p14", "p15")]
    selection = {"by_run": by_run,
                 "pearson_margin_ece_all9": pearson([sr[k]["margin"] for k in all_keys], [sr[k]["ece"] for k in all_keys]),
                 "pearson_margin_ece_cuda6": pearson([sr[k]["margin"] for k in cuda_keys], [sr[k]["ece"] for k in cuda_keys])}

    diffs = [sr[("p12", s)]["margin"] - sr[("p14", s)]["margin"] for s in SEEDS]
    dm, dsd, dt, dp = paired_t_df2(diffs)
    dstatus, same_sign = device_status(diffs)
    device_gap = {"pairs": [{"seed": s, "mps_p12": sr[("p12", s)]["margin"], "cuda_p14": sr[("p14", s)]["margin"], "diff": d}
                            for s, d in zip(SEEDS, diffs)],
                  "mean_diff": dm, "sd_diff": dsd, "t": dt, "df": 2, "p_two_sided": dp, "all_same_sign": same_sign,
                  "status": dstatus}

    e12, e4 = spread["p14"], spread["p15"]
    lever_se = math.sqrt(e12["sd"] ** 2 / 3.0 + e4["sd"] ** 2 / 3.0)
    lever_z = (e4["mean"] - e12["mean"]) / lever_se
    pooled = math.sqrt((e12["sd"] ** 2 + e4["sd"] ** 2) / 2.0)
    epochs_lever = {"epochs12": {"run": "p14", "mean": e12["mean"], "sd": e12["sd"]},
                    "epochs4": {"run": "p15", "mean": e4["mean"], "sd": e4["sd"]},
                    "diff": e4["mean"] - e12["mean"], "se": lever_se, "z": lever_z,
                    "detectable": abs(lever_z) >= S["lever"]["detectable_abs_z"],
                    "mde_normal_approx": S["lever"]["mde_multiplier"] * pooled * math.sqrt(2.0 / 3.0),
                    "best_epochs": {rid: [e["local"]["best_epoch"] for e in sorted(fx["seed_runs"], key=lambda e: e["seed"])
                                          if e["run"] == rid] for rid in RUN_IDS}}

    floor_value = floor_rule_value(cm, csd)
    resample = {"basis": RESAMPLE_BASIS,
                "declared": resample_with_loo(cuda_units, "shipped", FLOOR),
                "mean_rule": resample_with_loo(cuda_units, "mean", FLOOR),
                "floor_rule": resample_with_loo(cuda_units, "shipped", floor_value),
                "floor_rule_value": floor_value}
    resample["p_both_miss_if_independent"] = (1.0 - resample["declared"]["p_pass"]) ** 2

    full_status = deficiency_status(cm, csd, 6)
    jack = []
    for d, u in enumerate(cuda_units):
        rest = [m for i, m in enumerate(cuda_margins) if i != d]
        jack.append({"dropped": "%s:%s" % (u["run"], u["seed"]), "mean": mean_of(rest), "sd": sd_of(rest),
                     "status": deficiency_status(mean_of(rest), sd_of(rest), 5)})
    name, preds = class_rule(full_status, [j["status"] for j in jack], shortfall["shortfall"], csd, median_se,
                             resample["declared"]["p_pass"])
    tests = {"deficiency": {"status": full_status, "jackknife": jack},
             "single_draw": dict(preds),
             "device_anchoring": {"status": dstatus, "band_midpoint": (FLOOR + CEILING) / 2.0,
                                  "p12_shipped_margin": by_run["p12"]["shipped_margin"],
                                  "mps_mean": spread["p12"]["mean"], "cuda_mean": cm}}
    klass = {"name": name, "rule_sha256": rule_sha256(),
             "evidence": {"nine_seed_run_margins": {"%s:%d" % k: sr[k]["margin"] for k in all_keys},
                          "cuda_mean": cm, "cuda_sd": csd, "cuda_n": 6, "shortfall": shortfall["shortfall"],
                          "median_row_resampling_se": median_se, "declared_p_pass": resample["declared"]["p_pass"],
                          "declared_loo_min": resample["declared"]["loo_min"], "declared_loo_max": resample["declared"]["loo_max"],
                          "deficiency_full": full_status, "deficiency_loo": [j["status"] for j in jack],
                          "epochs_12_to_4_diff": epochs_lever["diff"], "epochs_lever_se": lever_se}}
    rejudge = {"consumed_run_mean_margins": {"p14": spread["p14"]["mean"], "p15": spread["p15"]["mean"]},
               "exploratory": True, "sets_next_trusted": False,
               "note": "post hoc, a statistic chosen after seeing both misses; it is information, not trust"}
    options = build_options(resample, floor_value, recipe_id, billable, pack_seconds)
    rec_ok = name == "single-draw-acceptance" and options["mean-rule"]["p_pass"] > options["redraw"]["p_pass"]
    recommended = {"option": "mean-rule" if rec_ok else "stop-accept",
                   "rule_text": "mean-rule iff class.name is single-draw-acceptance AND the mean-rule p_pass exceeds the "
                                "redraw p_pass, else stop-accept"}
    diag = {"schema": DIAGNOSIS_SCHEMA,
            "generated_from": {"inputs_sha256": sha256_hex(raw_inputs),
                               "evidence_sha256": {rid: fx["runs"][rid]["evidence_sha256"] for rid in RUN_IDS},
                               "drafts_sha256": sha256_hex(reader(DRAFTS_PATH))},
            "rule_spec": RULE_SPEC, "rule_sha256": rule_sha256(),
            "recompute": {"seed_runs": recompute_rows, "all_exact": all_exact},
            "spread": spread, "shortfall": shortfall, "measurement": measurement, "selection": selection,
            "device_gap": device_gap, "epochs_lever": epochs_lever, "resample": resample,
            "provenance": {"citations": citation_rows(reader)}, "tests": tests, "class": klass, "rejudge": rejudge,
            "options": options,
            "options_common": {"seeds": list(SEEDS), "device": "cuda", "pack_image": PACK_IMAGE,
                               "recipe_note": "the Phase 12 and attempt 1 recipe, the closest reproduction (the epochs-4 lever showed no effect)"},
            "recommended": recommended, "next_trusted_effect": "none"}
    diag["brief_facts"] = brief_facts(diag, fx)
    return diag


def brief_facts(d, fx):
    """Single-line strings the brief must contain verbatim. Floats to four decimals, probabilities to three."""
    f = []
    f.append("class: %s" % d["class"]["name"])
    sh = d["shortfall"]
    f.append("shortfall: the shipped seed %d margin %s is below the %s floor by %s"
             % (sh["shipped_seed"], f4(sh["shipped_margin"]), f4(sh["floor"]), f4(sh["shortfall"])))
    c = d["spread"]["cuda"]
    f.append("CUDA seed-runs: n=%d, mean %s, sd %s" % (c["n"], f4(c["mean"]), f4(c["sd"])))
    m = d["measurement"]
    f.append("row-resampling se of one seed's margin on 100 rows: median %s, range %s to %s"
             % (f4(m["median_se"]), f4(m["min_se"]), f4(m["max_se"])))
    r = d["resample"]
    f.append("declared criterion: p_pass %s (n=6 seed-runs, resampling estimate), leave-one-out range %s to %s"
             % (p3(r["declared"]["p_pass"]), p3(r["declared"]["loo_min"]), p3(r["declared"]["loo_max"])))
    f.append("mean-rule p_pass %s; mean-rule-retry within cap %s; floor-rule with floor %s p_pass %s"
             % (p3(r["mean_rule"]["p_pass"]), p3(r["mean_rule"]["within_cap2"]), f4(r["floor_rule_value"]),
                p3(r["floor_rule"]["p_pass"])))
    l = d["epochs_lever"]
    f.append("epochs lever (12 -> 4 on CUDA): difference %s, se %s, z %s, minimum detectable %s"
             % ("%+.4f" % l["diff"], f4(l["se"]), "%+.4f" % l["z"], f4(l["mde_normal_approx"])))
    g = d["device_gap"]
    f.append("device gap (MPS minus CUDA by seed): %s, mean %s, t %s, p %s, %s (n=3)"
             % (" / ".join("%+.4f" % p["diff"] for p in g["pairs"]), "%+.4f" % g["mean_diff"], f4(g["t"]), f4(g["p_two_sided"]),
                g["status"]))
    t = d["tests"]["deficiency"]
    f.append("deficiency: %s on the full set; leave-one-out %s" % (t["status"], ", ".join(j["status"] for j in t["jackknife"])))
    f.append("independent re-draw: two misses in a row under the declared rule have probability %s" % p3(r["p_both_miss_if_independent"]))
    f.append("best epoch per seed: " + "; ".join("%s %s" % (rid, d["epochs_lever"]["best_epochs"][rid]) for rid in RUN_IDS))
    f.append("recommended: %s" % d["recommended"]["option"])
    f.append("the floor sits %s sd below the CUDA mean" % f4(c["mean_minus_floor_in_sd"]))
    sel = d["selection"]["by_run"]
    f.append("the shipped seed is the lowest-margin seed of its run: " + ", ".join("%s %s" % (rid, "yes" if sel[rid]["shipped_is_lowest_margin"] else "no") for rid in RUN_IDS))
    f.append("gate (shipped ECE at most %s) passes with probability %s under every rule; declared criterion: defect probability %s, in-band gate fail %s"
             % (f4(GATE_MAX_ECE), p3(r["declared"]["p_gate"]), p3(r["declared"]["p_defect"]), p3(r["declared"]["p_in_band_gate_fail"])))
    f.append("post hoc view of the consumed runs (exploratory, sets nothing): p14 mean margin %s, p15 mean margin %s"
             % (f4(d["rejudge"]["consumed_run_mean_margins"]["p14"]), f4(d["rejudge"]["consumed_run_mean_margins"]["p15"])))
    for oid in OPTION_IDS:
        o = d["options"][oid]
        if not o["paid_run"]:
            f.append("option %s: no paid run, supersedes none, SC4 stays unmet" % oid)
            continue
        f.append("option %s: judging %s, margin band [%s, %s], ece band [%s, %s], attempt cap %d, retry classes %s, supersedes %s, "
                 "p_pass %s, within cap %s, leave-one-out %s to %s"
                 % (oid, o["judging"], f4(o["margin_band"][0]), f4(o["margin_band"][1]), f4(o["ece_band"][0]), f4(o["ece_band"][1]),
                    o["attempt_cap"], ", ".join(o["retry_classes"]) or "none", ", ".join(o["supersedes"]) or "none",
                    p3(o["p_pass"]), p3(o["within_cap"]), p3(o["loo_min"]), p3(o["loo_max"])))
        f.append("option %s claim scope: %s" % (oid, o["claim_scope"]))
    po = d["options"]["redraw"]
    f.append("each paid option: run name %s, dataset prefix %s, epochs %d, recipe %s, seeds %s, device %s, pack image %s; %s"
             % (po["run_name"], po["dataset_prefix"], po["epochs"], po["recipe_id"], d["options_common"]["seeds"],
                d["options_common"]["device"], d["options_common"]["pack_image"], po["cost_report_only"]))
    rows = {(x["run"], x["seed"]): x for x in d["recompute"]["seed_runs"]}
    for rid in RUN_IDS:
        for s in SEEDS:
            x = rows[(rid, s)]
            f.append("| %s | %s | %d | %d | %s | %s | %s |"
                     % (rid, x["device"], fx["runs"][rid]["epochs"], s,
                        "yes" if fx["runs"][rid]["shipped_seed"] == s else "no", f4(x["recomputed_margin"]),
                        f4(x["recomputed_ece_post"])))
    return f


def make_reader(root):
    def reader(path):
        return read_bytes(os.path.join(root, path), "analyze")
    return reader


def cmd_analyze(opts):
    raw = read_bytes(opts["--inputs"], "analyze")
    fx = parse_json_bytes(raw, opts["--inputs"])
    diag = analyze_obj(fx, raw, make_reader(REPO))
    write_text(opts["--out"], dumps(diag))
    print("ANALYZE OK class=%s recommended=%s" % (diag["class"]["name"], diag["recommended"]["option"]))
    return 0


def _true_next_trusted(o):
    if isinstance(o, dict):
        for k, v in o.items():
            if k == "next_trusted" and v is True:
                return True
            if _true_next_trusted(v):
                return True
    elif isinstance(o, list):
        return any(_true_next_trusted(v) for v in o)
    return False


def check_diagnosis_obj(diag_text, fx, raw_inputs, reader, B=None):
    """Structural refusals first (each named), then byte equality with a fresh in-memory analyze."""
    d = parse_json_bytes(diag_text.encode("utf-8"), "the diagnosis")
    if not isinstance(d, dict) or d.get("schema") != DIAGNOSIS_SCHEMA:
        raise Refused("diagnosis", "SCHEMA the diagnosis schema is not %r" % DIAGNOSIS_SCHEMA)
    if d.get("rule_sha256") != rule_sha256():
        raise Refused("diagnosis", "RULE-HASH rule_sha256 differs from the hash of the current RULE_SPEC")
    if d.get("next_trusted_effect") != "none" or _true_next_trusted(d):
        raise Refused("diagnosis", "TRUST next_trusted_effect is not none, or a next_trusted true exists")
    if (d.get("rejudge") or {}).get("sets_next_trusted") is not False:
        raise Refused("diagnosis", "TRUST rejudge.sets_next_trusted is not false")
    if (d.get("recompute") or {}).get("all_exact") is not True:
        raise Refused("diagnosis", "RECOMPUTE all_exact is not true")
    for c in ((d.get("provenance") or {}).get("citations") or []):
        if c.get("found") is not True:
            raise Refused("diagnosis", "CITATION not found: %s" % c.get("needle"))
    fresh = dumps(analyze_obj(fx, raw_inputs, reader, B))
    if fresh != diag_text:
        raise Refused("diagnosis", "STALE the file differs from a fresh in-memory analyze of the same fixture")
    return d


def cmd_check_diagnosis(opts):
    raw = read_bytes(opts["--inputs"], "diagnosis")
    fx = parse_json_bytes(raw, opts["--inputs"])
    text = read_bytes(opts["--diagnosis"], "diagnosis").decode("utf-8")
    d = check_diagnosis_obj(text, fx, raw, make_reader(REPO))
    print("DIAGNOSIS OK class=%s recommended=%s" % (d["class"]["name"], d["recommended"]["option"]))
    return 0


# ---- the owner's brief, reply and disposition (plan 15-09 task 3) ------------------------------------------------------
#
# Nothing here asks for, relays or infers an approval. The owner's reply is written by the orchestrator (plan 15-10);
# these functions only VALIDATE its fields and derive the disposition from them. No function judges a substring of
# owner_reply_verbatim: a literal reply such as `go` is judged on its option and run_go fields.

BRIEF_HEADINGS = ("## Question", "## The class", "## Evidence", "## What the epochs lever did", "## Options",
                  "## Recommendation", "## Safeguards", "## What each reply authorizes", "## Limits of the claim",
                  "## Reply format")
REPLY_SCHEMA = "laya-gap-owner-reply-v1"
DISPOSITION_SCHEMA = "laya-gap-disposition-v1"
REPLY_KEYS = ("schema", "recorded_by", "recorded_at", "owner_reply_verbatim", "option", "run_go", "supersedes", "brief_sha256")
FORBIDDEN_KEYS = ("overrides", "waiver")
STOP_ACCEPT_STATE = ("SC4 stays unmet by the owner's decision; the training server stays untrusted; this is not an override")
PAID_STATE = ("SC4 stays unmet until a NEW run under a declaration committed before it meets its declared band; the training "
              "server stays untrusted; no waiver and no override applies")


def _diag_options(diag):
    if not isinstance(diag, dict) or diag.get("schema") != DIAGNOSIS_SCHEMA:
        raise Refused("diagnosis", "SCHEMA the diagnosis schema is not %r" % DIAGNOSIS_SCHEMA)
    opts = diag.get("options")
    if not isinstance(opts, dict) or sorted(opts) != sorted(OPTION_IDS):
        raise Refused("diagnosis", "COUNT the diagnosis does not carry exactly the five option ids")
    return opts


def check_brief_text(text, diag):
    """Every brief_facts string verbatim, the ten headings once and in order, all five option ids."""
    _diag_options(diag)
    facts = diag.get("brief_facts")
    if not isinstance(facts, list) or not facts or not all(isinstance(f, str) and f and "\n" not in f for f in facts):
        raise Refused("brief", "FACTS the diagnosis brief_facts is not a non-empty list of single-line strings")
    for f in facts:
        if f not in text:
            raise Refused("brief", "FACT missing from the brief: %s" % f[:100])
    pos = []
    for h in BRIEF_HEADINGS:
        if text.count("\n" + h + "\n") + (1 if text.startswith(h + "\n") else 0) != 1:
            raise Refused("brief", "HEADING %r must appear exactly once as its own line" % h)
        pos.append(text.index(h + "\n"))
    if pos != sorted(pos):
        raise Refused("brief", "HEADING order: the ten headings are not in the required order")
    for o in OPTION_IDS:
        if o not in text:
            raise Refused("brief", "OPTION %s is not named in the brief" % o)
    return len(facts)


def cmd_check_brief(opts):
    text = read_bytes(opts["--brief"], "brief").decode("utf-8")
    diag = read_json(opts["--diagnosis"], "brief")
    n = check_brief_text(text, diag)
    print("BRIEF OK facts=%d options=%d" % (n, len(OPTION_IDS)))
    return 0


def check_reply_obj(reply, diag, brief_sha256=None):
    """Validate the orchestrator-recorded owner reply by its FIELDS. brief_sha256=None skips only the digest comparison
    (write-disposition has no brief; check-reply always has one)."""
    opts = _diag_options(diag)
    if not isinstance(reply, dict):
        raise Refused("reply", "SHAPE the reply is not a JSON object")
    for k in reply:
        if k not in REPLY_KEYS:
            raise Refused("reply", "KEY the reply carries the unknown key %r" % k)
    for k in REPLY_KEYS:
        if k not in reply:
            raise Refused("reply", "KEY the reply lacks %r" % k)
    if reply["schema"] != REPLY_SCHEMA:
        raise Refused("reply", "SCHEMA the reply schema is not %r" % REPLY_SCHEMA)
    if reply["recorded_by"] != "orchestrator":
        raise Refused("reply", "RECORDED-BY must be exactly 'orchestrator', never a relayed approval")
    ts = reply["recorded_at"]
    try:
        if not isinstance(ts, str) or not ts.endswith("Z"):
            raise ValueError
        datetime.datetime.strptime(ts, "%Y-%m-%dT%H:%M:%SZ")
    except ValueError:
        raise Refused("reply", "RECORDED-AT must be an ISO 8601 UTC string like 2026-11-01T05:36:02Z")
    if not isinstance(reply["owner_reply_verbatim"], str) or not reply["owner_reply_verbatim"].strip():
        raise Refused("reply", "VERBATIM owner_reply_verbatim must be a non-empty string")
    opt = reply["option"]
    if not isinstance(opt, str) or opt not in OPTION_IDS:
        raise Refused("reply", "OPTION %r is not one of %s" % (opt, ", ".join(OPTION_IDS)))
    paid = opts[opt]["paid_run"]
    go = reply["run_go"]
    if paid and not isinstance(go, bool):
        raise Refused("reply", "RUN-GO must be a boolean for the paid option %s" % opt)
    if not paid and go is not None:
        raise Refused("reply", "RUN-GO must be null for stop-accept")
    if reply["supersedes"] != opts[opt]["supersedes"]:
        raise Refused("reply", "SUPERSEDES %r differs from the option's %r" % (reply["supersedes"], opts[opt]["supersedes"]))
    if brief_sha256 is not None and reply["brief_sha256"] != brief_sha256:
        raise Refused("reply", "BRIEF-DIGEST brief_sha256 is not the sha256 of the brief file")
    return opt, go


def cmd_check_reply(opts):
    reply = read_json(opts["--reply"], "reply")
    diag = read_json(opts["--diagnosis"], "reply")
    brief_sha = sha256_hex(read_bytes(opts["--brief"], "reply"))
    opt, go = check_reply_obj(reply, diag, brief_sha)
    print("REPLY OK option=%s run_go=%s" % (opt, "null" if go is None else ("true" if go else "false")))
    return 0


def build_disposition(reply, diag):
    opts = _diag_options(diag)
    opt, go = check_reply_obj(reply, diag, None)
    o = opts[opt]
    sel = diag["selection"]["by_run"]
    return {"schema": DISPOSITION_SCHEMA, "gap": "SC4", "option": opt, "paid_run_planned": o["paid_run"], "run_go": go,
            "supersedes": o["supersedes"], "rule": None if not o["paid_run"] else o,
            "band_before": {"margin": [FLOOR, CEILING], "ece_post": [ECE_LO, ECE_HI]},
            "class": diag["class"]["name"],
            "consumed_attempts": [
                {"phase": "14", "attempt": 1, "run": "p14", "shipped_seed": sel["p14"]["shipped_seed"],
                 "shipped_margin": sel["p14"]["shipped_margin"], "rejudged_for_trust": False},
                {"phase": "15", "attempt": 1, "run": "p15", "shipped_seed": sel["p15"]["shipped_seed"],
                 "shipped_margin": sel["p15"]["shipped_margin"], "rejudged_for_trust": False}],
            "next_trusted": False, "trust_basis": "none", "declaration_pending": bool(o["paid_run"]),
            "phase_state": PAID_STATE if o["paid_run"] else STOP_ACCEPT_STATE,
            "reply_brief_sha256": reply["brief_sha256"]}


def _has_forbidden_key(o):
    if isinstance(o, dict):
        for k, v in o.items():
            if k in FORBIDDEN_KEYS or _has_forbidden_key(v):
                return True
    elif isinstance(o, list):
        return any(_has_forbidden_key(v) for v in o)
    return False


def check_disposition_obj(disp, reply, diag):
    opts = _diag_options(diag)
    if not isinstance(disp, dict) or disp.get("schema") != DISPOSITION_SCHEMA:
        raise Refused("disposition", "SCHEMA the disposition schema is not %r" % DISPOSITION_SCHEMA)
    if disp.get("next_trusted") is not False:
        raise Refused("disposition", "TRUST next_trusted must be exactly false")
    if disp.get("trust_basis") != "none":
        raise Refused("disposition", "TRUST trust_basis must be none")
    att = disp.get("consumed_attempts")
    if not isinstance(att, list) or len(att) != 2 or not all(isinstance(a, dict) and a.get("rejudged_for_trust") is False for a in att):
        raise Refused("disposition", "REJUDGE two consumed attempts, each with rejudged_for_trust exactly false")
    if _has_forbidden_key(disp):
        raise Refused("disposition", "OVERRIDE a key named overrides or waiver exists (D-02)")
    opt, go = check_reply_obj(reply, diag, None)
    if disp.get("option") != opt:
        raise Refused("disposition", "OPTION %r differs from the reply's %r" % (disp.get("option"), opt))
    if disp.get("supersedes") != opts[opt]["supersedes"]:
        raise Refused("disposition", "SUPERSEDES differs from the option's")
    want_rule = opts[opt] if opts[opt]["paid_run"] else None
    if disp.get("rule") != want_rule:
        raise Refused("disposition", "RULE differs from the option's block")
    if disp.get("run_go") != go or disp.get("paid_run_planned") != opts[opt]["paid_run"]:
        raise Refused("disposition", "RUN-GO or paid_run_planned differs from the reply's and the option's")
    if disp.get("declaration_pending") is not bool(opts[opt]["paid_run"]):
        raise Refused("disposition", "DECLARATION declaration_pending must be true exactly for a paid option")
    if disp.get("class") != diag["class"]["name"] or disp.get("reply_brief_sha256") != reply["brief_sha256"]:
        raise Refused("disposition", "CLASS or reply_brief_sha256 differs from the diagnosis and the reply")
    return opt


def cmd_write_disposition(opts):
    reply = read_json(opts["--reply"], "disposition")
    diag = read_json(opts["--diagnosis"], "disposition")
    disp = build_disposition(reply, diag)
    write_text(opts["--out"], dumps(disp))
    print("DISPOSITION WRITTEN option=%s next_trusted=false" % disp["option"])
    return 0


def cmd_check_disposition(opts):
    disp = read_json(opts["--disposition"], "disposition")
    reply = read_json(opts["--reply"], "disposition")
    diag = read_json(opts["--diagnosis"], "disposition")
    opt = check_disposition_obj(disp, reply, diag)
    print("DISPOSITION OK option=%s next_trusted=false" % opt)
    return 0


# ---- scan-identifiers and classify-run (plan 15-11, Task 3) ---------------------------------------------------------

_ID_PATTERNS = (
    ("account-id", re.compile(r"(?<![A-Za-z0-9_])[0-9]{12}(?![A-Za-z0-9_])")),
    ("arn", re.compile(r"arn:aws", re.I)),
    ("aws-host", re.compile(r"amazonaws\.com", re.I)),
    ("presigned", re.compile(r"X-Amz-", re.I)),
    ("execute-api", re.compile(r"execute-api", re.I)),
)


def scan_line(line):
    """The identifier kinds one line holds, in table order (a twelve-digit run delimited by non-word characters, arn:aws,
    amazonaws.com, X-Amz-, execute-api). A 64-hex digest or a 10-digit timestamp holds none."""
    return [kind for kind, rx in _ID_PATTERNS if rx.search(line)]


def scan_text(text):
    """[(line_number, kind)] for every identifier kind on every line, 1-based."""
    out = []
    for n, line in enumerate(text.split("\n"), 1):
        for kind in scan_line(line):
            out.append((n, kind))
    return out


def scan_files(paths):
    """The number of files scanned, or Refused identifiers naming the first file:line:kind."""
    if not paths:
        raise Refused("usage", "scan-identifiers needs at least one file")
    for path in paths:
        try:
            text = read_bytes(path, "identifiers").decode("utf-8")
        except UnicodeDecodeError:
            raise Refused("identifiers", "%s is not UTF-8 text" % path)
        hits = scan_text(text)
        if hits:
            raise Refused("identifiers", "%s:%d:%s" % (path, hits[0][0], hits[0][1]))
    return len(paths)


def cmd_scan_identifiers(args):
    n = scan_files(args)
    print("NO IDENTIFIERS files=%d" % n)
    return 0


def _plain_file(path):
    return os.path.isfile(path) and not os.path.islink(path)


def classify_run(run_dir):
    """The CONSUMPTION RULE of 15-GATE-RUN-EVIDENCE.json, in its order: {class, reason}. A missing file alone is never
    'not consumed'.

    1. manifest.json absent, or logs/outcome.json absent or without a string exit_class -> unresolved.
    2. exit_class pass or gate_fail, or run/gate-report.json exists, or logs/train.log has a line equal to GATE PASS or
       GATE FAIL -> consumed.
    3. exit_class refused or crash with none of that evidence -> pre-verdict.
    4. anything else -> unresolved."""
    if not _plain_file(os.path.join(run_dir, "manifest.json")):
        return {"class": "unresolved", "reason": "manifest.json is absent (the fetch failed or never ran)"}
    exit_class = None
    try:
        outcome = read_json(os.path.join(run_dir, "logs", "outcome.json"), "classify-run")
        if isinstance(outcome, dict) and isinstance(outcome.get("exit_class"), str):
            exit_class = outcome["exit_class"]
    except Refused:
        pass
    if exit_class is None:
        return {"class": "unresolved", "reason": "logs/outcome.json is absent or has no string exit_class"}
    if exit_class in ("pass", "gate_fail"):
        return {"class": "consumed", "reason": "exit_class is %s: the gate executed" % exit_class}
    if _plain_file(os.path.join(run_dir, "run", "gate-report.json")):
        return {"class": "consumed", "reason": "run/gate-report.json exists: the gate executed"}
    try:
        log = read_bytes(os.path.join(run_dir, "logs", "train.log"), "classify-run").decode("utf-8", "replace")
    except Refused:
        log = ""
    if any(ln.rstrip("\r") in ("GATE PASS", "GATE FAIL") for ln in log.split("\n")):
        return {"class": "consumed", "reason": "logs/train.log holds a GATE PASS or GATE FAIL line: the gate executed"}
    if exit_class in ("refused", "crash"):
        return {"class": "pre-verdict", "reason": "exit_class is %s with no gate report and no GATE line" % exit_class}
    return {"class": "unresolved", "reason": "exit_class %r with no gate evidence" % exit_class}


def cmd_classify_run(opts):
    print(json.dumps(classify_run(opts["--run-dir"]), sort_keys=True, separators=(",", ":")))
    return 0


def check_run_go(run_go_path, blob_path, commit):
    """Plan 15-12 (D-03): is this orchestrator-recorded go bound to the exact committed declaration? It judges FIELDS only:
    schema laya-run-go-v1, recorded_by orchestrator, interpreted_as run-go, declaration_commit equal to the given commit and
    declaration_sha256 equal to the sha256 of the materialized blob. It never inspects owner_reply_verbatim (the lesson of
    15-RUN-GO.json, whose literal reply 'go' holds no 'run-go' substring)."""
    rec = read_json(run_go_path, "run-go")
    if not isinstance(rec, dict):
        raise Refused("run-go", "the record is not a JSON object")
    if rec.get("schema") != "laya-run-go-v1":
        raise Refused("run-go", "schema is not laya-run-go-v1")
    if rec.get("recorded_by") != "orchestrator":
        raise Refused("run-go", "recorded_by is not orchestrator")
    if rec.get("interpreted_as") != "run-go":
        raise Refused("run-go", "interpreted_as is not run-go")
    if not isinstance(commit, str) or not commit or rec.get("declaration_commit") != commit:
        raise Refused("run-go", "declaration_commit differs from the committed declaration's commit")
    digest = sha256_hex(read_bytes(blob_path, "run-go"))
    if rec.get("declaration_sha256") != digest:
        raise Refused("run-go", "declaration_sha256 differs from the sha256 of the committed declaration blob")
    return True


def cmd_check_run_go(opts):
    check_run_go(opts["--run-go"], opts["--blob"], opts["--commit"])
    print("RUN-GO OK")
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
               "gate": {"zero_shot_macro_f1": zs_f1,
                        "thresholds": {"min_macro_f1_margin": 0.05, "max_ece": 0.10, "ece_bins": 15}},
               "pack_task_seconds": 149.31}
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


def _unit(seed, margin, ece, run=None):
    u = {"seed": seed, "margin": margin, "ece": ece}
    if run:
        u["run"] = run
    return u


def selftest_rule(c):
    """Task 2 cases fixed BEFORE analyze runs on the real fixture: the class rule and its statistics."""
    # The three class outcomes, each label naming the class it returns.
    def run_class(mean, sd, shortfall, median_se, p_declared, n=6):
        full = deficiency_status(mean, sd, n)
        loo = [deficiency_status(mean, sd, n - 1)] * n
        return class_rule(full, loo, shortfall, sd, median_se, p_declared)[0]

    c.check("class rule: recipe-deficiency on a CUDA set with mean 0.20 and sd 0.01 (upper 90 percent bound below the floor)",
            run_class(0.20, 0.01, 0.10, 0.065, 0.0) == "recipe-deficiency")
    c.check("class rule: single-draw-acceptance on a set shaped like the real one (mean 0.328, sd 0.044, shortfall 0.0205)",
            run_class(0.328, 0.044, 0.0205, 0.065, 0.259) == "single-draw-acceptance")
    c.check("class rule: undetermined when the shortfall exceeds the between-seed sd (mean 0.35, sd 0.002, shortfall 0.03)",
            run_class(0.35, 0.002, 0.03, 0.065, 0.259) == "undetermined")
    c.check("class rule: undetermined when the declared pass probability is at least 0.5",
            run_class(0.328, 0.044, 0.0205, 0.065, 0.5) == "undetermined")
    c.check("class rule: a leave-one-out set that supports deficiency blocks single-draw-acceptance",
            class_rule("refuted", ["refuted", "supported"], 0.0205, 0.044, 0.065, 0.259)[0] == "undetermined")
    c.check("deficiency: the three statuses at the bounds (supported, undetermined, refuted)",
            [deficiency_status(0.20, 0.01, 6), deficiency_status(0.30, 0.044, 6), deficiency_status(0.40, 0.01, 6)]
            == ["supported", "undetermined", "refuted"])
    # The pass probability of a hand-built three-unit set equals a hand enumeration of the 27 ordered triples.
    # A (ece 0.08, margin 0.35) < B (0.09, 0.28) < C (0.11, 0.40) by ECE. The median is C iff at least two draws are C
    # (7 of 27), likewise A (7) and B (13). A passes, B is a defect (margin below the floor), C is an in-band gate fail.
    units = [_unit(13, 0.35, 0.08), _unit(17, 0.28, 0.09), _unit(23, 0.40, 0.11)]
    r = resample_rule(units, "shipped", FLOOR)
    c.check("resample: p_gate 20/27, p_pass 7/27, p_in_band_gate_fail 7/27, p_defect 13/27 equal the hand enumeration",
            r["triples"] == 27 and abs(r["p_gate"] - 20 / 27) < 1e-15 and abs(r["p_pass"] - 7 / 27) < 1e-15
            and abs(r["p_in_band_gate_fail"] - 7 / 27) < 1e-15 and abs(r["p_defect"] - 13 / 27) < 1e-15, repr(r))
    c.check("resample: within_cap2 is p_pass + p_in_band_gate_fail x p_pass",
            abs(r["within_cap2"] - (7 / 27 + 7 / 27 * 7 / 27)) < 1e-15)
    rm = resample_rule(units, "mean", FLOOR)
    # Mean rule by multiset (a, b, c draws of A, B, C): pass 1 AAA + 3 AAB + 3 AAC + 3 ABB + 6 ABC + 3 BBC = 19; in-band
    # gate fail 3 ACC + 3 BCC + 1 CCC = 7; defect 1 BBB (mean 0.28 is below the floor).
    c.check("resample: the mean rule judges the mean of the three drawn margins (hand count 19, 7 and 1 of 27)",
            abs(rm["p_pass"] - 19 / 27) < 1e-15 and abs(rm["p_in_band_gate_fail"] - 7 / 27) < 1e-15
            and abs(rm["p_defect"] - 1 / 27) < 1e-15 and abs(rm["p_gate"] - 20 / 27) < 1e-15, repr(rm))
    a = resample_with_loo(units, "shipped", FLOOR)
    c.check("resample: the leave-one-out list has one entry per unit and its min/max bracket every entry",
            len(a["loo"]) == 3 and a["loo_min"] == min(e["p_pass"] for e in a["loo"])
            and a["loo_max"] == max(e["p_pass"] for e in a["loo"]) and a["loo_min"] <= a["loo_max"])
    lo_only = resample_rule([units[0], units[2]], "shipped", FLOOR)
    c.check("resample: two units give 8 triples", lo_only["triples"] == 8)
    # The rank-key tie-break ships the smaller seed.
    k1, k2, low = _unit(17, 0.3, 0.08015), _unit(13, 0.3, 0.08018), _unit(23, 0.3, 0.05)
    c.check("ship: equal rank keys (floor(ece x 10000) = 801) ship the smaller seed 13",
            rank_key(0.08015) == rank_key(0.08018) == 801 and ship([low, k1, k2])["seed"] == 13 and ship([k1, k2, low])["seed"] == 13)
    c.check("ship: the median by ECE, not by margin", ship([_unit(13, 0.9, 0.05), _unit(17, 0.1, 0.07), _unit(23, 0.5, 0.09)])["seed"] == 17)
    # df 2 two-sided p.
    c.check("p_df2: t = 2.899 gives 0.1012 and t = 0 gives 1.0", abs(p_df2(2.899) - 0.1012) < 5e-5 and p_df2(0.0) == 1.0, repr(p_df2(2.899)))
    m, sd, t, p = paired_t_df2([0.0380, 0.0641, 0.0168])
    c.check("paired t: three differences give mean 0.0396 and p near 0.101", abs(m - 0.0396333) < 1e-6 and 0.100 < p < 0.103, repr((m, t, p)))
    c.check("device anchoring: same sign with p above 0.05 is unproven; a tight same-sign set is supported",
            device_status([0.0380, 0.0641, 0.0168])[0] == "unproven" and device_status([0.10, 0.11, 0.12])[0] == "supported"
            and device_status([0.10, -0.11, 0.12])[0] == "unproven")
    c.check("floor rule value: mean minus 2 sd rounded DOWN to hundredths (0.2396 -> 0.23, 0.25 -> 0.25)",
            floor_rule_value(0.32788, 0.04412) == 0.23 and floor_rule_value(0.30, 0.025) == 0.25)
    c.check("statistics anchors: mean, sd (ddof 1) and Pearson", mean_of([1.0, 2.0, 3.0]) == 2.0 and abs(sd_of([1.0, 2.0, 3.0]) - 1.0) < 1e-15
            and abs(pearson([1.0, 2.0, 3.0], [2.0, 4.0, 6.0]) - 1.0) < 1e-12 and abs(pearson([1.0, 2.0, 3.0], [3.0, 2.0, 1.0]) + 1.0) < 1e-12)
    # The one-pass bootstrap F1 equals macro_f1, and the bootstrap is deterministic for its seed.
    rng = random.Random(3)
    yy = [rng.randrange(3) for _ in range(100)]
    pp = [rng.randrange(3) for _ in range(100)]
    c.check("bootstrap: the one-pass macro-F1 equals macro_f1 bit for bit on 200 random draws",
            all(_counts_f1(a, b) == macro_f1(a, b) for a, b in
                [([rng.randrange(3) for _ in range(30)], [rng.randrange(3) for _ in range(30)]) for _ in range(200)]))
    s1, s2 = bootstrap_index_sets(100, 5, 11), bootstrap_index_sets(100, 5, 11)
    c.check("bootstrap: the same seed gives the same index sets, another seed another", s1 == s2 and s1 != bootstrap_index_sets(100, 5, 12))
    bm = bootstrap_margins(yy, pp, pp, s1)
    c.check("bootstrap: identical fine-tuned and zero-shot predictions give margin 0.0 on every resample", all(x == 0.0 for x in bm))
    # RULE_SPEC: the constants asserted against the plan and the hash sensitive to every change.
    c.check("RULE_SPEC carries the plan's constants (floor 0.30, B 2000, seed 20261031, z, rank_scale)",
            RULE_SPEC["floor"] == 0.30 and RULE_SPEC["ceiling"] == 0.45 and RULE_SPEC["ece_band"] == [0.07, 0.12]
            and RULE_SPEC["bootstrap"]["B"] == 2000 and RULE_SPEC["bootstrap"]["seed"] == 20261031
            and RULE_SPEC["z_one_sided_90"] == 1.2815515655446004 and RULE_SPEC["rank_scale"] == 10000)
    changed = _clone(RULE_SPEC)
    changed["floor"] = 0.29
    c.check("RULE_SPEC: a changed constant changes rule_sha256", rule_sha256(changed) != rule_sha256() and rule_sha256() == rule_sha256(_clone(RULE_SPEC)))


def synth_reader(evidence):
    """A tracked-file reader over synthetic bytes: the three evidence files, the drafts file and every cited file."""
    files = {RUNS[rid]["evidence"]: evidence[rid] for rid in RUN_IDS}
    files[DRAFTS_PATH] = json.dumps({"drafts": {"epochs-12-redraw": {
        "must": {"recipe_id": "cd" * 32}, "report_only": {"references": {"attempt_1_billable_seconds": 1346}}}}}).encode("utf-8")
    for path, needle in CITATIONS:
        files[path] = (files.get(path, b"") + b"x " + needle.encode("utf-8") + b" y\n")

    def reader(path):
        if path not in files:
            raise Refused("analyze", "%s is missing" % path)
        return files[path]
    return reader


def selftest_analyze(c):
    """Task 2 cases after the rule: the assembled diagnosis, its determinism and its refusals."""
    fx, evidence = synth_fixture()
    raw = dumps(fx).encode("utf-8")
    reader = synth_reader(evidence)
    d1 = analyze_obj(fx, raw, reader, B=40)
    d2 = analyze_obj(fx, raw, reader, B=40)
    c.check("analyze: a second run on the same fixture is byte-identical", dumps(d1) == dumps(d2))
    c.check("analyze: the recompute block shows 9 of 9 exact", len(d1["recompute"]["seed_runs"]) == 9 and d1["recompute"]["all_exact"] is True)
    c.check("analyze: five option ids with the plan's key set, a paid_run flag and a supersedes list each",
            sorted(d1["options"]) == sorted(OPTION_IDS) and all(
                sorted(o) == sorted(["paid_run", "judging", "margin_band", "ece_band", "attempt_cap", "retry_classes", "epochs",
                                     "recipe_id", "run_name", "dataset_prefix", "supersedes", "p_pass", "within_cap", "loo_min",
                                     "loo_max", "cost_report_only", "claim_scope"]) and isinstance(o["supersedes"], list)
                for o in d1["options"].values()))
    o = d1["options"]
    c.check("analyze: supersedes lists (redraw none, mean-rule D-11, mean-rule-retry D-11 and D-12, floor-rule D-11, stop-accept none)",
            [o[k]["supersedes"] for k in ("redraw", "mean-rule", "mean-rule-retry", "floor-rule", "stop-accept")]
            == [[], ["D-11"], ["D-11", "D-12"], ["D-11"], []])
    c.check("analyze: every paid claim_scope starts with the literal prefix; run name motivation-v1-p15b; 12 epochs",
            all(o[k]["claim_scope"].startswith(CLAIM_PREFIX) and o[k]["run_name"] == "motivation-v1-p15b" and o[k]["epochs"] == 12
                for k in OPTION_IDS if k != "stop-accept") and o["stop-accept"]["paid_run"] is False)
    c.check("analyze: the retry option is the only one with attempt_cap 2 and within_cap above its p_pass",
            o["mean-rule-retry"]["attempt_cap"] == 2 and o["mean-rule"]["attempt_cap"] == 1
            and o["mean-rule-retry"]["within_cap"] >= o["mean-rule-retry"]["p_pass"] and o["mean-rule"]["within_cap"] == o["mean-rule"]["p_pass"])
    c.check("analyze: recommended is rule-derived (mean-rule iff single-draw-acceptance and mean p_pass above redraw p_pass)",
            d1["recommended"]["option"] == ("mean-rule" if (d1["class"]["name"] == "single-draw-acceptance"
                                                            and o["mean-rule"]["p_pass"] > o["redraw"]["p_pass"]) else "stop-accept"))
    c.check("analyze: next_trusted_effect none, rejudge exploratory and not trust-setting, no next_trusted true",
            d1["next_trusted_effect"] == "none" and d1["rejudge"]["sets_next_trusted"] is False
            and d1["rejudge"]["exploratory"] is True and not _true_next_trusted(d1))
    c.check("analyze: every brief fact is a single line", all("\n" not in x for x in d1["brief_facts"]) and len(d1["brief_facts"]) >= 20)
    text = dumps(d1)
    try:
        ok = check_diagnosis_obj(text, fx, raw, reader, B=40)["class"]["name"] == d1["class"]["name"]
        c.check("check-diagnosis: the untouched diagnosis is accepted", ok)
    except Refused as ex:
        c.check("check-diagnosis: the untouched diagnosis is accepted", False, str(ex))

    def corrupt(fn):
        m = _clone(d1)
        fn(m)
        return dumps(m)

    c.refused("check-diagnosis: one statistic corrupted (the CUDA mean) is refused",
              lambda: check_diagnosis_obj(corrupt(lambda m: m["spread"]["cuda"].update({"mean": m["spread"]["cuda"]["mean"] + 1e-9})),
                                          fx, raw, reader, B=40), "STALE")
    c.refused("check-diagnosis: one resampled pass probability corrupted is refused",
              lambda: check_diagnosis_obj(corrupt(lambda m: m["resample"]["declared"].update({"p_pass": 0.5})), fx, raw, reader, B=40),
              "STALE")
    c.refused("check-diagnosis: a brief fact edited is refused",
              lambda: check_diagnosis_obj(corrupt(lambda m: m["brief_facts"].__setitem__(0, "class: x")), fx, raw, reader, B=40), "STALE")
    c.refused("check-diagnosis: a changed rule hash is refused",
              lambda: check_diagnosis_obj(corrupt(lambda m: m.update({"rule_sha256": "0" * 64})), fx, raw, reader, B=40), "RULE-HASH")
    c.refused("check-diagnosis: a true next_trusted at depth is refused",
              lambda: check_diagnosis_obj(corrupt(lambda m: m["rejudge"].update({"next_trusted": True})), fx, raw, reader, B=40), "TRUST")
    c.refused("check-diagnosis: next_trusted_effect other than none is refused",
              lambda: check_diagnosis_obj(corrupt(lambda m: m.update({"next_trusted_effect": "sets"})), fx, raw, reader, B=40), "TRUST")
    c.refused("check-diagnosis: all_exact false is refused",
              lambda: check_diagnosis_obj(corrupt(lambda m: m["recompute"].update({"all_exact": False})), fx, raw, reader, B=40), "RECOMPUTE")
    c.refused("check-diagnosis: a citation not found is refused",
              lambda: check_diagnosis_obj(corrupt(lambda m: m["provenance"]["citations"][0].update({"found": False})), fx, raw, reader, B=40),
              "CITATION")
    # A fixture whose evidence disagrees refuses before any statistic is produced.
    bad = _clone(fx)
    bad["seed_runs"][0]["pred"][0] = (bad["seed_runs"][0]["pred"][0] + 1) % 3
    c.refused("analyze: a fixture that is not bit-exact against the evidence is refused", lambda: analyze_obj(bad, raw, reader, B=40), "MISMATCH")
    fx2, ev2 = synth_fixture()
    doc = json.loads(ev2["p15"].decode("utf-8"))
    doc["gate"]["thresholds"]["max_ece"] = 0.2
    ev2["p15"] = json.dumps(doc, sort_keys=True).encode("utf-8")
    fx2["runs"]["p15"]["evidence_sha256"] = sha256_hex(ev2["p15"])
    c.refused("analyze: gate thresholds that differ from RULE_SPEC are refused",
              lambda: analyze_obj(fx2, dumps(fx2).encode("utf-8"), synth_reader(ev2), B=40), "thresholds")

def _good_reply(diag, opt, brief_sha, go=None):
    paid = diag["options"][opt]["paid_run"]
    return {"schema": REPLY_SCHEMA, "recorded_by": "orchestrator", "recorded_at": "2026-11-01T06:00:00Z",
            "owner_reply_verbatim": "%s %s" % (opt, "run-hold" if paid else ""), "option": opt,
            "run_go": (False if go is None else go) if paid else None,
            "supersedes": diag["options"][opt]["supersedes"], "brief_sha256": brief_sha}


def selftest_decision(c):
    """Task 3 cases: the brief check, the reply check, the disposition and its refusals."""
    fx, evidence = synth_fixture()
    raw = dumps(fx).encode("utf-8")
    diag = analyze_obj(fx, raw, synth_reader(evidence), B=40)
    brief = "\n\n".join(h + "\n\n" + "body" for h in BRIEF_HEADINGS) + "\n\n" + "\n".join(diag["brief_facts"]) + "\n" + " ".join(OPTION_IDS) + "\n"
    brief_sha = sha256_hex(brief.encode("utf-8"))
    try:
        n = check_brief_text(brief, diag)
        c.check("check-brief: a brief containing every fact, the ten headings and the five ids is accepted", n == len(diag["brief_facts"]))
    except Refused as ex:
        c.check("check-brief: a brief containing every fact, the ten headings and the five ids is accepted", False, str(ex))
    c.refused("check-brief: a removed fact is refused", lambda: check_brief_text(brief.replace(diag["brief_facts"][2], ""), diag), "FACT")
    digit_fact = diag["brief_facts"][2]
    changed = digit_fact.replace(digit_fact[-1], "9" if digit_fact[-1] != "9" else "8")
    c.refused("check-brief: one changed digit is refused", lambda: check_brief_text(brief.replace(digit_fact, changed), diag), "FACT")
    c.refused("check-brief: a missing heading is refused", lambda: check_brief_text(brief.replace("## Safeguards\n", "## Safe\n"), diag), "HEADING")
    swapped = brief.replace("## Options\n", "## TMP\n").replace("## Evidence\n", "## Options\n").replace("## TMP\n", "## Evidence\n")
    c.refused("check-brief: headings out of order are refused", lambda: check_brief_text(swapped, diag), "HEADING order")
    c.refused("check-brief: a missing option id is refused",
              lambda: check_brief_text(brief.replace("floor-rule", "floorrule"), dict(diag, brief_facts=[f for f in diag["brief_facts"] if "floor-rule" not in f])), "OPTION")
    # check-reply: each of the five options, then the red side.
    for opt in OPTION_IDS:
        try:
            got = check_reply_obj(_good_reply(diag, opt, brief_sha), diag, brief_sha)
            c.check("check-reply: a well-formed reply for %s is accepted" % opt, got[0] == opt)
        except Refused as ex:
            c.check("check-reply: a well-formed reply for %s is accepted" % opt, False, str(ex))

    def rep(**kw):
        r = _good_reply(diag, "mean-rule", brief_sha)
        r.update(kw)
        return r

    c.refused("check-reply: recorded_by other than orchestrator is refused", lambda: check_reply_obj(rep(recorded_by="executor"), diag, brief_sha), "RECORDED-BY")
    c.refused("check-reply: an option outside the five is refused", lambda: check_reply_obj(rep(option="epochs-4"), diag, brief_sha), "OPTION")
    c.refused("check-reply: a brief digest that is not the brief's is refused", lambda: check_reply_obj(rep(brief_sha256="0" * 64), diag, brief_sha), "BRIEF-DIGEST")
    c.refused("check-reply: a supersedes list that differs from the option's is refused", lambda: check_reply_obj(rep(supersedes=[]), diag, brief_sha), "SUPERSEDES")
    c.refused("check-reply: run_go null on a paid option is refused", lambda: check_reply_obj(rep(run_go=None), diag, brief_sha), "RUN-GO")
    c.refused("check-reply: run_go as the string 'true' is refused", lambda: check_reply_obj(rep(run_go="true"), diag, brief_sha), "RUN-GO")
    c.refused("check-reply: run_go non-null on stop-accept is refused",
              lambda: check_reply_obj(dict(_good_reply(diag, "stop-accept", brief_sha), run_go=False), diag, brief_sha), "RUN-GO")
    c.refused("check-reply: a timestamp that is not ISO 8601 UTC is refused", lambda: check_reply_obj(rep(recorded_at="yesterday"), diag, brief_sha), "RECORDED-AT")
    c.refused("check-reply: an empty verbatim reply is refused", lambda: check_reply_obj(rep(owner_reply_verbatim="  "), diag, brief_sha), "VERBATIM")
    c.refused("check-reply: an unknown key such as overrides is refused", lambda: check_reply_obj(rep(overrides={}), diag, brief_sha), "KEY")
    # The reply is judged on its fields, never on a substring of the verbatim text (the lesson of 15-RUN-GO.json).
    go_reply = rep(owner_reply_verbatim="go", run_go=True)
    hold_with_go_word = rep(owner_reply_verbatim="mean-rule run-go", run_go=False)
    c.check("check-reply: the literal reply 'go' is judged on option and run_go, not on its text",
            check_reply_obj(go_reply, diag, brief_sha) == ("mean-rule", True)
            and build_disposition(go_reply, diag)["run_go"] is True)
    c.check("check-reply: a verbatim text containing 'run-go' with run_go false stays false in the disposition",
            build_disposition(hold_with_go_word, diag)["run_go"] is False)
    c.check("check-reply: a verbatim text naming another option does not change the recorded option",
            build_disposition(rep(owner_reply_verbatim="stop-accept"), diag)["option"] == "mean-rule")
    # write-disposition and check-disposition.
    for opt in OPTION_IDS:
        r = _good_reply(diag, opt, brief_sha)
        try:
            d = build_disposition(r, diag)
            check_disposition_obj(d, r, diag)
            ok = (d["next_trusted"] is False and d["trust_basis"] == "none" and d["gap"] == "SC4"
                  and d["declaration_pending"] is diag["options"][opt]["paid_run"] and len(d["consumed_attempts"]) == 2)
            c.check("write-disposition: %s is derived from the reply and the diagnosis and check-disposition accepts it" % opt, ok)
        except Refused as ex:
            c.check("write-disposition: %s is derived from the reply and the diagnosis and check-disposition accepts it" % opt, False, str(ex))
    r0 = _good_reply(diag, "mean-rule", brief_sha)
    d0 = build_disposition(r0, diag)
    c.check("write-disposition: stop-accept says SC4 stays unmet and is not an override",
            "this is not an override" in build_disposition(_good_reply(diag, "stop-accept", brief_sha), diag)["phase_state"])

    def disp_with(fn):
        m = _clone(d0)
        fn(m)
        return m

    c.refused("check-disposition: next_trusted true is refused", lambda: check_disposition_obj(disp_with(lambda m: m.update({"next_trusted": True})), r0, diag), "TRUST")
    c.refused("check-disposition: next_trusted absent is refused", lambda: check_disposition_obj(disp_with(lambda m: m.pop("next_trusted")), r0, diag), "TRUST")
    c.refused("check-disposition: trust_basis other than none is refused", lambda: check_disposition_obj(disp_with(lambda m: m.update({"trust_basis": "mean-view"})), r0, diag), "TRUST")
    c.refused("check-disposition: a consumed attempt re-judged for trust is refused",
              lambda: check_disposition_obj(disp_with(lambda m: m["consumed_attempts"][1].update({"rejudged_for_trust": True})), r0, diag), "REJUDGE")
    c.refused("check-disposition: an overrides key at depth is refused",
              lambda: check_disposition_obj(disp_with(lambda m: m["rule"].update({"overrides": {}})), r0, diag), "OVERRIDE")
    c.refused("check-disposition: a waiver key is refused", lambda: check_disposition_obj(disp_with(lambda m: m.update({"waiver": "x"})), r0, diag), "OVERRIDE")
    c.refused("check-disposition: an option that differs from the reply's is refused",
              lambda: check_disposition_obj(disp_with(lambda m: m.update({"option": "redraw"})), r0, diag), "OPTION")
    c.refused("check-disposition: a rule that differs from the option's block is refused",
              lambda: check_disposition_obj(disp_with(lambda m: m["rule"].update({"p_pass": 0.99})), r0, diag), "RULE")
    c.refused("check-disposition: a supersedes list that differs is refused",
              lambda: check_disposition_obj(disp_with(lambda m: m.update({"supersedes": []})), r0, diag), "SUPERSEDES")
    c.refused("check-disposition: a flipped run_go is refused",
              lambda: check_disposition_obj(disp_with(lambda m: m.update({"run_go": True})), r0, diag), "RUN-GO")


def selftest_gap_tools(c):
    """Plan 15-11 cases: scan-identifiers and classify-run, fixed BEFORE the functions were written."""
    import shutil
    import tempfile
    # scan-identifiers: the red side (each kind refused) and the green side (a digest and a timestamp pass).
    twelve = "1" * 12
    for kind, line in (("account-id", "account " + twelve + " here"),
                       ("account-id", '"' + twelve + '"'),
                       ("arn", "role arn:aws:iam::x:role/y"),
                       ("aws-host", "https://bucket.s3.amazonaws.com/key"),
                       ("presigned", "?X-Amz-Signature=abc"),
                       ("execute-api", "https://abc.execute-api.region.invalid/mcp")):
        c.check("scan-identifiers: %s is flagged (%s)" % (kind, line[:30]), scan_line(line) == [kind], repr(scan_line(line)))
    c.check("scan-identifiers: a 64-hex digest and a 10-digit timestamp are accepted",
            scan_line("sha256:" + "ab" * 32 + " at 1761999999") == [] and scan_line("d" + twelve + "e") == [])
    c.check("scan-identifiers: 11 and 13 digit runs are accepted", scan_line("1" * 11) == [] and scan_line("1" * 13) == [])
    c.check("scan-identifiers: a file with a clean line and a bad line names the bad line",
            scan_text("ok\nx " + twelve + "\n") == [(2, "account-id")])
    d = tempfile.mkdtemp(prefix="margin-gap-selftest-")
    try:
        good = os.path.join(d, "good.json")
        bad = os.path.join(d, "bad.json")
        write_text(good, '{"a": "sha256:' + "ab" * 32 + '"}\n')
        write_text(bad, "line one\narn:aws:s3:::b\n")
        c.check("scan-identifiers: NO IDENTIFIERS files=1 on a clean file", scan_files([good]) == 1)
        c.refused("scan-identifiers: a bad file is refused with file:line:kind", lambda: scan_files([good, bad]), "bad.json:2:arn")
        c.refused("scan-identifiers: a missing file is refused", lambda: scan_files([os.path.join(d, "none.json")]), "identifiers")
        c.refused("scan-identifiers: no file at all is refused", lambda: scan_files([]), "usage")

        # classify-run: the four failure paths of the CONSUMPTION RULE.
        def run(name, outcome="pass", report=True, train_log="GATE PASS\n", manifest=True):
            r = os.path.join(d, name)
            os.makedirs(os.path.join(r, "logs"))
            os.makedirs(os.path.join(r, "run"))
            if manifest:
                write_text(os.path.join(r, "manifest.json"), "{}\n")
            if outcome is not None:
                write_text(os.path.join(r, "logs", "outcome.json"),
                           outcome if isinstance(outcome, str) and outcome.startswith("{") or outcome == "garbage"
                           else json.dumps({"exit_class": outcome}))
            if report:
                write_text(os.path.join(r, "run", "gate-report.json"), "{}\n")
            write_text(os.path.join(r, "logs", "train.log"), train_log)
            return r

        def cls(r):
            return classify_run(r)["class"]

        c.check("classify-run: exit_class pass -> consumed", cls(run("a1")) == "consumed")
        c.check("classify-run: a GATE FAIL line only (exit_class crash, no report) -> consumed",
                cls(run("a2", outcome="crash", report=False, train_log="x\nGATE FAIL\n")) == "consumed")
        c.check("classify-run: exit_class gate_fail -> consumed", cls(run("a3", outcome="gate_fail", report=False, train_log="")) == "consumed")
        c.check("classify-run: a gate report beside exit_class crash -> consumed", cls(run("a4", outcome="crash", report=True, train_log="")) == "consumed")
        c.check("classify-run: crash with no gate evidence -> pre-verdict",
                cls(run("b1", outcome="crash", report=False, train_log="Traceback\n")) == "pre-verdict")
        c.check("classify-run: refused with no gate evidence -> pre-verdict",
                cls(run("b2", outcome="refused", report=False, train_log="")) == "pre-verdict")
        c.check("classify-run: no manifest (the fetch failed) -> unresolved", cls(run("u1", manifest=False)) == "unresolved")
        c.check("classify-run: no outcome.json -> unresolved, never not-consumed", cls(run("u2", outcome=None)) == "unresolved")
        c.check("classify-run: an outcome.json that is not JSON -> unresolved", cls(run("u3", outcome="garbage")) == "unresolved")
        c.check("classify-run: an unknown exit_class with no gate evidence -> unresolved",
                cls(run("u4", outcome="weird", report=False, train_log="")) == "unresolved")
        c.check("classify-run: a GATE PASS substring inside a longer line is not a GATE line",
                cls(run("u5", outcome="weird", report=False, train_log="not a GATE PASS line\n")) == "unresolved")
        c.check("classify-run: a run directory that does not exist -> unresolved", cls(os.path.join(d, "no-such")) == "unresolved")
        c.check("classify-run: the reason is a non-empty string", all(classify_run(os.path.join(d, n))["reason"] for n in ("a1", "b1", "u1")))
    finally:
        shutil.rmtree(d, ignore_errors=True)


def selftest_run_go(c):
    """Plan 15-12 cases for check-run-go, fixed BEFORE the function was written."""
    import shutil
    import tempfile
    d = tempfile.mkdtemp(prefix="margin-rungo-selftest-")
    try:
        blob = os.path.join(d, "decl.json")
        write_text(blob, '{"declaration": "bytes"}\n')
        digest = sha256_hex(read_bytes(blob, "selftest"))
        commit = "ab" * 20

        def rec(name, **over):
            r = {"schema": "laya-run-go-v1", "recorded_by": "orchestrator", "recorded_at": "2026-11-01T00:00:00Z",
                 "owner_reply_verbatim": "go", "interpreted_as": "run-go", "declaration_path": "x.json",
                 "declaration_commit": commit, "declaration_sha256": digest}
            r.update(over)
            path = os.path.join(d, name)
            write_text(path, dumps(r))
            return path

        def ok(path, cm=commit):
            return check_run_go(path, blob, cm) is True

        c.check("check-run-go: accepts a good record", ok(rec("g1.json")))
        c.check("check-run-go: accepts a record whose verbatim reply is the literal 'go'", ok(rec("g2.json", owner_reply_verbatim="go")))
        c.check("check-run-go: judges fields, so a verbatim reply with no run-go substring still passes",
                ok(rec("g3.json", owner_reply_verbatim="yes, go ahead")))
        c.refused("check-run-go: refuses a wrong declaration digest", lambda: ok(rec("b1.json", declaration_sha256="0" * 64)), "declaration_sha256")
        c.refused("check-run-go: refuses a wrong commit", lambda: ok(rec("b2.json"), "cd" * 20), "declaration_commit")
        c.refused("check-run-go: refuses recorded_by not orchestrator", lambda: ok(rec("b3.json", recorded_by="executor")), "recorded_by")
        c.refused("check-run-go: refuses interpreted_as other", lambda: ok(rec("b4.json", interpreted_as="run-hold")), "interpreted_as")
        c.refused("check-run-go: refuses interpreted_as other even when the verbatim reply says run-go",
                  lambda: ok(rec("b5.json", interpreted_as="run-hold", owner_reply_verbatim="run-go")), "interpreted_as")
        c.refused("check-run-go: refuses a wrong schema", lambda: ok(rec("b6.json", schema="laya-run-go-v0")), "schema")
        c.refused("check-run-go: refuses a missing file", lambda: ok(os.path.join(d, "none.json")), "run-go")
        c.refused("check-run-go: refuses a missing blob", lambda: check_run_go(rec("b7.json"), os.path.join(d, "no-blob.json"), commit), "run-go")
        c.refused("check-run-go: refuses an empty commit", lambda: ok(rec("b8.json"), ""), "declaration_commit")
    finally:
        shutil.rmtree(d, ignore_errors=True)


SELFTEST_SECTIONS = [selftest_inputs, selftest_rule, selftest_analyze, selftest_decision, selftest_gap_tools, selftest_run_go]


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
    "analyze": (cmd_analyze, {"--inputs": True, "--out": True}),
    "check-diagnosis": (cmd_check_diagnosis, {"--diagnosis": True, "--inputs": True}),
    "check-brief": (cmd_check_brief, {"--brief": True, "--diagnosis": True}),
    "check-reply": (cmd_check_reply, {"--reply": True, "--brief": True, "--diagnosis": True}),
    "write-disposition": (cmd_write_disposition, {"--reply": True, "--diagnosis": True, "--out": True}),
    "check-disposition": (cmd_check_disposition, {"--disposition": True, "--reply": True, "--diagnosis": True}),
    "scan-identifiers": (cmd_scan_identifiers, None),
    "classify-run": (cmd_classify_run, {"--run-dir": True}),
    "check-run-go": (cmd_check_run_go, {"--run-go": True, "--blob": True, "--commit": True}),
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
        if spec is None:  # positional arguments (scan-identifiers FILE...)
            return fn(argv[1:])
        return fn(parse_opts(argv[1:], spec))
    except Refused as ex:
        print("REFUSED %s: %s" % (ex.rule, ex.reason))
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
