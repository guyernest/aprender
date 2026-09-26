"""Task and row validation, text-level overlap refusal and the seeded calibration split (D-05, D-06).

No torch import (module level or anywhere): `python data.py --selftest` runs with numpy + pyyaml only.

    load_task(path)            task.json, the D-05 / decide-apr-v1 `task_json_schema`: exactly
                               {type: "choice", instructions: string, criteria: object}; criteria
                               DOCUMENT ORDER is the label index (parsed with object_pairs_hook=list
                               so both order and duplicates are observable); >= 2 unique names; each
                               description a string or null.
    load_rows(path, task)      train.jsonl / eval.jsonl, decide-apr-v1 `train_row_schema`: exactly
                               {text: string, label: <criterion NAME>}; unknown names and keys refused.
    normalized_sha256(text)    sha256 of `nfc-trim-ws-v1` (aprender-contrastive-data hash.rs): NFC, trim,
                               collapse every Unicode White_Space run to one U+0020 -- the Rust
                               `split_whitespace` set, NOT Python's str.split() (which also splits on
                               U+001C..U+001F).
    refuse_overlap(train, ev)  an eval text whose normalized hash is a train hash is refused.
    group_train(rows)          train rows grouped by normalized hash; a group whose copies carry
                               different labels is refused (row-disjoint is not text-disjoint).
    calibration_split(rows, fraction, min_per_class, seed, labels)
                               draws whole GROUPS, stratified by label, seeded: per class at least
                               max(min_per_class, ceil(fraction * n_class)) rows, so every copy of a
                               text lands on one side. Returns (fit_ids, calib_ids, slice_ids,
                               slice_ids_sha256) with slice_ids the SORTED 0-based train.jsonl row
                               indices and its sha256 over the compact JSON array bytes.

Every refusal is a DataError whose message starts `REFUSED <rule>:` so the rule is named.
"""
import hashlib
import json
import math
import sys
import unicodedata
from pathlib import Path

import numpy as np

# Unicode White_Space (the property Rust's char::is_whitespace / str::split_whitespace use).
_WHITE_SPACE = frozenset(
    [chr(c) for c in range(0x09, 0x0E)] + [" ", "\u0085", " ", " "]
    + [chr(c) for c in range(0x2000, 0x200B)] + [" ", " ", " ", " ", "　"])
TASK_KEYS = ("type", "instructions", "criteria")
ROW_KEYS = ("text", "label")


class DataError(ValueError):
    """A refused input; `rule` names the violated rule."""

    def __init__(self, rule, detail):
        super().__init__("REFUSED %s: %s" % (rule, detail))
        self.rule = rule


def sha256_bytes(b):
    return hashlib.sha256(b).hexdigest()


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def normalize(text):
    composed = unicodedata.normalize("NFC", text)
    words, cur = [], []
    for ch in composed:
        if ch in _WHITE_SPACE:
            if cur:
                words.append("".join(cur))
                cur = []
        else:
            cur.append(ch)
    if cur:
        words.append("".join(cur))
    return " ".join(words)


def normalized_sha256(text):
    return sha256_bytes(normalize(text).encode("utf-8"))


def exact_sha256(text):
    return sha256_bytes(text.encode("utf-8"))


# ------------------------------------------------------------------------------------------ task.json

def _pairs(obj, where):
    """object_pairs_hook=list gives objects as lists of (key, value) pairs; refuse anything else."""
    if not (isinstance(obj, list) and all(isinstance(p, tuple) and len(p) == 2 for p in obj)):
        raise DataError("task-schema", "%s must be a JSON object" % where)
    return obj


def load_task(path):
    path = Path(path)
    if not path.is_file():
        raise DataError("task-missing", "%s does not exist" % path)
    try:
        raw = json.loads(path.read_bytes().decode("utf-8"), object_pairs_hook=list)
    except (UnicodeDecodeError, json.JSONDecodeError) as e:
        raise DataError("task-schema", "%s is not UTF-8 JSON (%s)" % (path, e))
    top = _pairs(raw, "task.json")
    keys = [k for k, _ in top]
    if len(set(keys)) != len(keys):
        raise DataError("task-schema", "task.json repeats a key: %s" % keys)
    unknown = sorted(set(keys) - set(TASK_KEYS))
    if unknown:
        raise DataError("task-unknown-key", "task.json has unknown key(s) %s; allowed %s" % (unknown, list(TASK_KEYS)))
    missing = [k for k in TASK_KEYS if k not in keys]
    if missing:
        raise DataError("task-schema", "task.json is missing %s" % missing)
    d = dict(top)
    if d["type"] != "choice":
        raise DataError("task-type", "type must be \"choice\", got %r" % (d["type"],))
    if not isinstance(d["instructions"], str) or not d["instructions"].strip():
        raise DataError("task-schema", "instructions must be a non-empty string")
    crit = _pairs(d["criteria"], "criteria")
    names = [k for k, _ in crit]
    if len(set(names)) != len(names):
        dup = sorted({n for n in names if names.count(n) > 1})
        raise DataError("task-duplicate-criterion", "criterion name(s) %s appear more than once" % dup)
    if len(names) < 2:
        raise DataError("task-too-few-criteria", "a choice task needs at least 2 criteria, got %d" % len(names))
    for name, desc in crit:
        if not name.strip():
            raise DataError("task-schema", "a criterion name is empty")
        if desc is not None and not isinstance(desc, str):
            raise DataError("task-schema", "criterion %r description must be a string or null" % name)
    return {"type": "choice", "instructions": d["instructions"], "criteria": dict(crit), "labels": names}


