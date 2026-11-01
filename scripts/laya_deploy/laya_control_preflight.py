#!/usr/bin/env python3
"""The offline recipe, inputs, waiver and thresholds preflight of the Phase 16 control (plan 16-01 Task 2).

    uv run --project scripts/laya_train --frozen python -I scripts/laya_deploy/laya_control_preflight.py \\
        --data DIR --reference FIXTURE [--declaration DECL] [--epochs N] [--json-out FILE]
    uv run --project scripts/laya_train --frozen python -I scripts/laya_deploy/laya_control_preflight.py --selftest

It proves, BEFORE the declaration that names it is committed and before any paid step, that the recipe id the SageMaker
trainer will write for the local data dir is the one Phase 10 wrote, with the trainer's OWN code (OPS-03: it imports
scripts/laya_train/contract.py, data.py and gate.py and re-implements nothing). It lives under scripts/laya_deploy, not
scripts/laya_train, so the train image's context stays byte-identical to its build commit.

The checks, in order (each refusal is one `REFUSED <rule>: <reason>` line on stderr, exit 2):

    preflight-data        data.validate_data_dir (or contract) refuses the dir; the rule and the exception's own
                          message are printed, which never carry row text
    preflight-epochs      the validated epoch count is not the reference's
    preflight-recipe      contract.recipe_id_for (variant production, the declared seed, resolve_stopping(None),
                          seed_selection_decl(), production_base_block()) differs from the reference's recipe_id, or
                          from the declaration's must.recipe_id
    preflight-inputs      the three input digests differ from the reference's, or from the declaration's must
    preflight-labels      task.json's labels, in order, differ from the reference's
    preflight-thresholds  contract.thresholds() differs from the reference's
    preflight-waiver      a pilot-waiver declaration binds this dir's task, train or eval digest, or
                          gate.matching_pilot_waiver finds one (D-09: no waiver may apply)

No network, no AWS, no write outside --json-out and mktemp directories. Prints numbers and hashes, never row text.
Needs numpy and pyyaml from the trainer's locked environment (hence the uv invocation).
"""

import hashlib
import json
import os
import shutil
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
TRAIN_DIR = os.path.join(REPO, "scripts", "laya_train")
sys.path.insert(0, TRAIN_DIR)

MAX_FILE = 8 * 1024 * 1024


class Refused(Exception):
    def __init__(self, rule, reason):
        Exception.__init__(self, "%s: %s" % (rule, reason))
        self.rule = rule
        self.reason = reason


try:
    import contract  # noqa: E402
    import data  # noqa: E402
    import gate  # noqa: E402
except ImportError as _ex:  # pragma: no cover - the environment, not the logic
    print("REFUSED environment: the trainer modules need numpy and pyyaml (%s); run through "
          "`uv run --project scripts/laya_train --frozen python -I`" % (_ex,), file=sys.stderr)
    sys.exit(2)


# ---- the selftest (written first; the functions above and below did not exist when it first ran) ----------------

class Cases:
    def __init__(self):
        self.n = 0
        self.failed = []

    def check(self, label, fn):
        try:
            r = fn()
        except Exception as ex:
            ok, detail = False, "crashed with %r" % (ex,)
        else:
            ok, detail = (r if isinstance(r, tuple) else (bool(r), ""))
        self.n += 1
        print("  %-4s %s  %s" % ("ok" if ok else "FAIL", label, detail))
        if not ok:
            self.failed.append(label)

    def refused(self, label, fn, rule, forbid=None):
        def run():
            try:
                fn()
            except Refused as ex:
                text = str(ex)
                return (ex.rule == rule and (forbid is None or forbid not in text), "refused with %s: %s" % (ex.rule, ex.reason[:90]))
            except Exception as ex:
                return (False, "crashed with %r" % (ex,))
            return (False, "was accepted")
        self.check(label, run)


