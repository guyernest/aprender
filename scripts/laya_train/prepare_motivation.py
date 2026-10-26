"""The motivation CSV (the partner file) -> a Laya data dir, and the measured prefix of the condensed task (plan 12-01).

    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_motivation.py --draft a|b [--csv PATH] [--out DIR] [--no-pin]
    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_motivation.py --measure (--draft a|b | --task PATH) [--csv PATH] [--max-texts N]
    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_motivation.py --selftest | --selftest-tokenizer
    (or: just laya-prepare-motivation <the same arguments>)

PILOT (D-01): motivation-v1 is trained on Dr. Iris Yuster's classification guide, the 25 October 2026 draft (docx sha256
0e1d5a35ac3ccd455c5556fecbc5c1d1edec0e0a1100e2feac8bd16a42f29167). The guide is not frozen; the model is expected to be
retrained after the freeze. Nothing here is an accuracy claim: whether a fuller instruction helps is a HYPOTHESIS that only
the gate run measures.

Source: `datasets/motivation/Motivation Dataset - Sheet1.csv` of the partner repo motivation-experiment (default
`~/projects/mcp/motivation-experiment/datasets/motivation/`, override with --csv). It is READ IN PLACE and never copied into
this repo, never written next to. Header `Post (English, original),Class`.

DECLARED RULES (written before the run; the run only applies them):

    label map     `Class` "Social-affiliative" -> social_affiliative, "Conspiratorial explanation" -> conspiratorial,
                  "None" -> other. The criteria are listed in THAT order in task.json (D-16): document order is the
                  label index, so social_affiliative is label 0, conspiratorial 1, other 2.
    grouping      rows are grouped by data.normalize, i.e. the nfc-trim-ws-v1 text identity (NFC, trim, every Unicode
                  White_Space run -> one U+0020), hashed by data.normalized_sha256; the FIRST exact text of each group
                  is kept as the row text. No second normalizer exists here.
    RULE conflicting-labels
                  a group whose rows carry more than one label is DROPPED whole (a majority rule cannot resolve the
                  ties); the groups and the rows dropped are counted and reported.
    RULE split    seed 13 (numpy RandomState, the contract's declared seed), STRATIFIED by label and TEXT-DISJOINT
                  (group-disjoint): per class, ceil(0.25 x the class's groups) groups go to eval.jsonl, the rest to
                  train.jsonl, with EVAL_DIVISOR = 4 and the ceil done in integers; both files keep first-seen CSV
                  order. laya-finetune-gate-v1 has no eval-fraction key (the eval fraction is a constant of this
                  script), so D-13's contract-amendment branch does not fire; the 25 % fraction is declared HERE,
                  before the run. The calibration slice is NOT drawn here: train.py draws it from train.jsonl itself.
    no shift      no shift.jsonl is written: the requester's live labelled sample, when it exists, is a reported probe
                  for a later run.

What is asserted before anything is written (exactly what train.py asserts, via data.py's own validators): the rows
written are accepted by group_train, refuse_overlap and calibration_split, and all three criteria are present in eval.
Unless --no-pin is passed, the CSV must also be the pinned partner file (sha256 and counts below): a changed file fails
loudly (PREPARE FAILED) instead of silently moving the evaluation set.

NO TEXT: this script prints counts and sha256s only. A refusal names a 1-based row NUMBER (header excluded), a field and
at most a character count, never a field value (a misplaced post fragment in the Class column is still caller content); a
malformed header is reported by its column count, never echoed. Output lands under the root-anchored, gitignored /data/
(data/decide/motivation-v1 by default); post text is never committed or printed. Write-once: an existing non-empty --out
is refused unless every file already holds exactly the bytes this run would write (prepare_stance.write_once), so a re-run
is a no-op.

--measure is tokenizer-only (no model, no forward): it builds the task's empty-state row with the SERVED tokenizer (the
one measurement implementation of prepare_goldstandard.py, OPS-03) and reports P, the raw and kept token counts of the
head and of each option (exactly 48 tokens is kept; only more than 48 is a cut), the door this task would get, and, with
--csv, the post token distribution and the texts per call. An explicit --max-texts N is ENFORCED (REFUSED prefix-budget
when N x P exceeds the contract's classify_max_total_tokens). It always refuses a real cut (REFUSED prefix-cut) and a
door below experiment-coach's batch of 4 (REFUSED prefix-door, D-26).

The condensed guide (D-09): two drafts of Sections 4-6 of the guide, (a) short enough to keep the served door at 8 texts
(P at most 100) and (b) the fullest instruction the 192-token head keeps, with a per-artifact door of min(8, floor(800 / P)),
never below 4 (D-26). The owner chooses between them, and approves the exact task.json bytes, before anything trains.
"""
import argparse
import contextlib
import csv
import io
import json
import math
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import numpy as np  # noqa: E402
import contract  # noqa: E402
from common import jsonl_bytes, sha256_bytes, sha256_file  # noqa: E402
import data  # noqa: E402
from data import DataError, calibration_split, class_counts, group_train, normalized_sha256, refuse_overlap  # noqa: E402
from prepare_stance import write_once  # noqa: E402  (one write-once implementation, OPS-03)
# OPS-03: one measurement implementation. report() and read_rows() of that file are NOT imported (read_rows is bound to
# the antisemitism header); the generic pieces are.
from prepare_goldstandard import (  # noqa: E402
    DOOR_CEILING, HEAD_MAX, OPTION_MAX, ROW_MAX, PrepareFailed, _ids, contract_constant, door_fields, load_tokenizer,
    measure_task, report, tweet_report)

