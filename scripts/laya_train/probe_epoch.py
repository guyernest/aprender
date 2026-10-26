"""The one-epoch probe (phase 10, D-10): a MEASUREMENT, not a gate run.

    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/probe_epoch.py \
        --data DIR --out FILE [--dry-run] [--guard DIR]
    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/probe_epoch.py \
        --host-load [--ps-file PATH]
    (or: just laya-probe-epoch <data dir> <evidence.json> [--dry-run])

What it answers, before the hours-long declared run: what does ONE training step cost on this host at the
real row length of the approved task, and so how many epochs (D-08) and how many hours (D-18)?

What it reuses UNCHANGED (nothing is re-implemented, so the number is the production path's number):
  * data.py: task.json / train.jsonl / eval.jsonl validation, the overlap refusal and the contract's seed-13
    group-disjoint calibration split -- the call sequence of train.main step 1, call for call;
  * contract.py: every value (batch size, the calibration constants, the epoch rule, the early-stopping block);
  * train.Base (the contract's pinned base), train.request_device, train.train_seed, train.calibration_logits.

What it does:
  * trains seed 13 for ONE epoch with `fixed_epochs` on exactly the fit rows train.py would train on (the
    calibration slice removed), through train.train_seed, on train.request_device(None);
  * reads the device BACK from train_seed's info["device_used"] and the DEVICE log line, never from the request;
  * times one calibration-monitor pass (calibration_logits + gate.calibration_monitor over the seed-13 slice),
    which train_seed skips under fixed_epochs and the production run pays once per evaluated epoch;
  * records the built, raw-post and retained-post token lengths of the fit rows and how many rows the 512-token
    row right-truncates;
  * samples the host every 15 s for competing builds or tests (host_load), so a contended window is a
    recorded fact, never silently trusted.

What it writes: ONLY the evidence JSON named by --out. No run dir, no recipe, no checkpoint. It refuses
(REFUSED run-dir-exists, exit 2) once the declared run dir (--guard, default models/decide/laya-antisemitism-v1)
exists: the probe never follows a declared run.

Seed (13) and epochs (1) are fixed; there are no flags for them. Exit codes: 0 measured / dry-run OK / host
snapshot; 2 refused (including REFUSED probe-oom, which still writes an evidence record with outcome "oom").
Logs carry counts, hashes and timings only -- never input text.

--dry-run and --host-load are torch-free: `import train` happens inside the real-run function.
"""
import argparse
import datetime
import hashlib
import json
import math
import os
import platform
import subprocess
import sys
import threading
import time
import traceback
from pathlib import Path
from types import SimpleNamespace

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import contract  # noqa: E402
import data  # noqa: E402

EXIT_OK, EXIT_REFUSED = 0, 2
SEED, EPOCHS, STOPPING = 13, 1, "fixed_epochs"
GUARD_DEFAULT = "models/decide/laya-antisemitism-v1"
SAMPLE_EVERY_S = 15
THRESHOLD_S_PER_STEP = 3.0
EPOCHS_RULE = "4 unless s_per_step <= 1.0, then 6 (RESEARCH Q4)"
FAST_S_PER_STEP = 1.0
MOTIVATION_MAX_HOURS = 6.0
MOTIVATION_RULE = ("12 under early stopping, the contract's epochs_above_16_max (spike 027 es12; at about 220 fit rows "
                   "time is not the constraint), unless the projected three-seed training hours at E = 12 exceed 6, "
                   "then the largest E in [4, 12] projected at most 6")
TAIL_BUDGET_H, PACK_VERIFY_BUDGET_H = 1.0, 0.7
DOWNSTREAM_VERIFY_RUNS, DOWNSTREAM_VERIFY_MIN_EACH = 5, 18
SEEDS_IN_PRODUCTION = 3
ROW_MAX = 512
# Build and test tools whose presence on the host during the window inflates a step time. An exact BASENAME
# match, so `clang-format` is not `clang`.
COMPETING = ("cargo", "rustc", "rustdoc", "clippy-driver", "cargo-clippy", "cargo-nextest", "cargo-zigbuild",
             "zig", "clang", "cc1", "ld")