def write_synth_dir(root, per_class=24, eval_per_class=4):
    os.makedirs(root, exist_ok=True)
    task = {"type": "choice", "instructions": "Pick the one criterion that fits the text.",
            "criteria": {"neg": "the text is negative", "pos": "the text is positive"}}
    with open(os.path.join(root, "task.json"), "w", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps(task, indent=2) + "\n")
    for name, n, tag in (("train.jsonl", per_class, "train"), ("eval.jsonl", eval_per_class, "eval")):
        with open(os.path.join(root, name), "w", encoding="utf-8", newline="\n") as f:
            for i in range(n):
                for lab in ("neg", "pos"):
                    f.write(json.dumps({"text": "synthetic %s %s sentence number %d" % (tag, lab, i), "label": lab}) + "\n")
    return root


def reference_from(dir_, epochs):
    """A reference object built from `dir_`'s own recomputed values (the accept case)."""
    rec = compute(dir_, epochs)
    return {"epochs": rec["epochs"], "recipe_id": rec["recipe_id"], "labels": list(rec["labels"]),
            "inputs_sha256": dict(rec["inputs_sha256"]), "thresholds": dict(contract.thresholds())}


def selftest():
    c = Cases()
    work = tempfile.mkdtemp(prefix="laya-control-preflight-selftest-")
    try:
        d = write_synth_dir(os.path.join(work, "data"))
        ref = reference_from(d, 4)
        ok_line = lambda r=ref, **kw: preflight(d, r, **kw)[0]  # noqa: E731

        c.check("sensitivity: the recipe id at epochs 4 differs from the id at epochs 5",
                lambda: compute(d, 4)["recipe_id"] != compute(d, 5)["recipe_id"])
        c.check("sensitivity: the same dir and epochs give the same id twice", lambda: compute(d, 4)["recipe_id"] == compute(d, 4)["recipe_id"])
        c.check("accept: a reference built from the dir prints a PREFLIGHT OK line",
                lambda: (ok_line().startswith("PREFLIGHT OK recipe_id=%s shots_per_class=24 epochs=4 seed=%d stopping=%s rows=48/8 "
                                              "inputs=match waiver=none thresholds=match"
                                              % (ref["recipe_id"], contract.declared_seed(), contract.resolve_stopping(None))), ok_line()))
        c.check("accept: the record holds class counts, rows, the selection and no waiver match",
                lambda: (lambda r: r["class_counts"] == {"neg": 24, "pos": 24} and r["rows"] == {"train": 48, "eval": 8}
                         and r["waiver_match"] is False and r["selection"]["policy"] == contract.seed_selection_decl()["policy"]
                         and r["inputs_sha256"] == {"task_json": ref["inputs_sha256"]["task_json"],
                                                    "train_jsonl": ref["inputs_sha256"]["train_jsonl"],
                                                    "eval_jsonl": ref["inputs_sha256"]["eval_jsonl"]})(preflight(d, ref)[1]))
        c.check("accept: the record is numbers and hashes only (no string over 80 characters except hashes)",
                lambda: all(len(s) <= 80 for s in strings_of(preflight(d, ref)[1])))
        c.refused("epochs: a reference holding another epochs value is refused",
                  lambda: preflight(d, dict(ref, epochs=5), epochs=4), "preflight-epochs")
        c.refused("recipe: a reference with another recipe_id is refused",
                  lambda: preflight(d, dict(ref, recipe_id="ab" * 32)), "preflight-recipe")
        c.refused("recipe: a declaration whose must.recipe_id differs is refused",
                  lambda: preflight(d, ref, declaration={"must": {"recipe_id": "cd" * 32, "inputs_sha256": ref["inputs_sha256"]}}),
                  "preflight-recipe")
        c.check("recipe: a declaration that agrees is accepted",
                lambda: preflight(d, ref, declaration={"must": {"recipe_id": ref["recipe_id"], "inputs_sha256": ref["inputs_sha256"]}})[0]
                .startswith("PREFLIGHT OK "))

        d2 = write_synth_dir(os.path.join(work, "data2"))
        last = None
        with open(os.path.join(d2, "train.jsonl"), "rb") as f:
            raw = f.read()
        lines = raw.decode("utf-8").split("\n")
        last = json.loads(lines[-2])
        last["text"] += "x"
        lines[-2] = json.dumps(last)
        with open(os.path.join(d2, "train.jsonl"), "wb") as f:
            f.write("\n".join(lines).encode("utf-8"))
        c.check("inputs: one appended byte changes the train digest", lambda: compute(d2, 4)["inputs_sha256"]["train_jsonl"] != ref["inputs_sha256"]["train_jsonl"])
        c.refused("inputs: the changed dir is refused against the original reference",
                  lambda: preflight(d2, dict(ref, recipe_id=compute(d2, 4)["recipe_id"])), "preflight-inputs")
        c.refused("inputs: a declaration whose must.inputs_sha256 differs is refused",
                  lambda: preflight(d, ref, declaration={"must": {"recipe_id": ref["recipe_id"],
                                                                 "inputs_sha256": dict(ref["inputs_sha256"], eval_jsonl="0" * 64)}}),
                  "preflight-inputs")
        c.refused("labels: a reference with the labels in the other order is refused",
                  lambda: preflight(d, dict(ref, labels=list(reversed(ref["labels"])))), "preflight-labels")
        c.refused("thresholds: a reference whose thresholds differ from the contract is refused",
                  lambda: preflight(d, dict(ref, thresholds=dict(ref["thresholds"], max_ece=0.2))), "preflight-thresholds")

        digests = ref["inputs_sha256"]

        def with_waivers(decls, fn):
            old = contract.pilot_waiver_declarations
            contract.pilot_waiver_declarations = lambda: decls
            try:
                return fn()
            finally:
                contract.pilot_waiver_declarations = old
        applies = {"task_json_sha256": digests["task_json"], "train_jsonl_sha256": "00" * 32, "eval_jsonl_sha256": "11" * 32,
                   "recipe_id": "22" * 32}
        c.refused("waiver: a declaration binding this dir's task digest is refused (D-09)",
                  lambda: with_waivers([{"id": "w", "waives": ["ece_post"], "applies_to": applies}], lambda: preflight(d, ref)),
                  "preflight-waiver")
        c.refused("waiver: a declaration binding this dir, its recipe and ece_post (a full match) is refused",
                  lambda: with_waivers([{"id": "w", "waives": ["ece_post"], "applies_to": {
                      "task_json_sha256": digests["task_json"], "train_jsonl_sha256": digests["train_jsonl"],
                      "eval_jsonl_sha256": digests["eval_jsonl"], "recipe_id": ref["recipe_id"]}}], lambda: preflight(d, ref)),
                  "preflight-waiver")
        c.check("waiver: a declaration whose recipe_id alone matches does not refuse",
                lambda: with_waivers([{"id": "w", "waives": ["ece_post"], "applies_to": dict(
                    applies, task_json_sha256="33" * 32, recipe_id=ref["recipe_id"])}], lambda: preflight(d, ref))[0].startswith("PREFLIGHT OK "))
        c.check("waiver: the real contract's declarations do not apply to the synthetic dir and are counted",
                lambda: preflight(d, ref)[1]["pilot_waivers_declared"] == len(contract.pilot_waiver_declarations()))

        d3 = write_synth_dir(os.path.join(work, "data3"))
        with open(os.path.join(d3, "eval.jsonl"), "rb") as f:
            first_eval = f.read().decode("utf-8").split("\n")[0]
        with open(os.path.join(d3, "train.jsonl"), "ab") as f:
            f.write((first_eval + "\n").encode("utf-8"))
        marker = json.loads(first_eval)["text"]
        c.refused("data: an eval row copied into train is refused with the rule, never the row text",
                  lambda: preflight(d3, ref), "preflight-data", forbid=marker)

        # the command line end to end
        out_json = os.path.join(work, "pre.json")
        ref_path = os.path.join(work, "ref.json")
        with open(ref_path, "w", encoding="utf-8") as f:
            f.write(json.dumps(ref))
        o, e = [], []
        rc = main(["--data", d, "--reference", ref_path, "--epochs", "4", "--json-out", out_json], out=o.append, err=e.append)
        c.check("cli: main() prints one PREFLIGHT OK line and writes the record",
                lambda: (rc == 0 and len(o) == 1 and o[0].startswith("PREFLIGHT OK ") and os.path.isfile(out_json)
                         and json.load(open(out_json))["recipe_id"] == ref["recipe_id"], str(e)))
        o, e = [], []
        rc = main(["--data", d, "--reference", ref_path, "--epochs", "5"], out=o.append, err=e.append)
        c.check("cli: another epoch count is refused with exit 2 and one REFUSED line",
                lambda: (rc == 2 and o == [] and len(e) == 1 and e[0].startswith("REFUSED preflight-epochs: "), str(e)))
    finally:
        shutil.rmtree(work, ignore_errors=True)
    if c.failed:
        print("CONTROL PREFLIGHT SELFTEST FAILED")
        return 1
    print("CONTROL PREFLIGHT SELFTEST OK cases=%d" % c.n)
    return 0


