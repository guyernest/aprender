"""Gate metrics for the Laya back office -- numpy only, shared by fixtures.py and the 08-08 gate.

No torch import (at module level or anywhere): the self-test must run without the ML stack.

    macro_f1(P, y)          sklearn f1_score(average="macro") semantics: mean F1 over the labels
                            present in y UNION pred -- the set aprender-core
                            metrics::classification::f1_score(.., Average::Macro) averages over.
    f_avg(P, y, labels)     mean F1 over the GIVEN label indices (TweetEval stance: against=1,
                            favor=2); a label absent from both y and pred scores 0 (zero_division=0).
    ece_top_label(P, y, bins=15)
                            THE HOUSE ECE (contracts/laya-finetune-gate-v1.yaml `ece_top_label`,
                            aprender-core calibration::expected_calibration_error_top_label):
                            conf_i = max_k p_ik, pred_i = argmax_k p_ik,
                            bin(i) = min(floor(conf_i * bins), bins - 1),
                            ECE = sum_b (n_b / N) * |acc_b - conf_b|.
                            Spike 024 binned right-closed (lo, hi]; the two differ only for a
                            confidence exactly on a bin edge -- a recorded deviation, and the Rust
                            verifier is the authority (one Rust ECE, OPS-03).
    nll(P, y)               mean -log(clip(p_true, 1e-12, 1)), spike 024's definition.

Phase 12 bake-off functions (bakeoff.py reports through these and computes nothing itself):

    precision_recall_per_class(P, y)         per label: support, predicted, tp, precision, recall.
    false_assignment_rate(P, y, other, thr)  eval `other` rows whose largest non-`other` probability is
                                             strictly > thr (D-05), over all eval `other` rows;
                                             strict=False is the client-equivalent `>=`, reported only.
    recall_at_threshold(P, y, k, thr)        true-k rows whose p_k is strictly > thr, over the true-k rows.
    ece_bootstrap_ci(P, y, ...)              class-stratified percentile bootstrap around ece_top_label.
    check_distribution(P, k)                 every row: K entries, each finite in [0, 1], sum 1 +- 1e-5.
    length_tertile_cuts / length_band        the D-14 length bands.

`P` is an [N, K] array of probabilities (rows sum to 1), `y` an [N] array of true label indices.

    python metrics.py --selftest    hand-computed cases + a replay of every frozen house case in
                                    scripts/setfit_fixtures/claims_stats/ece_top_label_cases.json;
                                    exits non-zero on any mismatch, prints METRICS SELFTEST OK.
"""
import json
import math
import numbers
import struct
import sys
from pathlib import Path

import numpy as np

REPO = Path(__file__).resolve().parents[2]
ECE_CASES = REPO / "scripts" / "setfit_fixtures" / "claims_stats" / "ece_top_label_cases.json"
FROZEN_TOL = 1e-6


def _as_arrays(P, y):
    P = np.asarray(P, dtype=np.float64)
    y = np.asarray(y, dtype=np.int64)
    if P.ndim != 2 or P.shape[0] == 0 or P.shape[1] < 2:
        raise ValueError("P must be a non-empty [N, K] array with K >= 2, got shape %s" % (P.shape,))
    if y.shape != (P.shape[0],):
        raise ValueError("y must hold one label per row: %d rows, %s labels" % (P.shape[0], y.shape))
    if (y < 0).any() or (y >= P.shape[1]).any():
        raise ValueError("every label must index a column of P (K = %d)" % P.shape[1])
    if not np.isfinite(P).all():
        raise ValueError("P holds a non-finite probability")
    return P, y


def _f1(pred, y, label):
    tp = int(((pred == label) & (y == label)).sum())
    fp = int(((pred == label) & (y != label)).sum())
    fn = int(((pred != label) & (y == label)).sum())
    den = 2 * tp + fp + fn
    return 0.0 if den == 0 else 2.0 * tp / den


def macro_f1(P, y):
    P, y = _as_arrays(P, y)
    pred = P.argmax(1)
    labels = sorted(set(y.tolist()) | set(pred.tolist()))
    f1s = [_f1(pred, y, c) for c in labels]
    return math.fsum(f1s) / len(f1s)


def f_avg(P, y, labels):
    P, y = _as_arrays(P, y)
    if not labels:
        raise ValueError("f_avg needs at least one label index")
    pred = P.argmax(1)
    f1s = [_f1(pred, y, int(c)) for c in labels]
    return math.fsum(f1s) / len(f1s)


