"""The tracer's thin slice: a REAL train -> F16 save -> complete dir -> reload -> calibrate -> gate lifecycle
on the committed tiny Laya checkpoint, in seconds on CPU (just laya-train-lifecycle).

    uv run --project scripts/laya_train --frozen python scripts/laya_train/lifecycle.py

Runs train.py as a subprocess (the exact CLI a user runs) with --variant synthetic-fixture on
crates/aprender-decide/tests/fixtures/laya_tiny/{checkpoint,data} into a fresh temp dir (deleted
afterwards), then asserts:

  * the out dir holds every laya-finetune-gate-v1 run_dir_layout file;
  * CHECKPOINT COMPLETE was logged before SCORING START (rl_agent_config.json existed before the
    first reload), and RECIPE WRITTEN before both;
  * the reload left every checkpoint sha256 unchanged (train.py refuses otherwise; re-checked here by
    reloading once more and hashing);
  * recipe.json is variant synthetic-fixture with the tiny checkpoint's recorded sha256, and its
    sha256 is the gate report's recipe_id; the gate report parses in the contract schema;
  * the digest MAPPING is Laya's real API: the tiny base loads with {"model.safetensors": <its sha>}
    and a wrong sha is refused by Laya's own ValueError;
  * forcing --device cpu is recorded as device_used "cpu" / device_is_cpu true;
  * BOTH declared stopping rules run: early_stopping (the contract default, max 3 epochs here) logs
    EPOCH lines and a STOP line before CHECKPOINT COMPLETE, its recipe.json carries the contract's
    `early_stopping` object and its rl_agent_config.json `training.stopping` record names a best epoch
    in [first_candidate_epoch, epochs_run]; fixed_epochs (1 epoch) logs no STOP and its recipe.json has
    no `early_stopping` key (the 1.0.0 bytes); in both, mtime recipe.json <= checkpoint/model.safetensors
    <= eval-probs.json (no eval probability before the checkpoint is fixed).

The gate outcome itself is not asserted (a 1-3 epoch tiny model is not expected to pass): exit 0 or 3.
Prints LIFECYCLE OK.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import warnings
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import contract  # noqa: E402
from common import sha256_bytes, sha256_file, tree_sha256  # noqa: E402

REPO = contract.REPO
TINY = REPO / "crates" / "aprender-decide" / "tests" / "fixtures" / "laya_tiny"


def fail(msg):
    print("LIFECYCLE FAILED: " + msg, file=sys.stderr, flush=True)
    sys.exit(1)


def main():
    tiny_sha = json.loads((TINY / "recipe.json").read_text())["base"]["sha256"]
    if sha256_file(TINY / "checkpoint" / "model.safetensors") != tiny_sha:
        fail("the tiny checkpoint's model.safetensors does not match its recorded sha256")

    # The digest mapping against the REAL API (T-08-08-01): right sha loads, wrong sha is refused.
    from laya import Agent
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", RuntimeWarning)
        Agent(str(TINY / "checkpoint"), device="cpu", expected_sha256={"model.safetensors": tiny_sha})
        wrong = ("0" if tiny_sha[0] != "0" else "1") + tiny_sha[1:]
        try:
            Agent(str(TINY / "checkpoint"), device="cpu", expected_sha256={"model.safetensors": wrong})
            fail("Laya loaded the tiny base under a WRONG sha256")
        except ValueError as e:
            if "SHA-256 mismatch" not in str(e):
                fail("wrong-sha load raised an unexpected ValueError: %s" % e)
    print("digest mapping: right sha loads, wrong sha refused by Laya (ValueError: SHA-256 mismatch)")

    tmp = Path(tempfile.mkdtemp(prefix="laya-lifecycle-"))
    try:
        for stopping, epochs in (("early_stopping", 3), ("fixed_epochs", 1)):
            run_one(tmp / stopping, tiny_sha, stopping, epochs, Agent)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print("LIFECYCLE OK")


def run_one(out, tiny_sha, stopping, epochs, Agent):
    """One train.py run in `out`, then every assertion of the module docstring."""
    cmd = [sys.executable, str(HERE / "train.py"), "--data", str(TINY / "data"), "--out", str(out),
           "--variant", "synthetic-fixture", "--base", str(TINY / "checkpoint"), "--base-sha256", tiny_sha,
           "--epochs", str(epochs), "--stopping", stopping, "--device", "cpu"]
    proc = subprocess.run(cmd, capture_output=True, text=True, env=dict(os.environ))
    sys.stdout.write(proc.stdout)
    sys.stderr.write(proc.stderr)
    if proc.returncode not in (0, 3):
        fail("train.py exited %d" % proc.returncode)
    lines = proc.stdout.splitlines()

    def first(marker):
        idx = [i for i, ln in enumerate(lines) if ln.startswith(marker)]
        if not idx:
            fail("train.py never logged %r" % marker)
        return idx[0]
    if not first("RECIPE WRITTEN") < first("CHECKPOINT COMPLETE") < first("SCORING START"):
        fail("log order is not RECIPE WRITTEN < CHECKPOINT COMPLETE < SCORING START")
    has_stop = any(ln.startswith("STOP ") for ln in lines)
    if stopping == "early_stopping":
        if not first("RECIPE WRITTEN") < first("EPOCH ") < first("STOP ") < first("CHECKPOINT COMPLETE"):
            fail("early_stopping log order is not RECIPE WRITTEN < EPOCH < STOP < CHECKPOINT COMPLETE")
    elif has_stop:
        fail("fixed_epochs logged a STOP line")
    for rel in contract.run_dir_files():
        if not (out / rel).is_file():
            fail("run dir is missing %s" % rel)
    recipe_bytes = (out / "recipe.json").read_bytes()
    recipe = json.loads(recipe_bytes)
    if recipe["variant"] != "synthetic-fixture" or recipe["base"]["sha256"] != tiny_sha:
        fail("recipe.json is not the synthetic-fixture variant on the tiny base")
    want_keys = [k for k in contract.gate_contract()["recipe_json_schema"]
                 if k != "early_stopping" or stopping == "early_stopping"]
    if sorted(recipe) != sorted(want_keys):
        fail("recipe.json keys %s differ from recipe_json_schema for %s" % (sorted(recipe), stopping))
    if stopping == "early_stopping" and recipe["early_stopping"] != contract.early_stopping_decl():
        fail("recipe.json early_stopping differs from the contract block")
    if recipe["epochs"] != epochs:
        fail("recipe.json epochs %s != %s" % (recipe["epochs"], epochs))
    training = json.loads((out / "checkpoint" / "rl_agent_config.json").read_text())["training"]
    if stopping == "early_stopping":
        st = training.get("stopping") or {}
        decl = contract.early_stopping_decl()
        if not (decl["first_candidate_epoch"] <= st.get("best_epoch", -1) <= st.get("epochs_run", -2) <= epochs):
            fail("training.stopping record %s is not a best epoch within the run" % st)
        if [e["epoch"] for e in st["per_epoch"]] != list(range(1, st["epochs_run"] + 1)):
            fail("training.stopping per_epoch does not cover every run epoch")
    elif "stopping" in training:
        fail("fixed_epochs wrote a training.stopping record")
    t_recipe, t_ck, t_eval = ((out / rel).stat().st_mtime_ns for rel in
                              ("recipe.json", "checkpoint/model.safetensors", "eval-probs.json"))
    if not (t_recipe <= t_ck <= t_eval):
        fail("mtime order recipe.json <= model.safetensors <= eval-probs.json violated")
    report = json.loads((out / "gate-report.json").read_text())
    if sorted(report) != sorted(k for k in contract.gate_contract()["gate_report_schema"] if k != "f_avg_rule"):
        fail("gate-report.json keys %s differ from gate_report_schema" % sorted(report))
    if report["recipe_id"] != sha256_bytes(recipe_bytes):
        fail("gate report recipe_id is not the sha256 of recipe.json")
    if report["thresholds"] != contract.thresholds():
        fail("gate report thresholds differ from the contract")
    if report["device_used"] != "cpu" or report["device_is_cpu"] is not True:
        fail("forced --device cpu was not recorded as device_used cpu / device_is_cpu true")
    if report["seeds"] != {"declared": 13, "n": 1, "label": "single seed"}:
        fail("gate report seeds block %s" % report["seeds"])
    s = report["calibration"]["slice_ids"]
    if s != sorted(set(s)) or len(s) != report["calibration"]["slice_size"]:
        fail("calibration.slice_ids is not a sorted, duplicate-free list of slice_size")
    for name, key in (("eval-probs.json", "eval_probs_sha256"), ("zero-shot-probs.json", "zero_shot_probs_sha256"),
                      ("probes.json", "probes_sha256")):
        if sha256_file(out / name) != report[key]:
            fail("%s sha256 differs from the report's %s" % (name, key))
    before = tree_sha256(out / "checkpoint")
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", RuntimeWarning)
        Agent(str(out / "checkpoint"), device="cpu", expected_sha256={"model.safetensors": before["model.safetensors"]})
    if tree_sha256(out / "checkpoint") != before:
        fail("a further reload changed a checkpoint file")
    print("lifecycle[%s]: exit %d, %d checkpoint files unchanged across reload, gate pass=%s"
          % (stopping, proc.returncode, len(before), report["pass"]))


if __name__ == "__main__":
    main()