def strings_of(x):
    if isinstance(x, dict):
        for k, v in x.items():
            yield k
            for s in strings_of(v):
                yield s
    elif isinstance(x, list):
        for v in x:
            for s in strings_of(v):
                yield s
    elif isinstance(x, str):
        yield x


# ---- the preflight ------------------------------------------------------------------------------------------------

def _pairs(pairs):
    d = {}
    for k, v in pairs:
        if k in d:
            raise ValueError("duplicate key %r" % k)
        d[k] = v
    return d


def _const(name):
    raise ValueError("non-finite number %s" % name)


def read_json(path, rule):
    if os.path.islink(path) or not os.path.isfile(path):
        raise Refused(rule, "%s is missing" % path)
    if os.path.getsize(path) > MAX_FILE:
        raise Refused(rule, "%s is larger than %d bytes" % (path, MAX_FILE))
    try:
        with open(path, "rb") as f:
            return json.loads(f.read().decode("utf-8"), object_pairs_hook=_pairs, parse_constant=_const)
    except (ValueError, UnicodeDecodeError) as ex:
        raise Refused(rule, "%s is not valid JSON (%s)" % (path, str(ex)[:80]))


def compute(data_dir, epochs):
    """The recipe and inputs the trainer would bind for `data_dir`, by the trainer's own functions, in the order
    scripts/laya_train/train.py main() calls them. A DataError, RecipeError or ContractValueError is preflight-data."""
    try:
        seed = contract.declared_seed()
        v = data.validate_data_dir(data_dir, epochs, "production")
        stopping = contract.resolve_stopping(None)
        selection = contract.seed_selection_decl()
        _recipe, _bytes, recipe_id = contract.recipe_id_for("production", v.shots_per_class, v.epochs, seed,
                                                            contract.production_base_block(), stopping, selection)
    except (data.DataError, contract.RecipeError, contract.ContractValueError) as ex:
        msg = str(ex)
        raise Refused("preflight-data", msg[len("REFUSED "):] if msg.startswith("REFUSED ") else msg)
    labels = list(v.task["labels"])
    counts = data.class_counts(v.train_rows, len(labels))
    sha = v.input_sha256
    return {
        "recipe_id": recipe_id, "shots_per_class": v.shots_per_class, "epochs": v.epochs, "seed": seed,
        "stopping": stopping, "selection": selection, "labels": labels,
        "class_counts": {lab: n for lab, n in zip(labels, counts)},
        "rows": {"train": len(v.train_rows), "eval": len(v.eval_rows)},
        "inputs_sha256": {"task_json": sha["task.json"], "train_jsonl": sha["train.jsonl"], "eval_jsonl": sha["eval.jsonl"]},
        "thresholds": dict(contract.thresholds()),
    }


