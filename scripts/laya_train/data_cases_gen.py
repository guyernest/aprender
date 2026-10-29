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


def cases_task():
    """task.json: one case per rule data.py `load_task` can raise, plus the criteria boundary."""
    out = []
    t = lambda cid, note, task, rule: out.append(make(cid, note, dataset(task=task), refused(rule)))  # noqa: E731
    t("task-schema-not-json", "task.json is not JSON", "{not json", "task-schema")
    t("task-schema-bom", "a UTF-8 BOM before the JSON (Python keeps it, json.loads refuses it)",
      "\ufeff" + task_text(), "task-schema")
    t("task-schema-not-an-object", "task.json is a JSON string", '"choice"', "task-schema")
    t("task-schema-repeated-key",
      "a repeated top-level key (data.py reads pairs, so the repeat is visible)",
      '{"type":"choice","instructions":"Which?","type":"choice","criteria":{"a":null,"b":null}}', "task-schema")
    t("task-schema-missing-key", "no instructions key",
      '{"type":"choice","criteria":{"a":null,"b":null}}', "task-schema")
    t("task-schema-empty-instructions", "instructions is whitespace only", task_text(instructions="  \t "),
      "task-schema")
    t("task-schema-instructions-only-u001c", "instructions is U+001C only (str.strip strips it)",
      task_text(instructions="\u001c"), "task-schema")
    t("task-schema-instructions-not-a-string", "instructions is a number", task_text(instructions=5),
      "task-schema")
    t("task-schema-empty-criterion-name", "a criterion with an empty name",
      '{"type":"choice","instructions":"Which?","criteria":{"a":null,"":null}}', "task-schema")
    t("task-schema-blank-criterion-name", "a criterion whose name is whitespace only",
      task_text({"a": None, " ": None}), "task-schema")
    t("task-schema-bad-description-type", "a criterion description that is a number",
      task_text({"a": 5, "b": None}), "task-schema")
    t("task-schema-criteria-not-an-object", "criteria is a string", task_text(criteria="abc"), "task-schema")
    t("task-type-not-choice", "type is score", task_text(type="score"), "task-type")
    t("task-too-few-criteria", "one criterion", task_text({"a": None}), "task-too-few-criteria")
    t("task-too-few-criteria-empty-array", "criteria is [] (data.py reads it as an object with no members)",
      task_text(criteria=[]), "task-too-few-criteria")
    t("task-duplicate-criterion", "a criterion name twice",
      '{"type":"choice","instructions":"Which?","criteria":{"a":null,"b":null,"a":"again"}}',
      "task-duplicate-criterion")
    f = dataset(task=task_text(labels=["a"]))
    out.append(make("task-unknown-key-before-schema", "an unknown key AND a missing key: unknown wins",
                    dict(f, **{"task.json": F('{"type":"choice","criteria":{"a":null,"b":null},"x":1}')}),
                    refused("task-unknown-key")))
    # The criteria boundary: decide-apr-v1 max_criteria is 510. data.py has no maximum; pack's reader refuses 511.
    for n, verdict in ((510, "same"), (511, {"stricter": "task-serving-limit"})):
        labels = ["c%d" % i for i in range(n)]
        train = [{"text": "t%d-%d" % (i, j), "label": lab} for i, lab in enumerate(labels) for j in range(3)]
        ev = [{"text": "e%d" % i, "label": lab} for i, lab in enumerate(labels)]
        files = dataset(task=json.dumps({"type": "choice", "instructions": "Which?",
                                         "criteria": {lab: None for lab in labels}}),
                        train=rows_text(train), ev=rows_text(ev))
        out.append(make("criteria-%d" % n, "%d criteria, 3 train rows and 1 eval row each" % n, files,
                        ok(files, [3] * n, 12, labels=labels), rust=verdict))
    return out