def _declared_antisemitism(s_per_step, hours):
    """Phase 10 (RESEARCH Q4): 4, or 6 when a step is fast."""
    return 6 if s_per_step <= FAST_S_PER_STEP else 4


def _declared_motivation(s_per_step, hours):
    """Phase 12 (D-17): 12 under early stopping unless the three-seed hours at E = 12 exceed the budget, then the
    largest E in [4, 12] within it (4 when none is: the contract's lower bound)."""
    if hours["12"] <= MOTIVATION_MAX_HOURS:
        return 12
    return max([e for e in range(4, 13) if hours[str(e)] <= MOTIVATION_MAX_HOURS] or [4])


# id -> (rule text, function (s_per_step, hours-by-E) -> epochs, the declared run dir the probe refuses to follow)
EPOCHS_RULES = {
    "antisemitism-v1": (EPOCHS_RULE, _declared_antisemitism, "models/decide/laya-antisemitism-v1"),
    "motivation-v1": (MOTIVATION_RULE, _declared_motivation, "models/decide/laya-motivation-v1"),
}
RULE_DEFAULT = "antisemitism-v1"


def log(msg):
    print(msg, flush=True)


def refuse(msg):
    print(msg if msg.startswith("REFUSED") else "REFUSED " + msg, file=sys.stderr, flush=True)
    sys.exit(EXIT_REFUSED)


# ------------------------------------------------------------------------------------------ host load

def competing_names(ps_text):
    """Sorted distinct basenames of the COMPETING executables in `ps -axo comm=` output (one path per line)."""
    seen = set()
    for line in ps_text.splitlines():
        line = line.strip()
        if not line:
            continue
        base = os.path.basename(line)
        if base in COMPETING:
            seen.add(base)
    return sorted(seen)


def host_snapshot(ps_text=None):
    """{competing, load1, ncpu}; `ps_text` replaces the live `/bin/ps -axo comm=` call (the case table)."""
    if ps_text is None:
        ps_text = subprocess.run(["/bin/ps", "-axo", "comm="], capture_output=True, text=True, check=False).stdout
    return {"competing": competing_names(ps_text), "load1": os.getloadavg()[0], "ncpu": os.cpu_count()}


class HostSampler:
    """A daemon thread taking a host_snapshot every SAMPLE_EVERY_S seconds; the first one is taken at start()."""

    def __init__(self):
        self.snaps = []
        self._stop = threading.Event()
        self._lock = threading.Lock()
        self._thread = threading.Thread(target=self._run, daemon=True)

    def _take(self):
        s = host_snapshot()
        with self._lock:
            self.snaps.append(s)

    def _run(self):
        while not self._stop.wait(SAMPLE_EVERY_S):
            self._take()

    def start(self):
        self._take()
        self._thread.start()

    def finish(self):
        """Stop, take the closing snapshot, return the evidence `host_load` block."""
        self._stop.set()
        self._thread.join(timeout=5)
        self._take()
        with self._lock:
            snaps = list(self.snaps)
        busy = [s for s in snaps if s["competing"]]
        return {"sampled_every_s": SAMPLE_EVERY_S, "samples": len(snaps), "competing_samples": len(busy),
                "competing_names": sorted({n for s in busy for n in s["competing"]}),
                "load1_start": snaps[0]["load1"], "load1_end": snaps[-1]["load1"], "ncpu": snaps[0]["ncpu"],
                "contended": len(busy) > 0}


# ------------------------------------------------------------------------------------------ validation

