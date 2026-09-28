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


def _macro_f1_head_f32(pred, y):
    """aprender-core f1_score(.., Average::Macro) as it computes in f32 (2PR/(P+R), a sequential
    f32 sum, / count) -- used ONLY to pick a constructed case on which that path disagrees."""
    f = np.float32
    s = f(0.0)
    labels = sorted(set(y) | set(pred))
    for c in labels:
        tp = sum(1 for p, t in zip(pred, y) if p == c and t == c)
        fp = sum(1 for p, t in zip(pred, y) if p == c and t != c)
        fn = sum(1 for p, t in zip(pred, y) if p != c and t == c)
        pr = f(0.0) if tp + fp == 0 else f(tp) / f(tp + fp)
        rc = f(0.0) if tp + fn == 0 else f(tp) / f(tp + fn)
        s = s + (f(0.0) if pr + rc == f(0.0) else (f(2.0) * pr * rc) / (pr + rc))
    return float(s / f(len(labels)))


def _splits(n, k):
    """Every way to split n rows over k predicted classes, in lexicographic order."""
    if k == 1:
        yield (n,)
        return
    for i in range(n + 1):
        for rest in _splits(n - i, k - 1):
            yield (i,) + rest


def _confusion_f1(cm):
    """(exact rational macro-F1, the f64 metrics.py value, HEAD's f32 value) of a K x K matrix."""
    from fractions import Fraction
    k = len(cm)
    y = [r for r in range(k) for c in range(k) for _ in range(cm[r][c])]
    pred = [c for r in range(k) for c in range(k) for _ in range(cm[r][c])]
    labels = sorted(set(y) | set(pred))
    exact = []
    for c in labels:
        tp = cm[c][c]
        den = 2 * tp + sum(cm[r][c] for r in range(k) if r != c) + sum(cm[c][j] for j in range(k) if j != c)
        exact.append(Fraction(0) if den == 0 else Fraction(2 * tp, den))
    f1s = [_f1(np.array(pred), np.array(y), c) for c in labels]
    return sum(exact) / len(exact), math.fsum(f1s) / len(f1s), _macro_f1_head_f32(pred, y), y, pred


def _exact_margin_case(counts, name, py_pass):
    """The first (zero-shot, fine-tuned) pair of confusion matrices over true-class counts
    `counts` (lexicographic order) whose EXACT rational macro-F1 margin is 1/20 -- the gate
    boundary -- and on which metrics.py's f64 verdict is `py_pass` while HEAD's f32 path decides
    the other way: the reviewers' V7-a / A5-2 disagreement, reconstructed."""
    import itertools
    from fractions import Fraction
    k = len(counts)
    by_exact = {}
    for cm in itertools.product(*[list(_splits(n, k)) for n in counts]):
        exact, v64, v32, y, pred = _confusion_f1(cm)
        by_exact.setdefault(exact, []).append((v64, v32, y, pred))
    for zs_exact in sorted(by_exact):
        for zs in by_exact[zs_exact]:
            for ft in by_exact.get(zs_exact + Fraction(1, 20), []):
                if (ft[0] - zs[0] >= CASE_MIN_MARGIN) != py_pass:
                    continue
                if (ft[1] - zs[1] >= CASE_MIN_MARGIN) == py_pass:
                    continue
                y = zs[2]
                if ft[2] != y:
                    raise RuntimeError("row order drifted between the two matrices")
                confs = [0.55 + 0.05 * (i % 8) for i in range(len(y))]
                return {
                    "name": name, "k": k, "bins": 15, "labels": list(y), "f_avg_labels": None,
                    "probabilities_f32_hex": [[f32_hex(v) for v in _pred_row(k, p, c)]
                                              for p, c in zip(ft[3], confs)],
                    "zero_shot_probabilities_f32_hex": [[f32_hex(v) for v in _pred_row(k, p, c)]
                                                        for p, c in zip(zs[3], reversed(confs))],
                }
    raise RuntimeError("no exact-margin case over counts %s" % (counts,))


def case_expected(case):
    """The exact f64 results of one case, as metrics.py computes them (hex)."""
    y = np.array(case["labels"], dtype=np.int64)
    P = probs_from_hex(case["probabilities_f32_hex"])
    out = {"macro_f1_f64_hex": f64_hex(macro_f1(P, y))}
    if "zero_shot_probabilities_f32_hex" in case:
        zs = macro_f1(probs_from_hex(case["zero_shot_probabilities_f32_hex"]), y)
        margin = macro_f1(P, y) - zs
        out["zero_shot_macro_f1_f64_hex"] = f64_hex(zs)
        out["margin_f64_hex"] = f64_hex(margin)
        out["margin_pass"] = bool(math.isfinite(margin) and margin >= CASE_MIN_MARGIN)
    return out


def build_numeric_cases():
    cases = [_exact_margin_case((3, 3, 3), "margin_exact_1_20_9row", py_pass=True)]
    for c in cases:
        c["expected"] = case_expected(c)
    return {
        "schema": NUMERIC_SCHEMA,
        "generator": "uv run --project scripts/laya_train --frozen python scripts/laya_train/metrics.py "
                     "--write-numeric-cases",
        "hex": "f32 = 8 lowercase hex digits, f64 = 16, big-endian IEEE-754 bits (Rust to_bits)",
        "min_macro_f1_margin": CASE_MIN_MARGIN,
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
    if doc.get("min_macro_f1_margin") != CASE_MIN_MARGIN:
        failures.append("numeric cases: min_macro_f1_margin is not %r" % CASE_MIN_MARGIN)
    for c in ncases:
        got, want = case_expected(c), c["expected"]
        ok = got == want
        print("  %-4s %s %s" % ("ok" if ok else "FAIL", c["name"],
                                "" if ok else "got %s want %s" % (got, want)))
        if not ok:
            failures.append("numeric " + c["name"])

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