def ece_top_label(P, y, bins=15):
    P, y = _as_arrays(P, y)
    if bins < 1:
        raise ValueError("bins must be >= 1")
    conf = P.max(1)
    pred = P.argmax(1)
    correct = (pred == y).astype(np.float64)
    idx = np.minimum(np.floor(conf * bins).astype(np.int64), bins - 1)
    n = len(y)
    terms = []
    for b in range(bins):
        sel = idx == b
        nb = int(sel.sum())
        if nb:
            acc = math.fsum(correct[sel].tolist()) / nb
            mean_conf = math.fsum(conf[sel].tolist()) / nb
            terms.append((nb / n) * abs(acc - mean_conf))
    return math.fsum(terms)


def rank_key(ece_post, rank_scale):
    """laya-finetune-gate-v1 seed_policy.rank_rule: floor(ece_post x rank_scale) as an int, the
    INTEGER scale multiplied in (gate.rank_key refuses a non-finite ECE, then calls this)."""
    return int(math.floor(float(ece_post) * int(rank_scale)))


def nll(P, y):
    P, y = _as_arrays(P, y)
    return float(-np.log(np.clip(P[np.arange(len(y)), y], 1e-12, 1.0)).mean())


# ------------------------------------------------------------------------- Phase 12 bake-off functions
#
# Counts and rates only, over the same float64 widening of the stored f32 probabilities that the gate
# scores. Every rate carries its numerator and denominator: a rate on ~14 conspiratorial rows is noise
# unless the reader can see the 14.

BOOTSTRAP_NOTE = ("not required to contain the point: resampled ECE sums absolute bin gaps, so it is "
                  "biased upward and a correct point (0.0, say) can sit below lo")


def precision_recall_per_class(P, y):
    """Per label index: {label, support, predicted, tp, precision, recall}. Predictions are the argmax with
    the same tie rule as `_f1` (numpy argmax: the lowest index). precision is None when nothing was
    predicted as the label, recall None when the label has no support."""
    P, y = _as_arrays(P, y)
    pred = P.argmax(1)
    out = []
    for c in range(P.shape[1]):
        tp = int(((pred == c) & (y == c)).sum())
        predicted = int((pred == c).sum())
        support = int((y == c).sum())
        out.append({"label": c, "support": support, "predicted": predicted, "tp": tp,
                    "precision": None if predicted == 0 else tp / predicted,
                    "recall": None if support == 0 else tp / support})
    return out


def false_assignment_rate(P, y, other_index, threshold, strict=True):
    """D-05: of the rows whose true label is `other_index`, the share whose largest NON-`other` probability
    exceeds `threshold`. STRICT `>` is the declared rule; strict=False counts `>=` (what the client,
    experiment-coach crates/core/src/treatment.rs, assigns at) and is only ever reported beside it.
    The comparison is float64 on the stored f32 values widened, so a probability exactly equal to the
    threshold is not counted under `>`. rate is None when no row is `other`."""
    P, y = _as_arrays(P, y)
    if not 0 <= int(other_index) < P.shape[1]:
        raise ValueError("other_index %r is not a column of P (K = %d)" % (other_index, P.shape[1]))
    other = y == int(other_index)
    top = np.delete(P, int(other_index), axis=1).max(1)
    hit = (top > threshold) if strict else (top >= threshold)
    num, den = int((hit & other).sum()), int(other.sum())
    return {"threshold": threshold, "numerator": num, "denominator": den,
            "rate": None if den == 0 else num / den, "comparison": ">" if strict else ">="}


def recall_at_threshold(P, y, k, threshold):
    """Of the rows whose true label is k, the share assigned to k at `threshold`: p_k strictly > threshold.
    For any threshold >= 0.5 that already means k is the top label, so no separate argmax condition."""
    P, y = _as_arrays(P, y)
    if not 0 <= int(k) < P.shape[1]:
        raise ValueError("k %r is not a column of P (K = %d)" % (k, P.shape[1]))
    own = y == int(k)
    support = int(own.sum())
    assigned = int(((P[:, int(k)] > threshold) & own).sum())
    return {"label": int(k), "threshold": threshold, "assigned": assigned, "support": support,
            "recall": None if support == 0 else assigned / support}


def _stratified_indices(y, rng):
    """One class-stratified resample: for each class, that class's row count drawn with replacement from that
    class's rows (classes in index order, so the draw order is fixed by the seed alone)."""
    parts = []
    for c in sorted(set(int(v) for v in y)):
        rows = np.flatnonzero(y == c)
        parts.append(rows[rng.randint(0, len(rows), size=len(rows))])
    return np.concatenate(parts)