def sha256_file(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def validate(data_dir):
    """train.main step 1, call for call (minus the shift probe and the seed/stopping arguments the probe fixes)."""
    try:
        contract.check_numbers()
        seed = contract.declared_seed()
        task = data.load_task(data_dir / "task.json")
        train_rows = data.load_rows(data_dir / "train.jsonl", task, "train")
        eval_rows = data.load_rows(data_dir / "eval.jsonl", task, "eval")
        data.refuse_overlap(train_rows, eval_rows)
        k = len(task["labels"])
        fit_ids, calib_ids, _, _ = data.calibration_split(
            train_rows, contract.constant("calibration_slice_fraction", "float"),
            contract.constant("calibration_slice_min_per_class", "int"), seed, k)
        shots_per_class = max(data.class_counts(train_rows, k))
        contract.resolve_epochs("production", shots_per_class, 4)       # the production rule applies to this data
        if seed != SEED:
            raise contract.RecipeError("REFUSED seed: the contract's declared seed is %d, the probe measures %d"
                                       % (seed, SEED))
    except (data.DataError, contract.RecipeError, contract.ContractValueError) as e:
        refuse(str(e))
    sha = {name: sha256_file(data_dir / name) for name in ("task.json", "train.jsonl", "eval.jsonl")}
    return {"task": task, "train_rows": train_rows, "k": k, "fit_ids": fit_ids, "calib_ids": calib_ids,
            "shots_per_class": shots_per_class, "sha": sha}


def steps_planned(n_fit):
    return math.ceil(n_fit / contract.recipe_number("batch_size", "int"))


# ------------------------------------------------------------------------------------------ measurement

def pct(values):
    import numpy as np
    a = np.asarray(values, dtype=np.float64)
    return {"p50": round(float(np.percentile(a, 50)), 1), "p95": round(float(np.percentile(a, 95)), 1),
            "p99": round(float(np.percentile(a, 99)), 1), "max": int(a.max()), "mean": round(float(a.mean()), 2)}


def scrub(text):
    """No home path in any record (the verify checks for it)."""
    home = os.path.expanduser("~")
    return text.replace(home, "~") if home and home != "~" else text


def git_head():
    r = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True, check=False)
    return r.stdout.strip() or None


def common_record(args, v, requested, torch_version, host_load):
    return {
        "schema": "laya-probe-evidence-v1",
        "rule": args.rule,
        "measured_on": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "commit": git_head(),
        "host": {"machine": platform.machine(), "macos": platform.mac_ver()[0] or None,
                 "python": platform.python_version()},
        "data_dir": scrub(str(args.data)),
        "data_sha256": v["sha"],
        "task_json_sha256": v["sha"]["task.json"],
        "seed": SEED, "epochs": EPOCHS, "stopping": STOPPING,
        "device_requested": requested, "torch_version": torch_version,
        "host_load": host_load,
    }


def is_oom(exc):
    msg = str(exc).lower()
    return isinstance(exc, MemoryError) or "out of memory" in msg or "outofmemory" in type(exc).__name__.lower()


