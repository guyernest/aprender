"""The motivation bake-off: one counts-only report over the three arms' eval probabilities (Phase 12, plan 12-05).

    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/bakeoff.py \\
        --data DIR --laya RUN_DIR --setfit PATH|blocked:<failure_class>:<evidence path> \\
        --kev PATH|blocked:<failure_class>:<evidence path> --out PATH
    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/bakeoff.py --render-tables EVIDENCE
    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/bakeoff.py --selftest

OPS-03: every number comes from scripts/laya_train/metrics.py (macro_f1, ece_top_label, precision_recall_per_class,
false_assignment_rate, recall_at_threshold, ece_bootstrap_ci, length_tertile_cuts, length_band, check_distribution),
the one Python home of the gate metrics. This script computes no metric of its own: it aligns rows, calls metrics.*,
and lays the results out. REPORTED, NOT A GATE: nothing here decides a pass, a deploy or a switch (`not_a_gate`).

DECLARED RULES (written before any arm's number exists; the run only applies them):

    arms          laya (eval-probs.json of the Laya run dir: the shipped median-ECE seed, calibrated), setfit and kev
                  (reference arms, D-06). laya_zero_shot (zero-shot-probs.json, the declared base on the same rows) is
                  reported beside them as a baseline and is not ranked. A blocked arm is reported with its
                  failure_class and evidence path and is absent from the ranking.
    ranking       by metrics.macro_f1 on the shared eval split, descending (D-05).
    tiebreak      two arms are TIED when their macro-F1 is equal rounded to 4 decimals; the tie is broken by the LOWER
                  false-assignment rate at 0.6 (experiment-coach's motivation_min_prob), then by the unrounded macro-F1.
                  The false-assignment rate is never used any other way: there is no false-assignment ceiling.
    clearly beats a reference arm whose macro-F1 exceeds Laya's by at least 0.05 (the gate's own margin threshold,
                  laya-finetune-gate-v1 gate_min_macro_f1_margin, checked against the contract by --selftest) is
                  written as the finding `reference_arm_clearly_beats_laya`. It is a finding, never a switch: Laya
                  stays the served arm (D-06).
    thresholds    0.5, 0.6, 0.7 and 0.85. The false-assignment rate of an arm is the share of its eval `other` rows
                  whose largest non-`other` probability is STRICTLY greater than the threshold (D-05, "exceeds"),
                  over all its eval `other` rows. Beside it, `client_ge` is the same count with `>=`
                  (experiment-coach assigns at p >= min_prob, crates/core/src/treatment.rs:59): reported only, never
                  ranked, so the hand-off can disclose the difference at exact threshold equality. Probabilities
                  are compared as float64; Laya's and Kev's are stored f32 values widened, so a value that prints
                  as 0.6 is the f32 0.60000002 and lies above 0.6.
    length        len(text) in code points of the eval.jsonl text. Bands are the tertiles of the eval lengths: cut
                  points c1, c2 = numpy.percentile at 100/3 and 200/3 (recorded), band 0 if len <= c1, 1 if len <= c2,
                  else 2 (D-14). Per-class precision and recall, per-class counts and the false-assignment rates are
                  reported inside each band for every arm, the counts beside every rate (about 4 conspiratorial rows
                  per band: say so).
    ECE           the house top-label ECE (metrics.ece_top_label, 15 bins from the contract) with a class-stratified
                  percentile bootstrap 95 % interval, 10,000 resamples, numpy RandomState(13) (D-13). Reported, not a
                  gate. The interval is checked only for 0 <= lo <= hi <= 1 and for determinism, NEVER for containing
                  the point (the bootstrap is biased upward). Inside a length band the interval is reported only for
                  a band of at least 30 rows; a smaller band carries its point and the string `not_reported`.
    source batch  UNMEASURED: the mapping is not available (D-04, D-15). Nothing is computed by source.
    counts only   no post text is read into the report, printed or written; row identity is by index and sha256.

ALIGNMENT is by ROW IDENTITY, never by truth labels (two swapped same-label rows leave every label-based check green):
Laya and Kev by index and text_sha256 (data.exact_sha256 of the eval text); the SetFit dump by its row_ids
(`test:<i>`, the adapter's ids) and its per-row text_sha256, with truth equal to the eval labels only as a secondary
check. Every probability row of every arm passes metrics.check_distribution before any metric. Laya's numbers must
reproduce its gate report bit for bit (fine_tuned.macro_f1, fine_tuned.ece_post, zero_shot.macro_f1) or the run is
refused. Refusals print one `REFUSED bakeoff-...` line and exit 2.
"""
import argparse
import contextlib
import io
import json
import math
import re
import sys
import tempfile
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import common  # noqa: E402
import contract  # noqa: E402
import data  # noqa: E402
import metrics  # noqa: E402

SCHEMA = "motivation-bakeoff-v1"
THRESHOLDS = (0.5, 0.6, 0.7, 0.85)
LEAD_THRESHOLDS = (0.6, 0.85)
TIEBREAK_THRESHOLD = 0.6
CLEARLY_BEATS_MARGIN = 0.05
BAND_MIN_ROWS = 30
N_RESAMPLES = 10000
BOOTSTRAP_SEED = 13
OTHER_LABEL = "other"
ARM_ORDER = ("laya", "laya_zero_shot", "setfit", "kev")
RANKED_ARMS = ("laya", "setfit", "kev")
SOURCE_BATCH = "unmeasured (the mapping is not available; D-04, D-15)"
THRESHOLD_NOTE = ("the declared false-assignment rate counts a largest non-other probability STRICTLY greater than the "
                  "threshold (D-05); experiment-coach assigns at p >= min_prob, so client_ge holds the "
                  "client-equivalent counts (>=), reported only and never ranked; the two differ only at exact "
                  "threshold equality; Laya and Kev probabilities are stored f32 values widened to float64")
BAND_RULES = ("len <= c1", "c1 < len <= c2", "len > c2")


class BakeoffRefused(Exception):
    """A refused input: the message is the one `REFUSED bakeoff-...` line."""


def refuse(rule, detail):
    raise BakeoffRefused("REFUSED %s: %s" % (rule, detail))


def rules_block():
    return {
        "ranking": "metrics.macro_f1 on the shared eval split, descending (D-05)",
        "tiebreak": "macro-F1 equal rounded to 4 decimals -> lower false-assignment rate at %s ranks first; "
                    "no false-assignment ceiling" % TIEBREAK_THRESHOLD,
        "clearly_beats": "reference macro-F1 - Laya macro-F1 >= %s -> finding reference_arm_clearly_beats_laya; "
                         "a finding, never a switch (D-06)" % CLEARLY_BEATS_MARGIN,
        "clearly_beats_margin": CLEARLY_BEATS_MARGIN,
        "thresholds": list(THRESHOLDS),
        "false_assignment": "eval other rows whose largest non-other probability is strictly > threshold, over all "
                            "eval other rows (D-05); client_ge is the same with >=, reported only",
        "length": "len(text) in code points; tertile cuts numpy.percentile at 100/3 and 200/3; band 0 if len <= c1, "
                  "1 if len <= c2, else 2 (D-14)",
        "ece": "metrics.ece_top_label, class-stratified percentile bootstrap 95 %% interval, %d resamples, "
               "RandomState(%d); reported, not a gate; the interval is not required to contain the point (D-13)"
               % (N_RESAMPLES, BOOTSTRAP_SEED),
        "band_interval_min_rows": BAND_MIN_ROWS,
        "source_batch": "unmeasured (D-04, D-15)",
        "alignment": "row identity (index + text_sha256; SetFit row_ids), never truth labels",
        "counts_only": True,
    }


