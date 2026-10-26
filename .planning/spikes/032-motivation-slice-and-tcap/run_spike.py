"""Spike 032: motivation-v1 calibration slice x temperature cap -- a MEASUREMENT, not a gate run.

    uv run --project scripts/laya_train --frozen python -I .planning/spikes/032-motivation-slice-and-tcap/run_spike.py \
        (--guard-selftest | --identity --fraction F --seeds S --epochs E | --fraction F --seeds S,.. --epochs E
         | --grid --epochs E | --aggregate --epochs E)

Adapted from spike 027's run_cell.py. Reuses scripts/laya_train UNCHANGED: train.SeedRun.train_and_score (train ->
complete F16 checkpoint -> fp32 reload -> bounded T fit on the calibration slice -> reload -> scoring), the
contract's recipe, data.calibration_split, gate.fit_temperature / calibration_monitor, metrics.

Differences from the gate run, all spike-only:
  * the scored set is a HOLDOUT carved from train.jsonl (25 % stratified, seed 13, the eval rule re-applied to
    train); the gate's eval.jsonl is NEVER opened -- an audit hook installed at import refuses any open of it
    (REFUSED spike-eval-guard);
  * the calibration fraction is a parameter passed to data.calibration_split (the contract value is not edited);
  * the slice and holdout logits are kept so T can be refitted unbounded, on [0.5, 5] and on [0.5, 10], and a
    holdout-oracle T found (a diagnostic that reads holdout labels, never a selection rule);
  * results are bound to an identity; a cached cell is reused only when its stored identity equals the current one.

No input text is ever printed or written to a log: counts, hashes and timings only.
"""
# --- the eval guard: installed before anything else is imported (T-12-23) -------------------------------------
import os
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
DATA_SRC = REPO / "data" / "decide" / "motivation-v1"
EVAL_ABS = os.path.realpath(str(DATA_SRC / "eval.jsonl"))
EVAL_ATTEMPTS = []


def _eval_guard(event, args):
    if event != "open" or not args:
        return
    target = args[0]
    if not isinstance(target, (str, bytes, os.PathLike)):
        return                                  # an fd or a handle: not a path open
    try:
        resolved = os.path.realpath(os.fsdecode(target))
    except (TypeError, ValueError):
        return
    if resolved == EVAL_ABS:
        EVAL_ATTEMPTS.append(1)
        raise PermissionError("REFUSED spike-eval-guard: the gate's eval.jsonl is never opened by spike 032")


sys.addaudithook(_eval_guard)

import argparse  # noqa: E402
import ast  # noqa: E402
import hashlib  # noqa: E402
import inspect  # noqa: E402
import json  # noqa: E402
import math  # noqa: E402
import re  # noqa: E402
import shutil  # noqa: E402
import statistics  # noqa: E402
import textwrap  # noqa: E402
import time  # noqa: E402
import types  # noqa: E402
import warnings  # noqa: E402

sys.path.insert(0, str(REPO / "scripts" / "laya_train"))
import numpy as np  # noqa: E402

import contract  # noqa: E402
import data  # noqa: E402
import gate  # noqa: E402
import metrics  # noqa: E402
import train  # noqa: E402
from common import f32_list, jsonl_bytes, sha256_bytes  # noqa: E402

HERE = Path(__file__).resolve().parent
SPIKE_DATA = REPO / "data" / "decide" / "motivation-v1-spike"
OUT_ROOT = REPO / "models" / "decide" / "spike-032"
SPLIT_SEED = 13
HOLDOUT_DIVISOR = 4                                   # 0.25, ceil done in integers (prepare_motivation.eval_count)
MIN_PER_CLASS = 2
FRACTIONS = (0.25, 0.35, 0.5)
SEEDS = (13, 17, 23)
T_WIDE = (0.05, 1000.0)                               # "unclamped": NLL is convex in 1/T, so clamp(T_unc) == the bounded fit
T_GRID = np.exp(np.linspace(math.log(0.5), math.log(50.0), 461))
CLAMP_ECE_MAX = 0.10
SLICE_ECE_DROP, SLICE_MARGIN_COST = 0.02, 0.02
DEVICE_LINES = []
EXIT_REFUSED, EXIT_GRID_INCOMPLETE = 2, 3