def write_evidence(path, obj):
    Path(path).write_text(json.dumps(obj, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def measure(args, v):
    """The real run: one epoch through train.train_seed, then the monitor and row-length records."""
    sampler = HostSampler()
    sampler.start()                                    # before the base load: the first snapshot sees what is already running
    import train                                       # noqa: PLC0415 -- torch, only in the real run
    import torch                                       # noqa: PLC0415
    import gate                                        # noqa: PLC0415
    import numpy as np                                 # noqa: PLC0415
    from laya.common import encode_text, serialize_state  # noqa: PLC0415

    task, k = v["task"], v["k"]
    question = data.laya_question(task)
    fit_rows = [v["train_rows"][i] for i in v["fit_ids"]]
    calib_rows = [v["train_rows"][i] for i in v["calib_ids"]]
    base = train.Base(SimpleNamespace(variant="production", base=None, base_sha256=None))
    requested = train.request_device(None)
    try:
        agent, info = train.train_seed(SEED, base, requested, question, fit_rows, k, epochs=EPOCHS,
                                       stopping=STOPPING)
    except BaseException as exc:                       # noqa: BLE001 -- an OOM is a checkpoint, anything else re-raises
        if not is_oom(exc):
            sampler.finish()
            raise
        rec = common_record(args, v, requested, torch.__version__, sampler.finish())
        last = traceback.format_exc().strip().splitlines()[-1]
        rec.update({"outcome": "oom", "oom": True, "oom_last_line": scrub(last)[:400], "rc": EXIT_REFUSED})
        write_evidence(args.out, rec)
        refuse("REFUSED probe-oom: %s" % scrub(last)[:200])

    s_per_step = info["train_seconds"] / info["steps"]
    # -- the per-epoch monitor the production run pays and train_seed skips under fixed_epochs --------------
    internal = {"q": train.Agent._to_internal(question)}
    calib_items = [agent._encode_state(t, ["q"], internal)[0] for t, _ in calib_rows]      # outside the timer
    yc = np.array([y for _, y in calib_rows], dtype=np.int64)
    t_min = contract.constant("calibration_temp_min", "float")
    t_max = contract.constant("calibration_temp_max", "float")
    bs = contract.recipe_number("batch_size", "int")
    t0 = time.time()
    zc = train.calibration_logits(agent.model, calib_items, k, agent.tok.pad_token_id, agent.device, bs)
    m, t_star = gate.calibration_monitor(zc, yc, t_min, t_max)
    monitor_s = time.time() - t0
    log("MONITOR seed=%d calibration_rows=%d seconds=%.1f nll=%.6f t_star=%s"
        % (SEED, len(calib_rows), monitor_s, m, "%.4f" % t_star if t_star is not None else "nan"))
    # -- row lengths: built (the same encode path train_seed uses), raw post, retained post ------------------
    built, raw = [], []
    mask = agent.tok.mask_token
    for t, _ in fit_rows:
        built.append(len(agent._encode_state(t, ["q"], internal)[0]["ids"]))
        raw.append(len(encode_text(agent.tok, serialize_state(t).replace(mask, " "),
                                   add_special_tokens=False)["input_ids"]))
    p_len = len(agent._encode_state("", ["q"], internal)[0]["ids"])
    retained = [b - p_len for b in built]
    room = ROW_MAX - p_len
    truncated = [r for r in raw if r > room]
    sampler_block = sampler.finish()

    rec = common_record(args, v, requested, torch.__version__, sampler_block)
    rec.update(post_measure_record(info, s_per_step, monitor_s, len(calib_rows), pct(built), pct(raw), pct(retained),
                                   {"count": len(truncated), "share": round(len(truncated) / len(fit_rows), 6),
                                    "max_lost_tokens": max([r - room for r in truncated], default=0)}, p_len,
                                   args.rule))
    write_evidence(args.out, rec)
    log("PROBE OK device_used=%s s_per_step=%.4f steps=%d P=%d" % (info["device_used"], s_per_step, info["steps"], p_len))
    return rec


def post_measure_record(info, s_per_step, monitor_s, n_calib, row_tokens, post_raw, post_retained, rows_truncated, p_len,
                        rule=RULE_DEFAULT):
    """The `measured` outcome's fields; pure arithmetic over the measurement (also the place the rule lives)."""
    steps_pl = info["steps_planned"]
    hours = {str(e): round((s_per_step * steps_pl * e + monitor_s * e) * 3 / 3600, 2) for e in range(4, 13)}
    h4, h6 = hours["4"], hours["6"]
    es = contract.early_stopping_decl()
    earliest = es["first_candidate_epoch"] + es["patience_epochs"]
    rule_text, rule_fn, _ = EPOCHS_RULES[rule]
    declared = rule_fn(s_per_step, hours)
    can_end = declared > earliest
    exceeds = s_per_step > THRESHOLD_S_PER_STEP
    used = str(info["device_used"])
    cpu = used.startswith("cpu")
    if cpu:
        flag = "CPU: flagged, re-priced"
    elif used.startswith("mps"):
        flag = "MPS as expected"
    else:
        flag = "%s: not the cpu; the contract's device_order picked it" % used
    downstream_h = round(DOWNSTREAM_VERIFY_RUNS * DOWNSTREAM_VERIFY_MIN_EACH / 60, 2)
    to_artifact = round(hours[str(declared)] + TAIL_BUDGET_H + PACK_VERIFY_BUDGET_H, 2)
    to_live = round(to_artifact + downstream_h, 2)
    if rule == "motivation-v1":
        es_note = ("Early stopping (D-08) is active at E = %d: its earliest stop is epoch %d (first_candidate_epoch %d + "
                   "patience_epochs %d), so a seed can end before E; the training hours above are the upper bound "
                   "(every seed runs all %d epochs)." % (declared, earliest, es["first_candidate_epoch"],
                                                          es["patience_epochs"], declared))
        date_note = ""
        reasoning = ("Rule: %s. Measured s_per_step %s s, monitor_s_per_epoch %s s. Three-seed training hours "
                     "including the per-epoch calibration monitor: %s h at E = 12, %s h at E = %d declared (budget %s h). "
                     "Plus the scoring tail %s h and the pack/verify budget %s h: %s h to the artifact, %s h to live "
                     "with the %d downstream verify runs of %d min each (%s h). %s"
                     % (rule_text, s_per_step, round(monitor_s, 3), str(hours["12"]), str(hours[str(declared)]),
                        declared, MOTIVATION_MAX_HOURS, TAIL_BUDGET_H, PACK_VERIFY_BUDGET_H, to_artifact, to_live,
                        DOWNSTREAM_VERIFY_RUNS, DOWNSTREAM_VERIFY_MIN_EACH, downstream_h, es_note))
    else:
        if declared == 4:
            es_note = ("Early stopping (D-08) cannot fire at E = 4: its earliest stop is first_candidate_epoch %d + "
                       "patience_epochs %d = epoch %d = E, so every seed runs all 4 epochs and the rule acts only as "
                       "best-epoch restore (restore: best). E = 6 is not taken because s_per_step exceeds %s s."
                       % (es["first_candidate_epoch"], es["patience_epochs"], earliest, FAST_S_PER_STEP))
        else:
            es_note = ("s_per_step is at most %s s, which admits E = 6 (RESEARCH Q4): at E = 6 early stopping, whose "
                       "earliest stop is epoch %d, can end a seed at epoch %d or %d instead of running all 6."
                       % (FAST_S_PER_STEP, earliest, earliest, earliest + 1))
        now = datetime.datetime.now()
        live = {e: round(hours[str(e)] + TAIL_BUDGET_H + PACK_VERIFY_BUDGET_H + downstream_h, 2) for e in (4, 6)}
        on = {e: (now + datetime.timedelta(hours=live[e])).strftime("%Y-%m-%d") for e in (4, 6)}
        date_note = ("Starting now, live in %s h (%s) at E = 4 against %s h (%s) at E = 6, versus the 2026-10-29 date."
                     % (live[4], on[4], live[6], on[6]))
        reasoning = ("Rule: %s. Measured s_per_step %s s (threshold %s s), monitor_s_per_epoch %s s. Three-seed training "
                     "hours including the per-epoch calibration monitor: %s h at E = 4 and %s h at E = 6 (E = %d declared). "
                     "Plus the scoring tail %s h and the pack/verify budget %s h: %s h to the artifact, %s h to live with the "
                     "%d downstream verify runs of %d min each (%s h). %s %s"
                     % (rule_text, s_per_step, THRESHOLD_S_PER_STEP, round(monitor_s, 3), str(h4), str(h6), declared,
                        TAIL_BUDGET_H, PACK_VERIFY_BUDGET_H, to_artifact, to_live, DOWNSTREAM_VERIFY_RUNS,
                        DOWNSTREAM_VERIFY_MIN_EACH, downstream_h, date_note, es_note))
    repriced = None
    if exceeds or cpu:
        # one gate run to live plus the one budgeted re-run (to the artifact), from today
        days = math.ceil((to_live + to_artifact) / 24.0)
        repriced = (datetime.date.today() + datetime.timedelta(days=days)).isoformat()
    return {
        "outcome": "measured", "device_used": used, "device_flag": flag,
        "fit_rows": info["fit_rows"], "calibration_rows": n_calib,
        "steps": info["steps"], "steps_planned": steps_pl, "train_seconds": info["train_seconds"],
        "s_per_step": s_per_step, "monitor_s_per_epoch": monitor_s,
        "ce_first": info["ce_first"], "ce_last": info["ce_last"],
        "row_tokens": row_tokens, "post_tokens_raw": post_raw, "post_tokens_retained": post_retained,
        "rows_truncated": rows_truncated, "P": p_len,
        "threshold_s_per_step": THRESHOLD_S_PER_STEP, "exceeds_threshold": exceeds,
        "projected_training_hours": hours, "projected_training_hours_basis":
            "(s_per_step x steps_planned x E + monitor_s_per_epoch x E) x 3 seeds / 3600",
        "hours_at_E4": h4, "hours_at_E6": h6,
        "tail_budget_hours": TAIL_BUDGET_H, "pack_verify_budget_hours": PACK_VERIFY_BUDGET_H,
        "downstream_verify_runs": DOWNSTREAM_VERIFY_RUNS, "downstream_verify_budget_hours": downstream_h,
        "epochs_rule": rule_text, "epochs_declared": declared,
        "earliest_stop_epoch": earliest, "early_stopping_can_end_early": can_end,
        "epochs_reasoning": reasoning,
        "expected_hours_to_artifact": to_artifact, "expected_hours_to_live": to_live,
        "repriced_ready_by": repriced,
    }


# ------------------------------------------------------------------------------------------ main

def parse_args(argv=None):
    ap = argparse.ArgumentParser(description="the D-10 one-epoch probe: a measurement, not a gate run")
    ap.add_argument("--data", help="a Laya data dir (task.json + train.jsonl + eval.jsonl)")
    ap.add_argument("--out", help="the evidence JSON to write (required unless --dry-run / --host-load)")
    ap.add_argument("--dry-run", action="store_true", help="validate and print the planned steps; load no model, write nothing")
    ap.add_argument("--rule", choices=sorted(EPOCHS_RULES), default=RULE_DEFAULT,
                    help="the epochs rule table entry (default %(default)s: Phase 10's rule, unchanged)")
    ap.add_argument("--guard", default=None,
                    help="refuse when this run dir exists (default: the rule's declared run dir)")
    ap.add_argument("--host-load", action="store_true", help="print one host snapshot JSON line and exit")
    ap.add_argument("--ps-file", help="with --host-load: read ps output from this file instead of running ps")
    return ap.parse_args(argv)


def main(argv=None):
    args = parse_args(argv)
    if args.guard is None:
        args.guard = EPOCHS_RULES[args.rule][2]
    if args.host_load:
        text = Path(args.ps_file).read_text() if args.ps_file else None
        print(json.dumps(host_snapshot(text)), flush=True)
        return EXIT_OK
    if not args.data:
        refuse("REFUSED arguments: --data is required")
    if not args.dry_run and not args.out:
        refuse("REFUSED arguments: --out is required unless --dry-run")
    if Path(args.guard).exists():
        refuse("REFUSED run-dir-exists: %s exists; the probe never follows a declared run" % args.guard)
    data_dir = Path(args.data)
    v = validate(data_dir)
    n_fit = len(v["fit_ids"])
    if args.dry_run:
        log("PROBE DRY-RUN OK fit_rows=%d calibration_rows=%d steps_planned=%d shots_per_class=%d task_json_sha256=%s"
            % (n_fit, len(v["calib_ids"]), steps_planned(n_fit), v["shots_per_class"], v["sha"]["task.json"]))
        return EXIT_OK
    log("PROBE START fit_rows=%d calibration_rows=%d steps_planned=%d task_json_sha256=%s"
        % (n_fit, len(v["calib_ids"]), steps_planned(n_fit), v["sha"]["task.json"]))
    measure(args, v)
    return EXIT_OK


if __name__ == "__main__":
    sys.exit(main())
