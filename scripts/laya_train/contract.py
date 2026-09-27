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
PARITY_CONTRACT = REPO / "contracts" / "laya-parity-v1.yaml"

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


def parity_contract():
    return _load(PARITY_CONTRACT)


def noise_policy():
    """(k, floor, ceiling) of the A1 re-score bar, read from laya-parity-v1 (plan 08-13):
    k = constants.pack_rescore_noise_k, floor = equations.pack_rescore_probs_abs.float_tolerance,
    ceiling = constants.pack_rescore_bound_max_abs. bound = max(floor, k x noise); the trainer only
    RECORDS the noise, the Rust verifier derives and enforces the bound (plan 08-15)."""
    p = parity_contract()
    return (int(p["constants"]["pack_rescore_noise_k"]),
            float(p["equations"]["pack_rescore_probs_abs"]["float_tolerance"]),
            float(p["constants"]["pack_rescore_bound_max_abs"]))


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


def run_dir_files(seeds=(13,), has_shift=False):
    """The run_dir_layout entries a run with `seeds` (the list resolve_seeds returned) and, when
    `has_shift`, a data-dir shift.jsonl must hold, as relative paths (the prose after the first space
    dropped). The CONDITIONAL entries are decided by the rule their own prose states, never by name:
    `(only when more than one seed ran)` and a `<s>` path template (one file per gate seed) hold only
    when more than one seed ran -- a single-seed run keeps the 1.x layout --, and `(only with
    shift.jsonl)` only with a shift probe. An entry with an `(only ...)` condition this function does
    not know is refused, so a new conditional file cannot silently become required or optional."""
    multi = len(list(seeds)) > 1
    out = []
    for entry in gate_contract()["run_dir_layout"]["run_dir"]:
        path, _, prose = str(entry).partition(" ")
        if "(only" in prose:
            if "only when more than one seed ran" in prose:
                keep = multi
            elif "only with shift.jsonl" in prose:
                keep = bool(has_shift)
            else:
                raise KeyError("run_dir_layout entry %r carries a condition contract.run_dir_files does not know"
                               % (entry,))
            if not keep:
                continue
        if "<s>" in path:
            if multi:
                out.extend(path.replace("<s>", str(int(s))) for s in seeds)
            continue
        out.append(path)
    return out


# The rule each OPTIONAL key of recipe_json_schema / gate_report_schema is written under, by the 1.4.0
# trainer (the contract's own "OPTIONAL ... REQUIRED when / present exactly when" wording). A key the
# contract marks OPTIONAL that is not listed here is refused by expected_keys (fail-closed).
_OPTIONAL_KEY_RULES = {
    "early_stopping": lambda stopping, n, shift: stopping == "early_stopping",
    "seed_selection": lambda stopping, n, shift: n > 1,
    "rescore_noise_sha256": lambda stopping, n, shift: True,      # every 1.4.0 run writes the record
    "shift_probe": lambda stopping, n, shift: bool(shift),
}


def expected_keys(schema, stopping, n_seeds, has_shift):
    """The exact top-level key set of recipe.json (`recipe_json_schema`) or gate-report.json
    (`gate_report_schema`) for a run: every schema key, minus rule notes (`*_rule`), minus each key
    marked OPTIONAL whose rule does not hold for this run."""
    out = []
    for key, desc in gate_contract()[schema].items():
        if key.endswith("_rule"):
            continue
        if str(desc).startswith("OPTIONAL"):
            if key not in _OPTIONAL_KEY_RULES:
                raise KeyError("%s.%s is OPTIONAL but contract.expected_keys has no rule for it" % (schema, key))
            if not _OPTIONAL_KEY_RULES[key](stopping, int(n_seeds), has_shift):
                continue
        out.append(key)
    return sorted(out)


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