def log(msg):
    print(msg, flush=True)


def refuse(msg, code=EXIT_REFUSED):
    print(msg if msg.startswith("REFUSED") else "REFUSED " + msg, file=sys.stderr, flush=True)
    sys.exit(code)


def sha256_file(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def cell_name(fraction, seed):
    return "f%s-seed%d" % (repr(float(fraction)), int(seed))


# ------------------------------------------------------------------------------------------ data

def build_spike_data():
    """(task, labels, spike_train_rows, holdout_rows, bytes of the three spike files). Reads task.json and
    train.jsonl only; never eval.jsonl."""
    task = data.load_task(DATA_SRC / "task.json")
    labels = list(task["labels"])
    k = len(labels)
    train_rows = data.load_rows(DATA_SRC / "train.jsonl", task, "train")
    rng = np.random.RandomState(SPLIT_SEED)
    in_holdout = set()
    for c in range(k):
        idx = [i for i, (_, y) in enumerate(train_rows) if y == c]
        order = rng.permutation(len(idx))
        need = (len(idx) + HOLDOUT_DIVISOR - 1) // HOLDOUT_DIVISOR
        in_holdout.update(idx[j] for j in order[:need])
    hold = [train_rows[i] for i in sorted(in_holdout)]
    spike_train = [train_rows[i] for i in range(len(train_rows)) if i not in in_holdout]
    data.group_train(spike_train)
    data.refuse_overlap(spike_train, hold)
    named = lambda rs: [(t, labels[y]) for t, y in rs]  # noqa: E731
    files = {"task.json": (DATA_SRC / "task.json").read_bytes(),
             "train.jsonl": jsonl_bytes(named(spike_train)),
             "holdout.jsonl": jsonl_bytes(named(hold))}
    return task, labels, spike_train, hold, files


def write_once(files):
    SPIKE_DATA.mkdir(parents=True, exist_ok=True)
    for name, body in files.items():
        p = SPIKE_DATA / name
        if p.exists():
            if p.read_bytes() != body:
                refuse("REFUSED spike-data-differs: %s exists with other bytes; it is write-once" % name)
        else:
            p.write_bytes(body)


def class_counts(rows, k):
    return data.class_counts(rows, k)


# ------------------------------------------------------------------------------------------ identity

TRAINING_PATH = (("train", "SeedRun"), ("train", "train_seed"), ("train", "calibration_logits"),
                 ("data", "calibration_split"), ("gate", "fit_temperature"), ("gate", "calibration_monitor"),
                 ("gate", "EarlyStopper"), ("metrics", "ece_top_label"))


def training_path_sha256():
    mods = {"train": train, "data": data, "gate": gate, "metrics": metrics}
    parts = []
    for mod, name in TRAINING_PATH:
        src = textwrap.dedent(inspect.getsource(getattr(mods[mod], name)))
        parts.append("%s.%s:%s" % (mod, name, ast.dump(ast.parse(src))))
    return sha256_bytes("\n".join(parts).encode("utf-8"))


def common_identity(files, epochs):
    return {
        "source_task_json_sha256": sha256_file(DATA_SRC / "task.json"),
        "source_train_jsonl_sha256": sha256_file(DATA_SRC / "train.jsonl"),
        "holdout_sha256": sha256_bytes(files["holdout.jsonl"]),
        "spike_train_sha256": sha256_bytes(files["train.jsonl"]),
        "epochs": int(epochs),
        "harness_sha256": sha256_file(__file__),
        "training_path_sha256": training_path_sha256(),
    }


def identity_for(common, fraction, seed):
    return {**common, "fraction": float(fraction), "seed": int(seed)}


def first_difference(stored, current):
    for key in sorted(set(stored) | set(current)):
        if stored.get(key) != current.get(key):
            return key
    return None


# ------------------------------------------------------------------------------------------ measurement

def as64(P):
    return np.array([f32_list(r) for r in P], dtype=np.float64)


def ece(P, y, bins):
    return metrics.ece_top_label(np.asarray(P, dtype=np.float64), y, bins)


def measure(Zc, yc, Ze, y, P_zs, bins):
    """The cell's numbers from the slice logits Zc / labels yc and the holdout logits Ze / labels y."""
    sm = gate.softmax
    t_unc = gate.fit_temperature(Zc, yc, *T_WIDE)[0]
    t5 = min(5.0, max(0.5, t_unc))
    t10 = min(10.0, max(0.5, t_unc))
    zs_f1 = metrics.macro_f1(np.asarray(P_zs, np.float64), y)
    ft_f1 = metrics.macro_f1(sm(Ze, 1.0), y)
    grid_ece = np.array([ece(sm(Ze, t), y, bins) for t in T_GRID])
    i_or = int(np.argmin(grid_ece))
    e5 = ece(sm(Ze, t5), y, bins)
    return {
        "t_unbounded": float(t_unc), "t_clamped5": float(t5), "clamp_hit5": bool(t_unc >= 5.0),
        "t_clamped10": float(t10), "t_oracle": float(T_GRID[i_or]),
        "ece_at": {"fitted5": e5, "t5": e5, "fitted10": ece(sm(Ze, t10), y, bins),
                   "oracle": float(grid_ece[i_or]), "exact5": ece(sm(Ze, 5.0), y, bins)},
        "ft_macro_f1": float(ft_f1), "zs_macro_f1": float(zs_f1), "margin": float(ft_f1 - zs_f1),
    }


def zero_shot(base, hold_rows, question, holdout_sha):
    cache = OUT_ROOT / "zero-shot-holdout.json"
    if cache.is_file():
        obj = json.loads(cache.read_text())
        if obj.get("holdout_sha256") == holdout_sha and obj.get("base_digest") == base.digest:
            return np.array(obj["probabilities"], dtype=np.float32)
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", RuntimeWarning)
        zs_agent = train.load_for_scoring(base.src, base.digest, base.revision)
    P, _ = train.score_rows(zs_agent, hold_rows, question)
    del zs_agent
    OUT_ROOT.mkdir(parents=True, exist_ok=True)
    cache.write_text(json.dumps({"holdout_sha256": holdout_sha, "base_digest": base.digest,
                                 "probabilities": [f32_list(r) for r in P]}) + "\n")
    return P


def install_device_capture():
    orig = train.log

    def captured(msg):
        m = re.match(r"DEVICE seed=(\d+) requested=(\S+) used=(\S+)", str(msg))
        if m:
            DEVICE_LINES.append((int(m.group(1)), m.group(3)))
        orig(msg)
    train.log = captured


def run_cell(ctx, fraction, seed):
    """Train, score and measure one cell; returns the result dict (written by the caller after success)."""
    fit_ids, calib_ids, _, slice_sha = data.calibration_split(ctx["spike_train"], fraction, MIN_PER_CLASS, SPLIT_SEED, ctx["k"])
    fit_rows = [ctx["spike_train"][i] for i in fit_ids]
    calib_rows = [ctx["spike_train"][i] for i in calib_ids]
    spc = max(class_counts(ctx["spike_train"], ctx["k"]))
    out = OUT_ROOT / cell_name(fraction, seed)
    if out.exists():                                        # a crashed earlier attempt: moved aside, never deleted
        stale = OUT_ROOT / "stale"
        stale.mkdir(parents=True, exist_ok=True)
        out.rename(stale / ("%s-%d" % (out.name, int(time.time()))))
    out.mkdir(parents=True)
    (out / "NOT-A-GATE-RUN.txt").write_text(
        "spike 032 measurement (fraction %s, seed %d, calibration split seed %d). Not a declared gate run.\n"
        % (fraction, seed, SPLIT_SEED))
    base, requested, question, epochs = ctx["base"], ctx["requested"], ctx["question"], ctx["epochs"]
    _, rb, rid = contract.recipe_id_for("production", spc, epochs, seed, base.block, "early_stopping")
    (out / "recipe.json").write_bytes(rb)
    started = int(time.time())
    t0 = time.time()
    n_dev = len(DEVICE_LINES)
    ck = out / "checkpoint"
    try:
        run = train.SeedRun(base, requested, question, fit_rows, calib_rows, ctx["holdout"], ctx["k"], epochs,
                            "early_stopping", rid)
        agent, _P_ft, P_pre, calib, info = run.train_and_score(seed, ck)
        _, Zc = train.score_rows(agent, calib_rows, question)
        del agent
        try:
            import torch
            if torch.backends.mps.is_available():
                torch.mps.empty_cache()
        except Exception:  # noqa: BLE001
            pass
    finally:
        shutil.rmtree(ck, ignore_errors=True)               # KEEP 0 (25 GiB free)
    Zc = Zc.astype(np.float64)
    Ze = calib["t_pre"] * np.log(np.asarray(P_pre, dtype=np.float64))     # softmax-equivalent raw logits
    yc = np.array([lab for _, lab in calib_rows], dtype=np.int64)
    y = np.array([lab for _, lab in ctx["holdout"]], dtype=np.int64)
    m = measure(Zc, yc, Ze, y, ctx["P_zs"], ctx["bins"])
    dev_lines = [d for s, d in DEVICE_LINES[n_dev:] if s == seed]
    if len(dev_lines) != 1 or dev_lines[0] != str(info["device_used"]):
        raise RuntimeError("the DEVICE trace line (%s) does not match the device train_seed reports (%s)"
                           % (dev_lines, info["device_used"]))
    st = info["stopping_record"] or {}
    ended = int(time.time())
    result = {
        "identity": identity_for(ctx["common"], fraction, seed), "fraction": float(fraction), "seed": int(seed),
        "slice_per_class": class_counts([ctx["spike_train"][i] for i in calib_ids], ctx["k"]),
        "slice_ids_sha256": slice_sha, "fit_rows": info["fit_rows"], "calib_slice_size": len(calib_rows),
        "holdout_rows": len(ctx["holdout"]), "best_epoch": st.get("best_epoch"), "epochs_run": info["epochs_run"],
        **m, "recipe_id": rid, "stop_reason": st.get("reason"), "device_used": dev_lines[0],
        "started_at": started, "ended_at": ended, "seconds": round(time.time() - t0, 1),
        "eval_open_attempts": len(EVAL_ATTEMPTS), "not_a_gate_run": True,
    }
    return out, result


def prepare_context(epochs, need_model):
    task, labels, spike_train, hold, files = build_spike_data()
    ctx = {"task": task, "labels": labels, "k": len(labels), "spike_train": spike_train, "holdout": hold,
           "files": files, "epochs": int(epochs), "bins": int(contract.thresholds()["ece_bins"])}
    ctx["common"] = common_identity(files, epochs)
    if need_model:
        write_once(files)
        ctx["question"] = data.laya_question(task)
        install_device_capture()
        ctx["base"] = train.Base(types.SimpleNamespace(variant="production", base=None, base_sha256=None))
        ctx["requested"] = train.request_device(None)
        ctx["P_zs"] = zero_shot(ctx["base"], hold, ctx["question"], ctx["common"]["holdout_sha256"])
        log("SPIKE DATA spike_train=%s holdout=%s holdout_sha256=%s spike_train_sha256=%s"
            % (class_counts(spike_train, ctx["k"]), class_counts(hold, ctx["k"]),
               ctx["common"]["holdout_sha256"][:12], ctx["common"]["spike_train_sha256"][:12]))
    return ctx


def run_cells(cells, epochs):
    """Run (fraction, seed) cells, reusing an identity-valid cached result, refusing a mismatching one, and
    CONTINUING past a failed cell. Returns the number of cells that failed or were refused."""
    ctx = prepare_context(epochs, need_model=True)
    bad = 0
    for fraction, seed in cells:
        name = cell_name(fraction, seed)
        rj = OUT_ROOT / name / "result.json"
        current = identity_for(ctx["common"], fraction, seed)
        if rj.is_file():
            stored = json.loads(rj.read_text()).get("identity", {})
            if stored == current:
                log("REUSE %s (identity-valid)" % name)
                continue
            print("REFUSED spike-cache-identity: %s %s" % (name, first_difference(stored, current)), file=sys.stderr, flush=True)
            bad += 1
            continue
        log("CELL %s epochs=%d" % (name, epochs))
        try:
            out, result = run_cell(ctx, fraction, seed)
        except (Exception, SystemExit) as exc:  # noqa: BLE001 -- a failed cell is a result, not a reason to stop the grid
            last = (str(exc).strip().splitlines() or [""])[-1][:300]
            log("CELL FAILED %s: %s: %s" % (name, type(exc).__name__, last))
            bad += 1
            continue
        (out / "result.json").write_text(json.dumps(result, indent=1, sort_keys=True) + "\n")
        log("RESULT %s T_unc=%.3f T5=%.3f T_oracle=%.2f ece@5=%.4f ece@oracle=%.4f margin=%.4f epochs_run=%s best=%s "
            "device=%s seconds=%.0f" % (name, result["t_unbounded"], result["t_clamped5"], result["t_oracle"],
                                        result["ece_at"]["fitted5"], result["ece_at"]["oracle"], result["margin"],
                                        result["epochs_run"], result["best_epoch"], result["device_used"], result["seconds"]))
    return bad


# ------------------------------------------------------------------------------------------ aggregate

def apply_rules(runs):
    """(per_fraction, slice_fraction_selected, clamp detail, binding) -- the two rules of the README, on the nine runs."""
    def med(f, pick):
        return statistics.median([pick(r) for r in runs if r["fraction"] == f])
    per_fraction = {}
    for f in FRACTIONS:
        sel = [r for r in runs if r["fraction"] == f]
        per_fraction[repr(f)] = {
            "median_ece_fitted5": med(f, lambda r: r["ece_at"]["fitted5"]),
            "median_margin": med(f, lambda r: r["margin"]),
            "median_t_unbounded": med(f, lambda r: r["t_unbounded"]),
            "seeds_t_unbounded_above_5": sum(1 for r in sel if r["t_unbounded"] > 5.0),
        }
    base_e, base_m = per_fraction[repr(0.25)]["median_ece_fitted5"], per_fraction[repr(0.25)]["median_margin"]
    ok = [f for f in FRACTIONS[1:]
          if per_fraction[repr(f)]["median_ece_fitted5"] <= base_e - SLICE_ECE_DROP
          and per_fraction[repr(f)]["median_margin"] >= base_m - SLICE_MARGIN_COST]
    selected = min(ok) if ok else 0.25
    sel = [r for r in runs if r["fraction"] == selected]
    a = sum(1 for r in sel if r["t_unbounded"] > 5.0)
    b = sum(1 for r in sel if r["ece_at"]["t5"] > CLAMP_ECE_MAX and r["t_oracle"] > 5.0 and r["ece_at"]["oracle"] <= CLAMP_ECE_MAX)
    detail = {"selected_fraction": selected, "seeds_t_unbounded_above_5": a, "seeds_oracle_rescue": b,
              "fractions_passing_slice_rule": ok,
              "per_seed": [{"seed": r["seed"], "t_unbounded": r["t_unbounded"], "t_oracle": r["t_oracle"],
                            "ece_at_t5": r["ece_at"]["t5"], "ece_at_oracle": r["ece_at"]["oracle"]}
                           for r in sorted(sel, key=lambda r: r["seed"])]}
    return per_fraction, selected, detail, bool(a >= 2 or b >= 2)


def aggregate(epochs):
    ctx = prepare_context(epochs, need_model=False)
    runs, missing = [], []
    for f in FRACTIONS:
        for s in SEEDS:
            rj = OUT_ROOT / cell_name(f, s) / "result.json"
            if not rj.is_file():
                missing.append("%s: no result" % cell_name(f, s))
                continue
            r = json.loads(rj.read_text())
            diff = first_difference(r.get("identity", {}), identity_for(ctx["common"], f, s))
            if diff is not None:
                missing.append("%s: identity differs at %s" % (cell_name(f, s), diff))
                continue
            runs.append(r)
    if missing:
        refuse("REFUSED spike-grid-incomplete: " + "; ".join(missing), EXIT_GRID_INCOMPLETE)
    per_fraction, selected, detail, binding = apply_rules(runs)
    attempts = len(EVAL_ATTEMPTS) + sum(int(r.get("eval_open_attempts", 0)) for r in runs)
    print(json.dumps({
        "slice_fraction_selected": selected, "clamp_binding": binding,
        "outcome": "clamp_binding" if binding else "clamp_not_binding", "clamp_rule_detail": detail,
        "per_fraction": per_fraction, "eval_jsonl_opened": attempts > 0, "eval_open_attempts": attempts,
        "devices": sorted({r["device_used"] for r in runs}), "epochs": int(epochs),
        "first_run_started_at": min(int(r["started_at"]) for r in runs),
        "identity_current": ctx["common"],
        "data": {"holdout": dict(zip(ctx["labels"], class_counts(ctx["holdout"], ctx["k"]))),
                 "spike_train": dict(zip(ctx["labels"], class_counts(ctx["spike_train"], ctx["k"])))},
        "runs": runs}, sort_keys=True), flush=True)


# ------------------------------------------------------------------------------------------ guard selftest

def guard_selftest():
    refused = 0
    for how, fn in (("absolute", lambda: open(EVAL_ABS, "rb")),
                    ("relative", lambda: open(os.path.relpath(EVAL_ABS), "rb")),
                    ("os.open", lambda: os.open(EVAL_ABS, os.O_RDONLY)),
                    ("pathlib", lambda: Path(EVAL_ABS).read_bytes())):
        try:
            h = fn()
            if hasattr(h, "close"):
                h.close()
            print("GUARD LEAK: %s open of eval.jsonl succeeded" % how)
        except PermissionError as e:
            if "REFUSED spike-eval-guard" in str(e):
                refused += 1
    with open(DATA_SRC / "task.json", "rb"):
        pass                                           # the guard must not refuse the other files
    print("REFUSED spike-eval-guard: %d of 4 open paths refused; attempts counted %d" % (refused, len(EVAL_ATTEMPTS)))
    sys.exit(0 if refused == 4 and len(EVAL_ATTEMPTS) == 4 else 1)


# ------------------------------------------------------------------------------------------ main

def main():
    ap = argparse.ArgumentParser(description="spike 032: a measurement, not a gate run")
    ap.add_argument("--data", default=str(DATA_SRC.relative_to(REPO)), help="the source data dir (fixed to motivation-v1)")
    ap.add_argument("--fraction", type=float, default=0.25)
    ap.add_argument("--seeds", default="13,17,23")
    ap.add_argument("--epochs", type=int)
    ap.add_argument("--identity", action="store_true")
    ap.add_argument("--grid", action="store_true")
    ap.add_argument("--aggregate", action="store_true")
    ap.add_argument("--guard-selftest", action="store_true")
    a = ap.parse_args()
    if (REPO / a.data).resolve() != DATA_SRC.resolve():
        refuse("REFUSED spike-data: spike 032 reads %s only" % DATA_SRC.relative_to(REPO))
    if a.guard_selftest:
        guard_selftest()
    if a.epochs is None:
        refuse("REFUSED arguments: --epochs is required (from 12-PROBE-EVIDENCE.json epochs_declared)")
    seeds = [int(s) for s in a.seeds.split(",")]
    if a.identity:
        ctx = prepare_context(a.epochs, need_model=False)
        print(json.dumps(identity_for(ctx["common"], a.fraction, seeds[0]), sort_keys=True), flush=True)
        return 0
    if a.aggregate:
        aggregate(a.epochs)
        return 0
    cells = [(f, s) for f in FRACTIONS for s in SEEDS] if a.grid else [(a.fraction, s) for s in seeds]
    bad = run_cells(cells, a.epochs)
    log("GRID DONE cells=%d not_ok=%d" % (len(cells), bad))
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