def ece_bootstrap_ci(P, y, bins=15, n_resamples=10000, seed=13):
    """D-13: the house ECE (ece_top_label, never a second ECE) with a class-stratified percentile bootstrap
    95 % interval (2.5 / 97.5), numpy RandomState(seed). Deterministic for a seed. The interval is NOT
    required to contain the point (see BOOTSTRAP_NOTE); callers check 0 <= lo <= hi <= 1 only."""
    P, y = _as_arrays(P, y)
    if int(n_resamples) < 1:
        raise ValueError("n_resamples must be >= 1")
    rng = np.random.RandomState(seed)
    vals = np.empty(int(n_resamples), dtype=np.float64)
    for b in range(int(n_resamples)):
        idx = _stratified_indices(y, rng)
        vals[b] = ece_top_label(P[idx], y[idx], bins)
    lo, hi = np.percentile(vals, 2.5), np.percentile(vals, 97.5)
    return {"point": ece_top_label(P, y, bins), "lo": float(lo), "hi": float(hi),
            "n_resamples": int(n_resamples), "seed": int(seed),
            "method": "class-stratified percentile bootstrap, 2.5/97.5", "note": BOOTSTRAP_NOTE}


def check_distribution(P, k, tol=1e-5):
    """Refuse a malformed probability set before any metric is computed (`_as_arrays` checks finiteness
    only): every row must hold exactly k numbers, each finite and in [0, 1], summing to 1 within tol.
    Raises ValueError naming the ROW INDEX and the rule, never a value."""
    rows = list(P)
    if not rows:
        raise ValueError("no probability rows")
    for i, row in enumerate(rows):
        try:
            vals = list(row)
        except TypeError:
            raise ValueError("row %d: not a list of probabilities" % i) from None
        if len(vals) != k:
            raise ValueError("row %d: %d probabilities, expected %d" % (i, len(vals), k))
        for v in vals:
            if isinstance(v, bool) or not isinstance(v, numbers.Real):
                raise ValueError("row %d: a probability is not a number" % i)
            if not math.isfinite(float(v)):
                raise ValueError("row %d: a probability is not finite" % i)
            if float(v) < 0.0 or float(v) > 1.0:
                raise ValueError("row %d: a probability is outside [0, 1]" % i)
        if abs(math.fsum(float(v) for v in vals) - 1.0) > tol:
            raise ValueError("row %d: the probabilities sum to 1 +- more than %g" % (i, tol))


def length_tertile_cuts(lengths):
    """D-14: the cut points (c1, c2) of the length tertiles, numpy.percentile at 100/3 and 200/3."""
    a = np.asarray(lengths, dtype=np.float64)
    if a.ndim != 1 or a.size == 0 or not np.isfinite(a).all():
        raise ValueError("lengths must be a non-empty finite 1-D list")
    return float(np.percentile(a, 100 / 3)), float(np.percentile(a, 200 / 3))


def length_band(n, cuts):
    """0 if n <= c1, 1 if n <= c2, else 2 (the declared band rule)."""
    c1, c2 = cuts
    return 0 if n <= c1 else (1 if n <= c2 else 2)


# ------------------------------------------------------------------------------ numeric agreement
#
# contracts/laya-finetune-gate-v1.yaml `numeric_agreement`: every gate quantity is f64 with
# exactly-rounded sums (math.fsum here, aprender metrics::fsum in Rust), in the same order, so the
# two languages return the SAME BITS. numeric_cases.json freezes constructed boundary cases and
# seeded random cases with their exact results; `--selftest` here and the Rust test
# aprender_decide verify::tests::gate_numeric_cases_agree_bit_for_bit replay every one.

NUMERIC_CASES = REPO / "scripts" / "laya_train" / "numeric_cases.json"
NUMERIC_SCHEMA = "laya-numeric-cases-v1"
# laya-finetune-gate-v1 constants.gate_min_macro_f1_margin (the Rust replay asserts they agree).
CASE_MIN_MARGIN = 0.05


def f64_hex(x):
    """IEEE-754 binary64 bits, 16 lowercase hex digits, big-endian (Rust `format!("{:016x}", x.to_bits())`)."""
    return struct.pack(">d", float(x)).hex()


def f64_from_hex(h):
    return struct.unpack(">d", bytes.fromhex(h))[0]


def f32_hex(x):
    """IEEE-754 binary32 bits, 8 lowercase hex digits; x must already be a float32 value."""
    v = np.float32(x)
    if float(v) != float(x):
        raise ValueError("%r is not a float32 value" % (x,))
    return struct.pack(">f", float(v)).hex()


def f32_from_hex(h):
    return struct.unpack(">f", bytes.fromhex(h))[0]


def probs_from_hex(rows):
    """float32 bits -> the float64 array the trainer scores (train.py `as64`: an exact widening)."""
    return np.array([[f32_from_hex(h) for h in r] for r in rows], dtype=np.float64)


def _pred_row(k, pred, conf):
    """A float32 probability row whose argmax is `pred` with confidence `conf` (> 1/k)."""
    other = np.float32((1.0 - float(np.float32(conf))) / (k - 1))
    row = [other] * k
    row[pred] = np.float32(conf)
    return [float(v) for v in row]


