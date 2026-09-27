"""The ONE place the Laya back office reads contracts/laya-finetune-gate-v1.yaml (D-04, D-07).

Nothing downstream writes a threshold, a recipe value, a seed or the base pin as a literal: train.py,
gate.py and data.py all read them from here, and here reads them from the committed contract at run
time. A threshold chosen after seeing a result is not a gate (D-07), so the values live where
`pv diff` sees every change.

No torch import: gate.py and data.py self-tests run with numpy + pyyaml only.
"""
from pathlib import Path

import yaml

REPO = Path(__file__).resolve().parents[2]
GATE_CONTRACT = REPO / "contracts" / "laya-finetune-gate-v1.yaml"
DECIDE_CONTRACT = REPO / "contracts" / "decide-apr-v1.yaml"

_CACHE = {}


def _load(path):
    key = str(path)
    if key not in _CACHE:
        _CACHE[key] = yaml.safe_load(Path(path).read_text())
    return _CACHE[key]


def load_yaml(path):
    """Any committed contract, parsed once (e.g. laya-parity-v1 tolerances for the lifecycle re-score)."""
    return _load(path)


def gate_contract():
    """The parsed laya-finetune-gate-v1 contract: `constants`, `seed_policy`, `recipe`, `base`, `demo`,
    `device_order`, `run_dir_layout` and the four schema blocks, exactly as committed."""
    c = _load(GATE_CONTRACT)
    for key in ("constants", "seed_policy", "recipe", "early_stopping", "base", "demo", "device_order",
                "run_dir_layout", "recipe_json_schema", "gate_report_schema", "eval_probs_schema", "probes_json_schema"):
        if key not in c:
            raise KeyError("%s has no top-level %r block" % (GATE_CONTRACT.relative_to(REPO), key))
    return c


def decide_contract():
    return _load(DECIDE_CONTRACT)


def constants():
    return gate_contract()["constants"]


def thresholds():
    """The gate thresholds in the gate-report `thresholds` shape, read from the contract."""
    c = constants()
    return {"min_macro_f1_margin": c["gate_min_macro_f1_margin"], "max_ece": c["gate_max_ece"],
            "ece_bins": int(c["ece_bins"])}


def recipe():
    return gate_contract()["recipe"]


def base():
    return gate_contract()["base"]


def seed_policy():
    return gate_contract()["seed_policy"]


def demo():
    return gate_contract()["demo"]


def device_order():
    return list(gate_contract()["device_order"])


def run_dir_files():
    """The run_dir_layout entries as relative paths (the prose after the first space dropped),
    without the conditional variance-report.json."""
    out = []
    for entry in gate_contract()["run_dir_layout"]["run_dir"]:
        path = str(entry).split(" ", 1)[0]
        if path != "variance-report.json":
            out.append(path)
    return out


class RecipeError(ValueError):
    """A refused recipe request; the message names the rule."""


def resolve_epochs(variant, shots_per_class, epochs_arg):
    """The epoch count per `recipe.epoch_rule` (D-04).

    production: <= 16 shots/class is FIXED to epochs_at_most_16_per_class (an --epochs is refused, not
    ignored); above 16 an --epochs in [epochs_above_16_min, epochs_above_16_max] is REQUIRED.
    synthetic-fixture: the caller's --epochs (required, 0 <= epochs <= epochs_above_16_max)."""
    r = recipe()
    fixed = int(r["epochs_at_most_16_per_class"])
    lo, hi = int(r["epochs_above_16_min"]), int(r["epochs_above_16_max"])
    if variant == "synthetic-fixture":
        if epochs_arg is None or not (0 <= int(epochs_arg) <= hi):
            raise RecipeError("REFUSED epochs: the synthetic-fixture variant needs --epochs in [0, %d]" % hi)
        return int(epochs_arg)
    if variant != "production":
        raise RecipeError("REFUSED variant: %r is neither production nor synthetic-fixture" % (variant,))
    if shots_per_class <= 16:
        if epochs_arg is not None:
            raise RecipeError("REFUSED epochs: at <= 16 shots/class the recipe fixes epochs to %d; "
                              "--epochs %s is not accepted" % (fixed, epochs_arg))
        return fixed
    if epochs_arg is None:
        raise RecipeError("REFUSED epochs: above 16 shots/class (%d) --epochs is required, in [%d, %d]"
                          % (shots_per_class, lo, hi))
    if not (lo <= int(epochs_arg) <= hi):
        raise RecipeError("REFUSED epochs: --epochs %s is outside [%d, %d] at %d shots/class"
                          % (epochs_arg, lo, hi, shots_per_class))
    return int(epochs_arg)


