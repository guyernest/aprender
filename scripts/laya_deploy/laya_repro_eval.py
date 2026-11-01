#!/usr/bin/env python3
"""The ONE pure evaluator of the D-16 reproduction claim (plan 14-11; used by 14-14 and 14-15).

    python3 -I scripts/laya_deploy/laya_repro_eval.py check-declaration DECL
    python3 -I scripts/laya_deploy/laya_repro_eval.py evaluate --declaration DECL --run-dir DIR --attempt N
    python3 -I scripts/laya_deploy/laya_repro_eval.py --selftest

PURE: no network, no subprocess, no clock, no environment. The verdict is a function of the committed declaration and
the files of ONE fetched run (the layout `laya_fetch.py` writes):

    DIR/run/gate-report.json     margin and pass (top level), fine_tuned.ece_post, device_used, device_is_cpu,
                                 recipe_id, inputs_sha256, seeds.per_seed[].seed, seeds.shipped, pilot_waiver
    DIR/verdict.json             phase, verdict.waived (the finalizer's copy of the status payload)
    DIR/logs/pack-outcome.json   accepted, deploy_eligible, waived

`check-declaration DECL` validates schema laya-reproduction-declaration-v1 and prints `DECLARATION OK`, or one
`REFUSED declaration: <reason>` line and exit 2. The MUST clause keys are the fixed tuple MUST_KEYS, never the
declaration's own keys, so an empty or partial `must` can never pass vacuously; the band must hold both intervals,
each [lo, hi] with lo <= hi; retry.attempt_cap >= 1; retry.retry_classes may only name in-band-gate-fail and
waived (a defect class is never retriable); waiver_rule is inconclusive or counts.

`evaluate` runs check-declaration first, then prints exactly ONE JSON line:

    {attempt, metrics {margin, ece_post, gate_pass}, must {deploy_eligible, device_prefix, recipe_id,
     inputs_sha256, seeds}, band {margin, ece_post}, waived, class, defect_reasons, retry_allowed,
     verdict_if_final, recorded {device_used, recipe_id, inputs_sha256 (every key the report carries), seeds,
     shipped_seed}}

Every band clause is recomputed from the report's own numbers (fine_tuned.ece_post and top-level margin); no band
value is ever taken from anything else. The inputs clause compares the declaration's inputs_sha256 keys (the three
required task_json, train_jsonl, eval_jsonl, plus any more it declares) against the report's, and records every key
the report carries (base_model, tokenizer_json, optionally shift_jsonl).

class: defect (device not the declared prefix, recipe or inputs differ, seeds not exactly the declared set, a metric
outside its band, a pack refused while the gate passed or a waiver was claimed, a pack accepted with neither a gate
pass nor a waiver, files that disagree, a required file missing; every reason listed) | pass | waived (gate failed,
pack accepted under a waiver) | in-band-gate-fail (gate failed, nothing wrong with the run itself).
retry_allowed = class in retry_classes AND attempt < attempt_cap, for any cap.
verdict_if_final: pass -> reproduced; waived -> reproduced only under waiver_rule counts, else inconclusive;
in-band-gate-fail -> inconclusive; defect -> not-reproduced.

judging (optional, plan 15-11): a top-level `judging {margin_statistic, note}` declares which statistic the margin clause
judges. margin_statistic is shipped_seed (the default, and the behaviour of a declaration with no `judging` key: the
report's top-level margin, the shipped seed's) or seed_mean (the mean of the declared seeds' per-seed margins,
seeds.per_seed[].margin). Any other key or statistic is refused by check-declaration, so a declared statistic can
never be silently ignored. Under seed_mean ONLY, metrics gains margin_statistic ("seed_mean") and shipped_seed_margin
(the report's top-level margin); the ece clause, the gate, the pack and every other clause are untouched. If the
report's per-seed rows are not exactly the declared seeds with a finite margin each, the margin stays null, its band
clause is false and a `report-malformed: per-seed margins` reason is listed.

stdlib only.
"""

import json
import math
import os
import re
import sys
import shutil
import tempfile

SCHEMA = "laya-reproduction-declaration-v1"
MUST_KEYS = ("deploy_eligible", "device_prefix", "recipe_id", "inputs_sha256", "seeds")
REQUIRED_INPUTS = ("task_json", "train_jsonl", "eval_jsonl")
BAND_KEYS = ("margin", "ece_post")
RETRIABLE = ("in-band-gate-fail", "waived")
JUDGING_STATS = ("shipped_seed", "seed_mean")
JUDGING_KEYS = ("margin_statistic", "note")
WAIVER_RULES = ("inconclusive", "counts")
CLASSES = ("pass", "waived", "in-band-gate-fail", "defect")
SHA_RE = re.compile(r"[0-9a-f]{64}")
PREFIX_RE = re.compile(r"[a-z][a-z0-9_:-]{0,15}")
MAX_FILE = 8 * 1024 * 1024


class DeclarationRefused(Exception):
    pass


class UsageRefused(Exception):
    pass


# ---- reading JSON ------------------------------------------------------------------------------------------

def _pairs(pairs):
    d = {}
    for k, v in pairs:
        if k in d:
            raise ValueError("duplicate key %r" % k)
        d[k] = v
    return d


def _const(name):
    raise ValueError("non-finite number %s" % name)


def parse_json(raw):
    return json.loads(raw, object_pairs_hook=_pairs, parse_constant=_const)


def read_json_file(path):
    """(dict, None) or (None, reason): a file that is not a JSON object is a reason, never a silent None.
    Never follows a symlink, never reads more than MAX_FILE bytes."""
    if os.path.islink(path) or not os.path.isfile(path):
        return None, "missing-file: %s" % path
    try:
        if os.path.getsize(path) > MAX_FILE:
            return None, "unreadable-file: %s is larger than %d bytes" % (path, MAX_FILE)
        with open(path, "rb") as f:
            obj = parse_json(f.read().decode("utf-8"))
    except (OSError, ValueError, UnicodeDecodeError) as ex:
        return None, "unreadable-file: %s (%s)" % (path, str(ex)[:60])
    if not isinstance(obj, dict):
        return None, "unreadable-file: %s is not a JSON object" % path
    return obj, None


def is_num(x):
    return isinstance(x, (int, float)) and not isinstance(x, bool) and math.isfinite(x)


def is_int(x):
    return isinstance(x, int) and not isinstance(x, bool)


# ---- the declaration ---------------------------------------------------------------------------------------