# ------------------------------------------------------------------------------------------------- inputs

def _read_json(path, arm, what):
    try:
        return json.loads(Path(path).read_bytes().decode("utf-8"))
    except FileNotFoundError:
        refuse("bakeoff-alignment", "%s %s is missing" % (arm, what))
    except (UnicodeDecodeError, json.JSONDecodeError):
        refuse("bakeoff-alignment", "%s %s is not UTF-8 JSON" % (arm, what))


def load_data_dir(data_dir):
    d = Path(data_dir)
    task = data.load_task(d / "task.json")
    train = data.load_rows(d / "train.jsonl", task, "train")
    ev = data.load_rows(d / "eval.jsonl", task, "eval")
    if OTHER_LABEL not in task["labels"]:
        refuse("bakeoff-alignment", "task.json has no criterion named %r" % OTHER_LABEL)
    return task, train, ev


def distribution_or_refuse(arm, rows, k):
    try:
        metrics.check_distribution(rows, k)
    except ValueError as e:
        refuse("bakeoff-alignment", "%s distribution %s" % (arm, e))
    return np.array(rows, dtype=np.float64)


def _expected_hashes(ev_rows):
    return [data.exact_sha256(t) for t, _ in ev_rows]


def read_eval_probs(path, arm, labels, ev_rows):
    """eval_probs_schema `{labels, rows: [{row, text_sha256, probabilities}]}` -> the [N, K] float64 array,
    aligned to eval.jsonl by row index and text_sha256 (data.exact_sha256), never by truth labels."""
    doc = _read_json(path, arm, "probability file")
    if not isinstance(doc, dict) or not isinstance(doc.get("rows"), list):
        refuse("bakeoff-alignment", "%s probability file is not {labels, rows}" % arm)
    if doc.get("labels") != labels:
        refuse("bakeoff-alignment", "%s labels differ from task.json" % arm)
    rows = doc["rows"]
    if len(rows) != len(ev_rows):
        refuse("bakeoff-alignment", "%s has %d rows, eval.jsonl has %d" % (arm, len(rows), len(ev_rows)))
    want = _expected_hashes(ev_rows)
    probs = []
    for i, r in enumerate(rows):
        if not isinstance(r, dict) or "probabilities" not in r:
            refuse("bakeoff-alignment", "%s row %d has no probabilities" % (arm, i))
        idx = r.get("row")
        if isinstance(idx, bool) or not isinstance(idx, int) or idx != i:
            refuse("bakeoff-alignment", "%s rows are not 0..n-1 (the row at position %d)" % (arm, i))
        if r.get("text_sha256") != want[i]:
            refuse("bakeoff-alignment", "%s text_sha256 differs from eval.jsonl at row %d" % (arm, i))
        probs.append(r["probabilities"])
    return distribution_or_refuse(arm, probs, len(labels))


def read_setfit(path, labels, ev_rows):
    """The plan 12-06 dump (ordered_labels, row_ids, text_sha256, truth, predicted, probabilities, artifact_sha256,
    dataset_fingerprint, n_rows). Row identity first (row_ids `test:<i>` and the per-row text_sha256), truth only
    as a secondary check: two swapped same-label rows leave every label-based check green."""
    doc = _read_json(path, "setfit", "probability dump")
    if not isinstance(doc, dict):
        refuse("bakeoff-alignment", "setfit probability dump is not a JSON object")
    if "row_ids" not in doc or "text_sha256" not in doc:
        refuse("bakeoff-alignment", "setfit dump lacks row identity (row_ids / text_sha256): regenerate it with "
                                    "the revised --dump-probabilities")
    if doc.get("ordered_labels") != labels:
        refuse("bakeoff-alignment", "setfit ordered_labels differ from task.json")
    n = len(ev_rows)
    for key in ("row_ids", "text_sha256", "truth", "predicted", "probabilities"):
        if not isinstance(doc.get(key), list) or len(doc[key]) != n:
            refuse("bakeoff-alignment", "setfit %s does not hold one entry per eval row (%d)" % (key, n))
    if doc["row_ids"] != ["test:%d" % i for i in range(n)]:
        refuse("bakeoff-alignment", "setfit row identity: row_ids are not test:0 .. test:%d" % (n - 1))
    want = _expected_hashes(ev_rows)
    for i in range(n):
        if doc["text_sha256"][i] != want[i]:
            refuse("bakeoff-alignment", "setfit row identity: text_sha256 differs from eval.jsonl at row %d" % i)
    for i, (_, lab) in enumerate(ev_rows):
        t = doc["truth"][i]
        if isinstance(t, bool) or not isinstance(t, int) or t != lab:
            refuse("bakeoff-alignment", "setfit truth differs from the eval label indices at row %d" % i)
    return distribution_or_refuse("setfit", doc["probabilities"], len(labels)), doc


def _seed_files(run):
    found = []
    for p in (Path(run) / "seeds").glob("seed-*/eval-probs.json"):
        m = re.fullmatch(r"seed-(\d+)", p.parent.name)
        if m:
            found.append((int(m.group(1)), p))
    return sorted(found)


def read_laya_run(run_dir, labels, ev_rows):
    run = Path(run_dir)
    ft = read_eval_probs(run / "eval-probs.json", "laya", labels, ev_rows)
    zs = read_eval_probs(run / "zero-shot-probs.json", "laya_zero_shot", labels, ev_rows)
    seeds = [(seed, read_eval_probs(p, "laya seed %d" % seed, labels, ev_rows)) for seed, p in _seed_files(run)]
    gate = _read_json(run / "gate-report.json", "laya", "gate-report.json")
    return ft, zs, seeds, gate


def _number_at(doc, path):
    """doc[a][b]... as a finite JSON number, else a bakeoff-laya-recompute refusal naming the field."""
    cur = doc
    for key in path.split("."):
        if not isinstance(cur, dict) or key not in cur:
            refuse("bakeoff-laya-recompute", "%s is missing from gate-report.json" % path)
        cur = cur[key]
    if isinstance(cur, bool) or not isinstance(cur, (int, float)) or not math.isfinite(cur):
        refuse("bakeoff-laya-recompute", "%s is not a finite number in gate-report.json" % path)
    return float(cur)