# The recipe.json `early_stopping` object: exactly these keys, copied from the contract block.
EARLY_STOPPING_KEYS = ("monitor", "mode", "eval_every_epochs", "first_candidate_epoch", "patience_epochs",
                       "min_delta", "restore", "tie_break")


def stopping_rules():
    return list(recipe()["stopping_rules"])


def stopping_default():
    return str(recipe()["stopping_default"])


def early_stopping_decl():
    """The recipe.json `early_stopping` object (laya-finetune-gate-v1 1.1.0 `early_stopping` block)."""
    es = gate_contract()["early_stopping"]
    out = {k: es[k] for k in EARLY_STOPPING_KEYS}
    for k in ("eval_every_epochs", "first_candidate_epoch", "patience_epochs"):
        out[k] = int(out[k])
    out["min_delta"] = float(out["min_delta"])
    return out


def resolve_stopping(arg):
    """`fixed_epochs` or `early_stopping` (the contract default when `arg` is None)."""
    rule = stopping_default() if arg is None else arg
    if rule not in stopping_rules():
        raise RecipeError("REFUSED stopping: %r is not one of %s" % (rule, stopping_rules()))
    return rule


def recipe_json(variant, shots_per_class, epochs, seed, base_block, stopping="fixed_epochs"):
    """The recipe.json object in `recipe_json_schema` order of keys (serialized sort_keys anyway).

    fixed_epochs carries no `early_stopping` key, so its bytes -- and recipe_id -- are exactly the
    1.0.0 recipe's; early_stopping adds the contract's object and `epochs` becomes the maximum."""
    r = recipe()
    out = {
        "variant": variant, "optimizer": r["optimizer"], "encoder_lr": r["encoder_lr"], "head_lr": r["head_lr"],
        "eta_min": r["eta_min"], "weight_decay": r["weight_decay"], "grad_clip": r["grad_clip"],
        "batch_size": int(r["batch_size"]), "proper_reward_w_sph": r["proper_reward_w_sph"],
        "proper_reward_w_rps": r["proper_reward_w_rps"], "schedule": "cosine",
        "shots_per_class": int(shots_per_class), "epochs": int(epochs), "seed": int(seed), "base": base_block,
    }
    if stopping == "early_stopping":
        out["early_stopping"] = early_stopping_decl()
    elif stopping != "fixed_epochs":
        raise RecipeError("REFUSED stopping: %r is not one of %s" % (stopping, stopping_rules()))
    return out


def production_base_block():
    b = base()
    return {"family": b["family"], "repo": b["repo"], "revision": b["revision"], "checkpoint": b["checkpoint"],
            "sha256": b["model_safetensors_sha256"]}


def resolve_seeds(n):
    """The seeds a run trains, per seed_policy (D-08): the first `n` of `variance_seeds` (default 1), the
    declared seed FIRST. Only the declared seed's checkpoint is ever kept; the rest report variance."""
    sp = seed_policy()
    pool = [int(s) for s in sp["variance_seeds"]]
    declared = int(sp["declared_seed"])
    if not pool or pool[0] != declared:
        raise RecipeError("REFUSED seeds: seed_policy.variance_seeds %s must start with the declared seed %d"
                          % (pool, declared))
    n = 1 if n is None else int(n)
    if not 1 <= n <= len(pool):
        raise RecipeError("REFUSED seeds: --seeds %d is outside [1, %d] (seed_policy.variance_seeds %s)"
                          % (n, len(pool), pool))
    return pool[:n]


def seeds_label(n):
    return "single seed" if n == 1 else "mean ± sd over %d seeds" % n


def probe_policy():
    d = decide_contract()
    pp = d["probe_policy"]
    task = {k: pp["probe_task"][k] for k in ("type", "instructions", "criteria")}
    return task, list(pp["inputs"]), int(d["constants"]["probe_max_row_tokens"])