def recipe_json(variant, shots_per_class, epochs, seed, base_block, stopping="fixed_epochs", seed_selection=None):
    """The recipe.json object in `recipe_json_schema` order of keys (serialized sort_keys anyway).

    fixed_epochs carries no `early_stopping` key, so its bytes -- and recipe_id -- are exactly the
    1.0.0 recipe's; early_stopping adds the contract's object and `epochs` becomes the maximum.
    `seed_selection` (1.4.0, A3) is written only when given -- the trainer passes
    seed_selection_decl() for every three-seed run, so for every production run -- and a single-seed
    run keeps the 1.x bytes (the legacy rule)."""
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
    if seed_selection is not None:
        if seed_selection != seed_selection_decl():
            raise RecipeError("REFUSED seeds: seed_selection %r is not the contract's %r"
                              % (seed_selection, seed_selection_decl()))
        out["seed_selection"] = seed_selection
    return out


def production_base_block():
    b = base()
    return {"family": b["family"], "repo": b["repo"], "revision": b["revision"], "checkpoint": b["checkpoint"],
            "sha256": b["model_safetensors_sha256"]}


def seed_selection_decl():
    """The recipe.json `seed_selection` object (1.4.0, A3), copied from seed_policy."""
    sp = seed_policy()
    return {"policy": str(sp["selection"]), "seeds": [int(s) for s in sp["variance_seeds"]],
            "rank_scale": int(sp["rank_scale"]), "tie_break": str(sp["tie_break"])}


def resolve_seeds(n, variant="production"):
    """The seeds a run trains, per seed_policy (D-08 as amended by A3, 1.4.0), the declared seed FIRST.

    production: exactly seed_policy.production_seeds_required seeds (the default when --seeds is
    omitted); any other --seeds is REFUSED -- a single MPS draw is not a gate run (spike 027).
    synthetic-fixture: 1 (the default; the legacy single-seed rule, the declared seed ships) or
    production_seeds_required (the median rule). Any other N is refused: the median rule needs an odd N
    and no legacy multi-seed variant is written any more."""
    sp = seed_policy()
    pool = [int(s) for s in sp["variance_seeds"]]
    declared = int(sp["declared_seed"])
    need = int(sp["production_seeds_required"])
    if not pool or pool[0] != declared:
        raise RecipeError("REFUSED seeds: seed_policy.variance_seeds %s must start with the declared seed %d"
                          % (pool, declared))
    if need != len(pool):
        raise RecipeError("REFUSED seeds: seed_policy.production_seeds_required %d != len(variance_seeds %s)"
                          % (need, pool))
    if variant == "production":
        n = need if n is None else int(n)
        if n != need:
            raise RecipeError("REFUSED seeds: production trains exactly seed_policy.production_seeds_required = %d "
                              "seeds %s and ships the median-ECE seed (A3); --seeds %d is not accepted"
                              % (need, pool, n))
        return pool[:need]
    n = 1 if n is None else int(n)
    if n not in (1, need):
        raise RecipeError("REFUSED seeds: --seeds %d; synthetic-fixture trains 1 seed (legacy rule, seed %d ships) "
                          "or %d seeds %s (median-ECE rule)" % (n, declared, need, pool))
    return pool[:n]


def seeds_label(n, policy=None):
    """The seeds label literal the contract declares (seed_policy.rule, gate_report_schema.seeds): "single seed"
    for one seed; "median-ECE seed of N seeds" for gate-report seeds.label under the median rule (policy
    "median_ece": ONE seed ships, never a mean); otherwise "mean ± sd over N seeds" -- a legacy multi-seed gate
    report, and variance-report.json under both rules (that file does report the mean and sd)."""
    if n == 1:
        return "single seed"
    if policy == "median_ece":
        return "median-ECE seed of %d seeds" % n
    if policy is not None:
        raise ValueError("seeds_label: unknown seed policy %r" % (policy,))
    return "mean ± sd over %d seeds" % n


def probe_policy():
    d = decide_contract()
    pp = d["probe_policy"]
    task = {k: pp["probe_task"][k] for k in ("type", "instructions", "criteria")}
    return task, list(pp["inputs"]), int(d["constants"]["probe_max_row_tokens"])