def laya_question(task):
    """The Laya question dict for a validated task (criteria in document order)."""
    return {"type": task["type"], "instructions": task["instructions"], "criteria": dict(task["criteria"])}


# ------------------------------------------------------------------------------------------ rows

def load_rows(path, task, role):
    """[(text, label_index)] from a jsonl file; `role` names the file in refusals."""
    path = Path(path)
    if not path.is_file():
        raise DataError("%s-missing" % role, "%s does not exist (%s.jsonl is required)" % (path, role))
    labels = task["labels"]
    rows = []
    for n, line in enumerate(path.read_bytes().decode("utf-8").splitlines(), 1):
        if not line.strip():
            raise DataError("%s-row-schema" % role, "%s line %d is blank" % (path.name, n))
        try:
            obj = json.loads(line, object_pairs_hook=list)
        except json.JSONDecodeError as e:
            raise DataError("%s-row-schema" % role, "%s line %d is not JSON (%s)" % (path.name, n, e))
        if not (isinstance(obj, list) and all(isinstance(p, tuple) for p in obj)):
            raise DataError("%s-row-schema" % role, "%s line %d is not a JSON object" % (path.name, n))
        keys = [k for k, _ in obj]
        extra = sorted(set(keys) - set(ROW_KEYS))
        if extra:
            raise DataError("%s-row-unknown-key" % role, "%s line %d has unknown key(s) %s" % (path.name, n, extra))
        if sorted(keys) != sorted(ROW_KEYS):
            raise DataError("%s-row-schema" % role, "%s line %d must hold exactly %s" % (path.name, n, list(ROW_KEYS)))
        d = dict(obj)
        if not isinstance(d["text"], str) or not d["text"].strip():
            raise DataError("%s-row-schema" % role, "%s line %d text must be a non-empty string" % (path.name, n))
        if not isinstance(d["label"], str):
            raise DataError("%s-row-label" % role, "%s line %d label must be a criterion NAME, got %r"
                            % (path.name, n, d["label"]))
        if d["label"] not in labels:
            raise DataError("%s-row-label" % role, "%s line %d label %r is not a criterion (%s)"
                            % (path.name, n, d["label"], labels))
        rows.append((d["text"], labels.index(d["label"])))
    if not rows:
        raise DataError("%s-empty" % role, "%s has no rows" % path)
    return rows


def refuse_overlap(train, ev):
    train_hashes = {normalized_sha256(t) for t, _ in train}
    hits = [i for i, (t, _) in enumerate(ev) if normalized_sha256(t) in train_hashes]
    if hits:
        raise DataError("eval-train-overlap", "%d eval row(s) equal a train text after NFC/trim/whitespace "
                        "collapse (eval.jsonl rows %s)" % (len(hits), hits[:10]))


def group_train(rows):
    """{normalized hash: [row indices]} in first-seen order; conflicting labels refused."""
    groups = {}
    for i, (t, _) in enumerate(rows):
        groups.setdefault(normalized_sha256(t), []).append(i)
    for h, ids in groups.items():
        labs = sorted({rows[i][1] for i in ids})
        if len(labs) > 1:
            raise DataError("train-conflicting-labels", "train.jsonl rows %s carry the same normalized text "
                            "under different labels %s" % (ids, labs))
    return groups


def class_counts(rows, k):
    counts = [0] * k
    for _, y in rows:
        counts[y] += 1
    return counts


def calibration_split(rows, fraction, min_per_class, seed, k):
    """Group-disjoint, stratified, seeded calibration slice of the train rows."""
    groups = group_train(rows)
    counts = class_counts(rows, k)
    rng = np.random.RandomState(int(seed))
    calib = []
    for c in range(k):
        need = max(int(min_per_class), int(math.ceil(float(fraction) * counts[c])))
        cls_groups = [ids for ids in groups.values() if rows[ids[0]][1] == c]
        order = rng.permutation(len(cls_groups))
        took = 0
        for g in order:
            if took >= need:
                break
            calib.extend(cls_groups[g])
            took += len(cls_groups[g])
        if took < need or took >= counts[c]:
            raise DataError("train-class-too-small", "class %d has %d train row(s) in %d text group(s): a "
                            "calibration slice of >= %d row(s) must leave at least one fit row"
                            % (c, counts[c], len(cls_groups), need))
    slice_ids = sorted(calib)
    in_slice = set(slice_ids)
    fit_ids = [i for i in range(len(rows)) if i not in in_slice]
    return fit_ids, slice_ids, slice_ids, slice_ids_sha256(slice_ids)


def slice_ids_sha256(slice_ids):
    return sha256_bytes(json.dumps(list(slice_ids), separators=(",", ":")).encode())


if __name__ == "__main__":
    print("usage: python data.py --selftest", file=sys.stderr)
    sys.exit(2)