def preflight(data_dir, reference, declaration=None, epochs=None):
    """(the PREFLIGHT OK line, the record), or Refused with one of the rules in the module docstring."""
    if not isinstance(reference, dict):
        raise Refused("preflight-recipe", "the reference is not a JSON object")
    want_epochs = epochs if epochs is not None else reference.get("epochs")
    rec = compute(data_dir, want_epochs)
    if rec["epochs"] != reference.get("epochs"):
        raise Refused("preflight-epochs", "the validated epoch count %s is not the reference's %s"
                      % (rec["epochs"], reference.get("epochs")))
    if rec["recipe_id"] != reference.get("recipe_id"):
        raise Refused("preflight-recipe", "the trainer's recipe id %s is not the reference's %s"
                      % (rec["recipe_id"], reference.get("recipe_id")))
    must = declaration.get("must") if isinstance(declaration, dict) else None
    if declaration is not None:
        if not isinstance(must, dict) or must.get("recipe_id") != rec["recipe_id"]:
            raise Refused("preflight-recipe", "the declaration's must.recipe_id is not the trainer's recipe id")
    if rec["inputs_sha256"] != reference.get("inputs_sha256"):
        raise Refused("preflight-inputs", "the input digests of the data dir differ from the reference's")
    if declaration is not None and must.get("inputs_sha256") != rec["inputs_sha256"]:
        raise Refused("preflight-inputs", "the input digests of the data dir differ from the declaration's must.inputs_sha256")
    if rec["labels"] != reference.get("labels"):
        raise Refused("preflight-labels", "task.json's labels, in order, are not the reference's")
    if rec["thresholds"] != reference.get("thresholds"):
        raise Refused("preflight-thresholds", "contract.thresholds() %s is not the reference's %s"
                      % (rec["thresholds"], reference.get("thresholds")))
    decls = contract.pilot_waiver_declarations()
    mine = {rec["inputs_sha256"]["task_json"], rec["inputs_sha256"]["train_jsonl"], rec["inputs_sha256"]["eval_jsonl"]}
    for d in decls:
        a = d.get("applies_to", {})
        if mine & {a.get("task_json_sha256"), a.get("train_jsonl_sha256"), a.get("eval_jsonl_sha256")}:
            raise Refused("preflight-waiver", "pilot waiver %s binds one of this dir's input digests (D-09: no waiver may apply)"
                          % d.get("id"))
    if gate.matching_pilot_waiver(decls, rec["inputs_sha256"], rec["recipe_id"], ["ece_post"]) is not None:
        raise Refused("preflight-waiver", "a pilot waiver matches this dir, recipe and ece_post (D-09)")
    rec["pilot_waivers_declared"] = len(decls)
    rec["waiver_match"] = False
    line = ("PREFLIGHT OK recipe_id=%s shots_per_class=%d epochs=%d seed=%d stopping=%s rows=%d/%d inputs=match waiver=none "
            "thresholds=match" % (rec["recipe_id"], rec["shots_per_class"], rec["epochs"], rec["seed"], rec["stopping"],
                                  rec["rows"]["train"], rec["rows"]["eval"]))
    return line, rec