def cases_rows():
    """train / eval / shift rows: one case per rule data.py `load_rows` can raise, in its order, and the traps."""
    out = []
    base = train_rows(4)
    ev = eval_rows()

    def bad_train(cid, note, extra, rule, **kw):
        out.append(make(cid, note, dataset(train=rows_text(base) + extra, **kw), refused(rule)))

    def bad_eval(cid, note, extra, rule):
        out.append(make(cid, note, dataset(ev=rows_text(ev) + extra), refused(rule)))

    def bad_shift(cid, note, extra, rule):
        out.append(make(cid, note, dataset(shift=rows_text(ev) + extra), refused(rule)))

    out.append(make("train-missing", "no train.jsonl", drop(dataset(), "train.jsonl"), refused("train-missing")))
    out.append(make("order-train-error-before-eval-missing", "a bad train row AND no eval.jsonl: train wins",
                    drop(dataset(train=rows_text(base) + '{"text": "x", "label": "zzz"}\n'), "eval.jsonl"),
                    refused("train-row-label")))
    out.append(make("train-empty", "train.jsonl has zero bytes", dataset(train=""), refused("train-empty")))
    out.append(make("eval-empty", "eval.jsonl has zero bytes", dataset(ev=""), refused("eval-empty")))
    out.append(make("shift-empty", "shift.jsonl has zero bytes", dataset(shift=""), refused("shift-empty")))

    # row-schema: every way a line fails to be {text: non-empty string, label}
    bad_train("train-row-schema-blank-line", "a blank line in the middle", "\n" + rows_text(base[:1]),
              "train-row-schema")
    bad_eval("eval-row-schema-not-json", "a line that is not JSON", "{broken\n", "eval-row-schema")
    bad_shift("shift-row-schema-not-an-object", "a line that is a JSON array", "[1, 2]\n", "shift-row-schema")
    bad_train("train-row-schema-missing-key", "a row without label", '{"text": "only text"}\n', "train-row-schema")
    bad_train("train-row-schema-duplicate-key", "a row repeating text",
              '{"text": "x", "text": "y", "label": "a"}\n', "train-row-schema")
    bad_train("train-row-schema-empty-text", "a text of spaces", '{"text": "   ", "label": "a"}\n',
              "train-row-schema")
    bad_train("train-row-schema-text-only-u001c", "a text of only U+001C", '{"text": "\\u001c", "label": "a"}\n',
              "train-row-schema")
    bad_train("train-row-schema-text-only-u0085", "a text of only U+0085", '{"text": "\u0085", "label": "a"}\n',
              "train-row-schema")
    bad_train("train-row-schema-text-not-a-string", "a numeric text", '{"text": 5, "label": "a"}\n',
              "train-row-schema")
    out.append(make("train-row-schema-bom", "a UTF-8 BOM before the first row",
                    dataset(train="\ufeff" + rows_text(base)), refused("train-row-schema")))
    bad_train("train-row-schema-trailing-bare-cr-line", "the file ends with a lone CR line (not an LF line ending)",
              "\r", "train-row-schema")
    out.append(make("order-blank-before-bad-label", "line 2 blank and line 3 with a bad label: row-schema",
                    dataset(train=rows_text(base[:1]) + "\n" + '{"text": "x", "label": "zzz"}\n'),
                    refused("train-row-schema")))

    # unknown keys and labels
    bad_train("train-row-unknown-key", "a train row with an id key", '{"text": "x", "label": "a", "id": 1}\n',
              "train-row-unknown-key")
    bad_eval("eval-row-unknown-key", "an eval row with an id key", '{"text": "x", "label": "a", "id": 1}\n',
             "eval-row-unknown-key")
    bad_shift("shift-row-unknown-key", "a shift row with an id key", '{"text": "x", "label": "a", "id": 1}\n',
              "shift-row-unknown-key")
    bad_train("train-row-label-index", "a label given as an index", '{"text": "x", "label": 0}\n', "train-row-label")
    bad_eval("eval-row-label", "an eval label that is not a criterion", '{"text": "x", "label": "zzz"}\n',
             "eval-row-label")
    bad_shift("shift-row-label", "a shift label that is not a criterion", '{"text": "x", "label": "zzz"}\n',
              "shift-row-label")
    out.append(make("order-unknown-key-before-surrogate", "an unknown key AND a lone surrogate: unknown-key",
                    dataset(train=rows_text(base) + '{"text": "x \\ud800", "label": "a", "id": 1}\n'),
                    refused("train-row-unknown-key")))

    # encoding: an invalid byte (file level) and a lone surrogate escape (value level)
    blob = rows_text(ev[:3]).encode("utf-8") + b'{"text": "caf\xe9", "label": "a"}\n'
    out.append(make("eval-row-encoding-invalid-byte", "an invalid UTF-8 byte in eval.jsonl", dataset(ev=blob),
                    refused("eval-row-encoding")))
    out.append(make("shift-row-encoding-invalid-byte", "an invalid UTF-8 byte in shift.jsonl", dataset(shift=blob),
                    refused("shift-row-encoding")))
    bad_train("train-row-encoding-lone-surrogate-text", "a lone surrogate escape in a text",
              '{"text": "bad \\ud800 text", "label": "a"}\n', "train-row-encoding")
    bad_shift("shift-row-encoding-lone-surrogate-label", "a lone surrogate escape in a label",
              '{"text": "fine", "label": "\\udc00"}\n', "shift-row-encoding")
    bad_train("train-row-encoding-low-then-high-surrogate", "a low surrogate escape followed by a high one: both lone",
              '{"text": "bad \\udc00\\ud800 text", "label": "a"}\n', "train-row-encoding")
    return out