def check_laya_recompute(gate, ft, zs, seeds, y, bins):
    """Laya's numbers must reproduce the gate report bit for bit (the metrics are bit-identical by construction:
    the trainer computed them with metrics.py on exactly these probabilities), or the run is refused."""
    if not isinstance(gate, dict):
        refuse("bakeoff-laya-recompute", "gate-report.json is not a JSON object")
    for field, got in (("fine_tuned.macro_f1", metrics.macro_f1(ft, y)),
                       ("fine_tuned.ece_post", metrics.ece_top_label(ft, y, bins)),
                       ("zero_shot.macro_f1", metrics.macro_f1(zs, y))):
        if _number_at(gate, field) != got:
            refuse("bakeoff-laya-recompute", field)
    per_seed = (gate.get("seeds") or {}).get("per_seed") if isinstance(gate.get("seeds"), dict) else None
    if per_seed is None:
        return
    by_seed = dict(seeds)
    if not isinstance(per_seed, list) or sorted(r.get("seed") for r in per_seed if isinstance(r, dict)) != sorted(by_seed):
        refuse("bakeoff-laya-recompute", "seeds.per_seed does not list the seeds/seed-<s>/eval-probs.json files")
    for r in per_seed:
        P = by_seed[r["seed"]]
        for key, got in (("macro_f1", metrics.macro_f1(P, y)), ("ece_post", metrics.ece_top_label(P, y, bins))):
            if _number_at(r, key) != got:
                refuse("bakeoff-laya-recompute", "seeds.per_seed[%d].%s" % (r["seed"], key))


def parse_arm_spec(arm, value):
    """`PATH` or `blocked:<failure_class>:<evidence path>` -> ("path", PATH) | ("blocked", class, evidence)."""
    if not value.startswith("blocked:"):
        return ("path", value)
    parts = value.split(":", 2)
    if len(parts) != 3 or not re.fullmatch(r"[a-z][a-z0-9_-]*", parts[1]) or not parts[2]:
        refuse("bakeoff-alignment", "%s blocked form must be blocked:<failure_class>:<evidence path>" % arm)
    return ("blocked", parts[1], parts[2])


def blocked_record(spec):
    _, failure_class, evidence = spec
    p = Path(evidence)
    return {"status": "blocked", "failure_class": failure_class, "evidence": evidence,
            "evidence_sha256": common.sha256_file(p) if p.is_file() else None}


# ----------------------------------------------------------------------------------------------- the report

class Ctx:
    """What every arm is measured against: the labels, the eval truth, the bands."""

    def __init__(self, task, ev_rows, bins, n_resamples):
        self.labels = task["labels"]
        self.k = len(self.labels)
        self.other = self.labels.index(OTHER_LABEL)
        self.y = np.array([lab for _, lab in ev_rows], dtype=np.int64)
        self.lengths = [len(t) for t, _ in ev_rows]
        self.cuts = metrics.length_tertile_cuts(self.lengths)
        self.band = np.array([metrics.length_band(n, self.cuts) for n in self.lengths], dtype=np.int64)
        self.bins = bins
        self.n_resamples = n_resamples


def _fa_record(P, y, ctx, thr):
    d = metrics.false_assignment_rate(P, y, ctx.other, thr)
    g = metrics.false_assignment_rate(P, y, ctx.other, thr, strict=False)
    d["client_ge"] = {k: g[k] for k in ("numerator", "denominator", "rate", "comparison")}
    return d


def _scope(P, y, ctx):
    per_class = []
    for r in metrics.precision_recall_per_class(P, y):
        per_class.append(dict(r, label=ctx.labels[r["label"]]))
    recall_at = {}
    for thr in LEAD_THRESHOLDS:
        recall_at[str(thr)] = {}
        for k in range(ctx.k):
            if k != ctx.other:
                r = metrics.recall_at_threshold(P, y, k, thr)
                recall_at[str(thr)][ctx.labels[k]] = {"assigned": r["assigned"], "support": r["support"],
                                                      "recall": r["recall"]}
    return {"per_class": per_class,
            "false_assignment": {str(t): _fa_record(P, y, ctx, t) for t in THRESHOLDS},
            "recall_at": recall_at}


def _band_ece(Pb, yb, ctx):
    point = metrics.ece_top_label(Pb, yb, ctx.bins)
    if len(yb) < BAND_MIN_ROWS:
        return {"point": point, "interval": "not_reported"}
    ci = metrics.ece_bootstrap_ci(Pb, yb, ctx.bins, ctx.n_resamples, BOOTSTRAP_SEED)
    return {"point": point, "interval": {"lo": ci["lo"], "hi": ci["hi"], "n_resamples": ci["n_resamples"],
                                         "seed": ci["seed"]}}


def arm_record(P, ctx):
    y = ctx.y
    rec = {"status": "completed", "n": int(len(y)), "macro_f1": metrics.macro_f1(P, y)}
    rec.update(_scope(P, y, ctx))
    rec["ece"] = metrics.ece_bootstrap_ci(P, y, ctx.bins, ctx.n_resamples, BOOTSTRAP_SEED)
    bands = []
    for b in range(3):
        sel = ctx.band == b
        if not sel.any():
            bands.append({"band": b, "rule": BAND_RULES[b], "n": 0, "empty": True})
            continue
        Pb, yb = P[sel], y[sel]
        band = {"band": b, "rule": BAND_RULES[b], "n": int(sel.sum())}
        band.update(_scope(Pb, yb, ctx))
        band["ece"] = _band_ece(Pb, yb, ctx)
        bands.append(band)
    rec["bands"] = bands
    return rec


def rank_arms(arms):
    """Ranking by macro-F1 (D-05). Arms whose macro-F1 is equal rounded to 4 decimals are tied and ordered by the
    lower false-assignment rate at 0.6; a still-equal pair by the unrounded macro-F1, then the arm name."""
    ranked = [(n, a) for n, a in arms.items() if n in RANKED_ARMS and a["status"] == "completed"]

    def rate(a):
        r = a["false_assignment"][str(TIEBREAK_THRESHOLD)]["rate"]
        return math.inf if r is None else r
    rounded = {n: round(a["macro_f1"], 4) for n, a in ranked}
    ranked.sort(key=lambda na: (-rounded[na[0]], rate(na[1]), -na[1]["macro_f1"], na[0]))
    return [{"rank": i + 1, "arm": n, "macro_f1": a["macro_f1"],
             "false_assignment_0_6": {k: a["false_assignment"][str(TIEBREAK_THRESHOLD)][k]
                                      for k in ("numerator", "denominator", "rate")},
             "tiebreak_applied": sum(1 for v in rounded.values() if v == rounded[n]) > 1}
            for i, (n, a) in enumerate(ranked)]