# ---- command line ------------------------------------------------------------------------------------------------

SPEC = {"--data": True, "--reference": True, "--declaration": False, "--epochs": False, "--json-out": False}


def parse_opts(args):
    opts = {}
    i = 0
    while i < len(args):
        k = args[i]
        if k not in SPEC:
            raise Refused("usage", "unknown option %r" % k)
        if i + 1 >= len(args):
            raise Refused("usage", "%s needs a value" % k)
        if k in opts:
            raise Refused("usage", "%s given twice" % k)
        opts[k] = args[i + 1]
        i += 2
    for k, required in SPEC.items():
        if required and k not in opts:
            raise Refused("usage", "%s is required" % k)
    return opts


def main(argv, out=None, err=None):
    out = out or (lambda s: print(s, flush=True))
    err = err or (lambda s: print(s, file=sys.stderr, flush=True))
    try:
        if argv == ["--selftest"]:
            return selftest()
        opts = parse_opts(argv)
        epochs = None
        if "--epochs" in opts:
            if not opts["--epochs"].isdigit():
                raise Refused("usage", "--epochs must be a non-negative integer")
            epochs = int(opts["--epochs"])
        reference = read_json(opts["--reference"], "preflight-reference")
        declaration = read_json(opts["--declaration"], "preflight-declaration") if "--declaration" in opts else None
        line, rec = preflight(opts["--data"], reference, declaration, epochs)
        if "--json-out" in opts:
            with open(opts["--json-out"], "w", encoding="utf-8", newline="\n") as f:
                f.write(json.dumps(rec, sort_keys=True, indent=2) + "\n")
        out(line)
        return 0
    except Refused as ex:
        err("REFUSED %s: %s" % (ex.rule, ex.reason))
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
