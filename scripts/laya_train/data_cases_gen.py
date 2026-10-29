"""Generator of the shared dataset-rule case table `data_cases.json` (schema laya-data-cases-v1, D-08).

Run as `python3 -I scripts/laya_train/data_cases_gen.py > scripts/laya_train/data_cases.json`.

The table is the ONE statement of what a dataset directory must satisfy. It is replayed twice:

    python  `data.py --selftest`   through data.validate_data_dir (the trainer's own step-1 function)
    rust    `cargo test -p aprender-mcp-laya-train --lib validate`   through validate_for_submit

so the request-path pre-filter and the trainer cannot drift apart without a case turning red.

Determinism: standard library only, no import of data.py (the expected results below are written by hand
from the rule, never copied from the code they judge), no floats, `json.dumps(sort_keys=True,
ensure_ascii=True, indent=2)` and nothing else in the output (no timestamps, no paths, no set order). The bytes
therefore do not depend on the Python minor version: the verify runs this file under the system `python3 -I`
and `data.py --selftest` runs `render()` in-process under the project's interpreter, and both must equal the
committed file byte for byte.

A case is {id, note, files, epochs, python, rust}:

    files    {name: {"text": exact text} | {"b64": base64 bytes}}; a name that is absent means the file is absent
    epochs   null | int: what validate_data_dir / validate_for_submit receive
    python   {"refused": rule} | {"ok": {labels, class_counts, shots_per_class, epochs, shift_present,
             inputs_sha256}}
    rust     "same" | "deferred" | {"stricter": rule}
             "same": the Rust pre-filter's result equals `python`.
             "deferred": python refused a rule in the header's `deferrable_rules` that Rust cannot decide in the
             request path (it depends on a seeded permutation); Rust accepts and reports it in `deferred_rules`.
             {"stricter": r}: python accepted and Rust refuses r, a rule in the header's `stricter_rules`.
"""
import base64
import hashlib
import json
import sys

SCHEMA = "laya-data-cases-v1"
LABELS = ["a", "b", "c"]
GOOD_TASK = {"type": "choice", "instructions": "Which?", "criteria": {"a": "first", "b": None, "c": "third"}}


# ------------------------------------------------------------------------------------------ builders

def task_text(criteria=None, **over):
    doc = dict(GOOD_TASK, **over)
    if criteria is not None:
        doc["criteria"] = criteria
    return json.dumps(doc, ensure_ascii=False)


def rows_text(rows, newline="\n"):
    return "".join(json.dumps(r, ensure_ascii=False) + newline for r in rows)


def train_rows(per_class, labels=LABELS):
    return [{"text": "train %s %d" % (lab, i), "label": lab} for lab in labels for i in range(per_class)]


def eval_rows(per_class=2, labels=LABELS):
    return [{"text": "eval %s %d" % (lab, i), "label": lab} for lab in labels for i in range(per_class)]


def F(value):
    """A file spec: str -> exact text, bytes -> base64."""
    if isinstance(value, bytes):
        return {"b64": base64.b64encode(value).decode("ascii")}
    return {"text": value}


def dataset(per_class=4, task=None, train=None, ev=None, shift=None, labels=LABELS):
    """The files of an ordinary valid dataset; any piece can be replaced by text/bytes, and None for a
    replacement `task`/`train`/`ev` argument means 'the default'. Absent files are dropped by `drop`."""
    files = {"task.json": F(task if task is not None else task_text(
                {lab: GOOD_TASK["criteria"].get(lab) for lab in labels})),
             "train.jsonl": F(train if train is not None else rows_text(train_rows(per_class, labels))),
             "eval.jsonl": F(ev if ev is not None else rows_text(eval_rows(2, labels)))}
    if shift is not None:
        files["shift.jsonl"] = F(shift)
    return files


def drop(files, *names):
    return {k: v for k, v in files.items() if k not in names}


def _bytes(spec):
    return spec["text"].encode("utf-8") if "text" in spec else base64.b64decode(spec["b64"])