def findings_for(arms):
    """D-06: a reference arm whose macro-F1 exceeds Laya's by at least the gate's own 0.05 is a FINDING, never a
    switch. Plain float64 subtraction and `>=`, the gate's own comparison."""
    laya = arms.get("laya")
    if not laya or laya["status"] != "completed":
        return []
    out = []
    for name in ("setfit", "kev"):
        arm = arms.get(name)
        if arm and arm["status"] == "completed" and arm["macro_f1"] - laya["macro_f1"] >= CLEARLY_BEATS_MARGIN:
            out.append({"id": "reference_arm_clearly_beats_laya", "arm": name, "macro_f1": arm["macro_f1"],
                        "laya_macro_f1": laya["macro_f1"], "difference": arm["macro_f1"] - laya["macro_f1"],
                        "margin": CLEARLY_BEATS_MARGIN,
                        "note": "a finding for the hand-off, never a switch: Laya stays the served arm (D-06)"})
    return out


def _class_table(rows, labels):
    counts = data.class_counts(rows, len(labels))
    n = len(rows)
    return {"n": n, "counts": {lab: c for lab, c in zip(labels, counts)},
            "fractions": {lab: c / n for lab, c in zip(labels, counts)}}


def build_report(task, train, ev, laya_run, setfit_spec, kev_spec, input_files, n_resamples=N_RESAMPLES):
    labels = task["labels"]
    bins = int(contract.thresholds()["ece_bins"])
    ctx = Ctx(task, ev, bins, n_resamples)
    ft, zs, seeds, gate = read_laya_run(laya_run, labels, ev)
    check_laya_recompute(gate, ft, zs, seeds, ctx.y, bins)
    arms = {"laya": arm_record(ft, ctx), "laya_zero_shot": arm_record(zs, ctx)}
    setfit_doc = {}
    if setfit_spec[0] == "blocked":
        arms["setfit"] = blocked_record(setfit_spec)
    else:
        setfit_P, setfit_doc = read_setfit(setfit_spec[1], labels, ev)
        arms["setfit"] = arm_record(setfit_P, ctx)
    if kev_spec[0] == "blocked":
        arms["kev"] = blocked_record(kev_spec)
    else:
        arms["kev"] = arm_record(read_eval_probs(kev_spec[1], "kev", labels, ev), ctx)
    laya_seeds = [{"seed": s, "macro_f1": metrics.macro_f1(P, ctx.y), "ece_post": metrics.ece_top_label(P, ctx.y, bins)}
                  for s, P in seeds]
    band_counts = [int((ctx.band == b).sum()) for b in range(3)]
    inputs = [{"label": lab, "path": str(p), "sha256": common.sha256_file(p)} for lab, p in input_files]
    return {
        "schema": SCHEMA,
        "not_a_gate": True,
        "rules": rules_block(),
        "eval": {"n": int(len(ctx.y)), "per_class_counts": _class_table(ev, labels)["counts"],
                 "labels": list(labels), "other_index": ctx.other,
                 "length": {"unit": "code points", "cuts": [ctx.cuts[0], ctx.cuts[1]], "band_counts": band_counts}},
        "prior": {"pi_train": _class_table(train, labels)},
        "arms": {n: arms[n] for n in ARM_ORDER},
        "laya_seeds": laya_seeds,
        "ranking": rank_arms(arms),
        "findings": findings_for(arms),
        "source_batch": SOURCE_BATCH,
        "threshold_note": THRESHOLD_NOTE,
        "inputs": inputs,
        "setfit_provenance": {"artifact_sha256": setfit_doc.get("artifact_sha256"),
                              "dataset_fingerprint": setfit_doc.get("dataset_fingerprint")},
    }


# ------------------------------------------------------------------------------------------ rendering

def _f4(x):
    return "n/a" if x is None else "%.4f" % x


def _frac(num, den):
    return "%d/%d" % (num, den)


def _blocked_label(arm):
    return "blocked (%s)" % arm.get("failure_class", "unknown")


def _scopes(arm):
    """(scope label, record) for the whole eval set and each non-empty band, in a fixed order."""
    out = [("all", arm)]
    for b in arm.get("bands", []):
        out.append(("band %d (%s)" % (b["band"], b["rule"]), b))
    return out


def render_tables(ev):
    if ev.get("schema") != SCHEMA:
        refuse("bakeoff-render", "not a %s evidence file" % SCHEMA)
    arms = ev["arms"]
    cuts = ev["eval"]["length"]["cuts"]
    lines = []
    emit = lines.append
    emit("Eval rows: %d; length cuts c1 = %.4f, c2 = %.4f (code points); band counts %s."
         % (ev["eval"]["n"], cuts[0], cuts[1], ev["eval"]["length"]["band_counts"]))
    emit("")
    emit("### LEAD: false assignment (eval `other` rows whose largest non-other probability is > threshold)")
    emit("")
    emit("| arm | scope | other rows | >0.6 num/den | rate | client_ge num | >0.85 num/den | rate | client_ge num |")
    emit("|---|---|---|---|---|---|---|---|---|")
    for name in ARM_ORDER:
        arm = arms[name]
        if arm["status"] != "completed":
            emit("| %s | %s | - | - | - | - | - | - | - |" % (name, _blocked_label(arm)))
            continue
        for scope, rec in _scopes(arm):
            if rec.get("empty"):
                emit("| %s | %s | 0 | - | - | - | - | - | - |" % (name, scope))
                continue
            f6 = rec["false_assignment"]["0.6"]
            f85 = rec["false_assignment"]["0.85"]
            emit("| %s | %s | %d | %s | %s | %d | %s | %s | %d |" % (
                name, scope, f6["denominator"], _frac(f6["numerator"], f6["denominator"]), _f4(f6["rate"]),
                f6["client_ge"]["numerator"], _frac(f85["numerator"], f85["denominator"]), _f4(f85["rate"]),
                f85["client_ge"]["numerator"]))
    emit("")
    emit("### RECALL: true rows assigned to their class at the threshold (p > threshold)")
    emit("")
    emit("| arm | scope | class | @0.6 assigned/support | recall | @0.85 assigned/support | recall |")
    emit("|---|---|---|---|---|---|---|")
    for name in ARM_ORDER:
        arm = arms[name]
        if arm["status"] != "completed":
            emit("| %s | %s | - | - | - | - | - |" % (name, _blocked_label(arm)))
            continue
        for scope, rec in _scopes(arm):
            if rec.get("empty"):
                continue
            for lab in rec["recall_at"]["0.6"]:
                r6, r85 = rec["recall_at"]["0.6"][lab], rec["recall_at"]["0.85"][lab]
                emit("| %s | %s | %s | %s | %s | %s | %s |" % (
                    name, scope, lab, _frac(r6["assigned"], r6["support"]), _f4(r6["recall"]),
                    _frac(r85["assigned"], r85["support"]), _f4(r85["recall"])))
    emit("")
    emit("### RANKING: macro-F1 on the shared eval split (laya_zero_shot is a baseline, not ranked)")
    emit("")
    emit("| rank | arm | macro-F1 | false assignment >0.6 | tiebreak applied |")
    emit("|---|---|---|---|---|")
    for r in ev["ranking"]:
        fa = r["false_assignment_0_6"]
        emit("| %d | %s | %s | %s (%s) | %s |" % (r["rank"], r["arm"], _f4(r["macro_f1"]),
                                                  _frac(fa["numerator"], fa["denominator"]), _f4(fa["rate"]),
                                                  "yes" if r["tiebreak_applied"] else "no"))
    for name in ARM_ORDER:
        arm = arms[name]
        if name in RANKED_ARMS and arm["status"] != "completed":
            emit("| - | %s | %s | - | - |" % (name, _blocked_label(arm)))
    base = arms["laya_zero_shot"]
    if base["status"] == "completed":
        emit("| baseline | laya_zero_shot | %s | - | - |" % _f4(base["macro_f1"]))
    emit("")
    for f in ev["findings"]:
        emit("Finding: %s (%s macro-F1 %s vs laya %s)" % (f["id"], f["arm"], _f4(f["macro_f1"]), _f4(f["laya_macro_f1"])))
    if not ev["findings"]:
        emit("Findings: none")
    emit("")
    emit("### PER-CLASS: precision / recall (support = true rows, predicted = argmax rows)")
    emit("")
    emit("| arm | scope | class | support | predicted | precision | recall |")
    emit("|---|---|---|---|---|---|---|")
    for name in ARM_ORDER:
        arm = arms[name]
        if arm["status"] != "completed":
            emit("| %s | %s | - | - | - | - | - |" % (name, _blocked_label(arm)))
            continue
        for scope, rec in _scopes(arm):
            if rec.get("empty"):
                continue
            for pc in rec["per_class"]:
                emit("| %s | %s | %s | %d | %d | %s | %s |" % (name, scope, pc["label"], pc["support"], pc["predicted"],
                                                              _f4(pc["precision"]), _f4(pc["recall"])))
    emit("")
    emit("### ECE: house top-label ECE (15 bins) with the class-stratified bootstrap 95 % interval")
    emit("")
    emit("| arm | scope | rows | ECE | 95 % interval |")
    emit("|---|---|---|---|---|")
    for name in ARM_ORDER:
        arm = arms[name]
        if arm["status"] != "completed":
            emit("| %s | %s | - | - | - |" % (name, _blocked_label(arm)))
            continue
        emit("| %s | all | %d | %s | [%s, %s] |" % (name, arm["n"], _f4(arm["ece"]["point"]), _f4(arm["ece"]["lo"]),
                                                   _f4(arm["ece"]["hi"])))
        for b in arm["bands"]:
            if b.get("empty"):
                continue
            iv = b["ece"]["interval"]
            shown = iv if isinstance(iv, str) else "[%s, %s]" % (_f4(iv["lo"]), _f4(iv["hi"]))
            emit("| %s | band %d (%s) | %d | %s | %s |" % (name, b["band"], b["rule"], b["n"], _f4(b["ece"]["point"]), shown))
    return "\n".join(lines) + "\n"


