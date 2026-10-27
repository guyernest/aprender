"""Raw, uncalibrated, zero-shot read of the Kev-4B partial dump (Phase 12 plan 12-15, closed by owner decision).

Reads models/kev/motivation-v1/scores.jsonl (identity header + one line per scored row) and prints ONE JSON object with
  run_stats    : rows scored per split, per-row ms median, total hours (sum of per-row `ms` / 3.6e6), the file's sha256
  partial_eval : one-vs-rest AUC per class, argmax accuracy and predicted-class counts on the EVAL rows

No fit of any kind happens here and no eval row is used for fitting: the numbers are the dumped probabilities as they stand
(zero-shot, the head's own temperature), so they are NOT a calibrated Kev number and must not enter the bake-off as one.

Keying rule (the defect this script exists to prevent): eval rows 0-99 and train rows 0-99 share `row` values, so every
lookup is keyed by (split, row). Gold labels come from rows-chosen.json (split eval) and are asserted equal to the label
sequence of data/decide/motivation-v1/eval.jsonl read through scripts/laya_train/data.py; class counts are asserted.

Label order of the three probabilities is task.json's criteria order (data.load_task -> labels), which is the option order the
Kev encoder lays out: kev/api.py question_keys() = list(criteria) and kev/model.py encode() builds option spans in
q["options"] order (criteria.items()); the dump's opts are the </opt> token positions in that order (asserted ascending).

Run, offline:  uv run --project scripts/laya_train --frozen python -I .planning/spikes/033-motivation-kev-4b-arm/tools/partial_eval.py
It prints numbers only, never post text.
"""
import collections, hashlib, importlib.util, json, statistics
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent.parent.parent
spec = importlib.util.spec_from_file_location("laya_data", REPO / "scripts" / "laya_train" / "data.py")
data = importlib.util.module_from_spec(spec)
spec.loader.exec_module(data)

OUT = REPO / "models" / "kev" / "motivation-v1"
EXPECT_EVAL_COUNTS = {"other": 63, "social_affiliative": 23, "conspiratorial": 14}


def auc_ovr(scores, positive):
    """Mann-Whitney AUC of `scores` for the 0/1 vector `positive`, ties as half."""
    pos = [s for s, p in zip(scores, positive) if p]
    neg = [s for s, p in zip(scores, positive) if not p]
    wins = sum((a > b) + 0.5 * (a == b) for a in pos for b in neg)
    return wins / (len(pos) * len(neg))


def main():
    task = data.load_task(REPO / "data" / "decide" / "motivation-v1" / "task.json")
    labels = task["labels"]
    assert labels == ["social_affiliative", "conspiratorial", "other"], labels
    ev = data.load_rows(REPO / "data" / "decide" / "motivation-v1" / "eval.jsonl", task, "eval")
    eval_y = [labels[y] for _, y in ev]

    chosen = json.loads((OUT / "rows-chosen.json").read_text())
    assert all(r["opts"] == sorted(r["opts"]) and len(r["opts"]) == 3 for r in chosen), "opts not ascending"
    by_key = {}
    for r in chosen:
        k = (r["split"], r["row"])
        assert k not in by_key, ("duplicate (split,row) in rows-chosen", k)
        by_key[k] = r["label"]
    ch_eval = [by_key[("eval", i)] for i in range(100)]
    assert ch_eval == eval_y, "rows-chosen eval labels differ from eval.jsonl"
    assert dict(collections.Counter(eval_y)) == EXPECT_EVAL_COUNTS, collections.Counter(eval_y)

    raw = (OUT / "scores.jsonl").read_bytes()
    assert raw.endswith(b"\n"), "torn final line"
    lines = [json.loads(l) for l in raw.decode().splitlines()]
    ident = json.loads((OUT / "scores-identity.json").read_text())
    assert lines[0] == {"identity": ident}, "identity header mismatch"
    rows = lines[1:]
    seen = {}
    for x in rows:
        k = (x["split"], x["row"])
        assert k not in seen, ("duplicate scored row", k)
        assert len(x["probs"]) == 3 and abs(sum(x["probs"]) - 1) < 1e-5, k
        seen[k] = x
    n_eval = sum(1 for k in seen if k[0] == "eval")
    n_train = sum(1 for k in seen if k[0] == "train")
    assert n_eval == 100, n_eval

    ms = [x["ms"] for x in rows]
    run_stats = {
        "rows_scored": {"eval": n_eval, "train": n_train},
        "per_row_ms_median": statistics.median(ms),
        "per_row_warm_s": statistics.median(ms) / 1000.0,
        "per_row_ms_min": min(ms), "per_row_ms_max": max(ms),
        "n_tokens_min": min(x["n_tokens"] for x in rows), "n_tokens_max": max(x["n_tokens"] for x in rows),
        "total_hours": sum(ms) / 3.6e6,
        "total_hours_derivation": "sum of the `ms` field over every scored row / 3.6e6 (excludes the 56 s model load)",
        "scores_sha256": hashlib.sha256(raw).hexdigest(),
        "scores_bytes": len(raw),
    }

    P = [seen[("eval", i)]["probs"] for i in range(100)]
    pred = [labels[max(range(3), key=lambda j: p[j])] for p in P]
    acc = sum(a == b for a, b in zip(pred, eval_y)) / 100
    auc = {labels[j]: auc_ovr([p[j] for p in P], [y == labels[j] for y in eval_y]) for j in range(3)}
    partial_eval = {
        "calibration": "none: raw dumped probabilities, zero-shot, the head's own temperature; no fit, no eval row used for any fitting",
        "n_eval_rows": 100,
        "gold_class_counts": dict(collections.Counter(eval_y)),
        "auc_one_vs_rest": auc,
        "argmax_accuracy": acc,
        "predicted_class_counts": {l: pred.count(l) for l in labels},
        "keying": "(split, row); eval label sequence of rows-chosen.json asserted equal to eval.jsonl; counts asserted",
    }
    print(json.dumps({"run_stats": run_stats, "partial_eval": partial_eval}, indent=1))


main()
