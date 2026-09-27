"""Task and row validation, text-level overlap refusal and the seeded calibration split (D-05, D-06).

No torch import (module level or anywhere): `python data.py --selftest` runs with numpy + pyyaml only.

    load_task(path)            task.json, the D-05 / decide-apr-v1 `task_json_schema`: exactly
                               {type: "choice", instructions: string, criteria: object}; criteria
                               DOCUMENT ORDER is the label index (parsed with object_pairs_hook=list
                               so both order and duplicates are observable); >= 2 unique names; each
                               description a string or null.
    load_rows(path, task, role)
                               train.jsonl / eval.jsonl / shift.jsonl (role train | eval | shift),
                               decide-apr-v1 `train_row_schema`: exactly {text: string, label:
                               <criterion NAME>}; unknown names and keys refused.
    normalized_sha256(text)    sha256 of `nfc-trim-ws-v1` (aprender-contrastive-data hash.rs): NFC, trim,
                               collapse every Unicode White_Space run to one U+0020 -- the Rust
                               `split_whitespace` set, NOT Python's str.split() (which also splits on
                               U+001C..U+001F).
    refuse_overlap(train, ev, role="eval")
                               an eval (or shift) text whose normalized hash is a train hash is refused.
    in_distribution_heldout(validation, train_pool, shot_ids, excluded_ids, group_member_ids, shots)
                               laya-finetune-gate-v1 `eval_set.demo_rule` (A2, 1.4.0) as a pure function.
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
import json
import math
import sys
import unicodedata
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import sha256_bytes, sha256_file  # noqa: E402,F401  (re-exported: data.sha256_file)

# Unicode White_Space (the property Rust's char::is_whitespace / str::split_whitespace use).
_WHITE_SPACE = frozenset(
    [chr(c) for c in range(0x09, 0x0E)] + [" ", "\u0085", " ", " "]
    + [chr(c) for c in range(0x2000, 0x200B)] + [" ", " ", " ", " ", "　"])
TASK_KEYS = ("type", "instructions", "criteria")
ROW_KEYS = ("text", "label")
ROW_ROLES = ("train", "eval", "shift")


class DataError(ValueError):
    """A refused input; `rule` names the violated rule."""

    def __init__(self, rule, detail):
        super().__init__("REFUSED %s: %s" % (rule, detail))
        self.rule = rule


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
    """[(text, label_index)] from a jsonl file; `role` (train | eval | shift) names the file in refusals."""
    if role not in ROW_ROLES:
        raise ValueError("load_rows role %r is not one of %s" % (role, ROW_ROLES))
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


def refuse_overlap(train, ev, role="eval"):
    """An `role` row (eval, or the shift probe) whose normalized text equals a train text is refused."""
    train_hashes = {normalized_sha256(t) for t, _ in train}
    hits = [i for i, (t, _) in enumerate(ev) if normalized_sha256(t) in train_hashes]
    if hits:
        raise DataError("%s-train-overlap" % role, "%d %s row(s) equal a train text after NFC/trim/whitespace "
                        "collapse (%s.jsonl rows %s)" % (len(hits), role, role, hits[:10]))


def in_distribution_heldout(validation, train_pool, shot_ids, excluded_ids, group_member_ids, shots):
    """laya-finetune-gate-v1 `eval_set.demo_rule` (A2, 1.4.0), torch-free and I/O-free.

    `validation` and `train_pool` are [(id, text, label)] in source file order; `shot_ids` the selection's
    ordered_examples ids; `excluded_ids` its exclusions.excluded_train_ids; `group_member_ids` every
    exclusions.groups[*].members entry as a (split, id) pair; `shots` the [(text, label)] shots.
    Validation rows first, then train rows, each in file order; a row is dropped when its id is a shot,
    in excluded_ids, or (split, id) is a group member. A remaining row whose nfc-trim-ws-v1 text equals a
    shot's is REFUSED (DataError heldout-shot-overlap), never silently dropped. Duplicates by normalized
    text are dropped keeping the FIRST occurrence. Returns [(text, label)]."""
    shot_ids, excluded_ids = set(shot_ids), set(excluded_ids)
    members = {(str(sp), str(i)) for sp, i in group_member_ids}
    kept = []
    for split, rows in (("validation", validation), ("train", train_pool)):
        for rid, text, label in rows:
            if rid in shot_ids or rid in excluded_ids or (split, str(rid)) in members:
                continue
            kept.append((split, rid, text, label))
    shot_norm = {normalized_sha256(t) for t, _ in shots}
    hits = [(sp, rid) for sp, rid, t, _ in kept if normalized_sha256(t) in shot_norm]
    if hits:
        raise DataError("heldout-shot-overlap", "%d held-out row(s) equal a shot after NFC/trim/whitespace collapse "
                        "(%s)" % (len(hits), hits[:10]))
    seen, out = set(), []
    for _, _, text, label in kept:
        h = normalized_sha256(text)
        if h not in seen:
            seen.add(h)
            out.append((text, label))
    return out


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


# ------------------------------------------------------------------------------------------ self-test

# The D-19 demo's train.jsonl label layout (16 none, 16 against, 16 favor, 48 distinct texts) and the
# calibration slice both failing demo runs recorded (gate-report calibration.slice_ids / sha256). The
# split reads labels, text GROUPS and the seed only -- never the text itself -- so 48 distinct synthetic
# texts in the same label layout must reproduce the recorded slice exactly. No tweet text is needed.
DEMO_LABEL_LAYOUT = [0] * 16 + [1] * 16 + [2] * 16
DEMO_RECORDED_SLICE = [1, 8, 11, 12, 23, 24, 25, 26, 41, 43, 45, 47]
DEMO_RECORDED_SLICE_SHA256 = "0640137d67666af226d719823648bbfdc2fc6310476ae0fa93d7c6aac37802e4"


def _demo_rule_on_local_data(case):
    """The demo_rule on the REAL pinned splits (FALSIFY-LAYA-GATE-013): prepare_stance's s64 cell must rebuild
    exactly demo_s64.eval_rows rows with eval_class_counts. Local evidence only (the dataset is gitignored):
    absent -> an explicit SKIP line, never a pass. Counts only; no text is printed."""
    import contract
    repo = contract.REPO
    src = repo / "data" / "tweet-eval-stance"
    need = [src / n for n in ("train.jsonl", "validation.jsonl", "test.jsonl")]
    missing = [str(q.relative_to(repo)) for q in need if not q.is_file()]
    if missing:
        print("  SKIP demo_rule on the pinned splits: %s not present (gitignored local dataset)" % ", ".join(missing))
        return
    import prepare_stance                          # torch-free
    decl = contract.gate_contract()["demo_s64"]
    name = "demo_rule on the pinned splits: %s rows, class counts %s" % (decl["eval_rows"], list(decl["eval_class_counts"]))
    try:
        files, summary = prepare_stance.build("s64")
    except SystemExit:                             # prepare_stance.fail() printed PREPARE FAILED: <why> above
        case(name, False, "prepare_stance refused the s64 cell (PREPARE FAILED above)")
        return
    rows = [json.loads(ln) for ln in files["eval.jsonl"].decode("utf-8").splitlines()]
    order = list(decl["criteria_order"])
    got = [sum(1 for r in rows if r["label"] == lab) for lab in order]
    case(name, len(rows) == int(decl["eval_rows"]) and got == [int(x) for x in decl["eval_class_counts"]],
         "%d rows %s, shift %s, eval sha256 %s..." % (len(rows), got, summary["shift"],
                                                     sha256_bytes(files["eval.jsonl"])[:16]))


def selftest():
    """Every data refusal over temporary files, plus split determinism / stratification / text-disjointness
    and the recipe epoch rule. numpy + pyyaml only (no torch)."""
    import tempfile

    import contract

    failures = []

    def case(name, ok, detail=""):
        print("  %-4s %-68s %s" % ("ok" if ok else "FAIL", name, detail))
        if not ok:
            failures.append(name)

    def expect(name, rule, fn):
        try:
            fn()
            case(name, False, "accepted")
        except DataError as e:
            case(name, e.rule == rule, str(e)[:110])

    c = contract.constants()
    frac, min_pc = float(c["calibration_slice_fraction"]), int(c["calibration_slice_min_per_class"])
    good_task = {"type": "choice", "instructions": "Which?", "criteria": {"a": "first", "b": None, "c": "third"}}

    with tempfile.TemporaryDirectory(prefix="laya-data-selftest-") as tmp:
        tmp = Path(tmp)

        def write(name, text):
            path = tmp / name
            path.write_text(text, encoding="utf-8")
            return path

        def rows_text(rows):
            return "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in rows)

        print("task.json refusals:")
        expect('type "score"', "task-type",
               lambda: load_task(write("t1.json", json.dumps(dict(good_task, type="score")))))
        expect("one criterion", "task-too-few-criteria",
               lambda: load_task(write("t2.json", json.dumps(dict(good_task, criteria={"a": None})))))
        expect("duplicate criterion", "task-duplicate-criterion",
               lambda: load_task(write("t3.json", '{"type":"choice","instructions":"Which?",'
                                                  '"criteria":{"a":null,"b":null,"a":"again"}}')))
        expect("unknown task key", "task-unknown-key",
               lambda: load_task(write("t4.json", json.dumps(dict(good_task, labels=["a"])))))
        expect("task.json missing", "task-missing", lambda: load_task(tmp / "absent.json"))
        t = load_task(write("task.json", json.dumps(good_task)))
        case("criteria document order is the label index", t["labels"] == ["a", "b", "c"])
        t_rev = load_task(write("task_rev.json", '{"type":"choice","instructions":"Which?",'
                                                  '"criteria":{"c":null,"a":null,"b":null}}'))
        case("document order kept (not sorted)", t_rev["labels"] == ["c", "a", "b"])

        print("row refusals:")
        base_rows = [{"text": "row %d of class %s" % (i, lab), "label": lab} for lab in "abc" for i in range(4)]
        expect("train label not a criterion", "train-row-label",
               lambda: load_rows(write("r1.jsonl", rows_text(base_rows + [{"text": "x", "label": "zzz"}])), t, "train"))
        expect("train label given as an index", "train-row-label",
               lambda: load_rows(write("r2.jsonl", rows_text(base_rows + [{"text": "x", "label": 0}])), t, "train"))
        expect("train row with an extra key", "train-row-unknown-key",
               lambda: load_rows(write("r3.jsonl", rows_text(base_rows + [{"text": "x", "label": "a", "id": 1}])),
                                 t, "train"))
        expect("eval.jsonl missing", "eval-missing", lambda: load_rows(tmp / "eval.jsonl", t, "eval"))
        train = load_rows(write("train.jsonl", rows_text(base_rows)), t, "train")
        case("valid train.jsonl loads as (text, label index)", len(train) == 12 and train[4][1] == 1)
        expect("eval text equal to a train text after NFC/trim/whitespace collapse", "eval-train-overlap",
               lambda: refuse_overlap(train, [("a fresh eval row", 1), ("row 0 of\tclass  a ", 0)]))
        refuse_overlap(train, [("  Row 0 OF class a", 0)])    # case differs: no casefolding, not an overlap
        case("case-different eval text is not an overlap", True)
        expect("two train rows, same normalized text, different labels", "train-conflicting-labels",
               lambda: group_train(train + [(" row 0 of  class a", 1)]))
        too_small = [r for r in train if r[1] != 2] + [("only c %d" % i, 2) for i in range(min_pc)]
        expect("a class with only calibration_slice_min_per_class shots", "train-class-too-small",
               lambda: calibration_split(too_small, frac, min_pc, 13, 3))
        just_enough = [r for r in train if r[1] != 2] + [("enough c %d" % i, 2) for i in range(min_pc + 1)]
        fit, sl, _, _ = calibration_split(just_enough, frac, min_pc, 13, 3)
        case("a class with calibration_slice_min_per_class + 1 shots is accepted",
             class_counts([just_enough[i] for i in fit], 3)[2] >= 1)

        print("NFC / whitespace normalization (nfc-trim-ws-v1, the Rust split_whitespace set):")
        case("NFC composes e + U+0301", normalize("é") == "é")
        case("Unicode White_Space collapses (U+3000, U+0085, runs)", normalize("　a  b\u0085") == "a b")
        case("U+001F is NOT whitespace (unlike str.split)", normalize("a\u001fb") == "a\u001fb")

    print("calibration_split:")
    rows = [("shot %d" % i, y) for i, y in enumerate(DEMO_LABEL_LAYOUT)]
    a, b = calibration_split(rows, frac, min_pc, 13, 3), calibration_split(rows, frac, min_pc, 13, 3)
    case("deterministic for a seed (slice_ids and sha)", a == b, "sha=%s..." % a[3][:16])
    counts = class_counts([rows[i] for i in a[2]], 3)
    case("stratified: ceil(0.25 * 16) = 4 per class", counts == [4, 4, 4], str(counts))
    case("slice_ids sorted, unique, disjoint from fit, together cover train",
         a[2] == sorted(set(a[2])) and not set(a[0]) & set(a[2]) and sorted(a[0] + a[2]) == list(range(48)))
    case("slice_ids_sha256 is sha256 of the compact JSON array",
         a[3] == sha256_bytes(json.dumps(a[2], separators=(",", ":")).encode()))
    case("reproduces BOTH failing demo runs' recorded slice (seed 13, 16/16/16 layout)",
         a[2] == DEMO_RECORDED_SLICE and a[3] == DEMO_RECORDED_SLICE_SHA256, str(a[2]))
    case("a different seed changes the slice", calibration_split(rows, frac, min_pc, 17, 3)[2] != a[2])
    dup = rows + [("  shot   0 ", 0), ("shot 0", 0)]          # "shot 0" three times after normalization
    group = [i for i, (txt, _) in enumerate(dup) if normalized_sha256(txt) == normalized_sha256("shot 0")]
    sides, in_slice = set(), 0
    for seed in range(50):
        fit, _, sl, _ = calibration_split(dup, frac, min_pc, seed, 3)
        side = {i in set(sl) for i in group}
        sides.add(len(side))
        in_slice += int(True in side)
    case("a text duplicated 3x lands wholly on one side, over 50 seeds",
         len(group) == 3 and sides == {1} and 0 < in_slice < 50, "group in slice for %d/50 seeds" % in_slice)

    print("in-distribution held-out rule (eval_set.demo_rule, A2, FALSIFY-LAYA-GATE-013) on a synthetic manifest:")
    shots = [("shot a", "none"), ("shot b", "against")]
    shot_ids = ["train:0", "train:1"]
    excluded = ["train:3"]
    members = [("train", "train:3"), ("validation", "validation:3")]      # the real manifest's group shape
    val = [("validation:0", "val zero", "none"), ("validation:1", "val one", "against"),
           ("validation:2", "dup  text", "favor"), ("validation:3", "group partner", "against")]
    pool = [("train:0", "shot a", "none"), ("train:1", "shot b", "against"), ("train:2", " dup text ", "none"),
            ("train:3", "excluded row", "favor"), ("train:4", "train four", "favor"), ("train:5", "train five", "none")]
    held = in_distribution_heldout(val, pool, shot_ids, excluded, members, shots)
    texts = [normalize(t) for t, _ in held]
    case("a shot's row is dropped (by id), never re-used as eval", "shot a" not in texts and "shot b" not in texts)
    case("an excluded_train_ids row is dropped", "excluded row" not in texts)
    case("a validation row that is an exclusion-group member is dropped", "group partner" not in texts)
    case("a duplicate by normalized text is kept once, the validation copy (label) winning",
         texts.count("dup text") == 1 and dict((normalize(t), lab) for t, lab in held)["dup text"] == "favor")
    case("order: validation rows first, then train rows, each in file order",
         texts == ["val zero", "val one", "dup text", "train four", "train five"], str(texts))
    labs = ["none", "against", "favor"]
    case("the resulting class counts", [sum(1 for _, x in held if x == lab) for lab in labs] == [2, 1, 2])
    expect("a held-out row whose normalized text equals a shot is REFUSED, not dropped", "heldout-shot-overlap",
           lambda: in_distribution_heldout(val + [("validation:9", "  Shot  b", "none"), ("validation:8", " shot\tb ", "none")],
                                           pool, shot_ids, excluded, members, shots))
    _demo_rule_on_local_data(case)

    print("recipe epoch rule (contract.resolve_epochs):")

    def epochs(name, variant, spc, arg, want):
        try:
            got = contract.resolve_epochs(variant, spc, arg)
            case(name, got == want, "epochs=%s" % got)
        except contract.RecipeError as e:
            case(name, want is None, str(e)[:110])
    epochs("16/class, no --epochs -> 12", "production", 16, None, 12)
    epochs("16/class, --epochs 12 refused (fixed, not ignored)", "production", 16, 12, None)
    epochs("64/class, --epochs 8 -> 8", "production", 64, 8, 8)
    epochs("64/class, --epochs 3 refused (< 4)", "production", 64, 3, None)
    epochs("64/class, --epochs 13 refused (> 12)", "production", 64, 13, None)
    epochs("64/class, no --epochs refused", "production", 64, None, None)
    epochs("synthetic-fixture, --epochs 1 -> 1", "synthetic-fixture", 4, 1, 1)

    if failures:
        print("DATA SELFTEST FAILED: %s" % ", ".join(failures))
        return 1
    print("DATA SELFTEST OK")
    return 0


if __name__ == "__main__":
    if sys.argv[1:] == ["--selftest"]:
        sys.exit(selftest())
    print("usage: python data.py --selftest", file=sys.stderr)
    sys.exit(2)