# ----------------------------------------------------------------------------------------------- the CLI

def run(args):
    task, train, ev = load_data_dir(args.data)
    setfit_spec, kev_spec = parse_arm_spec("setfit", args.setfit), parse_arm_spec("kev", args.kev)
    run_dir = Path(args.laya)
    d = Path(args.data)
    input_files = [("data/task.json", d / "task.json"), ("data/train.jsonl", d / "train.jsonl"),
                   ("data/eval.jsonl", d / "eval.jsonl"),
                   ("laya/eval-probs.json", run_dir / "eval-probs.json"),
                   ("laya/zero-shot-probs.json", run_dir / "zero-shot-probs.json"),
                   ("laya/gate-report.json", run_dir / "gate-report.json")]
    input_files += [("laya/seeds/%s/eval-probs.json" % p.parent.name, p) for _, p in _seed_files(run_dir)]
    for label, spec in (("setfit/probability-dump.json", setfit_spec), ("kev/eval-probs.json", kev_spec)):
        if spec[0] == "path":
            input_files.append((label, Path(spec[1])))
        elif Path(spec[2]).is_file():
            input_files.append((label.split("/")[0] + "/blocked-evidence", Path(spec[2])))
    report = build_report(task, train, ev, run_dir, setfit_spec, kev_spec, input_files, N_RESAMPLES)
    json.dumps(report, allow_nan=False)
    common.write_json(args.out, report)
    print("BAKEOFF OK %s arms=%d" % (args.out, sum(1 for a in report["arms"].values() if a["status"] == "completed")))
    for name, a in report["arms"].items():
        if a["status"] == "completed":
            f6 = a["false_assignment"]["0.6"]
            print("ARM %s macro_f1=%s ece=%s fa@0.6=%s" % (name, _f4(a["macro_f1"]), _f4(a["ece"]["point"]),
                                                          _frac(f6["numerator"], f6["denominator"])))
        else:
            print("ARM %s blocked (%s)" % (name, a["failure_class"]))
    return 0


def main(argv):
    ap = argparse.ArgumentParser(prog="bakeoff.py", description=__doc__.split("\n")[0])
    ap.add_argument("--data")
    ap.add_argument("--laya")
    ap.add_argument("--setfit")
    ap.add_argument("--kev")
    ap.add_argument("--out")
    ap.add_argument("--render-tables", metavar="EVIDENCE")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)
    try:
        if args.selftest:
            return selftest()
        if args.render_tables:
            ev = _read_json(args.render_tables, "evidence", "file")
            sys.stdout.write(render_tables(ev))
            return 0
        missing = [f for f in ("data", "laya", "setfit", "kev", "out") if getattr(args, f) is None]
        if missing:
            ap.error("missing --%s" % ", --".join(missing))
        return run(args)
    except BakeoffRefused as e:
        print(str(e), file=sys.stderr)
        return 2
    except data.DataError as e:
        print(str(e), file=sys.stderr)
        return 2


# --------------------------------------------------------------------------------------------- self-test

SENTINEL = "ZQXJ-sentinel-bakeoff-7mw"
FIX_LABELS = ["social_affiliative", "conspiratorial", "other"]
FIX_PATTERN = [0, 2, 2, 1, 2, 0, 2, 2, 2]          # 99 rows: 22 social_affiliative, 11 conspiratorial, 66 other
FIX_RESAMPLES = 500


def _fix_rows(n, tag):
    rows = []
    for i in range(n):
        rows.append(("%s row %03d %s %s" % (tag, i, SENTINEL, "x" * i), FIX_PATTERN[i % len(FIX_PATTERN)]))
    return rows


def _fix_probs(rng, ys, strength):
    out = []
    for y in ys:
        z = rng.randn(3)
        z[y] += strength
        e = np.exp(z - z.max())
        out.append(e / e.sum())
    return np.array(out)


def _eval_probs_doc(labels, rows, P):
    return {"labels": list(labels), "rows": [
        {"row": i, "text_sha256": data.exact_sha256(t), "probabilities": common.f32_list(P[i])}
        for i, (t, _) in enumerate(rows)]}