def _splits(n, k):
    """Every way to split n rows over k predicted classes, in lexicographic order."""
    if k == 1:
        yield (n,)
        return
    for i in range(n + 1):
        for rest in _splits(n - i, k - 1):
            yield (i,) + rest


def _cm_counts(cm):
    k = len(cm)
    tp = [cm[c][c] for c in range(k)]
    fp = [sum(cm[r][c] for r in range(k) if r != c) for c in range(k)]
    fn = [sum(cm[c][j] for j in range(k) if j != c) for c in range(k)]
    present = [c for c in range(k) if sum(cm[c]) > 0 or fp[c] > 0]
    return tp, fp, fn, present


def _cm_f64(cm):
    """metrics.py macro_f1 of a confusion matrix (the same 2.0 * tp / den and fsum mean)."""
    tp, fp, fn, present = _cm_counts(cm)
    f1s = [0.0 if 2 * tp[c] + fp[c] + fn[c] == 0 else 2.0 * tp[c] / (2 * tp[c] + fp[c] + fn[c])
           for c in present]
    return math.fsum(f1s) / len(f1s)


def _cm_exact(cm):
    from fractions import Fraction
    tp, fp, fn, present = _cm_counts(cm)
    den = [2 * tp[c] + fp[c] + fn[c] for c in present]
    return sum((Fraction(0) if d == 0 else Fraction(2 * tp[c], d)) for c, d in zip(present, den)) / len(present)


def _cm_head_f32(cm):
    """aprender-core f1_score(.., Average::Macro) as HEAD computes it in f32 (2PR/(P+R), a
    sequential f32 sum, / count) -- used ONLY to pick constructed cases that path gets wrong."""
    f = np.float32
    tp, fp, fn, present = _cm_counts(cm)
    s = f(0.0)
    for c in present:
        pr = f(0.0) if tp[c] + fp[c] == 0 else f(tp[c]) / f(tp[c] + fp[c])
        rc = f(0.0) if tp[c] + fn[c] == 0 else f(tp[c]) / f(tp[c] + fn[c])
        s = s + (f(0.0) if pr + rc == f(0.0) else (f(2.0) * pr * rc) / (pr + rc))
    return float(s / f(len(present)))


def _cm_rows(cm):
    k = len(cm)
    y = [r for r in range(k) for c in range(k) for _ in range(cm[r][c])]
    pred = [c for r in range(k) for c in range(k) for _ in range(cm[r][c])]
    return y, pred


def _exact_margin_case(counts, name, py_pass):
    """The first (zero-shot, fine-tuned) pair of confusion matrices over true-class counts
    `counts` (lexicographic, then f64 order) whose EXACT rational macro-F1 margin is 1/20 -- the
    gate boundary -- and on which metrics.py's f64 verdict is `py_pass` while HEAD's f32 path
    decides the other way: the reviewers' V7-a / A5-2 disagreement, reconstructed."""
    import bisect
    import itertools
    from fractions import Fraction
    k = len(counts)
    seen = {}
    for cm in itertools.product(*[list(_splits(n, k)) for n in counts]):
        seen.setdefault((_cm_f64(cm), _cm_head_f32(cm)), cm)
    table = sorted(seen.items())
    keys = [v64 for (v64, _), _ in table]
    for (zs64, zs32), zs_cm in table:
        lo = bisect.bisect_left(keys, zs64 + CASE_MIN_MARGIN - 1e-12)
        hi = bisect.bisect_right(keys, zs64 + CASE_MIN_MARGIN + 1e-12)
        for (ft64, ft32), ft_cm in table[lo:hi]:
            if (ft64 - zs64 >= CASE_MIN_MARGIN) != py_pass or (ft32 - zs32 >= CASE_MIN_MARGIN) == py_pass:
                continue
            if _cm_exact(ft_cm) - _cm_exact(zs_cm) != Fraction(1, 20):
                continue
            y, ft_pred = _cm_rows(ft_cm)
            y_zs, zs_pred = _cm_rows(zs_cm)
            if y != y_zs:
                raise RuntimeError("row order drifted between the two matrices")
            confs = [0.55 + 0.05 * (i % 8) for i in range(len(y))]
            return {
                "name": name, "k": k, "bins": 15, "labels": y, "f_avg_labels": None,
                "probabilities_f32_hex": [[f32_hex(v) for v in _pred_row(k, p, c)]
                                          for p, c in zip(ft_pred, confs)],
                "zero_shot_probabilities_f32_hex": [[f32_hex(v) for v in _pred_row(k, p, c)]
                                                    for p, c in zip(zs_pred, reversed(confs))],
            }
    raise RuntimeError("no exact-margin case over counts %s" % (counts,))