def check_declaration_obj(d):
    """Raise DeclarationRefused with one reason, or return d."""
    def bad(msg):
        raise DeclarationRefused(msg)

    if not isinstance(d, dict):
        bad("the declaration is not a JSON object")
    if d.get("schema") != SCHEMA:
        bad("schema is %r, expected %r" % (d.get("schema"), SCHEMA))
    must = d.get("must")
    if not isinstance(must, dict) or not must:
        bad("must is missing or empty: the MUST clauses may never pass vacuously")
    for k in MUST_KEYS:
        if k not in must:
            bad("must lacks the mandatory clause %r" % k)
    for k in must:
        if k not in MUST_KEYS:
            bad("must carries the unknown clause %r (it would never be evaluated)" % k)
    if must["deploy_eligible"] is not True:
        bad("must.deploy_eligible must be exactly true")
    if not isinstance(must["device_prefix"], str) or not PREFIX_RE.fullmatch(must["device_prefix"]):
        bad("must.device_prefix must be a short lowercase device prefix such as 'cuda'")
    if not isinstance(must["recipe_id"], str) or not SHA_RE.fullmatch(must["recipe_id"]):
        bad("must.recipe_id must be a 64-character lowercase hex id")
    ins = must["inputs_sha256"]
    if not isinstance(ins, dict) or not ins:
        bad("must.inputs_sha256 is missing or empty")
    for k in REQUIRED_INPUTS:
        if k not in ins:
            bad("must.inputs_sha256 lacks the required key %r" % k)
    for k, v in ins.items():
        if not isinstance(v, str) or not SHA_RE.fullmatch(v):
            bad("must.inputs_sha256[%r] is not a 64-character lowercase hex digest" % k)
    seeds = must["seeds"]
    if not isinstance(seeds, list) or not seeds or not all(is_int(s) for s in seeds) or len(set(seeds)) != len(seeds):
        bad("must.seeds must be a non-empty list of distinct integers")
    band = d.get("band")
    if not isinstance(band, dict) or not band:
        bad("band is missing or empty")
    for k in BAND_KEYS:
        if k not in band:
            bad("band lacks the interval %r" % k)
    for k in band:
        if k not in BAND_KEYS:
            bad("band carries the unknown interval %r" % k)
    for k in BAND_KEYS:
        iv = band[k]
        if not isinstance(iv, list) or len(iv) != 2 or not all(is_num(x) for x in iv):
            bad("band.%s must be [lo, hi], two finite numbers" % k)
        if iv[0] > iv[1]:
            bad("band.%s has lo %r above hi %r" % (k, iv[0], iv[1]))
    retry = d.get("retry")
    if not isinstance(retry, dict):
        bad("retry is missing")
    cap = retry.get("attempt_cap")
    if not is_int(cap) or cap < 1:
        bad("retry.attempt_cap must be an integer of at least 1")
    classes = retry.get("retry_classes")
    if not isinstance(classes, list):
        bad("retry.retry_classes must be a list")
    for c in classes:
        if c not in RETRIABLE:
            bad("retry.retry_classes names %r; a defect class is never retriable (only %s may be listed)"
                % (c, " and ".join(RETRIABLE)))
    if d.get("waiver_rule") not in WAIVER_RULES:
        bad("waiver_rule %r is not one of %s" % (d.get("waiver_rule"), ", ".join(WAIVER_RULES)))
    if "judging" in d:
        j = d["judging"]
        if not isinstance(j, dict):
            bad("judging must be an object")
        for k in j:
            if k not in JUDGING_KEYS:
                bad("judging carries the unknown key %r (it would never be evaluated)" % k)
        if j.get("margin_statistic") not in JUDGING_STATS:
            bad("judging.margin_statistic %r is not one of %s" % (j.get("margin_statistic"), ", ".join(JUDGING_STATS)))
        if "note" in j and not isinstance(j["note"], str):
            bad("judging.note must be a string")
    return d


def load_declaration(path):
    obj, why = read_json_file(path)
    if why:
        raise DeclarationRefused(why)
    return check_declaration_obj(obj)


# ---- the evaluation ----------------------------------------------------------------------------------------

def in_band(value, interval):
    return is_num(value) and interval[0] <= value <= interval[1]