def make_fixture(root, n=99):
    """A synthetic data dir, a Laya run dir, a SetFit dump and a Kev file under `root`; returns the paths and the
    arrays the independent checks use. Every text carries SENTINEL."""
    root = Path(root)
    d = root / "data"
    d.mkdir(parents=True)
    common.write_json(d / "task.json", {"type": "choice", "instructions": "synthetic instructions %s" % SENTINEL,
                                        "criteria": {lab: "synthetic criterion %s" % lab for lab in FIX_LABELS}})
    ev_rows = _fix_rows(n, "eval")
    train_rows = _fix_rows(30, "train")
    for name, rows in (("eval.jsonl", ev_rows), ("train.jsonl", train_rows)):
        (d / name).write_bytes(common.jsonl_bytes([(t, FIX_LABELS[y]) for t, y in rows]))
    ys = [y for _, y in ev_rows]
    y = np.array(ys)
    out = {"data": d, "ev_rows": ev_rows, "y": y}

    run = root / "laya-run"
    (run / "seeds").mkdir(parents=True)
    doc_ft = _eval_probs_doc(FIX_LABELS, ev_rows, _fix_probs(np.random.RandomState(101), ys, 1.6))
    doc_zs = _eval_probs_doc(FIX_LABELS, ev_rows, _fix_probs(np.random.RandomState(102), ys, 0.4))
    common.write_json(run / "eval-probs.json", doc_ft)
    common.write_json(run / "zero-shot-probs.json", doc_zs)
    as64 = lambda doc: np.array([r["probabilities"] for r in doc["rows"]], dtype=np.float64)  # noqa: E731
    bins = int(contract.thresholds()["ece_bins"])
    per_seed = []
    for s, strength in ((13, 1.3), (17, None), (23, 1.9)):
        doc = doc_ft if strength is None else _eval_probs_doc(FIX_LABELS, ev_rows,
                                                              _fix_probs(np.random.RandomState(200 + s), ys, strength))
        (run / "seeds" / ("seed-%d" % s)).mkdir()
        common.write_json(run / "seeds" / ("seed-%d" % s) / "eval-probs.json", doc)
        Ps = as64(doc)
        per_seed.append({"seed": s, "macro_f1": metrics.macro_f1(Ps, y), "ece_post": metrics.ece_top_label(Ps, y, bins)})
    P_ft, P_zs = as64(doc_ft), as64(doc_zs)
    common.write_json(run / "gate-report.json", {
        "schema": "laya-gate-report-v1", "thresholds": contract.thresholds(),
        "zero_shot": {"macro_f1": metrics.macro_f1(P_zs, y), "ece": metrics.ece_top_label(P_zs, y, bins)},
        "fine_tuned": {"macro_f1": metrics.macro_f1(P_ft, y), "ece_post": metrics.ece_top_label(P_ft, y, bins)},
        "seeds": {"shipped": 17, "per_seed": per_seed}})
    out.update(run=run, P_ft=P_ft, P_zs=P_zs)

    P_sf = _fix_probs(np.random.RandomState(103), ys, 1.2)
    common.write_json(root / "setfit.json", {
        "ordered_labels": list(FIX_LABELS), "row_ids": ["test:%d" % i for i in range(n)],
        "text_sha256": [data.exact_sha256(t) for t, _ in ev_rows], "truth": ys,
        "predicted": [int(v) for v in P_sf.argmax(1)], "probabilities": P_sf.tolist(),
        "artifact_sha256": "a" * 64, "dataset_fingerprint": "b" * 64, "n_rows": n})
    doc_kev = _eval_probs_doc(FIX_LABELS, ev_rows, _fix_probs(np.random.RandomState(104), ys, 1.0))
    common.write_json(root / "kev.json", doc_kev)
    out.update(setfit=root / "setfit.json", kev=root / "kev.json", P_sf=P_sf, P_kev=as64(doc_kev))
    return out


def _capture(argv):
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        try:
            rc = main(argv)
        except SystemExit as e:
            rc = e.code
    return rc, out.getvalue(), err.getvalue()


def _argv(fx, out, setfit=None, kev=None, laya=None):
    return ["--data", str(fx["data"]), "--laya", str(laya or fx["run"]), "--setfit", str(setfit or fx["setfit"]),
            "--kev", str(kev or fx["kev"]), "--out", str(out)]


def _py_fa(P, y, other, thr, strict):
    """The false-assignment count by a plain loop, independent of metrics.py."""
    num = den = 0
    for row, t in zip(np.asarray(P).tolist(), np.asarray(y).tolist()):
        if t != other:
            continue
        den += 1
        top = max(v for j, v in enumerate(row) if j != other)
        if (top > thr) if strict else (top >= thr):
            num += 1
    return num, den


def _mutate(path, fn):
    doc = json.loads(Path(path).read_text(encoding="utf-8"))
    fn(doc)
    Path(path).write_text(json.dumps(doc), encoding="utf-8")


def _stub_arm(f1, rate06):
    return {"status": "completed", "macro_f1": f1,
            "false_assignment": {"0.6": {"numerator": int(round(rate06 * 10)), "denominator": 10, "rate": rate06}}}