def _ece_head_f32(rows, y, bins):
    """aprender-core expected_calibration_error_top_label as HEAD computes it in f32 (f32 bin
    index, sequential f32 bin sums, f32 accumulation) -- used ONLY to pick the grid-line case."""
    f = np.float32
    sums = [f(0.0)] * bins
    hits = [f(0.0)] * bins
    counts = [0] * bins
    for row, label in zip(rows, y):
        r = [f(v) for v in row]
        pred = max(range(len(r)), key=lambda j: (r[j], -j))
        conf = r[pred]
        b = min(int(conf * f(bins)), bins - 1)
        sums[b] = sums[b] + conf
        hits[b] = hits[b] + (f(1.0) if pred == label else f(0.0))
        counts[b] += 1
    n = f(len(y))
    ece = f(0.0)
    for b in range(bins):
        if counts[b]:
            c = f(counts[b])
            ece = ece + (c / n) * abs(sums[b] / c - hits[b] / c)
    return float(ece)


def _random_rows(rng, n, k, sharp):
    """n float32 rows of k probabilities (argmax sharpened by `sharp`) and labels drawn from them."""
    rows, y = [], []
    for _ in range(n):
        w = rng.random_sample(k) + 1e-3
        w = w ** sharp
        p = (w / w.sum()).astype(np.float32)
        rows.append([float(v) for v in p])
        cum = np.cumsum(p.astype(np.float64))
        y.append(int(min(np.searchsorted(cum, rng.random_sample() * cum[-1], side="right"), k - 1)))
    return rows, y


def _rank_grid_case(name, rank_scale, n=459, k=3, bins=15):
    """The first seeded 459 x 3 set whose rank key floor(ece x rank_scale) differs between
    metrics.py's f64 ECE and HEAD's f32 ECE: an ECE on a 1e-4 grid line, decided by the last bit
    (the reviewers' 0.0679999937 vs 0.0680000111 pair, reconstructed)."""
    for seed in range(1, 200000):
        rng = np.random.RandomState(0x0827_0000 + seed)
        rows, y = _random_rows(rng, n, k, 2)
        e64 = ece_top_label(np.array(rows), np.array(y), bins)
        e32 = _ece_head_f32(rows, y, bins)
        if rank_key(e64, rank_scale) != rank_key(e32, rank_scale):
            return {"name": name, "k": k, "bins": bins, "labels": y, "f_avg_labels": [1, 2],
                    "probabilities_f32_hex": [[f32_hex(v) for v in r] for r in rows],
                    "search_seed": seed}
    raise RuntimeError("no grid-line case found")


def _random_case(i):
    """Seeded random case i: N 3..600 rows (case 1 at the floor, N = 3), K = 2 + i mod 5 (every
    K in 2..6 four times), 15 bins; a zero-shot set on even i."""
    rng = np.random.RandomState(0x0827_1000 + i)
    n = 3 + int(rng.randint(598))
    n = 3 if i == 1 else n
    k = 2 + i % 5
    sharp = [1, 2, 4][int(rng.randint(3))]
    rows, y = _random_rows(rng, n, k, sharp)
    case = {"name": "random_%02d" % i, "k": k, "bins": 15, "labels": y,
            "f_avg_labels": [1, 2] if k >= 3 else None,
            "probabilities_f32_hex": [[f32_hex(v) for v in r] for r in rows]}
    if i % 2 == 0:
        zs_rows, _ = _random_rows(rng, n, k, 1)
        case["zero_shot_probabilities_f32_hex"] = [[f32_hex(v) for v in r] for r in zs_rows]
    return case


CASE_RANK_SCALE = 10000
N_RANDOM_CASES = 20


def case_expected(case):
    """The exact f64 results of one case, as metrics.py computes them (hex; rank_key an int)."""
    y = np.array(case["labels"], dtype=np.int64)
    bins = int(case["bins"])
    fav = case.get("f_avg_labels")

    def block(P, prefix):
        out = {prefix + "macro_f1_f64_hex": f64_hex(macro_f1(P, y)),
               prefix + "ece_f64_hex": f64_hex(ece_top_label(P, y, bins)),
               prefix + "f_avg_f64_hex": None if fav is None else f64_hex(f_avg(P, y, list(fav)))}
        return out

    P = probs_from_hex(case["probabilities_f32_hex"])
    out = block(P, "")
    out["rank_key"] = rank_key(ece_top_label(P, y, bins), CASE_RANK_SCALE)
    if "zero_shot_probabilities_f32_hex" in case:
        Z = probs_from_hex(case["zero_shot_probabilities_f32_hex"])
        out.update(block(Z, "zero_shot_"))
        margin = macro_f1(P, y) - macro_f1(Z, y)
        out["margin_f64_hex"] = f64_hex(margin)
        out["margin_pass"] = bool(math.isfinite(margin) and margin >= CASE_MIN_MARGIN)
    return out