def evaluate_run(decl, run_dir, attempt):
    """Pure: the evaluation dict for one fetched run directory."""
    must = decl["must"]
    band = decl["band"]
    retry = decl["retry"]
    reasons = []
    report, why = read_json_file(os.path.join(run_dir, "run", "gate-report.json"))
    if why:
        reasons.append(why)
    verdict, why = read_json_file(os.path.join(run_dir, "verdict.json"))
    if why:
        reasons.append(why)
    pack, why = read_json_file(os.path.join(run_dir, "logs", "pack-outcome.json"))
    if why:
        reasons.append(why)

    clause = {k: False for k in MUST_KEYS}
    bclause = {k: False for k in BAND_KEYS}
    metrics = {"margin": None, "ece_post": None, "gate_pass": None}
    judging = decl.get("judging")
    stat = judging["margin_statistic"] if isinstance(judging, dict) else "shipped_seed"
    seed_mean = stat == "seed_mean"
    recorded = {"device_used": None, "recipe_id": None, "inputs_sha256": None, "seeds": None, "shipped_seed": None}
    waived = None

    if report is not None:
        ft = report.get("fine_tuned")
        margin = report.get("margin")
        ece = ft.get("ece_post") if isinstance(ft, dict) else None
        gate_pass = report.get("pass")
        metrics["margin"] = margin if is_num(margin) else None
        if seed_mean:
            metrics["margin_statistic"] = "seed_mean"
            metrics["shipped_seed_margin"] = metrics["margin"]
            metrics["margin"] = None
            sdm = report.get("seeds")
            rows = sdm.get("per_seed") if isinstance(sdm, dict) else None
            ms = None
            if (isinstance(rows, list) and all(isinstance(r, dict) and is_int(r.get("seed")) for r in rows)
                    and sorted(r["seed"] for r in rows) == sorted(must["seeds"])
                    and all(is_num(r.get("margin")) for r in rows)):
                ms = [r["margin"] for r in rows]
            if ms is None:
                reasons.append("report-malformed: per-seed margins are not exactly the declared seeds with a finite margin each")
            else:
                metrics["margin"] = math.fsum(ms) / len(ms)
        metrics["ece_post"] = ece if is_num(ece) else None
        if isinstance(gate_pass, bool):
            metrics["gate_pass"] = gate_pass
        else:
            reasons.append("report-malformed: pass is not a boolean")
        for k, v in (("margin", metrics["margin"]), ("ece_post", metrics["ece_post"])):
            bclause[k] = in_band(v, band[k])
            if not bclause[k]:
                reasons.append("out-of-band: %s %s is outside [%s, %s]" % (k, v, band[k][0], band[k][1]))
        dev = report.get("device_used")
        recorded["device_used"] = dev if isinstance(dev, str) else None
        clause["device_prefix"] = (isinstance(dev, str) and dev.startswith(must["device_prefix"])
                                   and report.get("device_is_cpu") is not True)
        if not clause["device_prefix"]:
            reasons.append("device: device_used %r is not the declared %r (device_is_cpu %r)"
                           % (dev, must["device_prefix"], report.get("device_is_cpu")))
        rid = report.get("recipe_id")
        recorded["recipe_id"] = rid if isinstance(rid, str) else None
        clause["recipe_id"] = rid == must["recipe_id"]
        if not clause["recipe_id"]:
            reasons.append("recipe-mismatch: recipe_id %r is not the declared one" % rid)
        rin = report.get("inputs_sha256")
        if isinstance(rin, dict):
            recorded["inputs_sha256"] = {k: rin[k] for k in sorted(rin)}
        diff = [k for k in must["inputs_sha256"] if not isinstance(rin, dict) or rin.get(k) != must["inputs_sha256"][k]]
        clause["inputs_sha256"] = not diff
        if diff:
            reasons.append("inputs-mismatch: %s differ from the declaration" % ", ".join(diff))
        sd = report.get("seeds")
        per = sd.get("per_seed") if isinstance(sd, dict) else None
        ran = None
        if isinstance(per, list) and per and all(isinstance(r, dict) and is_int(r.get("seed")) for r in per):
            ran = sorted(r["seed"] for r in per)
        recorded["seeds"] = ran
        shipped = sd.get("shipped") if isinstance(sd, dict) else None
        recorded["shipped_seed"] = shipped if is_int(shipped) else None
        clause["seeds"] = (ran == sorted(must["seeds"]) and len(set(ran)) == len(ran)
                           and (shipped is None or shipped in must["seeds"] or shipped in ran)
                           and (not isinstance(sd.get("n"), int) or sd["n"] == len(must["seeds"])))
        if not clause["seeds"]:
            reasons.append("seeds: ran %s, declared %s" % (ran, sorted(must["seeds"])))

    accepted = deploy_eligible = None
    claim = None
    if pack is not None:
        accepted = pack.get("accepted") is True
        deploy_eligible = pack.get("deploy_eligible") is True
        w = pack.get("waived")
        waived = w if isinstance(w, str) and w else None
        clause["deploy_eligible"] = accepted and deploy_eligible
    if report is not None:
        pw = report.get("pilot_waiver")
        claim = pw.get("id") if isinstance(pw, dict) and isinstance(pw.get("id"), str) and pw.get("id") else None
    gate_pass = metrics["gate_pass"]
    if verdict is not None:
        phase = verdict.get("phase")
        vv = verdict.get("verdict")
        vwaived = vv.get("waived") if isinstance(vv, dict) else verdict.get("waived")
        if phase not in ("completed", "gate_failed"):
            reasons.append("phase: verdict.json phase is %r, expected completed or gate_failed" % (phase,))
        elif accepted is not None and (phase == "completed") != accepted:
            reasons.append("phase-disagreement: verdict.json phase %s but the pack accepted is %s" % (phase, accepted))
        if pack is not None and (vwaived or None) != waived:
            reasons.append("waiver-disagreement: verdict.json waived %r, pack-outcome waived %r" % (vwaived, waived))
    if pack is not None and gate_pass is not None:
        if gate_pass and not (accepted and deploy_eligible):
            reasons.append("pack-refused: the gate passed but the pack was not accepted and deploy-eligible")
        if gate_pass and waived is not None:
            reasons.append("waiver-on-passing-gate: a waiver %r on a run whose gate passed" % waived)
        if not gate_pass and (waived is not None or claim is not None) and not (accepted and deploy_eligible and waived is not None):
            reasons.append("pack-refused-under-waiver: a waiver was claimed but no pack was accepted under it")
        if not gate_pass and accepted and waived is None:
            reasons.append("pack-accepted-without-waiver: the gate failed and no waiver covers the accepted pack")
        if not gate_pass and waived is not None and accepted and claim is not None and claim != waived:
            reasons.append("waiver-disagreement: the report claims %r, the pack accepted %r" % (claim, waived))

    if reasons:
        klass = "defect"
    elif gate_pass:
        klass = "pass"
    elif waived is not None:
        klass = "waived"
    else:
        klass = "in-band-gate-fail"
    retry_allowed = bool(klass in retry["retry_classes"] and attempt < retry["attempt_cap"])
    if klass == "pass":
        final = "reproduced"
    elif klass == "waived":
        final = "reproduced" if decl["waiver_rule"] == "counts" else "inconclusive"
    elif klass == "in-band-gate-fail":
        final = "inconclusive"
    else:
        final = "not-reproduced"
    return {
        "attempt": attempt,
        "metrics": metrics,
        "must": clause,
        "band": bclause,
        "waived": waived,
        "class": klass,
        "defect_reasons": reasons,
        "retry_allowed": retry_allowed,
        "verdict_if_final": final,
        "recorded": recorded,
    }


# ---- CLI ---------------------------------------------------------------------------------------------------

def cmd_check(argv, out, err):
    if len(argv) != 1:
        raise UsageRefused("usage: laya_repro_eval.py check-declaration DECL")
    try:
        load_declaration(argv[0])
    except DeclarationRefused as ex:
        err("REFUSED declaration: %s" % ex)
        return 2
    out("DECLARATION OK")
    return 0


def cmd_evaluate(argv, out, err):
    opts = {}
    i = 0
    while i < len(argv):
        if argv[i] in ("--declaration", "--run-dir", "--attempt") and i + 1 < len(argv) and argv[i] not in opts:
            opts[argv[i]] = argv[i + 1]
            i += 2
        else:
            raise UsageRefused("usage: laya_repro_eval.py evaluate --declaration DECL --run-dir DIR --attempt N")
    if set(opts) != {"--declaration", "--run-dir", "--attempt"}:
        raise UsageRefused("usage: laya_repro_eval.py evaluate --declaration DECL --run-dir DIR --attempt N")
    if not re.fullmatch(r"[0-9]{1,6}", opts["--attempt"]) or int(opts["--attempt"]) < 1:
        raise UsageRefused("--attempt must be an integer of at least 1")
    try:
        decl = load_declaration(opts["--declaration"])
    except DeclarationRefused as ex:
        err("REFUSED declaration: %s" % ex)
        return 2
    out(json.dumps(evaluate_run(decl, opts["--run-dir"], int(opts["--attempt"])), sort_keys=True, separators=(",", ":")))
    return 0


def main(argv, out=None, err=None):
    out = out or (lambda s: print(s, flush=True))
    err = err or (lambda s: print(s, file=sys.stderr, flush=True))
    try:
        if not argv:
            raise UsageRefused("usage: laya_repro_eval.py check-declaration|evaluate|--selftest")
        if argv[0] == "check-declaration":
            return cmd_check(argv[1:], out, err)
        if argv[0] == "evaluate":
            return cmd_evaluate(argv[1:], out, err)
        raise UsageRefused("unknown command %r" % argv[0])
    except UsageRefused as ex:
        err("REFUSED usage: %s" % ex)
        return 2