def _selftest_rules(case, tmp):
    """Refusals, blocked arms, the tie and clearly-beats rules, strict thresholds and the no-text sentinel."""
    seen = []                                   # every stdout/stderr/output byte the cases below produced

    def refused(name, argv, needle):
        rc, o, e = _capture(argv)
        seen.extend([o, e])
        case(name, rc == 2 and e.startswith("REFUSED bakeoff-") and needle in e and "Traceback" not in e
             and e.count("\n") == 1, "exit %s %s" % (rc, e.strip()[:150]))

    def fresh(name):
        return make_fixture(tmp / name)

    print("alignment by row identity:")
    fx = fresh("sf-swap")

    def swap_rows(doc):                         # rows 1 and 2 are both `other`: truth and every label count unchanged
        for key in ("row_ids", "text_sha256", "probabilities"):
            doc[key][1], doc[key][2] = doc[key][2], doc[key][1]
    _mutate(fx["setfit"], swap_rows)
    refused("setfit: two swapped same-label rows (ids, hashes, probabilities) are refused",
            _argv(fx, tmp / "o1.json"), "REFUSED bakeoff-alignment: setfit row identity")
    fx = fresh("sf-ids")
    _mutate(fx["setfit"], lambda d: d["row_ids"].__setitem__(5, "train:5"))
    refused("setfit: row_ids that are not test:0 .. test:<n-1>", _argv(fx, tmp / "o2.json"),
            "REFUSED bakeoff-alignment: setfit row identity")
    fx = fresh("sf-hash")
    _mutate(fx["setfit"], lambda d: d["text_sha256"].__setitem__(7, "0" * 64))
    refused("setfit: a text_sha256 that is not the eval text's", _argv(fx, tmp / "o3.json"),
            "REFUSED bakeoff-alignment: setfit row identity")
    fx = fresh("sf-six")

    def six_keys(doc):
        for key in ("row_ids", "text_sha256", "dataset_fingerprint"):
            del doc[key]
    _mutate(fx["setfit"], six_keys)
    refused("setfit: the six-key dump (no row_ids)", _argv(fx, tmp / "o4.json"),
            "REFUSED bakeoff-alignment: setfit dump lacks row identity")
    fx = fresh("sf-truth")
    _mutate(fx["setfit"], lambda d: d["truth"].__setitem__(9, (d["truth"][9] + 1) % 3))
    refused("setfit: truth differs from the eval labels at one row (identity intact)", _argv(fx, tmp / "o5.json"),
            "REFUSED bakeoff-alignment: setfit truth")
    fx = fresh("kev-hash")
    _mutate(fx["kev"], lambda d: d["rows"][3].__setitem__("text_sha256", "f" * 64))
    refused("kev: a row whose text_sha256 differs", _argv(fx, tmp / "o6.json"), "REFUSED bakeoff-alignment: kev")
    fx = fresh("kev-rows")
    _mutate(fx["kev"], lambda d: d["rows"][4].__setitem__("row", 40))
    refused("kev: rows that are not 0..n-1", _argv(fx, tmp / "o7.json"), "REFUSED bakeoff-alignment: kev")
    print("distributions:")
    fx = fresh("kev-range")
    _mutate(fx["kev"], lambda d: d["rows"][4].__setitem__("probabilities", [1.2, -0.1, -0.1]))
    refused("kev: a probability outside [0, 1]", _argv(fx, tmp / "o8.json"),
            "REFUSED bakeoff-alignment: kev distribution row 4")
    fx = fresh("sf-sum")
    _mutate(fx["setfit"], lambda d: d["probabilities"].__setitem__(6, [0.5, 0.3, 0.3]))
    refused("setfit: a row summing to 1.1", _argv(fx, tmp / "o9.json"),
            "REFUSED bakeoff-alignment: setfit distribution row 6")
    print("Laya reproduces its gate report bit for bit:")
    for field, mut in (("fine_tuned.macro_f1", lambda g: g["fine_tuned"].__setitem__("macro_f1", g["fine_tuned"]["macro_f1"] + 1e-12)),
                       ("fine_tuned.ece_post", lambda g: g["fine_tuned"].__setitem__("ece_post", g["fine_tuned"]["ece_post"] + 1e-12)),
                       ("zero_shot.macro_f1", lambda g: g["zero_shot"].__setitem__("macro_f1", g["zero_shot"]["macro_f1"] - 1e-12)),
                       ("seeds.per_seed", lambda g: g["seeds"]["per_seed"][0].__setitem__("ece_post", 0.5))):
        fx = fresh("rc-" + field)
        _mutate(fx["run"] / "gate-report.json", mut)
        refused("laya recompute: %s off by a hair is refused" % field, _argv(fx, tmp / "o10.json"),
                "REFUSED bakeoff-laya-recompute: " + field)

    print("blocked arms:")
    fx = fresh("blocked")
    evidence = tmp / "kev-blocked-evidence.txt"
    evidence.write_text("kev arm blocked: no compute window\n", encoding="utf-8")
    outb = tmp / "blocked.json"
    rc, o, e = _capture(_argv(fx, outb, kev="blocked:compute:%s" % evidence))
    seen.extend([o, e])
    repb = json.loads(outb.read_text(encoding="utf-8")) if outb.is_file() else {}
    kev = repb.get("arms", {}).get("kev", {})
    case("--kev blocked:compute:<path>: the arm is blocked with its failure_class and evidence",
         rc == 0 and kev.get("status") == "blocked" and kev.get("failure_class") == "compute"
         and kev.get("evidence") == str(evidence) and "arms=3" in o, "exit %s %s" % (rc, e.strip()[:100]))
    case("a blocked arm is absent from the ranking", [r["arm"] for r in repb.get("ranking", [])].count("kev") == 0
         and len(repb.get("ranking", [])) == 2)
    rt = _capture(["--render-tables", str(outb)]) if outb.is_file() else (1, "", "")
    seen.extend([rt[1], rt[2]])
    case("--render-tables prints blocked (compute) for the blocked arm", rt[0] == 0 and "blocked (compute)" in rt[1])
    refused("a malformed blocked form is refused", _argv(fx, tmp / "o11.json", setfit="blocked:Bad Class:x"),
            "REFUSED bakeoff-alignment: setfit blocked form")

    print("the declared rules:")
    stub = {"laya": _stub_arm(0.71234, 0.20), "setfit": _stub_arm(0.71231, 0.10), "kev": _stub_arm(0.80, 0.30)}
    ranked = rank_arms(stub)
    case("macro-F1 equal to 4 decimals ranks by the lower false-assignment rate at 0.6",
         [r["arm"] for r in ranked] == ["kev", "setfit", "laya"] and [r["tiebreak_applied"] for r in ranked] == [False, True, True],
         str([(r["arm"], r["tiebreak_applied"]) for r in ranked]))
    ranked2 = rank_arms({"laya": _stub_arm(0.7124, 0.50), "setfit": _stub_arm(0.7120, 0.01)})
    case("a macro-F1 difference of 0.0004 is not a tie: the higher macro-F1 ranks first whatever the rate",
         [r["arm"] for r in ranked2] == ["laya", "setfit"] and not any(r["tiebreak_applied"] for r in ranked2))
    case("the clearly-beats margin is the gate's own 0.05", CLEARLY_BEATS_MARGIN == float(contract.thresholds()["min_macro_f1_margin"]) == 0.05)
    f_at = findings_for({"laya": _stub_arm(0.50, 0.1), "setfit": _stub_arm(0.55, 0.1), "kev": _stub_arm(0.549, 0.1)})
    case("a reference arm at Laya + 0.05 yields the finding; at Laya + 0.049 it does not",
         [f["arm"] for f in f_at] == ["setfit"] and f_at[0]["id"] == "reference_arm_clearly_beats_laya"
         and "switch" not in json.dumps(f_at[0]).replace("never a switch", ""), str(f_at))
    case("no finding when Laya leads; a blocked reference arm is skipped",
         findings_for({"laya": _stub_arm(0.7, 0.1), "setfit": _stub_arm(0.6, 0.1), "kev": {"status": "blocked"}}) == [])

    fx = fresh("equal")

    def exact_half(doc):                        # an `other` row whose top non-other probability is exactly 0.5
        doc["probabilities"][1] = [0.5, 0.25, 0.25]
    _mutate(fx["setfit"], exact_half)
    oute = tmp / "equal.json"
    rc, o, e = _capture(_argv(fx, oute))
    seen.extend([o, e])
    P_sf = np.array(json.loads(fx["setfit"].read_text(encoding="utf-8"))["probabilities"])
    sf = json.loads(oute.read_text(encoding="utf-8"))["arms"]["setfit"]["false_assignment"]["0.5"] if oute.is_file() else {}
    want, want_ge = _py_fa(P_sf, fx["y"], 2, 0.5, True), _py_fa(P_sf, fx["y"], 2, 0.5, False)
    case("a probability exactly equal to the threshold is not counted (strict >) but is in client_ge",
         rc == 0 and (sf["numerator"], sf["client_ge"]["numerator"]) == (want[0], want_ge[0]) and want_ge[0] > want[0],
         "strict %s client_ge %s" % (want, want_ge))

    print("no text on stdout, stderr or the output (sentinel %s):" % SENTINEL)
    out_ok = tmp / "sentinel-ok.json"
    rc, o, e = _capture(_argv(fresh("sent-ok"), out_ok))
    seen.extend([o, e, out_ok.read_text(encoding="utf-8"), _capture(["--render-tables", str(out_ok)])[1]])
    probe_out, probe_err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(probe_out), contextlib.redirect_stderr(probe_err):
        print(SENTINEL)
        print(SENTINEL, file=sys.stderr)
    case("control: the capture sees a sentinel on stdout and on stderr",
         SENTINEL in probe_out.getvalue() and SENTINEL in probe_err.getvalue())
    case("the sentinel is in every synthetic text yet in no stdout, stderr, output JSON or table of %d captures (success and "
         "every refusal above)" % len(seen), rc == 0 and len(seen) > 20 and not any(SENTINEL in s for s in seen))