def ok(files, counts, epochs, labels=LABELS):
    """The accepted summary the rules give for `files`: counts and epochs are stated by the case."""
    return {"ok": {"labels": list(labels), "class_counts": list(counts), "shots_per_class": max(counts),
                   "epochs": epochs, "shift_present": "shift.jsonl" in files,
                   "inputs_sha256": {n: hashlib.sha256(_bytes(s)).hexdigest() for n, s in sorted(files.items())}}}


def refused(rule):
    return {"refused": rule}


def make(cid, note, files, python, epochs=None, rust="same"):
    return {"id": cid, "note": note, "files": files, "epochs": epochs, "python": python, "rust": rust}


# ------------------------------------------------------------------------------------------ the cases

def cases_core():
    """The first group: the accepted paths, the D-21 epoch cases and one case per early rule."""
    out = []
    f = dataset(20)
    out.append(make("ok-3class-above-16-epochs-4", "20 shots per class, epochs 4 given: accepted, epochs 4", f,
                    ok(f, [20, 20, 20], 4), epochs=4))
    f = dataset(16)
    out.append(make("ok-3class-at-16-no-epochs", "16 shots per class, no epochs: the recipe fixes 12", f,
                    ok(f, [16, 16, 16], 12)))
    out.append(make("task-missing", "no task.json", drop(dataset(), "task.json"), refused("task-missing")))
    out.append(make("task-unknown-key", "task.json with a key outside {type, instructions, criteria}",
                    dataset(task=task_text(labels=["a"])), refused("task-unknown-key")))
    bad = train_rows(4) + [{"text": "an unlabelled row", "label": "zzz"}]
    out.append(make("train-row-label", "a train label that is not a criterion name",
                    dataset(train=rows_text(bad)), refused("train-row-label")))
    out.append(make("eval-missing", "no eval.jsonl", drop(dataset(), "eval.jsonl"), refused("eval-missing")))
    out.append(make("eval-train-overlap", "an eval text equal to a train text",
                    dataset(ev=rows_text(eval_rows() + [{"text": "train a 0", "label": "a"}])),
                    refused("eval-train-overlap")))
    conflict = train_rows(4) + [{"text": "train a 0", "label": "b"}]
    out.append(make("train-conflicting-labels", "one train text under two labels", dataset(train=rows_text(conflict)),
                    refused("train-conflicting-labels")))
    out.append(make("eval-class-coverage", "an eval file with no row of criterion c",
                    dataset(ev=rows_text([r for r in eval_rows() if r["label"] != "c"])),
                    refused("eval-class-coverage")))
    out.append(make("epochs-above-16-missing", "20 shots per class and no epochs", dataset(20), refused("epochs")))
    out.append(make("epochs-at-16-given", "16 shots per class and epochs 12: fixed, not ignored", dataset(16),
                    refused("epochs"), epochs=12))
    blob = rows_text(train_rows(4)[:3]).encode("utf-8") + b'{"text": "caf\xe9", "label": "a"}\n'
    out.append(make("train-row-encoding-invalid-byte", "an invalid UTF-8 byte in train.jsonl", dataset(train=blob),
                    refused("train-row-encoding")))
    return out


def all_cases():
    return cases_core()


def table():
    return {
        "schema": SCHEMA,
        "generator": "scripts/laya_train/data_cases_gen.py",
        "deferrable_rules": ["train-class-too-small"],
        "stricter_rules": ["task-serving-limit"],
        "stricter_reason": "decide-apr-v1 constants.max_criteria is 510 and pack's Task::from_slice refuses a "
                           "task.json with more (verify.rs read_data_dir), so a 511-criteria run could never "
                           "yield an .apr and the pre-filter refuses it at the door",
        "cases": all_cases(),
    }


def render():
    return json.dumps(table(), sort_keys=True, ensure_ascii=True, indent=2) + "\n"


if __name__ == "__main__":
    sys.stdout.write(render())