REPO = contract.REPO
DEFAULT_CSV = (Path.home() / "projects" / "mcp" / "motivation-experiment" / "datasets" / "motivation"
               / "Motivation Dataset - Sheet1.csv")
DEFAULT_OUT = REPO / "data" / "decide" / "motivation-v1"
CSV_HEADER = ["Post (English, original)", "Class"]
LABELS = ["social_affiliative", "conspiratorial", "other"]       # D-16: criteria document order is the label index
LABEL_OF = {"Social-affiliative": 0, "Conspiratorial explanation": 1, "None": 2}
SPLIT_SEED = 13                                         # RULE split: the contract's declared seed
EVAL_DIVISOR = 4                                        # RULE split: 0.25 = 1 / 4, ceil done in integers
SENTINEL = "ZQXJ-sentinel-motivation-7mw"               # planted in the selftest's synthetic rows only
STANCE_CONTROL_P = 57                                   # the stance task's measured prefix of record (a literal)
MIN_DOOR = 4                                            # experiment-coach sends batches of 4 (D-26)
GUIDE_DOCX_SHA256 = "0e1d5a35ac3ccd455c5556fecbc5c1d1edec0e0a1100e2feac8bd16a42f29167"
GUIDE_DESIGNATION = "25 October 2026 draft"

# The pinned partner file (RESEARCH "Dataset and guide facts", measured 2026-10-26): a changed CSV fails loudly (T-12-03).
PINNED = {"rows_in": 395, "distinct_texts": 395, "duplicates_collapsed": 0,
          "mixed_label_groups_dropped": 0, "mixed_label_rows_dropped": 0,
          "csv_sha256": "bc61c76bd29bcdd4c6466266b4e1161841f2107a4f02f5ff4fd96f1b7a3a4ae7"}

# The two condensed drafts of guide Sections 4-6 (D-09), held side by side until the owner chooses (plan 12-01 Task 2).
GUIDES = {
    "a": {
        "instructions": (
            "Which function does this post serve? Social affiliative: signals belonging to a group (we, our people, calls "
            "to join, mocking critics). Conspiratorial: explains events by hidden control or secret orchestration by a "
            "group. Other: neither clear, weak or both. If unsure, choose other."),
        "descriptions": {
            "social_affiliative": "group belonging or loyalty",
            "conspiratorial": "hidden control by a group",
            "other": "neither clear, weak or mixed",
        },
    },
    "b": {
        "instructions": (
            "Classify the motivational function this post expresses. Social affiliative: the post signals membership in, "
            "alignment with or loyalty to a group. Cues: we, us, our people; a stated group identity; they or these people "
            "as antagonist; everyone knows; community slogans; calls to share, join or act; mocking outsiders or critics. "
            "Conspiratorial: the post explains events or institutions by hidden control, ownership or secret orchestration "
            "by a group. Cues: own, control, puppet, takes orders from; behind, staged, caused. Open criticism of public "
            "actions is not conspiratorial. Other: neither function is clearly expressed, evidence is weak, or both are "
            "clear. Judge only the written words. When unsure choose other."),
        "descriptions": {
            "social_affiliative": "group membership, loyalty or alignment",
            "conspiratorial": "hidden control or secret orchestration",
            "other": "neither clear, weak or mixed evidence",
        },
    },
}