def build_numeric_cases():
    cases = [
        _exact_margin_case((3, 3, 3), "margin_exact_1_20_9row", py_pass=True),
        _exact_margin_case((10, 10, 10), "margin_exact_1_20_30row", py_pass=False),
        _rank_grid_case("rank_grid_line_459x3", CASE_RANK_SCALE),
    ] + [_random_case(i) for i in range(N_RANDOM_CASES)]
    for c in cases:
        c["expected"] = case_expected(c)
    return {
        "schema": NUMERIC_SCHEMA,
        "generator": "uv run --project scripts/laya_train --frozen python scripts/laya_train/metrics.py "
                     "--write-numeric-cases",
        "hex": "f32 = 8 lowercase hex digits, f64 = 16, big-endian IEEE-754 bits (Rust to_bits)",
        "min_macro_f1_margin": CASE_MIN_MARGIN,
        "rank_scale": CASE_RANK_SCALE,
        "cases": cases,
    }


def numeric_cases_text():
    """The file's exact bytes: the header keys pretty-printed, then ONE compact case per line."""
    doc = build_numeric_cases()
    head = {k: v for k, v in doc.items() if k != "cases"}
    lines = ["{"] + ["  %s: %s," % (json.dumps(k), json.dumps(v)) for k, v in head.items()]
    lines.append('  "cases": [')
    body = [json.dumps(c, separators=(",", ":")) for c in doc["cases"]]
    lines += ["    %s%s" % (c, "," if i + 1 < len(body) else "") for i, c in enumerate(body)]
    lines += ["  ]", "}"]
    return "\n".join(lines) + "\n"


# ------------------------------------------------------------------------------------------ self-test

def _check(name, got, want, tol, failures):
    ok = isinstance(got, float) and math.isfinite(got) and abs(got - want) <= tol
    print("  %-4s %-48s got %.12f want %.12f" % ("ok" if ok else "FAIL", name, got, want))
    if not ok:
        failures.append(name)


def _behaviour(name, ok, failures, detail=""):
    print("  %-4s %-72s %s" % ("ok" if ok else "FAIL", name, detail))
    if not ok:
        failures.append(name)
    return 1


