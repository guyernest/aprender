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

`P` is an [N, K] array of probabilities (rows sum to 1), `y` an [N] array of true label indices.

    python metrics.py --selftest    hand-computed cases + a replay of every frozen house case in
                                    scripts/setfit_fixtures/claims_stats/ece_top_label_cases.json;
                                    exits non-zero on any mismatch, prints METRICS SELFTEST OK.
"""
import json
import math
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
    return float(np.mean([_f1(pred, y, c) for c in labels]))


def f_avg(P, y, labels):
    P, y = _as_arrays(P, y)
    if not labels:
        raise ValueError("f_avg needs at least one label index")
    pred = P.argmax(1)
    return float(np.mean([_f1(pred, y, int(c)) for c in labels]))


def ece_top_label(P, y, bins=15):
    P, y = _as_arrays(P, y)
    if bins < 1:
        raise ValueError("bins must be >= 1")
    conf = P.max(1)
    pred = P.argmax(1)
    correct = (pred == y).astype(np.float64)
    idx = np.minimum(np.floor(conf * bins).astype(np.int64), bins - 1)
    n = len(y)
    ece = 0.0
    for b in range(bins):
        sel = idx == b
        nb = int(sel.sum())
        if nb:
            ece += (nb / n) * abs(correct[sel].mean() - conf[sel].mean())
    return float(ece)


def nll(P, y):
    P, y = _as_arrays(P, y)
    return float(-np.log(np.clip(P[np.arange(len(y)), y], 1e-12, 1.0)).mean())


# ------------------------------------------------------------------------------------------ self-test

def _check(name, got, want, tol, failures):
    ok = isinstance(got, float) and math.isfinite(got) and abs(got - want) <= tol
    print("  %-4s %-48s got %.12f want %.12f" % ("ok" if ok else "FAIL", name, got, want))
    if not ok:
        failures.append(name)


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

    if failures:
        print("METRICS SELFTEST FAILED: %s" % ", ".join(failures))
        return 1
    print("METRICS SELFTEST OK (%d frozen cases replayed within %g)" % (len(cases), FROZEN_TOL))
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--selftest"]:
        sys.exit(selftest())
    print("usage: python metrics.py --selftest", file=sys.stderr)
    sys.exit(2)