# ------------------------------------------------------------------------------------------------ the CSV

def read_rows(path):
    """[(row number, text, Class)] of the CSV, row numbers 1-based with the header excluded.

    Refusals (DataError -> one `REFUSED <rule>` line, exit 2) name a row number, a field and at most a character count,
    never a field value: csv-missing, csv-encoding, csv-header (reported by column count only), csv-row, csv-label,
    csv-text."""
    path = Path(path)
    if not path.is_file():
        raise DataError("csv-missing", "%s is not a file; the partner CSV lives in motivation-experiment datasets/motivation/" % path)
    rows = []
    reader = None
    try:
        with open(path, newline="", encoding="utf-8-sig") as f:
            reader = csv.reader(f)
            header = next(reader, None)
            if header != CSV_HEADER:
                raise DataError("csv-header", "the header row has %d column(s) and is not the expected %d-column header"
                                % (len(header or []), len(CSV_HEADER)))
            for n, rec in enumerate(reader, 1):
                if len(rec) != len(CSV_HEADER):
                    raise DataError("csv-row", "row %d has %d field(s), expected %d" % (n, len(rec), len(CSV_HEADER)))
                text, label = rec
                if label not in LABEL_OF:
                    raise DataError("csv-label", "row %d field Class: an unknown label of %d characters, expected one of %s"
                                    % (n, len(label), ", ".join(LABEL_OF)))
                if not text.strip():
                    raise DataError("csv-text", "row %d field Post is empty" % n)
                rows.append((n, text, label))
    except UnicodeDecodeError as e:
        raise DataError("csv-encoding", "%s is not valid UTF-8 (byte %d: %s)" % (path.name, e.start, e.reason))
    except csv.Error:
        raise DataError("csv-row", "%s cannot be parsed as CSV near line %d" % (path.name, reader.line_num if reader else 0))
    if not rows:
        raise DataError("csv-row", "%s has a header and no rows" % path.name)
    return rows


def eval_count(n_groups):
    """ceil(0.25 x n_groups), exactly, in integers."""
    return (n_groups + EVAL_DIVISOR - 1) // EVAL_DIVISOR


def build(csv_path, task):
    """({file name: bytes}, counts-only summary) for the CSV and the task dict {instructions, descriptions}."""
    csv_path = Path(csv_path)
    rows = read_rows(csv_path)
    groups = {}                                         # normalized hash -> [first exact text, [labels]]
    for _, text, label in rows:
        g = groups.setdefault(normalized_sha256(text), [text, []])
        g[1].append(LABEL_OF[label])
    mixed = [g for g in groups.values() if len(set(g[1])) > 1]
    kept = [(g[0], g[1][0]) for g in groups.values() if len(set(g[1])) == 1]      # first-seen order
    rng = np.random.RandomState(SPLIT_SEED)
    in_eval = set()
    for c in range(len(LABELS)):
        idx = [i for i, (_, y) in enumerate(kept) if y == c]
        order = rng.permutation(len(idx))
        in_eval.update(idx[j] for j in order[:eval_count(len(idx))])
    train_rows = [kept[i] for i in range(len(kept)) if i not in in_eval]
    eval_rows = [kept[i] for i in sorted(in_eval)]
    # What train.py asserts, on the rows about to be written (data.py's own validators, never a copy of them).
    group_train(train_rows)
    refuse_overlap(train_rows, eval_rows)
    counts = lambda rs: {LABELS[c]: sum(1 for _, y in rs if y == c) for c in range(len(LABELS))}  # noqa: E731
    train_counts, eval_counts = counts(train_rows), counts(eval_rows)
    if min(eval_counts.values()) == 0:
        raise PrepareFailed("eval.jsonl would hold no row of some criterion: %s" % eval_counts)
    if min(train_counts.values()) == 0:
        raise PrepareFailed("train.jsonl would hold no row of some criterion: %s" % train_counts)
    calibration_split(train_rows, contract.constant("calibration_slice_fraction", "float"),
                      contract.constant("calibration_slice_min_per_class", "int"), SPLIT_SEED, len(LABELS))
    task_doc = {"type": "choice", "instructions": task["instructions"], "criteria": dict(task["descriptions"])}
    if list(task_doc["criteria"]) != LABELS:
        raise PrepareFailed("the task's criteria %s are not %s in that order" % (list(task_doc["criteria"]), LABELS))
    named = lambda rs: [(t, LABELS[y]) for t, y in rs]  # noqa: E731
    files = {"task.json": (json.dumps(task_doc, ensure_ascii=False, indent=2) + "\n").encode("utf-8"),
             "train.jsonl": jsonl_bytes(named(train_rows)),
             "eval.jsonl": jsonl_bytes(named(eval_rows))}
    summary = {"rows_in": len(rows), "distinct_texts": len(groups), "duplicates_collapsed": len(rows) - len(groups),
               "mixed_label_groups_dropped": len(mixed), "mixed_label_rows_dropped": sum(len(g[1]) for g in mixed),
               "train": train_counts, "eval": eval_counts, "split_seed": SPLIT_SEED, "eval_divisor": EVAL_DIVISOR,
               "csv_sha256": sha256_file(csv_path)}
    return files, summary