def _selftest_bakeoff_functions(failures):
    """Hand-computed cases for the Phase 12 functions; returns the number of cases run."""
    n = 0
    P = np.array([[.9, .05, .05], [.1, .8, .1], [.2, .2, .6], [.6, .3, .1]])
    y = np.array([0, 1, 2, 2])

    pr = precision_recall_per_class(P, y)
    n += _behaviour("precision/recall: label 0 (2 predicted, 1 right, support 1)",
                    (pr[0]["precision"], pr[0]["recall"], pr[0]["predicted"], pr[0]["support"]) == (0.5, 1.0, 2, 1),
                    failures, str(pr[0]))
    n += _behaviour("precision/recall: label 1 perfect", (pr[1]["precision"], pr[1]["recall"]) == (1.0, 1.0), failures)
    n += _behaviour("precision/recall: label 2 (one right of two)", (pr[2]["precision"], pr[2]["recall"]) == (1.0, 0.5), failures)
    pr2 = precision_recall_per_class(np.array([[.9, .1, 0.0], [.8, .2, 0.0]]), np.array([0, 1]))
    n += _behaviour("precision is None for a label never predicted, recall None without support",
                    pr2[2]["precision"] is None and pr2[2]["recall"] is None and pr2[1]["precision"] is None
                    and pr2[1]["recall"] == 0.0, failures, str(pr2[1:]))

    f6 = false_assignment_rate(P, y, 2, 0.6)
    f5 = false_assignment_rate(P, y, 2, 0.5)
    g6 = false_assignment_rate(P, y, 2, 0.6, strict=False)
    n += _behaviour("false assignment at 0.6: a probability equal to the threshold is not counted (strict >)",
                    (f6["numerator"], f6["denominator"], f6["rate"], f6["comparison"]) == (0, 2, 0.0, ">"), failures, str(f6))
    n += _behaviour("false assignment at 0.5: one of two other rows", (f5["numerator"], f5["rate"]) == (1, 0.5), failures, str(f5))
    n += _behaviour("client-equivalent >= at 0.6 counts the equal probability",
                    (g6["numerator"], g6["denominator"], g6["comparison"]) == (1, 2, ">="), failures, str(g6))
    none_other = false_assignment_rate(P[:2], y[:2], 2, 0.6)
    n += _behaviour("false assignment: rate None when no row is other", none_other["rate"] is None and none_other["denominator"] == 0, failures)

    r = recall_at_threshold(P, y, 0, 0.85)
    n += _behaviour("recall at 0.85 for label 0: 0.9 > 0.85, support 1",
                    (r["assigned"], r["support"], r["recall"]) == (1, 1, 1.0), failures, str(r))
    r2 = recall_at_threshold(P, y, 1, 0.8)
    n += _behaviour("recall: 0.8 is not > 0.8", (r2["assigned"], r2["recall"]) == (0, 0.0), failures, str(r2))

    rng = np.random.RandomState(7)
    Q = rng.dirichlet([1, 1, 1], 60)
    z = rng.randint(0, 3, 60)
    a = ece_bootstrap_ci(Q, z, n_resamples=500)
    b = ece_bootstrap_ci(Q, z, n_resamples=500)
    n += _behaviour("ece_bootstrap_ci: point is exactly ece_top_label(P, y, 15)", a["point"] == ece_top_label(Q, z, 15), failures)
    n += _behaviour("ece_bootstrap_ci: 0 <= lo <= hi <= 1", 0.0 <= a["lo"] <= a["hi"] <= 1.0, failures, "lo %.4f hi %.4f" % (a["lo"], a["hi"]))
    n += _behaviour("ece_bootstrap_ci: same seed, identical result", a == b, failures)
    c = ece_bootstrap_ci(Q, z, n_resamples=500, seed=14)
    n += _behaviour("ece_bootstrap_ci: another seed moves the interval", (c["lo"], c["hi"]) != (a["lo"], a["hi"]), failures)
    rs = np.random.RandomState(5)
    counts_kept = all(np.array_equal(np.bincount(z[_stratified_indices(z, rs)], minlength=3), np.bincount(z, minlength=3))
                      for _ in range(50))
    n += _behaviour("every stratified resample keeps each class's count", counts_kept, failures)
    # A calibrated-looking set whose point is 0.0: accepted whatever its interval (no containment rule).
    cal = ece_bootstrap_ci(np.array([[1.0, 0.0]] * 3 + [[0.0, 1.0]] * 3), np.array([0] * 3 + [1] * 3), n_resamples=100)
    n += _behaviour("point 0.0 is accepted: bounds only, never containment",
                    cal["point"] == 0.0 and 0.0 <= cal["lo"] <= cal["hi"] <= 1.0, failures, "lo %.4f hi %.4f" % (cal["lo"], cal["hi"]))

    refused = []
    for label, bad, k in (("wrong length", np.array([[0.5, 0.5]]), 3), ("outside [0, 1]", np.array([[0.7, 0.4, -0.1]]), 3),
                          ("sum off 1", np.array([[0.5, 0.3, 0.3]]), 3), ("non-finite", np.array([[np.nan, 0.5, 0.5]]), 3),
                          ("above 1", np.array([[1.2, 0.0, -0.2]]), 3), ("not a number", [["a", "b", "c"]], 3)):
        try:
            check_distribution(bad, k)
        except ValueError as e:
            refused.append(label if str(e).startswith("row 0:") else label + "?")
    n += _behaviour("check_distribution refuses six bad inputs, naming the row index",
                    len(refused) == 6 and not any(x.endswith("?") for x in refused), failures, str(refused))
    try:
        check_distribution(np.vstack([Q, [[0.2, 0.2, 0.2]]]), 3)
        later = False
    except ValueError as e:
        later = str(e).startswith("row 60:")
    n += _behaviour("check_distribution names the offending row (row 60)", later, failures)
    ok_dist = True
    try:
        check_distribution(Q, 3)
        check_distribution(np.array([[1.0, 0.0, 0.0]]), 3)
    except ValueError:
        ok_dist = False
    n += _behaviour("check_distribution accepts a valid set, including 0 and 1", ok_dist, failures)

    c1, c2 = length_tertile_cuts(list(range(1, 10)))
    n += _behaviour("length_tertile_cuts equals numpy.percentile at 100/3 and 200/3",
                    (c1, c2) == (float(np.percentile(np.arange(1, 10), 100 / 3)), float(np.percentile(np.arange(1, 10), 200 / 3))),
                    failures, "%r %r" % (c1, c2))
    n += _behaviour("length_band boundaries: c1 -> 0, just above c1 -> 1, c2 -> 1, just above c2 -> 2",
                    (length_band(c1, (c1, c2)), length_band(c1 + 1e-9, (c1, c2)), length_band(c2, (c1, c2)),
                     length_band(c2 + 1e-9, (c1, c2))) == (0, 1, 1, 2), failures)
    return n