# ---- selftest ----------------------------------------------------------------------------------------------

def selftest():
    failures = []

    def case(label, ok, detail=""):
        print("  %-4s %-86s %s" % ("ok" if ok else "FAIL", label, detail))
        if not ok:
            failures.append(label)

    here = os.path.dirname(os.path.abspath(__file__))
    fx = os.path.join(here, "..", "..", "crates", "aprender-mcp-laya-train", "fixtures")
    with open(os.path.join(fx, "gate-report-v1.json"), "rb") as f:
        BASE_REPORT = parse_json(f.read().decode())
    with open(os.path.join(fx, "pack-outcome-v1.json"), "rb") as f:
        BASE_PACK = parse_json(f.read().decode())
    SEEDS = [13, 17, 23]
    work = tempfile.mkdtemp(prefix="laya-repro-eval-selftest-")
    counter = {"n": 0}

    def clone(o):
        return json.loads(json.dumps(o))

    def make_report():
        r = clone(BASE_REPORT)
        r["device_used"] = "cuda:0"
        r["device_is_cpu"] = False
        for row, s in zip(r["seeds"]["per_seed"], SEEDS):
            row["seed"] = s
        r["seeds"]["declared"] = 13
        r["seeds"]["shipped"] = 17
        return r

    def make_decl():
        r = make_report()
        return {
            "schema": SCHEMA,
            "plan": "selftest",
            "must": {
                "deploy_eligible": True,
                "device_prefix": "cuda",
                "recipe_id": r["recipe_id"],
                "inputs_sha256": {k: r["inputs_sha256"][k] for k in REQUIRED_INPUTS},
                "seeds": list(SEEDS),
            },
            "band": {"margin": [0.10, 0.30], "ece_post": [0.03, 0.09]},
            "retry": {"attempt_cap": 2, "retry_classes": ["in-band-gate-fail", "waived"], "rule": "selftest"},
            "waiver_rule": "inconclusive",
        }

    def write(path, obj):
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(obj if isinstance(obj, str) else json.dumps(obj))

    def make_run(report=None, pack=None, verdict=None, kind="pass", drop=()):
        """Build a fetched run dir; kind selects the consistent defaults."""
        counter["n"] += 1
        d = os.path.join(work, "run%d" % counter["n"])
        rep = report if report is not None else make_report()
        pk = pack if pack is not None else clone(BASE_PACK)
        vd = verdict
        if vd is None:
            vd = {"phase": "completed" if pk.get("accepted") else "gate_failed",
                  "verdict": {"pass": rep.get("pass"), "waived": pk.get("waived"), "ece_post": 0.05, "margin": 0.2}}
        files = {"run/gate-report.json": rep, "logs/pack-outcome.json": pk, "verdict.json": vd}
        for rel, obj in files.items():
            if rel not in drop:
                write(os.path.join(d, *rel.split("/")), obj)
        return d

    def skipped_pack():
        p = clone(BASE_PACK)
        p.update({"ran": False, "skipped": "gate_fail", "exit_code": None, "accepted": False, "apr_sha256": None,
                  "shipped_seed": None, "waived": None, "packed_line": None, "verify_line": None,
                  "deploy_eligible": False})
        return p

    def in_band_fail_report():
        r = make_report()
        r["pass"] = False
        r["fine_tuned"]["ece_post"] = 0.085
        r["margin"] = 0.12
        return r

    def waived_run():
        r = in_band_fail_report()
        r["pilot_waiver"] = {"id": "W-selftest", "waives": ["ece"]}
        p = clone(BASE_PACK)
        p["waived"] = "W-selftest"
        return make_run(report=r, pack=p)

    decl = make_decl()
    dpath = os.path.join(work, "decl.json")
    write(dpath, decl)

    def decl_with(edit):
        d = clone(decl)
        edit(d)
        return d

    def refused(d):
        try:
            check_declaration_obj(d)
            return None
        except DeclarationRefused as ex:
            return str(ex)

    def ev(run, attempt=1, d=None):
        return evaluate_run(d or decl, run, attempt)

    def cli(argv):
        o, e = [], []
        rc = main(argv, out=o.append, err=e.append)
        return rc, o, e

    try:
        # --- the declaration ------------------------------------------------------------------------------
        case("decl-ok: the selftest declaration is accepted", refused(decl) is None, str(refused(decl)))
        rc, o, e = cli(["check-declaration", dpath])
        case("decl-ok: check-declaration prints DECLARATION OK, exit 0", rc == 0 and o == ["DECLARATION OK"] and e == [])
        r = refused(decl_with(lambda d: d.__setitem__("must", {})))
        case("decl-empty-must: an empty must is refused", bool(r) and "vacuous" in r, str(r))
        r = refused(decl_with(lambda d: d.pop("must")))
        case("decl-empty-must: an absent must is refused", bool(r))
        r = refused(decl_with(lambda d: d.__setitem__("band", {})))
        case("decl-empty-band: an empty band is refused", bool(r) and "band" in r, str(r))
        r = refused(decl_with(lambda d: d.pop("band")))
        case("decl-empty-band: an absent band is refused", bool(r))
        for k in MUST_KEYS:
            r = refused(decl_with(lambda d, k=k: d["must"].pop(k)))
            case("decl-missing-key: must without %s is refused" % k, bool(r) and k in r, str(r))
        for k in REQUIRED_INPUTS:
            r = refused(decl_with(lambda d, k=k: d["must"]["inputs_sha256"].pop(k)))
            case("decl-missing-key: inputs_sha256 without %s is refused" % k, bool(r) and k in r, str(r))
        for k in BAND_KEYS:
            r = refused(decl_with(lambda d, k=k: d["band"].pop(k)))
            case("decl-missing-key: band without %s is refused" % k, bool(r) and k in r, str(r))
        r = refused(decl_with(lambda d: d["must"].__setitem__("extra_clause", True)))
        case("decl-missing-key: an unknown must clause (never evaluated) is refused", bool(r) and "unknown" in r, str(r))
        r = refused(decl_with(lambda d: d["must"].__setitem__("deploy_eligible", False)))
        case("decl-missing-key: must.deploy_eligible false is refused", bool(r))
        r = refused(decl_with(lambda d: d["must"].__setitem__("seeds", [])))
        case("decl-missing-key: empty seeds is refused", bool(r))
        for label, iv in (("lo>hi", [0.5, 0.1]), ("one-bound", [0.1]), ("non-number", [0.1, "x"]), ("three", [0.1, 0.2, 0.3]),
                          ("bool", [True, 2]), ("nan-as-string", ["nan", 1])):
            r = refused(decl_with(lambda d, iv=iv: d["band"].__setitem__("margin", iv)))
            case("decl-bad-interval: %s is refused" % label, bool(r) and "margin" in r, str(r))
        r = refused(decl_with(lambda d: d["band"].__setitem__("ece_post", [0.1, 0.1])))
        case("decl-bad-interval: lo == hi is a legal point interval", r is None, str(r))
        try:
            parse_json('{"a": NaN}')
            nan_refused = False
        except ValueError:
            nan_refused = True
        case("decl-bad-interval: a NaN literal in the JSON is refused at parse", nan_refused)
        for c in ("defect", "out-of-band", "pass", "bogus"):
            r = refused(decl_with(lambda d, c=c: d["retry"].__setitem__("retry_classes", ["in-band-gate-fail", c])))
            case("decl-defect-retry-class: retry_classes naming %r is refused" % c, bool(r) and "never retriable" in r, str(r))
        r = refused(decl_with(lambda d: d["retry"].__setitem__("retry_classes", [])))
        case("decl-defect-retry-class: an empty retry_classes (no retries) is legal", r is None, str(r))
        for cap in (0, -1, 1.5, "2", True):
            r = refused(decl_with(lambda d, cap=cap: d["retry"].__setitem__("attempt_cap", cap)))
            case("decl-attempt-cap: attempt_cap %r is refused" % (cap,), bool(r) and "attempt_cap" in r, str(r))
        r = refused(decl_with(lambda d: d["retry"].__setitem__("attempt_cap", 1)))
        case("decl-attempt-cap: attempt_cap 1 is legal", r is None, str(r))
        for w in ("maybe", None, "Counts"):
            r = refused(decl_with(lambda d, w=w: d.__setitem__("waiver_rule", w)))
            case("decl-waiver-rule: waiver_rule %r is refused" % (w,), bool(r) and "waiver_rule" in r, str(r))
        r = refused(decl_with(lambda d: d.__setitem__("schema", "laya-reproduction-declaration-v0")))
        case("decl-schema: another schema is refused", bool(r))
        bad = os.path.join(work, "bad-decl.json")
        write(bad, decl_with(lambda d: d.__setitem__("must", {})))
        rc, o, e = cli(["check-declaration", bad])
        case("decl-empty-must: the CLI exits 2 with one REFUSED declaration: line and no OK",
             rc == 2 and o == [] and len(e) == 1 and e[0].startswith("REFUSED declaration: "))
        rc, o, e = cli(["check-declaration", os.path.join(work, "nope.json")])
        case("decl-missing-file: a missing declaration file is refused", rc == 2 and e and e[0].startswith("REFUSED declaration: "))

        # --- eval-pass ------------------------------------------------------------------------------------
        r = ev(make_run())
        case("eval-pass: class pass, reproduced", r["class"] == "pass" and r["verdict_if_final"] == "reproduced" and r["defect_reasons"] == [],
             str(r["defect_reasons"]))
        case("eval-pass: every must clause and every band clause is true, and the key sets are exactly the mandatory ones",
             set(r["must"]) == set(MUST_KEYS) and all(v is True for v in r["must"].values())
             and set(r["band"]) == set(BAND_KEYS) and all(v is True for v in r["band"].values()))
        case("eval-pass: metrics come from the report (margin, fine_tuned.ece_post, pass); retry not allowed on a pass",
             r["metrics"] == {"margin": 0.17, "ece_post": 0.052, "gate_pass": True} and r["retry_allowed"] is False
             and r["waived"] is None and r["attempt"] == 1)
        run = make_run()
        rc, o, e = cli(["evaluate", "--declaration", dpath, "--run-dir", run, "--attempt", "1"])
        case("eval-pass: the CLI prints exactly one JSON line equal to the library result",
             rc == 0 and e == [] and len(o) == 1 and json.loads(o[0]) == ev(run), "")
        r1 = cli(["evaluate", "--declaration", dpath, "--run-dir", run, "--attempt", "1"])[1]
        r2 = cli(["evaluate", "--declaration", dpath, "--run-dir", run, "--attempt", "1"])[1]
        case("eval-pass: the output is deterministic (byte-identical across runs)", r1 == r2)
        rc, o, e = cli(["evaluate", "--declaration", bad, "--run-dir", run, "--attempt", "1"])
        case("eval-pass: evaluate runs check-declaration first (a bad declaration prints no evaluation)",
             rc == 2 and o == [] and e and e[0].startswith("REFUSED declaration: "))
        rc, o, e = cli(["evaluate", "--declaration", dpath, "--run-dir", run, "--attempt", "0"])
        case("eval-pass: --attempt 0 is refused", rc == 2 and o == [])

        # --- eval-inputs-subset ---------------------------------------------------------------------------
        rep = make_report()
        case("eval-inputs-subset: the report carries five inputs keys", len(rep["inputs_sha256"]) == 5)
        r = ev(make_run(report=rep))
        case("eval-inputs-subset: five report keys, the three required equal -> must.inputs_sha256 true, all recorded",
             r["must"]["inputs_sha256"] is True and r["class"] == "pass"
             and set(r["recorded"]["inputs_sha256"]) == set(rep["inputs_sha256"]))
        rep = make_report()
        rep["inputs_sha256"]["base_model"] = "f" * 64
        r = ev(make_run(report=rep))
        case("eval-inputs-subset: a different NON-required key (base_model) is recorded, not compared",
             r["must"]["inputs_sha256"] is True and r["recorded"]["inputs_sha256"]["base_model"] == "f" * 64 and r["class"] == "pass")
        for k in REQUIRED_INPUTS:
            rep = make_report()
            rep["inputs_sha256"][k] = "0" * 64
            r = ev(make_run(report=rep))
            case("eval-inputs-subset: required key %s different -> false and class defect" % k,
                 r["must"]["inputs_sha256"] is False and r["class"] == "defect" and r["verdict_if_final"] == "not-reproduced"
                 and any("inputs-mismatch" in x and k in x for x in r["defect_reasons"]))
        rep = make_report()
        del rep["inputs_sha256"]["eval_jsonl"]
        r = ev(make_run(report=rep))
        case("eval-inputs-subset: a required key absent from the report is a defect", r["must"]["inputs_sha256"] is False and r["class"] == "defect")
        d2 = decl_with(lambda d: d["must"]["inputs_sha256"].__setitem__("base_model", "e" * 64))
        r = ev(make_run(), d=d2)
        case("eval-inputs-subset: a base_model the declaration DOES declare is compared", r["must"]["inputs_sha256"] is False and r["class"] == "defect")

        # --- eval-in-band-fail ----------------------------------------------------------------------------
        run = make_run(report=in_band_fail_report(), pack=skipped_pack())
        r = ev(run, 1)
        case("eval-in-band-fail: gate fail, no waiver, pack skipped, metrics in band -> in-band-gate-fail, inconclusive",
             r["class"] == "in-band-gate-fail" and r["verdict_if_final"] == "inconclusive" and r["defect_reasons"] == []
             and r["metrics"]["gate_pass"] is False and all(v is True for v in r["band"].values()), str(r["defect_reasons"]))
        case("eval-in-band-fail: deploy_eligible is false (nothing was packed) while the class is not a defect",
             r["must"]["deploy_eligible"] is False and r["must"]["seeds"] is True and r["must"]["recipe_id"] is True)
        case("eval-in-band-fail: retry allowed at attempt 1 of cap 2", ev(run, 1)["retry_allowed"] is True)
        case("eval-in-band-fail: retry NOT allowed at attempt 2 of cap 2", ev(run, 2)["retry_allowed"] is False)
        d3 = decl_with(lambda d: d["retry"].__setitem__("attempt_cap", 3))
        case("eval-in-band-fail: retry allowed at attempt 2 of cap 3 (caps above 2, same rule)", ev(run, 2, d3)["retry_allowed"] is True)
        case("eval-in-band-fail: retry NOT allowed at attempt 3 of cap 3", ev(run, 3, d3)["retry_allowed"] is False)
        d9 = decl_with(lambda d: d["retry"].__setitem__("attempt_cap", 9))
        case("eval-in-band-fail: cap 9 allows attempts 1..8 and not 9",
             [ev(run, n, d9)["retry_allowed"] for n in range(1, 10)] == [True] * 8 + [False])
        d1 = decl_with(lambda d: d["retry"].__setitem__("attempt_cap", 1))
        case("eval-in-band-fail: cap 1 never allows a retry", ev(run, 1, d1)["retry_allowed"] is False)
        d0 = decl_with(lambda d: d["retry"].__setitem__("retry_classes", ["waived"]))
        case("eval-in-band-fail: a class missing from retry_classes is not retried", ev(run, 1, d0)["retry_allowed"] is False)

        # --- eval-out-of-band -----------------------------------------------------------------------------
        rep = in_band_fail_report()
        rep["fine_tuned"]["ece_post"] = 0.2
        run = make_run(report=rep, pack=skipped_pack())
        d3 = decl_with(lambda d: d["retry"].__setitem__("attempt_cap", 3))
        r = ev(run, 1, d3)
        case("eval-out-of-band: ece_post above the band -> defect, band.ece_post false, margin still true",
             r["class"] == "defect" and r["band"] == {"margin": True, "ece_post": False}
             and any("out-of-band" in x and "ece_post" in x for x in r["defect_reasons"]) and r["verdict_if_final"] == "not-reproduced")
        case("eval-out-of-band: retry not allowed at any attempt (1, 2 of cap 3)",
             all(ev(run, n, d3)["retry_allowed"] is False for n in (1, 2, 3)))
        rep = make_report()
        rep["margin"] = 0.05
        r = ev(make_run(report=rep))
        case("eval-out-of-band: a passing gate with margin below the band is still a defect (the band is the owner's, not the gate's)",
             r["class"] == "defect" and r["band"]["margin"] is False and r["band"]["ece_post"] is True)
        rep = make_report()
        rep["margin"] = 0.30
        rep["fine_tuned"]["ece_post"] = 0.03
        r = ev(make_run(report=rep))
        case("eval-out-of-band: both interval ends are inclusive", r["class"] == "pass" and all(v is True for v in r["band"].values()))
        rep = make_report()
        del rep["fine_tuned"]["ece_post"]
        r = ev(make_run(report=rep))
        case("eval-out-of-band: an absent metric is outside the band, never defaulted", r["class"] == "defect" and r["band"]["ece_post"] is False
             and r["metrics"]["ece_post"] is None)

        # --- eval-altered-metric --------------------------------------------------------------------------
        base = make_run()
        rep = make_report()
        rep["fine_tuned"]["ece_post"] = 0.2
        altered = make_run(report=rep)
        a, b = ev(base), ev(altered)
        case("eval-altered-metric: editing fine_tuned.ece_post in the fixture report flips the band clause",
             a["band"]["ece_post"] is True and b["band"]["ece_post"] is False and a["class"] == "pass" and b["class"] == "defect")
        vd = {"phase": "completed", "verdict": {"pass": True, "waived": None, "ece_post": 0.05, "margin": 0.2}, "ece_post": 0.05, "margin": 0.2}
        c = ev(make_run(report=rep, verdict=vd))
        case("eval-altered-metric: a verdict.json claiming in-band numbers cannot override the report's own",
             c["band"]["ece_post"] is False and c["metrics"]["ece_post"] == 0.2 and c["class"] == "defect")
        rep = make_report()
        rep["thresholds"]["max_ece"] = 99
        rep["fine_tuned"]["ece_post"] = 0.2
        c = ev(make_run(report=rep))
        case("eval-altered-metric: the band is never read from the report's own thresholds", c["band"]["ece_post"] is False)

        # --- eval-waived ----------------------------------------------------------------------------------
        run = waived_run()
        r = ev(run)
        case("eval-waived-inconclusive: pack accepted under waiver W, metrics in band -> class waived, W recorded",
             r["class"] == "waived" and r["waived"] == "W-selftest" and r["defect_reasons"] == [], str(r["defect_reasons"]))
        case("eval-waived-inconclusive: under waiver_rule inconclusive the verdict is inconclusive", r["verdict_if_final"] == "inconclusive")
        case("eval-waived-inconclusive: every must clause is true (the pack was accepted and deploy-eligible)",
             all(v is True for v in r["must"].values()))
        dc = decl_with(lambda d: d.__setitem__("waiver_rule", "counts"))
        r = ev(run, 1, dc)
        case("eval-waived-counts: under waiver_rule counts the same run is reproduced", r["class"] == "waived" and r["verdict_if_final"] == "reproduced")
        case("eval-waived-counts: waived is retriable while attempts < cap", ev(run, 1, dc)["retry_allowed"] is True and ev(run, 2, dc)["retry_allowed"] is False)
        d0 = decl_with(lambda d: d["retry"].__setitem__("retry_classes", ["in-band-gate-fail"]))
        case("eval-waived-counts: waived is not retried when the declaration omits it", ev(run, 1, d0)["retry_allowed"] is False)
        r = ev(make_run(report=(lambda x: (x["fine_tuned"].__setitem__("ece_post", 0.2), x)[1])(
            (lambda y: (y.__setitem__("pilot_waiver", {"id": "W-selftest"}), y)[1])(in_band_fail_report())),
            pack=(lambda p: (p.__setitem__("waived", "W-selftest"), p)[1])(clone(BASE_PACK))))
        case("eval-waived-counts: a waived run with an out-of-band metric is a defect, not a waiver", r["class"] == "defect")

        # --- defect classes -------------------------------------------------------------------------------
        d3 = decl_with(lambda d: d["retry"].__setitem__("attempt_cap", 3))
        rep = make_report()
        rep["device_used"] = "cpu"
        rep["device_is_cpu"] = True
        r = ev(make_run(report=rep), 1, d3)
        case("eval-cpu: a CPU device is a defect with the named reason, must.device_prefix false",
             r["class"] == "defect" and r["must"]["device_prefix"] is False and any(x.startswith("device:") for x in r["defect_reasons"]))
        rep = make_report()
        rep["device_used"] = "cuda"
        rep["device_is_cpu"] = True
        r = ev(make_run(report=rep))
        case("eval-cpu: device_used cuda with device_is_cpu true is still a defect (the report contradicts itself)",
             r["must"]["device_prefix"] is False and r["class"] == "defect")
        rep = make_report()
        rep["device_used"] = "cuda"
        del rep["device_is_cpu"]
        r = ev(make_run(report=rep))
        case("eval-cpu: the plain prefix match works without device_is_cpu", r["must"]["device_prefix"] is True)
        rep = make_report()
        rep["recipe_id"] = "0" * 64
        r = ev(make_run(report=rep), 1, d3)
        case("eval-recipe-mismatch: a different recipe_id is a defect with the named reason",
             r["class"] == "defect" and r["must"]["recipe_id"] is False and any(x.startswith("recipe-mismatch") for x in r["defect_reasons"]))
        for label, seeds in (("two of three", [13, 17]), ("a different seed", [13, 17, 24]), ("a duplicate", [13, 17, 17]),
                             ("an extra seed", [13, 17, 23, 29])):
            rep = make_report()
            rep["seeds"]["per_seed"] = [{"seed": s, "ece_post": 0.05} for s in seeds]
            r = ev(make_run(report=rep), 1, d3)
            case("eval-seeds: %s -> defect, must.seeds false" % label,
                 r["class"] == "defect" and r["must"]["seeds"] is False and any(x.startswith("seeds:") for x in r["defect_reasons"]))
        rep = make_report()
        del rep["seeds"]
        r = ev(make_run(report=rep))
        case("eval-seeds: a report with no seeds block is a defect", r["must"]["seeds"] is False and r["class"] == "defect")
        rep = make_report()
        rep["seeds"]["per_seed"] = [{"seed": s} for s in (23, 13, 17)]
        r = ev(make_run(report=rep))
        case("eval-seeds: the order of the seeds does not matter", r["must"]["seeds"] is True)
        pk = clone(BASE_PACK)
        pk.update({"accepted": False, "deploy_eligible": False})
        r = ev(make_run(pack=pk), 1, d3)
        case("eval-pack-refused: gate pass with pack accepted false -> defect with the named reason",
             r["class"] == "defect" and r["must"]["deploy_eligible"] is False and any(x.startswith("pack-refused") for x in r["defect_reasons"]))
        pk = clone(BASE_PACK)
        pk["deploy_eligible"] = False
        r = ev(make_run(pack=pk), 1, d3)
        case("eval-pack-refused: accepted but not deploy-eligible is also a defect", r["class"] == "defect" and r["must"]["deploy_eligible"] is False)
        rep = in_band_fail_report()
        rep["pilot_waiver"] = {"id": "W-selftest"}
        r = ev(make_run(report=rep, pack=skipped_pack()), 1, d3)
        case("eval-pack-refused: a waiver claimed but no pack accepted under it -> defect (not a retriable in-band fail)",
             r["class"] == "defect" and any(x.startswith("pack-refused-under-waiver") for x in r["defect_reasons"]) and r["retry_allowed"] is False)
        pk = clone(BASE_PACK)
        r = ev(make_run(report=in_band_fail_report(), pack=pk), 1, d3)
        case("eval-pack-refused: an accepted pack on a failed gate with no waiver -> defect", r["class"] == "defect"
             and any(x.startswith("pack-accepted-without-waiver") for x in r["defect_reasons"]))
        pk = clone(BASE_PACK)
        pk["waived"] = "W-selftest"
        r = ev(make_run(pack=pk), 1, d3)
        case("eval-pack-refused: a waiver on a passing gate -> defect", r["class"] == "defect")
        vd = {"phase": "completed", "verdict": {"waived": "X"}}
        r = ev(make_run(verdict=vd), 1, d3)
        case("eval-pack-refused: verdict.json and pack-outcome disagreeing about the waiver -> defect",
             r["class"] == "defect" and any(x.startswith("waiver-disagreement") for x in r["defect_reasons"]))
        vd = {"phase": "gate_failed", "verdict": {"waived": None}}
        r = ev(make_run(verdict=vd), 1, d3)
        case("eval-pack-refused: verdict.json phase gate_failed beside an accepted pack -> defect",
             r["class"] == "defect" and any(x.startswith("phase-disagreement") for x in r["defect_reasons"]))
        vd = {"phase": "failed", "verdict": {"waived": None}}
        r = ev(make_run(verdict=vd), 1, d3)
        case("eval-pack-refused: an unknown phase -> defect", r["class"] == "defect")

        # --- eval-forbidden-retry -------------------------------------------------------------------------
        rep = make_report()
        rep["device_used"] = "cpu"
        rep["device_is_cpu"] = True
        run = make_run(report=rep)
        dall = decl_with(lambda d: (d["retry"].__setitem__("attempt_cap", 3), d["retry"].__setitem__("retry_classes", ["in-band-gate-fail", "waived"])))
        case("eval-forbidden-retry: a defect at attempt 1 of cap 3 still refuses a retry (retry_classes naming both retriable classes)",
             ev(run, 1, dall)["class"] == "defect" and ev(run, 1, dall)["retry_allowed"] is False)
        case("eval-forbidden-retry: the same at attempts 2 and 3", ev(run, 2, dall)["retry_allowed"] is False and ev(run, 3, dall)["retry_allowed"] is False)

        # --- missing and unreadable files -----------------------------------------------------------------
        for rel in ("run/gate-report.json", "verdict.json", "logs/pack-outcome.json"):
            r = ev(make_run(drop=(rel,)), 1, d3)
            case("eval-missing-file: without %s -> defect, no retry" % rel,
                 r["class"] == "defect" and any(x.startswith("missing-file") and rel.split("/")[-1] in x for x in r["defect_reasons"])
                 and r["retry_allowed"] is False)
        r = ev(make_run(drop=("run/gate-report.json",)))
        case("eval-missing-file: no report -> device, recipe, inputs, seeds and every band clause false, metrics null",
             not any(r["must"][k] for k in ("device_prefix", "recipe_id", "inputs_sha256", "seeds"))
             and not any(r["band"].values()) and r["metrics"]["margin"] is None and r["metrics"]["ece_post"] is None)
        counter["n"] += 1
        junk = os.path.join(work, "junk%d" % counter["n"])
        write(os.path.join(junk, "run", "gate-report.json"), "{not json")
        write(os.path.join(junk, "verdict.json"), "[]")
        write(os.path.join(junk, "logs", "pack-outcome.json"), "null")
        r = ev(junk)
        case("eval-missing-file: unparseable or non-object files are a defect, not a crash",
             r["class"] == "defect" and sum(1 for x in r["defect_reasons"] if x.startswith("unreadable-file")) >= 3)
        nl = make_run()
        write(os.path.join(nl, "logs", "pack-outcome.json"), "null")
        r = ev(nl)
        case("eval-missing-file: a pack-outcome.json holding JSON null beside a passing report is a defect, never a pass",
             r["class"] == "defect" and r["verdict_if_final"] == "not-reproduced" and r["must"]["deploy_eligible"] is False)
        r = ev(os.path.join(work, "no-such-dir"))
        case("eval-missing-file: a run dir that does not exist is a defect with three missing files",
             r["class"] == "defect" and sum(1 for x in r["defect_reasons"] if x.startswith("missing-file")) == 3)
        counter["n"] += 1
        lk = make_run()
        os.replace(os.path.join(lk, "verdict.json"), os.path.join(lk, "verdict-real.json"))
        os.symlink(os.path.join(lk, "verdict-real.json"), os.path.join(lk, "verdict.json"))
        r = ev(lk)
        case("eval-missing-file: a symlinked input is never followed", r["class"] == "defect" and any(x.startswith("missing-file") for x in r["defect_reasons"]))

        # --- judging (plan 15-11): the declared margin statistic -------------------------------------------
        def sm_decl(stat="seed_mean"):
            return decl_with(lambda d: d.__setitem__("judging", {"margin_statistic": stat, "note": "selftest"}))

        def sm_report(shipped, per):
            r = make_report()
            r["margin"] = shipped
            for row, m in zip(r["seeds"]["per_seed"], per):
                row["margin"] = m
            return r

        dmean = sm_decl()
        r0 = ev(make_run())
        case("judging-default: a declaration with no judging key keeps exactly the three metrics keys",
             sorted(r0["metrics"]) == ["ece_post", "gate_pass", "margin"] and r0["class"] == "pass", str(sorted(r0["metrics"])))
        r1 = ev(make_run(), 1, sm_decl("shipped_seed"))
        case("judging-default: an explicit shipped_seed evaluates byte for byte as no judging key",
             json.dumps(r1, sort_keys=True) == json.dumps(r0, sort_keys=True))
        rep = sm_report(0.05, [0.12, 0.15, 0.18])
        r = ev(make_run(report=rep), 1, dmean)
        case("judging-seed-mean: shipped margin below the floor but the mean inside it passes (the Phase 15 shape)",
             r["class"] == "pass" and r["band"]["margin"] is True and r["verdict_if_final"] == "reproduced", str(r["defect_reasons"]))
        r = ev(make_run(report=rep), 1, decl)
        case("judging-seed-mean: the same run under the default statistic is a defect (shipped margin 0.05 out of band)",
             r["class"] == "defect" and r["band"]["margin"] is False)
        rep = sm_report(0.17, [0.05, 0.06, 0.07])
        r = ev(make_run(report=rep), 1, dmean)
        case("judging-seed-mean: the mean below the floor is a defect although the shipped margin is inside",
             r["class"] == "defect" and r["band"]["margin"] is False and any(x.startswith("out-of-band: margin") for x in r["defect_reasons"]))
        per = [0.11, 0.2, 0.29]
        r = ev(make_run(report=sm_report(0.2, per)), 1, dmean)
        case("judging-seed-mean: the judged margin is exactly fsum(per-seed margins) / 3",
             r["metrics"]["margin"] == math.fsum(per) / 3 and r["metrics"]["margin_statistic"] == "seed_mean"
             and r["metrics"]["shipped_seed_margin"] == 0.2)
        case("judging-seed-mean: margin_statistic and shipped_seed_margin appear only under seed_mean",
             "margin_statistic" not in ev(make_run(), 1, sm_decl("shipped_seed"))["metrics"]
             and "shipped_seed_margin" not in r0["metrics"])
        rep = sm_report(0.2, [0.2, 0.2, 0.2])
        rep["seeds"]["per_seed"][2]["seed"] = 99
        r = ev(make_run(report=rep), 1, dmean)
        case("judging-malformed: per-seed rows that are not the declared seeds -> defect, margin null, band false",
             r["class"] == "defect" and r["metrics"]["margin"] is None and r["band"]["margin"] is False
             and any(x.startswith("report-malformed: per-seed margins") for x in r["defect_reasons"]))
        rep = sm_report(0.2, [0.2, 0.2, 0.2])
        rep["seeds"]["per_seed"][1].pop("margin")
        r = ev(make_run(report=rep), 1, dmean)
        case("judging-malformed: a per-seed row without a margin -> defect, margin null",
             r["class"] == "defect" and r["metrics"]["margin"] is None)
        r = refused(sm_decl("median"))
        case("judging-refused: an unknown statistic is refused, naming it", bool(r) and "median" in r, str(r))
        r = refused(decl_with(lambda d: d.__setitem__("judging", {"margin_statistic": "seed_mean", "weight": 2})))
        case("judging-refused: an extra key inside judging is refused", bool(r) and "weight" in r, str(r))
        r = refused(decl_with(lambda d: d.__setitem__("judging", "seed_mean")))
        case("judging-refused: a judging value that is not an object is refused", bool(r) and "object" in r, str(r))
        r = refused(decl_with(lambda d: d.__setitem__("judging", {})))
        case("judging-refused: an empty judging object is refused (no statistic named)", bool(r), str(r))
        mpath = os.path.join(work, "decl-mean.json")
        write(mpath, dmean)
        rc, o, e = cli(["check-declaration", mpath])
        case("judging-check: check-declaration prints DECLARATION OK for a seed_mean declaration",
             rc == 0 and o == ["DECLARATION OK"] and e == [])

        # --- purity ---------------------------------------------------------------------------------------
        import ast
        src = open(os.path.abspath(__file__)).read()
        mods = set()
        for n in ast.walk(ast.parse(src)):
            if isinstance(n, ast.Import):
                mods |= {a.name.split(".")[0] for a in n.names}
            elif isinstance(n, ast.ImportFrom):
                mods.add((n.module or "").split(".")[0])
        banned = mods & {"urllib", "http", "socket", "subprocess", "boto3", "requests"}
        case("purity: no network or subprocess import (%s)" % ", ".join(sorted(mods)), not banned, str(sorted(banned)))
    finally:
        shutil.rmtree(work, ignore_errors=True)

    if failures:
        print("LAYA REPRO EVAL SELFTEST FAILED: %d case(s): %s" % (len(failures), "; ".join(failures)))
        return 1
    print("LAYA REPRO EVAL SELFTEST OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--selftest"]:
        sys.exit(selftest())
    sys.exit(main(sys.argv[1:]))