def selftest():
    failures = []
    n_cases = [0]

    def case(name, ok, detail=""):
        n_cases[0] += 1
        print("  %-4s %-86s %s" % ("ok" if ok else "FAIL", name, detail))
        if not ok:
            failures.append(name)

    with tempfile.TemporaryDirectory(prefix="bakeoff-selftest-") as tmp:
        tmp = Path(tmp)
        fx = make_fixture(tmp / "fx")
        old_resamples = globals()["N_RESAMPLES"]
        globals()["N_RESAMPLES"] = FIX_RESAMPLES
        try:
            print("the full report from a synthetic data dir and synthetic arm files:")
            out = tmp / "report.json"
            rc, o, e = _capture(_argv(fx, out))
            case("end to end: exit 0, BAKEOFF OK, four completed arms", rc == 0 and "BAKEOFF OK" in o and "arms=4" in o,
                 "exit %s %s" % (rc, e.strip()[:120]))
            rep = json.loads(out.read_text(encoding="utf-8")) if out.is_file() else {}
            case("schema, not_a_gate, unmeasured source batch, threshold note, rules",
                 rep.get("schema") == SCHEMA and rep.get("not_a_gate") is True and "unmeasured" in rep.get("source_batch", "")
                 and "client_ge" in rep.get("threshold_note", "") and rep.get("rules", {}).get("counts_only") is True)
            case("three laya seeds with macro_f1 and ece_post", [s["seed"] for s in rep.get("laya_seeds", [])] == [13, 17, 23])
            y, other = fx["y"], 2
            arrays = {"laya": fx["P_ft"], "laya_zero_shot": fx["P_zs"], "setfit": fx["P_sf"], "kev": fx["P_kev"]}
            bad = []
            for name, P in arrays.items():
                a = rep["arms"][name]
                if a["macro_f1"] != metrics.macro_f1(P, y) or a["ece"]["point"] != metrics.ece_top_label(P, y, 15):
                    bad.append(name + " point")
                for thr in THRESHOLDS:
                    rec = a["false_assignment"][str(thr)]
                    want = _py_fa(P, y, other, thr, True)
                    want_ge = _py_fa(P, y, other, thr, False)
                    if (rec["numerator"], rec["denominator"]) != want or \
                            (rec["client_ge"]["numerator"], rec["client_ge"]["denominator"]) != want_ge:
                        bad.append("%s fa@%s" % (name, thr))
                    if rec["client_ge"]["numerator"] < rec["numerator"]:
                        bad.append("%s client_ge<strict@%s" % (name, thr))
            case("every arm: macro-F1, ECE point and the false-assignment counts (strict and client_ge) match an "
                 "independent plain-Python count", not bad, str(bad))
            band_ok = True
            for name in arrays:
                a = rep["arms"][name]
                band_ok &= sum(b["n"] for b in a["bands"]) == a["n"] == len(y)
                for ci, lab in enumerate(FIX_LABELS):
                    band_ok &= sum(b["per_class"][ci]["support"] for b in a["bands"]) == a["per_class"][ci]["support"]
            case("the three bands partition the eval rows and each class's support", band_ok,
                 "band_counts %s cuts %s" % (rep["eval"]["length"]["band_counts"], rep["eval"]["length"]["cuts"]))
            ece_ok = True
            for name in arrays:
                a = rep["arms"][name]
                ece_ok &= 0.0 <= a["ece"]["lo"] <= a["ece"]["hi"] <= 1.0 and a["ece"]["n_resamples"] == FIX_RESAMPLES
                for b in a["bands"]:
                    iv = b["ece"]["interval"]
                    ece_ok &= (b["n"] >= BAND_MIN_ROWS) == isinstance(iv, dict) and (
                        isinstance(iv, str) and iv == "not_reported" or 0.0 <= iv["lo"] <= iv["hi"] <= 1.0)
            case("ECE intervals: 0 <= lo <= hi <= 1 (no containment check); bands of 30+ rows carry an interval", ece_ok)
            out2 = tmp / "report2.json"
            rc2, _, _ = _capture(_argv(fx, out2))
            case("determinism: a second run writes the same bytes", rc2 == 0 and out.read_bytes() == out2.read_bytes())
            fx60 = make_fixture(tmp / "fx60", n=60)
            out60 = tmp / "report60.json"
            rc60, _, e60 = _capture(_argv(fx60, out60))
            rep60 = json.loads(out60.read_text(encoding="utf-8")) if out60.is_file() else {"arms": {}}
            small = [b for a in rep60["arms"].values() if a.get("status") == "completed" for b in a["bands"]]
            case("a band of fewer than 30 rows reports its ECE point and the string not_reported",
                 rc60 == 0 and len(small) == 12 and all(b["n"] == 20 and b["ece"]["interval"] == "not_reported"
                                                        and isinstance(b["ece"]["point"], float) for b in small),
                 "exit %s %s" % (rc60, e60.strip()[:100]))
            print("the hand-off tables:")
            rt1 = _capture(["--render-tables", str(out)])
            rt2 = _capture(["--render-tables", str(out)])
            case("--render-tables: exit 0 and the same bytes twice", rt1[0] == 0 and rt1 == rt2 and rt1[1] != "")
            lead = [ln for ln in rt1[1].splitlines() if ln.startswith("| ") and ("laya" in ln or "setfit" in ln or "kev" in ln)]
            case("the lead table carries the 0.6 and 0.85 columns and rows for every completed arm and band",
                 ">0.6 num/den" in rt1[1] and ">0.85 num/den" in rt1[1]
                 and all(any(ln.startswith("| %s | all |" % n_) for ln in lead) for n_ in ARM_ORDER), "%d lead rows" % len(lead))
            _selftest_rules(case, tmp)
        finally:
            globals()["N_RESAMPLES"] = old_resamples

    if failures:
        print("BAKEOFF SELFTEST FAILED: %s" % ", ".join(failures))
        return 1
    print("BAKEOFF SELFTEST OK (%d cases)" % n_cases[0])
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