def selftest():
    failures = []
    print("hand-computed cases:")
    # 1. A perfect, fully confident classifier: every metric at its ideal.
    P = np.eye(3)[[0, 1, 2, 0, 1, 2]]
    y = np.array([0, 1, 2, 0, 1, 2])
    _check("perfect macro_f1", macro_f1(P, y), 1.0, 0.0, failures)
    _check("perfect f_avg(1,2)", f_avg(P, y, [1, 2]), 1.0, 0.0, failures)
    _check("perfect ece", ece_top_label(P, y), 0.0, 0.0, failures)
    _check("perfect nll", nll(P, y), 0.0, 1e-12, failures)

    # 2. Always predicts class 0 on y = [0, 1, 2, 0]: F1(0) = 2*2/(2*2+2+0) = 2/3, F1(1) = F1(2) = 0,
    #    macro over {0, 1, 2} = 2/9; f_avg over (1, 2) = 0. conf = 0.6 for every row -> bin 9 of 15,
    #    acc = 0.5, ECE = |0.5 - 0.6| = 0.1. NLL = -(2 ln 0.6 + 2 ln 0.2) / 4.
    P = np.array([[0.6, 0.2, 0.2]] * 4)
    y = np.array([0, 1, 2, 0])
    _check("one-class macro_f1", macro_f1(P, y), 2.0 / 9.0, 1e-15, failures)
    _check("one-class f_avg(1,2)", f_avg(P, y, [1, 2]), 0.0, 0.0, failures)
    _check("one-class ece", ece_top_label(P, y), 0.1, 1e-12, failures)
    _check("one-class nll", nll(P, y), -(2 * math.log(0.6) + 2 * math.log(0.2)) / 4, 1e-12, failures)

    # 3. Four rows, each alone in its 15-bin (conf 0.9 -> 13, 0.75 -> 11, 0.62 -> 9, 0.7 -> 10, no
    #    confidence on an edge): ECE = (|1-0.9| + |0-0.75| + |1-0.62| + |1-0.7|) / 4 = 1.53 / 4.
    P = np.array([[0.9, 0.1], [0.75, 0.25], [0.62, 0.38], [0.3, 0.7]])
    y = np.array([0, 1, 0, 1])
    _check("4-row ece (15 bins)", ece_top_label(P, y), 1.53 / 4, 1e-12, failures)
    # pred = [0, 0, 0, 1]: F1(0) = 2*2/(4+1+0) = 0.8, F1(1) = 2*1/(2+0+1) = 2/3.
    _check("4-row macro_f1", macro_f1(P, y), (0.8 + 2.0 / 3.0) / 2, 1e-15, failures)

    # 4. The saturated row: conf = 1 gives floor(1 * 15) = 15, which must clamp to the top bin.
    P = np.array([[1.0, 0.0], [0.0, 1.0]])
    y = np.array([0, 0])
    _check("saturated ece (conf 1 -> top bin)", ece_top_label(P, y), 0.5, 1e-12, failures)

    print("frozen house cases (%s):" % ECE_CASES.relative_to(REPO))
    cases = json.loads(ECE_CASES.read_text())["cases"]
    if not cases:
        failures.append("frozen cases: none found")
    for c in cases:
        got = ece_top_label(np.array(c["probabilities"]), np.array(c["labels"]), bins=int(c["n_bins"]))
        _check("frozen %s (bins=%d)" % (c["id"], c["n_bins"]), got, float(c["ece"]), FROZEN_TOL, failures)

    print("numeric agreement cases (%s, bit for bit):" % NUMERIC_CASES.relative_to(REPO))
    doc = json.loads(NUMERIC_CASES.read_text())
    ncases = doc.get("cases") or []
    if doc.get("schema") != NUMERIC_SCHEMA or not ncases:
        failures.append("numeric cases: missing or wrong schema")
    if doc.get("min_macro_f1_margin") != CASE_MIN_MARGIN or doc.get("rank_scale") != CASE_RANK_SCALE:
        failures.append("numeric cases: min_macro_f1_margin / rank_scale are not %r / %r"
                        % (CASE_MIN_MARGIN, CASE_RANK_SCALE))
    for c in ncases:
        got, want = case_expected(c), c["expected"]
        ok = got == want
        print("  %-4s %s %s" % ("ok" if ok else "FAIL", c["name"],
                                "" if ok else "got %s want %s" % (got, want)))
        if not ok:
            failures.append("numeric " + c["name"])

    print("bake-off functions (Phase 12, plan 12-05):")
    n_behaviour = _selftest_bakeoff_functions(failures)
    print("  %d bake-off function cases run" % n_behaviour)

    if failures:
        print("METRICS SELFTEST FAILED: %s" % ", ".join(failures))
        return 1
    print("METRICS SELFTEST OK (%d frozen cases replayed within %g; %d numeric cases bit for bit)"
          % (len(cases), FROZEN_TOL, len(ncases)))
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--selftest"]:
        sys.exit(selftest())
    if sys.argv[1:] == ["--write-numeric-cases"]:
        text = numeric_cases_text()
        NUMERIC_CASES.write_text(text)
        print("WROTE %s (%d cases)" % (NUMERIC_CASES.relative_to(REPO), len(json.loads(text)["cases"])))
        sys.exit(0)
    print("usage: python metrics.py --selftest | --write-numeric-cases", file=sys.stderr)
    sys.exit(2)