def check_pin(summary):
    """The CSV must be the pinned partner file (T-12-03); PrepareFailed names the differing keys, never content."""
    bad = [k for k, v in PINNED.items() if summary[k] != v]
    if bad:
        raise PrepareFailed("the CSV is not the pinned partner file (differs in %s); pass --no-pin only for a deliberate "
                            "update of PINNED" % ", ".join(bad))


def draft_task(draft):
    if draft not in GUIDES:
        raise DataError("draft-missing", "--draft a or --draft b is required (the owner has not chosen yet)")
    return GUIDES[draft]


# ------------------------------------------------------------------------------------------------ the served prefix

def post_token_lengths(tok, csv_path):
    """Token count of every CSV post, in file order (the state side of a built row), over THIS script's reader."""
    mask = tok.mask_token
    texts = [t.replace(mask, " ") for _, t, _ in read_rows(csv_path)]
    return [len(x) for x in _ids(tok, texts)]


def measure_report(tok, instructions, criteria, max_texts=None, csv_path=None):
    """The --measure JSON object. The refusals prefix-cut, prefix-row and (only for an explicit max_texts) prefix-budget
    come from prepare_goldstandard.report; prefix-door (a door below experiment-coach's batch of 4) is added here."""
    out = report(tok, instructions, criteria, max_texts, None)
    p = out["P"]
    total = contract_constant("classify_max_total_tokens")
    fit = min(DOOR_CEILING, total // p)
    if total // p < MIN_DOOR:
        raise DataError("prefix-door", "floor(%d / %d) = %d < %d (experiment-coach's batch)" % (total, p, total // p, MIN_DOOR))
    door_b = fit if p > 100 else DOOR_CEILING
    out.update({"max_texts_that_fit": fit, "door_b": door_b, "door_b_at_least_4": door_b >= MIN_DOOR})
    if csv_path is not None:
        lengths = post_token_lengths(tok, csv_path)
        rep = tweet_report(lengths, p, total)
        out["post_tokens"] = rep.pop("tweet_tokens")
        out.update(rep)
        pt = out["post_tokens"]
        out["batch_of_4"] = {k: 4 * (p + pt[k]) <= total for k in ("p50", "p95", "max")}
    return out


# ------------------------------------------------------------------------------------------------ self-tests

def _synthetic_csv(path, header=None):
    """A made-up CSV (never posts) with the sentinel in every text: an exact duplicate, a whitespace variant, an NFC
    variant, two mixed-label groups. Returns the expected counts."""
    rows = []
    for i in range(24):
        rows.append(("synthetic sentence %03d %s about topic %d" % (i, SENTINEL, i % 7), "Social-affiliative"))
    for i in range(16):
        rows.append(("made-up claim %03d %s regarding subject %d" % (i, SENTINEL, i % 5), "Conspiratorial explanation"))
    for i in range(60):
        rows.append(("plain note %03d %s concerning item %d" % (i, SENTINEL, i % 9), "None"))
    rows.append((rows[3][0], "Social-affiliative"))                              # exact duplicate
    rows.append(("  " + rows[45][0].replace(" ", "  ") + " ", "None"))            # whitespace variant of another text
    rows.append(("café " + SENTINEL + " nfc variant a", "None"))
    rows.append(("café " + SENTINEL + " nfc variant a", "None"))              # NFC variant of the line above
    rows.append(("conflicting made-up text one " + SENTINEL, "Social-affiliative"))
    rows.append(("conflicting made-up text one " + SENTINEL, "None"))
    rows.append(("conflicting made-up text two " + SENTINEL, "Conspiratorial explanation"))
    rows.append(("conflicting made-up text two " + SENTINEL, "None"))
    rows.append(("conflicting made-up text two " + SENTINEL, "None"))
    with open(path, "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(header if header is not None else CSV_HEADER)
        for text, label in rows:
            w.writerow([text, label])
    # distinct under nfc-trim-ws-v1: 24 + 16 + 60 + 1 (nfc pair) + 2 (mixed groups) = 103; rows 100 + 2 + 2 + 2 + 3 = 109
    return {"rows_in": len(rows), "distinct_texts": 103, "duplicates_collapsed": len(rows) - 103,
            "mixed_label_groups_dropped": 2, "mixed_label_rows_dropped": 5,
            "classes": {"social_affiliative": 24, "conspiratorial": 16, "other": 61}}


def _write(path, body):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(body)
    return path


def _mutate(src, dst, fn):
    """Copy a synthetic CSV, applying fn(record) to every data record (header kept)."""
    with open(src, newline="", encoding="utf-8") as f, open(dst, "w", newline="", encoding="utf-8") as g:
        r, w = csv.reader(f), csv.writer(g)
        w.writerow(next(r))
        for rec in r:
            fn(rec)
            w.writerow(rec)


def _capture(argv):
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        try:
            rc = main(argv)
        except SystemExit as e:
            rc = e.code
    return rc, out.getvalue(), err.getvalue()


def selftest():
    """The declared rule over a synthetic CSV (no post text): duplicate collapse, the mixed-label drop, the seeded
    stratified text-disjoint split, determinism, write-once, every refusal and the no-text promise over captured stdout
    and stderr. numpy + pyyaml only (no torch, no tokenizer)."""
    failures = []

    def case(name, ok, detail=""):
        print("  %-4s %-72s %s" % ("ok" if ok else "FAIL", name, detail))
        if not ok:
            failures.append(name)

    def refuses(name, rule, fn):
        try:
            fn()
            case(name, False, "accepted")
        except DataError as e:
            case(name, e.rule == rule, str(e)[:100])
        except Exception as e:                           # a traceback-class escape is a FAIL, never a crash
            case(name, False, "raised %s, not a DataError %s" % (type(e).__name__, rule))

    guide = GUIDES["a"]
    with tempfile.TemporaryDirectory(prefix="motivation-selftest-") as tmp:
        tmp = Path(tmp)
        good = tmp / "good.csv"
        want = _synthetic_csv(good)
        print("the declared rule:")
        files, s = build(good, guide)
        case("rows_in, distinct_texts, duplicates_collapsed", (s["rows_in"], s["distinct_texts"], s["duplicates_collapsed"])
             == (want["rows_in"], want["distinct_texts"], want["duplicates_collapsed"]),
             json.dumps({k: s[k] for k in ("rows_in", "distinct_texts", "duplicates_collapsed")}))
        case("mixed-label groups and rows dropped", (s["mixed_label_groups_dropped"], s["mixed_label_rows_dropped"]) == (2, 5))
        task = data.load_task(_write(tmp / "t" / "task.json", files["task.json"]))
        case("task.json lists social_affiliative, conspiratorial, other in that order (D-16)", task["labels"] == LABELS, str(task["labels"]))
        tr = data.load_rows(_write(tmp / "t" / "train.jsonl", files["train.jsonl"]), task, "train")
        ev = data.load_rows(_write(tmp / "t" / "eval.jsonl", files["eval.jsonl"]), task, "eval")
        refuse_overlap(tr, ev)
        group_train(tr)
        case("the written dir passes data.py's load_task / load_rows / refuse_overlap / group_train", True)
        case("eval and train are disjoint after nfc-trim-ws-v1",
             not ({normalized_sha256(t) for t, _ in tr} & {normalized_sha256(t) for t, _ in ev}))
        case("every label is in eval", {y for _, y in ev} == {0, 1, 2})
        counts_all = {c: sum(1 for _, y in tr if y == c) + sum(1 for _, y in ev if y == c) for c in range(3)}
        case("class totals are the distinct kept groups", counts_all == {i: want["classes"][n] for i, n in enumerate(LABELS)}, str(counts_all))
        case("per-class eval count == ceil(0.25 x class groups)",
             all(sum(1 for _, y in ev if y == c) == math.ceil(0.25 * counts_all[c]) for c in range(3)),
             str({c: sum(1 for _, y in ev if y == c) for c in range(3)}))
        slice_fit = calibration_split(tr, contract.constant("calibration_slice_fraction", "float"),
                                      contract.constant("calibration_slice_min_per_class", "int"), SPLIT_SEED, 3)
        case("train.py's calibration split succeeds on the written rows", len(slice_fit[1]) > 0 and sum(class_counts(tr, 3)) == len(tr))
        case("split seed is the contract's declared seed", SPLIT_SEED == contract.declared_seed() == 13)
        case("EVAL_DIVISOR is 4", EVAL_DIVISOR == 4)
        files2, s2 = build(good, guide)
        case("two builds are byte-identical (determinism)", files == files2 and s == s2)
        case("no shift.jsonl is built", "shift.jsonl" not in files)

        print("write-once:")
        out = tmp / "out"
        case("first write", write_once(out, files) == "written")
        case("a re-run is unchanged (byte-identical)", write_once(out, files) == "unchanged (byte-identical)")
        (out / "train.jsonl").write_bytes(files["train.jsonl"] + b" ")
        refuses("a differing byte is refused", "prepare-out-dir", lambda: write_once(out, files))
        out2 = tmp / "out2"
        write_once(out2, files)
        (out2 / "shift.jsonl").write_bytes(b"")
        refuses("a foreign file is refused", "prepare-out-dir", lambda: write_once(out2, files))

        print("refusals:")
        bad_header = tmp / "bad-header.csv"
        _synthetic_csv(bad_header, header=["Post", "Class", "Extra"])
        refuses("csv-header", "csv-header", lambda: read_rows(bad_header))
        refuses("csv-missing", "csv-missing", lambda: read_rows(tmp / "absent.csv"))
        bad_label = tmp / "bad-label.csv"
        _mutate(good, bad_label, lambda r: r.__setitem__(1, "Unknown") if "003" in r[0] else None)
        refuses("csv-label", "csv-label", lambda: read_rows(bad_label))
        empty_text = tmp / "empty-text.csv"
        _mutate(good, empty_text, lambda r: r.__setitem__(0, "  ") if "003" in r[0] else None)
        refuses("csv-text", "csv-text", lambda: read_rows(empty_text))
        short = tmp / "short.csv"
        short.write_text('"Post (English, original)",Class\nonly one field\n', encoding="utf-8")
        refuses("csv-row (a short record)", "csv-row", lambda: read_rows(short))
        not_utf8 = tmp / "not-utf8.csv"
        not_utf8.write_bytes(('"Post (English, original)",Class\n"row %s ' % SENTINEL).encode("utf-8") + b"\xff\xfe" + b'",None\n')
        refuses("csv-encoding", "csv-encoding", lambda: read_rows(not_utf8))
        huge = tmp / "huge.csv"
        with open(huge, "w", newline="", encoding="utf-8") as f:
            w = csv.writer(f)
            w.writerow(CSV_HEADER)
            w.writerow([SENTINEL + " " + "a" * (csv.field_size_limit() + 10), "None"])
        refuses("csv-row (a CSV parser error: a field over the field-size limit)", "csv-row", lambda: read_rows(huge))
        try:
            check_pin(s)
            case("the pin refuses a CSV that is not the partner file", False, "accepted")
        except PrepareFailed:
            case("the pin refuses a CSV that is not the partner file", True)
        refuses("an absent draft is refused", "draft-missing", lambda: draft_task(None))

        print("no text on stdout or stderr (sentinel %s):" % SENTINEL)
        base = ["--draft", "a", "--no-pin"]
        rc, o, e = _capture(["--csv", str(good), "--out", str(tmp / "out-a")] + base)
        case("success path: exit 0, PREPARE OK, no sentinel", rc == 0 and "PREPARE OK" in o and SENTINEL not in o + e, "exit %s" % rc)
        rc, o, e = _capture(["--csv", str(good), "--out", str(tmp / "out-pin"), "--draft", "a"])
        case("a CSV that is not the pinned file: exit 1, no sentinel, nothing written",
             rc == 1 and "PREPARE FAILED" in e and SENTINEL not in o + e and not (tmp / "out-pin").exists(), "exit %s" % rc)
        sentinel_header = tmp / "sentinel-header.csv"
        sentinel_header.write_text("%s,not-the-header\n%s,None\n" % (SENTINEL, SENTINEL), encoding="utf-8")
        rc, o, e = _capture(["--csv", str(sentinel_header), "--out", str(tmp / "out-b")] + base)
        case("csv-header path (the header holds the sentinel): exit 2, no sentinel", rc == 2 and "REFUSED csv-header" in e and SENTINEL not in o + e, "exit %s" % rc)
        sentinel_row = tmp / "sentinel-row.csv"
        sentinel_row.write_text('"Post (English, original)",Class\n"%s only one field"\n' % SENTINEL, encoding="utf-8")
        rc, o, e = _capture(["--csv", str(sentinel_row), "--out", str(tmp / "out-c")] + base)
        case("csv-row path: exit 2, no sentinel", rc == 2 and "REFUSED csv-row" in e and SENTINEL not in o + e, "exit %s" % rc)
        sentinel_label = tmp / "sentinel-label.csv"
        sentinel_label.write_text('"Post (English, original)",Class\nsome made-up text,%s\n' % SENTINEL, encoding="utf-8")
        rc, o, e = _capture(["--csv", str(sentinel_label), "--out", str(tmp / "out-d")] + base)
        case("csv-label path (the Class field holds the sentinel, %d characters): names row, field Class, a count only" % len(SENTINEL),
             rc == 2 and "REFUSED csv-label" in e and "row 1 field Class" in e and "%d characters" % len(SENTINEL) in e and SENTINEL not in o + e,
             "exit %s" % rc)
        sentinel_empty = tmp / "sentinel-empty.csv"
        sentinel_empty.write_text('"Post (English, original)",Class\n   ,None\n', encoding="utf-8")
        rc, o, e = _capture(["--csv", str(sentinel_empty), "--out", str(tmp / "out-e")] + base)
        case("csv-text path: exit 2, row number only", rc == 2 and "REFUSED csv-text" in e and "row 1" in e and SENTINEL not in o + e, "exit %s" % rc)
        rc, o, e = _capture(["--csv", str(not_utf8), "--out", str(tmp / "out-f")] + base)
        case("csv-encoding path (invalid UTF-8 in a sentinel row): exit 2, no sentinel", rc == 2 and "REFUSED csv-encoding" in e and SENTINEL not in o + e, "exit %s" % rc)
        rc, o, e = _capture(["--csv", str(huge), "--out", str(tmp / "out-g")] + base)
        case("CSV parser-error path (a sentinel row): exit 2, no sentinel", rc == 2 and "REFUSED csv-row" in e and "cannot be parsed as CSV" in e and SENTINEL not in o + e, "exit %s" % rc)
        rc, o, e = _capture(["--csv", str(tmp / "absent.csv"), "--out", str(tmp / "out-h")] + base)
        case("csv-missing path: exit 2, no sentinel", rc == 2 and "REFUSED csv-missing" in e and SENTINEL not in o + e, "exit %s" % rc)
        # The detector itself: prove the capture sees both streams, so the cases above cannot pass vacuously.
        probe_out, probe_err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(probe_out), contextlib.redirect_stderr(probe_err):
            print(SENTINEL)
            print(SENTINEL, file=sys.stderr)
        case("control: the capture sees a sentinel on stdout and on stderr", SENTINEL in probe_out.getvalue() and SENTINEL in probe_err.getvalue())

    print("%d case(s) failed" % len(failures) if failures else "all cases ok")
    if not failures:
        print("PREPARE MOTIVATION SELFTEST OK")
    return len(failures)


def selftest_tokenizer():
    """The measurement against the SERVED tokenizer (no model): exactly 48 option tokens is kept and 49 is a cut, an
    over-long head is a head cut, the stance task measures its prefix of record (a literal 57) with 3 markers, and a bare
    three-criteria task measures 3 markers."""
    tok = load_tokenizer()
    failures = []

    def case(name, ok, detail=""):
        print("  %-4s %-72s %s" % ("ok" if ok else "FAIL", name, detail))
        if not ok:
            failures.append(name)

    def three(desc, instructions="Which function does this post serve?"):
        return measure_task(tok, instructions, {"social_affiliative": "a", "conspiratorial": "b", "other": desc})

    n48 = next((n for n in range(1, 120) if three(" ".join(["the"] * n))["options"][2]["raw_tokens"] == 48), None)
    case("a repeated one-token word reaches an option of exactly 48 raw tokens", n48 is not None, "words %s" % n48)
    if n48 is not None:
        m48, m49 = three(" ".join(["the"] * n48)), three(" ".join(["the"] * (n48 + 1)))
        o48, o49 = m48["options"][2], m49["options"][2]
        case("exactly 48 tokens: kept 48, not a cut", o48["raw_tokens"] == OPTION_MAX and o48["kept_tokens"] == OPTION_MAX and o48["cut"] is False, json.dumps(o48))
        case("49 tokens: kept 48, a cut", o49["raw_tokens"] == OPTION_MAX + 1 and o49["kept_tokens"] == OPTION_MAX and o49["cut"] is True, json.dumps(o49))
    long_head = three("b", instructions=" ".join(["word"] * 400))
    case("a 400-word instruction is a head cut", long_head["head_cut"] is True and long_head["head_kept_tokens"] < long_head["head_raw_tokens"],
         "raw %d kept %d" % (long_head["head_raw_tokens"], long_head["head_kept_tokens"]))
    case("the head budget is %d tokens" % HEAD_MAX, long_head["head_kept_tokens"] <= HEAD_MAX, str(long_head["head_kept_tokens"]))
    stance_task = data.load_task(REPO / "data" / "decide" / "tweet-stance-64" / "task.json")
    stance = measure_task(tok, stance_task["instructions"], stance_task["criteria"])
    case("the stance task measures STANCE_CONTROL_P 57 (a literal) with 3 markers",
         stance["P"] == STANCE_CONTROL_P and stance["markers"] == 3, "P %d markers %d" % (stance["P"], stance["markers"]))
    case("the stance task has no head or option cut", not stance["head_cut"] and not any(o["cut"] for o in stance["options"]))
    bare = measure_task(tok, "Which function does this post serve?", {n: None for n in LABELS})
    case("a three-criteria bare task measures 3 markers", bare["markers"] == 3 and not bare["head_cut"], "P %d" % bare["P"])
    print("%d case(s) failed" % len(failures) if failures else "all cases ok")
    if not failures:
        print("PREPARE MOTIVATION TOKENIZER SELFTEST OK")
    return len(failures)


# ------------------------------------------------------------------------------------------------ main

def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--csv", default=None, help="the partner CSV (prepare mode default: the partner repo path; --measure reads a CSV only when given)")
    ap.add_argument("--out", default=None, help="data dir (default data/decide/motivation-v1)")
    ap.add_argument("--draft", choices=sorted(GUIDES), default=None, help="the condensed guide to build or measure (a or b)")
    ap.add_argument("--task", default=None, help="--measure: a task.json file instead of a draft")
    ap.add_argument("--measure", action="store_true", help="measure the task's prefix with the served tokenizer and print ONE JSON line")
    ap.add_argument("--max-texts", type=int, default=None, help="--measure: ENFORCE this served door (REFUSED prefix-budget when N x P exceeds the total)")
    ap.add_argument("--no-pin", action="store_true", help="accept a CSV that is not the pinned partner file")
    ap.add_argument("--selftest", action="store_true", help="the declared rule over a synthetic CSV, torch-free")
    ap.add_argument("--selftest-tokenizer", action="store_true", help="the measurement against the served tokenizer (no model)")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    try:
        if args.selftest_tokenizer:
            return selftest_tokenizer()
        if args.measure:
            return measure_main(ap, args)
        csv_path = Path(args.csv) if args.csv else DEFAULT_CSV
        out = Path(args.out) if args.out else DEFAULT_OUT
        files, summary = build(csv_path, draft_task(args.draft))
        if not args.no_pin:
            check_pin(summary)
        status = write_once(out, files)
    except DataError as e:                              # every input refusal: one REFUSED <rule> line, exit 2, no traceback
        print(str(e), file=sys.stderr)
        return 2
    except PrepareFailed as e:
        print("PREPARE FAILED: " + str(e), file=sys.stderr)
        return 1
    try:
        where = out.resolve().relative_to(REPO)
    except ValueError:
        where = out
    print("prepared %s (%s): %s" % (where, status, json.dumps(summary, sort_keys=True)))
    for name in sorted(files):
        print("  %-12s %5d lines  sha256 %s" % (name, files[name].count(b"\n"), sha256_bytes(files[name])))
    print("PREPARE OK")
    return 0


def measure_main(ap, args):
    if args.task:
        task = data.load_task(args.task)
        instructions, criteria = task["instructions"], task["criteria"]
    else:
        guide = draft_task(args.draft)
        instructions, criteria = guide["instructions"], guide["descriptions"]
    if args.max_texts is not None and args.max_texts < 1:
        ap.error("--max-texts must be at least 1")
    tok = load_tokenizer()
    csv_path = Path(args.csv) if args.csv else None
    print(json.dumps(measure_report(tok, instructions, criteria, args.max_texts, csv_path)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