def cases_overlap_and_split():
    """overlap, conflicts, coverage, the calibration rules and the epoch rule."""
    out = []
    base = train_rows(4)
    ev = eval_rows()

    def overlap(cid, note, train_text, eval_text, rule="eval-train-overlap", shift=False):
        train = base + [{"text": train_text, "label": "a"}]
        other = ev + ([] if shift else [{"text": eval_text, "label": "a"}])
        files = dataset(train=rows_text(train), ev=rows_text(other),
                        shift=rows_text(ev[:2] + [{"text": eval_text, "label": "a"}]) if shift else None)
        out.append(make(cid, note, files, refused(rule)))

    overlap("eval-train-overlap-nfc-equal", "an eval text equal to a train text after NFC (e + U+0301 vs U+00E9)",
            "caf\u00e9 bar", "cafe\u0301 bar")
    overlap("eval-train-overlap-whitespace-equal", "equal after whitespace collapse (a tab and a run of spaces)",
            "row  one", "row\tone")
    overlap("eval-train-overlap-trim-equal", "equal after trim (leading and trailing spaces)", "row two", "  row two ")
    overlap("shift-train-overlap", "a shift text equal to a train text", "row three", "row three",
            rule="shift-train-overlap", shift=True)
    out.append(make("secret-eval-train-overlap", "the overlapping text is SECRET-OVERLAP-TEXT (never echoed)",
                    dataset(train=rows_text(base + [{"text": "SECRET-OVERLAP-TEXT", "label": "a"}]),
                            ev=rows_text(ev + [{"text": "SECRET-OVERLAP-TEXT", "label": "b"}])),
                    refused("eval-train-overlap")))
    out.append(make("secret-train-row-label", "a bad label on a SECRET-ROW-TEXT row (the text is never echoed)",
                    dataset(train=rows_text(base + [{"text": "SECRET-ROW-TEXT", "label": "SECRET-LABEL"}])),
                    refused("train-row-label")))
    out.append(make("secret-row-schema", "a truncated row starting SECRET-JSON-TEXT",
                    dataset(train=rows_text(base) + '{"text": "SECRET-JSON-TEXT'), refused("train-row-schema")))
    out.append(make("secret-train-conflicting-labels", "SECRET-CONFLICT-TEXT under two labels",
                    dataset(train=rows_text(base + [{"text": "SECRET-CONFLICT-TEXT", "label": "a"},
                                                    {"text": " SECRET-CONFLICT-TEXT", "label": "b"}])),
                    refused("train-conflicting-labels")))
    out.append(make("train-conflicting-labels-whitespace-equal", "equal after whitespace, different labels",
                    dataset(train=rows_text(base + [{"text": "train  a\t0", "label": "c"}])),
                    refused("train-conflicting-labels")))

    # class-too-small: order-independent (count <= max(min_per_class 2, ceil(0.25 * count))), same on both sides
    small = [r for r in base if r["label"] != "c"] + [{"text": "only c %d" % i, "label": "c"} for i in range(2)]
    out.append(make("train-class-too-small-two-rows", "a class with exactly calibration_slice_min_per_class rows",
                    dataset(train=rows_text(small)), refused("train-class-too-small")))
    none_c = [r for r in base if r["label"] != "c"]
    out.append(make("train-class-too-small-no-rows", "a criterion with no train row at all",
                    dataset(train=rows_text(none_c)), refused("train-class-too-small")))
    three = [r for r in base if r["label"] != "c"] + [{"text": "enough c %d" % i, "label": "c"} for i in range(3)]
    f = dataset(train=rows_text(three))
    out.append(make("ok-class-with-min-per-class-plus-one", "a class with 3 rows is accepted", f, ok(f, [4, 4, 3], 12)))
    # class-too-small: ORDER-DEPENDENT. Class a holds one text three times and one other text: whether the seeded
    # draw (seed 13) takes the single text first decides it. Python refuses; Rust cannot decide in the request path.
    dup = [{"text": "dup text", "label": "a"}] * 3 + [{"text": "solo a", "label": "a"}]
    rows = dup + [r for r in base if r["label"] != "a"]
    out.append(make("train-class-too-small-order-dependent",
                    "class a = one text x3 + one text x1 (count 4, need 2): the seed-13 draw takes the single text "
                    "first, then the triple, and the slice swallows the class",
                    dataset(train=rows_text(rows)), refused("train-class-too-small"), rust="deferred"))

    # check ORDER across files and stages (the trainer's step-1 order: eval overlap, then the shift rows and their
    # overlap, then the calibration split, then the epoch rule). Found by re-mutating validate_data_dir: moving the
    # shift check before the eval overlap survived every case above.
    clash = {"text": "train a 0", "label": "a"}
    out.append(make("order-eval-overlap-before-shift-row-error", "eval overlaps train AND a shift row has a bad label: "
                    "the eval overlap is raised first",
                    dataset(ev=rows_text(ev + [clash]), shift=rows_text(ev[:2]) + '{"text": "x", "label": "zzz"}\n'),
                    refused("eval-train-overlap")))
    out.append(make("order-eval-overlap-before-shift-overlap", "eval AND shift each overlap train: eval first",
                    dataset(ev=rows_text(ev + [clash]), shift=rows_text(ev[:2] + [{"text": "train b 0", "label": "b"}])),
                    refused("eval-train-overlap")))
    pair = [r for r in train_rows(20) if r["label"] != "c"] + [{"text": "c one", "label": "c"}, {"text": "c two", "label": "c"}]
    out.append(make("order-class-too-small-before-epochs", "a 2-row class AND epochs missing above 16 shots: the split "
                    "is checked before the epoch rule", dataset(train=rows_text(pair)), refused("train-class-too-small")))
    both = base + [{"text": "dup a", "label": "a"}, {"text": "dup a", "label": "b"}]
    out.append(make("order-conflicting-labels-before-too-small", "conflicting labels AND a 2-row class: conflicts first",
                    dataset(train=rows_text([r for r in both if r["label"] != "c"]
                                            + [{"text": "c%d" % i, "label": "c"} for i in range(2)])),
                    refused("train-conflicting-labels")))

    # accepted traps
    crlf = dataset(train=rows_text(base, "\r\n"), ev=rows_text(ev, "\r\n"))
    out.append(make("ok-crlf", "CRLF line endings in train and eval (the CR is stripped, WR-05)", crlf,
                    ok(crlf, [4, 4, 4], 12)))
    f = dataset(train=rows_text(base) + '{"text": "x bare cr",\r "label": "a"}\n')
    out.append(make("ok-bare-cr-between-tokens", "a bare CR between two JSON tokens is JSON whitespace", f,
                    ok(f, [5, 4, 4], 12)))
    seps = [{"text": "one tweet%swith U+%04X inside" % (cp, ord(cp)), "label": "a"}
            for cp in ("\u0085", "\u2028", "\u2029")]
    f = dataset(train=rows_text(base + seps))
    out.append(make("ok-u0085-u2028-u2029-inside-texts", "U+0085, U+2028 and U+2029 inside texts never split a row",
                    f, ok(f, [7, 4, 4], 12)))
    f = dataset(train=rows_text(base + [{"text": "na\u00efve caf\u00e9 \u65e5\u672c\u8a9e", "label": "b"}]))
    out.append(make("ok-non-ascii-text", "a BOM-free file with non-ASCII text", f, ok(f, [4, 5, 4], 12)))
    f = dataset(train=rows_text(base) + '{"text": "pair \\ud83d\\ude00 ok", "label": "a"}\n'
                + '{"text": "esc \\\\ud800 literal", "label": "b"}\n')
    out.append(make("ok-surrogate-pair-and-escaped-backslash",
                    "a high+low surrogate pair is an emoji; an escaped backslash before ud800 is plain text", f,
                    ok(f, [5, 5, 4], 12)))
    f = dataset(ev=rows_text(ev + [{"text": "TRAIN A 0", "label": "a"}]))
    out.append(make("ok-case-different-eval-text", "an eval text that differs from a train text only in case", f,
                    ok(f, [4, 4, 4], 12)))
    f = dataset(shift=rows_text([r for r in ev if r["label"] != "c"]).replace("eval", "shift"))
    out.append(make("ok-shift-missing-a-criterion", "a shift file with no row of criterion c is accepted", f,
                    ok(f, [4, 4, 4], 12)))
    shift = [{"text": "shift %s %d" % (lab, i), "label": lab} for lab in "abc" for i in range(2)]
    f = dataset(20, shift=rows_text(shift))
    out.append(make("ok-shift-with-epochs-required", "a shift file on a 20-shot dataset with epochs 4 (the 14-14 smoke's "
                    "shape)", f, ok(f, [20, 20, 20], 4), epochs=4))
    f = dataset(17)
    out.append(make("ok-17-shots-epochs-4", "17 shots per class is above 16: epochs 4 accepted", f,
                    ok(f, [17, 17, 17], 4), epochs=4))
    f = dataset(20)
    out.append(make("ok-20-shots-epochs-12", "epochs 12 is inside [4, 12]", f, ok(f, [20, 20, 20], 12), epochs=12))

    # the D-21 epoch cases that refuse
    out.append(make("epochs-above-16-below-range", "20 shots per class and epochs 3", dataset(20), refused("epochs"),
                    epochs=3))
    out.append(make("epochs-above-16-above-range", "20 shots per class and epochs 13", dataset(20), refused("epochs"),
                    epochs=13))
    return out


def all_cases():
    return cases_core() + cases_task() + cases_rows() + cases_overlap_and_split()


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
