"""GoldStandard2024 (the partner CSV) -> a Laya data dir, and the measured prefix of the condensed task (plan 10-01).

    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_goldstandard.py [--csv PATH] [--out DIR] [--no-pin]
    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_goldstandard.py --measure [--task PATH] [--csv PATH] [--max-texts N]
    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_goldstandard.py --selftest | --selftest-tokenizer
    (or: just laya-prepare-goldstandard <the same arguments>)

Source: `datasets/antisemite/GoldStandard2024.csv` of the partner repo motivation-experiment
(default `~/projects/mcp/motivation-experiment/datasets/antisemite/`, override with --csv). It is READ IN PLACE and
never copied into this repo, never written next to. Header `ID,Username,CreateDate,Biased,Keyword,Text`.

DECLARED RULES (written before the run; the run only applies them):

    label map     `Biased` "0" -> not_antisemitic, "1" -> antisemitic. The criteria are listed in THAT order in
                  task.json (D-02): document order is the label index, so not_antisemitic is label 0.
    grouping      rows are grouped by data.normalize, i.e. the nfc-trim-ws-v1 text identity (NFC, trim, every Unicode
                  White_Space run -> one U+0020), hashed by data.normalized_sha256; the FIRST exact text of each
                  group is kept as the row text. No second normalizer exists here.
    RULE conflicting-labels
                  a group whose rows carry both labels is DROPPED whole (a majority rule cannot resolve the exact
                  ties); the groups and the rows dropped are counted and reported.
    RULE split    seed 13 (numpy RandomState, the contract's declared seed), STRATIFIED by label and GROUP-DISJOINT:
                  per class, ceil(0.2 x the class's groups) groups go to eval.jsonl, the rest to train.jsonl; both
                  files keep first-seen CSV order. The calibration slice is NOT drawn here: train.py draws it from
                  train.jsonl itself.
    no shift      no shift.jsonl is written (D-04): the requester's live labelled sample, when it exists, is a reported
                  probe for a later run.

What is asserted before anything is written (exactly what train.py asserts, via data.py's own validators): the rows
written are accepted by group_train, refuse_overlap, calibration_split, both criteria are present in eval, and the
largest class has more than 16 rows (so the run needs --epochs). Unless --no-pin is passed, the CSV must also be the
pinned partner file (sha256 and counts below): a changed file fails loudly (PREPARE FAILED) instead of silently
moving the evaluation set.

NO TEXT: this script prints counts and sha256s only. A refusal names a row NUMBER (1-based, header excluded) and a
field, never row content; a malformed header is reported by its column count, never echoed. Output lands under the
root-anchored, gitignored /data/ (data/decide/goldstandard-2024 by default); tweet text is never committed or printed.
Write-once: an existing non-empty --out is refused unless every file already holds exactly the bytes this run would
write (prepare_stance.write_once), so a re-run is a no-op.

--measure is tokenizer-only (no model, no forward): it builds the task's empty-state row with the SERVED tokenizer
(laya.common.build_sequence(tok, "", q, 512, 192), the method that reproduces the contract's 57 for the stance task)
and reports P, the raw and kept token counts of the head and of each option (an option of exactly 48 tokens is kept;
only more than 48 is a cut), the largest door the task admits, and, with --csv, the tweet token distribution and the
texts per call. The served door N is a PARAMETER of the report, not a hard-coded 8: by default it is read from the
contract's `classify_max_texts`; an explicit --max-texts N is ENFORCED (REFUSED prefix-budget when N x P exceeds the
contract's classify_max_total_tokens). It always refuses a real cut (REFUSED prefix-cut): instructions past the head
budget or a description past 48 tokens are cut silently by the served builder while the artifact still hashes the
full task.json.

The condensed guide (D-05, D-06) is derived from Jikeli, Karali, Miehling and Soemer, "Antisemitic Messages? A Guide
to High-Quality Annotation and a Labeled Dataset of Tweets", arXiv 2304.14599 (abstract
https://arxiv.org/abs/2304.14599). Fetched 2026-10-21 from https://arxiv.org/pdf/2304.14599: 357202 bytes, fetched paper
bytes sha256 75fbaabdba7f759542a30f0f9dfdf76fe06ddb552f69ba68177ea1f43921ba30 (a later re-fetch can differ).
What that paper contains and does not: it applies the IHRA Working Definition of Antisemitism, divided into 12
paragraphs, and it states the labelling rules the dataset follows (calling out or reporting antisemitism is NOT
antisemitic; Holocaust distortions mostly fall outside IHRA and carry their own label; a tweet felt to be antisemitic
with no applicable section of the definition is not antisemitic; "probably not" and "don't know" count as not
antisemitic). The 12-paragraph annotation guideline itself is a separate document the paper cites (Jikeli, Cavar,
Miehling 2019, arXiv 1910.01214), so the approved text below paraphrases the IHRA definition's examples as the paper applies
them; whether a fuller guide improves accuracy is a HYPOTHESIS measured only by the gate run (D-07).

APPROVED 2026-10-21 by the owner (plan 10-01 Task 2): door-8 p90; door 8 (chosen_max_texts 8, no contract, runtime, ledger
or documentation change); measured P 86. The owner approved the exact bytes of INSTRUCTIONS and DESCRIPTIONS below,
not_antisemitic first, and did NOT ask for a redraft from arXiv 1910.01214. These strings paraphrase the IHRA working
definition's examples with the labelling rules of arXiv 2304.14599; that paper cites the 12-paragraph annotation guideline
separately (1910.01214, not fetched). A fuller guide is a HYPOTHESIS for accuracy, measured only by the gate run (D-07):
the owner chose the draft that keeps the door at 8. With no --task, --measure measures this approved task.
"""
import argparse
import contextlib
import csv
import io
import json
import math
import os
import re
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import numpy as np  # noqa: E402
import contract  # noqa: E402
from common import jsonl_bytes, sha256_bytes, sha256_file  # noqa: E402
import data  # noqa: E402
from data import DataError, group_train, normalized_sha256, refuse_overlap  # noqa: E402
from prepare_stance import write_once  # noqa: E402  (one write-once implementation, OPS-03)
import prepare_stance  # noqa: E402  (only its stance task, the tokenizer control)

REPO = contract.REPO
DEFAULT_CSV = Path.home() / "projects" / "mcp" / "motivation-experiment" / "datasets" / "antisemite" / "GoldStandard2024.csv"
DEFAULT_OUT = REPO / "data" / "decide" / "goldstandard-2024"
CSV_HEADER = ["ID", "Username", "CreateDate", "Biased", "Keyword", "Text"]
LABELS = ["not_antisemitic", "antisemitic"]            # D-02: criteria document order is the label index
LABEL_OF = {"0": 0, "1": 1}
SPLIT_SEED = 13                                         # RULE split: the contract's declared seed
EVAL_DIVISOR = 5                                        # RULE split: 0.2 = 1 / 5, ceil done in integers
SENTINEL = "ZQXJ-sentinel-prepare-4kq"                  # planted in the selftest's synthetic rows only

# The pinned partner file (RESEARCH Q1, measured 2026-10-21): a changed CSV fails loudly (T-10-03).
PINNED = {"rows_in": 11311, "distinct_texts": 10644, "duplicates_collapsed": 667,
          "mixed_label_groups_dropped": 5, "mixed_label_rows_dropped": 13,
          "csv_sha256": "79f4d0ebc5f5ff529a25c0dfc1f7c8bbca48dbb6b8cff06b0059727da8db992f"}

# The served builder's geometry (builder.rs / laya.common.build_sequence) and the phase's door ceiling.
ROW_MAX = 512
HEAD_MAX = 192
OPTION_MAX = 48
DOOR_CEILING = 8                                        # the Phase 8 door, the ceiling of THIS phase
SNAPSHOT_REV = "55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851"
STANCE_CONTROL_P = 57                                   # the stance task's measured prefix of record (a literal, never the live contract constant)
HEADER_TEXT = "choice question:"
TOOL_BOUNDARY = "contracts/decide-tool-boundary-v1.yaml"

# The approved task (plan 10-01 Task 2, owner reply `door-8 p90`, 2026-10-21): ONE task per script (OPS-03). These are the
# exact approved strings; their bytes become task.json and so inputs_sha256.task_json. Criteria in label order (D-02).
INSTRUCTIONS = ("Is this tweet antisemitic by the IHRA definition? Antisemitic: hostile stereotypes of Jews, Jewish power or "
                "conspiracy myths, Holocaust denial, blaming Jews collectively. Not antisemitic: criticism of Israel, or "
                "reporting antisemitism.")
DESCRIPTIONS = {"not_antisemitic": "no hostility toward Jews as Jews", "antisemitic": "hostile to Jews or an antisemitic trope"}
TASK = {"instructions": INSTRUCTIONS, "descriptions": DESCRIPTIONS}


class PrepareFailed(Exception):
    """A consistency check on the partner file or the build failed (PREPARE FAILED, exit 1)."""


# ------------------------------------------------------------------------------------------------ the CSV

def read_rows(path):
    """[(row number, Biased, Text)] of the CSV, row numbers 1-based with the header excluded.

    Refusals (DataError -> one `REFUSED <rule>` line, exit 2) name a row number and a field, never content:
    csv-missing, csv-encoding, csv-header (reported by column count only), csv-row, csv-label, csv-text."""
    path = Path(path)
    if not path.is_file():
        raise DataError("csv-missing", "%s is not a file; the partner CSV lives in motivation-experiment datasets/antisemite/" % path)
    rows = []
    reader = None
    try:
        with open(path, newline="", encoding="utf-8") as f:
            reader = csv.reader(f)
            header = next(reader, None)
            if header != CSV_HEADER:
                raise DataError("csv-header", "the header row has %d column(s) and is not %s" % (len(header or []), ",".join(CSV_HEADER)))
            for n, rec in enumerate(reader, 1):
                if len(rec) != len(CSV_HEADER):
                    raise DataError("csv-row", "row %d has %d field(s), expected %d" % (n, len(rec), len(CSV_HEADER)))
                biased, text = rec[3], rec[5]
                if biased not in LABEL_OF:
                    shown = biased if len(biased) <= 8 else "<%d characters>" % len(biased)
                    raise DataError("csv-label", "row %d field Biased is %r, expected \"0\" or \"1\"" % (n, shown))
                if not text.strip():
                    raise DataError("csv-text", "row %d field Text is empty" % n)
                rows.append((n, biased, text))
    except UnicodeDecodeError as e:
        raise DataError("csv-encoding", "%s is not valid UTF-8 (byte %d: %s)" % (path.name, e.start, e.reason))
    except csv.Error as e:
        raise DataError("csv-row", "%s cannot be parsed as CSV near line %d" % (path.name, reader.line_num if reader else 0))
    if not rows:
        raise DataError("csv-row", "%s has a header and no rows" % path.name)
    return rows


def eval_count(n_groups):
    """ceil(0.2 x n_groups), exactly, in integers."""
    return (n_groups + EVAL_DIVISOR - 1) // EVAL_DIVISOR


def build(csv_path, task):
    """({file name: bytes}, counts-only summary) for the CSV and the task dict {instructions, criteria}."""
    csv_path = Path(csv_path)
    rows = read_rows(csv_path)
    groups = {}                                         # normalized hash -> [first exact text, [labels]]
    for _, biased, text in rows:
        g = groups.setdefault(normalized_sha256(text), [text, []])
        g[1].append(LABEL_OF[biased])
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
    if max(train_counts.values()) <= 16:
        raise PrepareFailed("the largest train class has %d row(s): train.py would not need --epochs, so this is not the "
                            "production-sized dataset this script is for" % max(train_counts.values()))
    data.calibration_split(train_rows, contract.constant("calibration_slice_fraction", "float"),
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
               "train": train_counts, "eval": eval_counts, "split_seed": SPLIT_SEED,
               "csv_sha256": sha256_file(csv_path)}
    return files, summary


def check_pin(summary):
    """The CSV must be the pinned partner file (T-10-03); PrepareFailed names the differing keys, never content."""
    bad = [k for k, v in PINNED.items() if summary[k] != v]
    if bad:
        raise PrepareFailed("the CSV is not the pinned partner file (differs in %s); pass --no-pin only for a deliberate "
                            "update of PINNED" % ", ".join(bad))


# ------------------------------------------------------------------------------------------------ the served prefix

def door_fields(p, n, total, explicit):
    """The door arithmetic of one prefix length P at a served door of n texts (a pure function of P, n, total)."""
    return {"door_max_texts": n, "door_enforced": explicit, "door_total": n * p, "fits_door": n * p <= total,
            "max_texts_that_fit": min(DOOR_CEILING, total // p)}


def contract_constant(name):
    """An integer `  <name>: <n>` constant of decide-tool-boundary-v1 (read, never a literal)."""
    text = (REPO / TOOL_BOUNDARY).read_text(encoding="utf-8")
    m = re.search(r"^  %s: (\d+)[ \t]*(?:#.*)?$" % re.escape(name), text, re.M)
    if not m:
        raise PrepareFailed("%s has no `  %s: <n>` constant" % (TOOL_BOUNDARY, name))
    return int(m.group(1))


def snapshot_dir():
    env = os.environ.get("LAYA_MODEL_DIR")
    return Path(env) if env else Path.home() / ".cache/huggingface/hub/models--convaiinnovations--laya/snapshots" / SNAPSHOT_REV


def load_tokenizer():
    d = snapshot_dir() / "tokenizer"
    if not d.is_dir():
        raise PrepareFailed("the served tokenizer %s is missing (the Laya-en snapshot %s)" % (d, SNAPSHOT_REV[:8]))
    from transformers import AutoTokenizer
    return AutoTokenizer.from_pretrained(str(d))


def _ids(tok, s):
    from laya.common import encode_text
    return encode_text(tok, s, add_special_tokens=False)["input_ids"]


def _build_prefix(tok, instructions, criteria):
    from laya.common import build_sequence
    return build_sequence(tok, "", {"t": "choice", "ins": instructions, "crit": dict(criteria)}, ROW_MAX, HEAD_MAX)


def measure_task(tok, instructions, criteria):
    """The empty-state row of a choice task, measured with the served tokenizer: P, marker count, raw versus kept
    tokens of the head and of every option, and the instruction room of this option set (cap build)."""
    mask = tok.mask_token
    ids, markers = _build_prefix(tok, instructions, criteria)
    p = len(ids)
    header = len(_ids(tok, HEADER_TEXT))
    head_raw = len(_ids(tok, "choice question: " + str(instructions).replace(mask, " ")))
    head_kept = markers[0] - 2                          # [CLS] head [SEP] [MASK] ...: the head is positions 1 .. markers[0]-2
    bounds = list(markers[1:]) + [p - 2]                # the last option ends before the prefix's closing [SEP] (position P-2)
    options = []
    for i, (name, desc) in enumerate(criteria.items()):
        rendered = str(name) if desc is None or desc == "" else "%s: %s" % (name, desc)
        raw = len(_ids(tok, " " + rendered.replace(mask, " ")))
        kept = bounds[i] - markers[i] - 1
        options.append({"name": name, "raw_tokens": raw, "kept_tokens": kept, "cut": raw > kept})
    cap_ids, cap_markers = _build_prefix(tok, " ".join(["word"] * 400), criteria)
    return {"P": p, "markers": len(markers), "options": options, "head_raw_tokens": head_raw,
            "head_kept_tokens": head_kept, "head_cut": head_raw > head_kept, "instructions_tokens": head_raw - header,
            "instruction_room_tokens": (cap_markers[0] - 2) - header, "p_cap": len(cap_ids), "state_room_tokens": ROW_MAX - p}


def tweet_tokens(tok, csv_path):
    """Token count of every CSV text, in file order (the state side of a built row)."""
    mask = tok.mask_token
    texts = [t.replace(mask, " ") for _, _, t in read_rows(csv_path)]
    return [len(x) for x in _ids(tok, texts)]


def tweet_report(lengths, p, total):
    a = np.asarray(lengths)
    q = {k: int(np.percentile(a, v)) for k, v in (("p50", 50), ("p75", 75), ("p90", 90), ("p95", 95), ("p99", 99))}
    return {
        "tweet_tokens": dict(q, max=int(a.max()), mean=round(float(a.mean()), 1), n=int(a.size)),
        "posts_truncated": {"count": int((p + a > ROW_MAX).sum()), "share": round(float((p + a > ROW_MAX).mean()), 4)},
        "texts_per_call": {k: min(DOOR_CEILING, total // (p + q[k])) for k in ("p50", "p95")},
        "share_fit_100": round(float((p + a <= 100).mean()), 4),
    }


def report(tok, instructions, criteria, max_texts=None, csv_path=None):
    """The --measure JSON object (keys in this order). Raises DataError for a real cut (prefix-cut), a row that cannot
    hold a state (prefix-row) and, ONLY for an explicit max_texts, N x P over classify_max_total_tokens (prefix-budget)."""
    m = measure_task(tok, instructions, criteria)
    p = m["P"]
    if m["head_cut"]:
        raise DataError("prefix-cut", "head: %d instruction-side tokens requested, %d kept (the builder cuts the rest silently)"
                        % (m["head_raw_tokens"], m["head_kept_tokens"]))
    for o in m["options"]:
        if o["cut"]:
            raise DataError("prefix-cut", "option %s: %d tokens requested, %d kept" % (o["name"], o["raw_tokens"], o["kept_tokens"]))
    if p + 1 > ROW_MAX or m["markers"] != len(criteria):
        raise DataError("prefix-row", "P + 1 > %d, or a marker is lost (P %d, %d of %d markers)" % (ROW_MAX, p, m["markers"], len(criteria)))
    total = contract_constant("classify_max_total_tokens")
    explicit = max_texts is not None
    n = max_texts if explicit else contract_constant("classify_max_texts")
    if explicit and n * p > total:
        raise DataError("prefix-budget", "N x P = %d x %d = %d > %d" % (n, p, n * p, total))
    out = dict(m, **door_fields(p, n, total, explicit))
    if csv_path is not None:
        out.update(tweet_report(tweet_tokens(tok, csv_path), p, total))
    return out


# ------------------------------------------------------------------------------------------------ self-tests

def _synthetic_csv(path, header=None):
    """A made-up CSV (never tweets) with the sentinel in every text: an exact duplicate, a whitespace variant, an NFC
    variant, two mixed-label groups. Returns the expected counts."""
    rows = []
    for i in range(60):
        rows.append(("synthetic sentence %03d %s about topic %d" % (i, SENTINEL, i % 7), "0"))
    for i in range(24):
        rows.append(("made-up claim %03d %s regarding subject %d" % (i, SENTINEL, i % 5), "1"))
    rows.append((rows[3][0], "0"))                                              # exact duplicate
    rows.append(("  " + rows[5][0].replace(" ", "  ") + " ", "0"))               # whitespace variant of another text
    rows.append(("café " + SENTINEL + " nfc variant a", "0"))
    rows.append(("café " + SENTINEL + " nfc variant a", "0"))               # NFC variant of the line above
    rows.append(("conflicting made-up text one " + SENTINEL, "0"))
    rows.append(("conflicting made-up text one " + SENTINEL, "1"))
    rows.append(("conflicting made-up text two " + SENTINEL, "0"))
    rows.append(("conflicting made-up text two " + SENTINEL, "1"))
    rows.append(("conflicting made-up text two " + SENTINEL, "1"))
    with open(path, "w", newline="", encoding="utf-8") as f:
        w = csv.writer(f)
        w.writerow(header if header is not None else CSV_HEADER)
        for i, (text, biased) in enumerate(rows):
            w.writerow([str(i), "user%d" % i, "2020-01-01", biased, "kw", text])
    # distinct under nfc-trim-ws-v1: 60 + 24 + 1 (nfc pair) + 2 (mixed groups) = 87; rows 60+24+2+2+5 = 93
    return {"rows_in": len(rows), "distinct_texts": 87, "duplicates_collapsed": len(rows) - 87,
            "mixed_label_groups_dropped": 2, "mixed_label_rows_dropped": 5, "classes": {"not_antisemitic": 61, "antisemitic": 24}}


def _capture(argv):
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        try:
            rc = main(argv)
        except SystemExit as e:
            rc = e.code
    return rc, out.getvalue(), err.getvalue()


def selftest():
    """The declared rule over a synthetic CSV (no tweet text): duplicate collapse, the mixed-label drop, the seeded
    stratified group-disjoint split, determinism, write-once, every refusal, the door arithmetic and the no-text
    promise over captured stdout and stderr. numpy + pyyaml only (no torch, no tokenizer)."""
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

    guide = TASK
    with tempfile.TemporaryDirectory(prefix="goldstandard-selftest-") as tmp:
        tmp = Path(tmp)
        good = tmp / "good.csv"
        want = _synthetic_csv(good)
        print("the declared rule:")
        files, s = build(good, guide)
        case("rows_in, distinct_texts, duplicates_collapsed", (s["rows_in"], s["distinct_texts"], s["duplicates_collapsed"])
             == (want["rows_in"], want["distinct_texts"], want["duplicates_collapsed"]), json.dumps({k: s[k] for k in ("rows_in", "distinct_texts", "duplicates_collapsed")}))
        case("mixed-label groups and rows dropped", (s["mixed_label_groups_dropped"], s["mixed_label_rows_dropped"]) == (2, 5))
        task = data.load_task(_write(tmp / "t" / "task.json", files["task.json"]))
        case("task.json lists not_antisemitic first (D-02)", task["labels"] == ["not_antisemitic", "antisemitic"], str(task["labels"]))
        tr = data.load_rows(_write(tmp / "t" / "train.jsonl", files["train.jsonl"]), task, "train")
        ev = data.load_rows(_write(tmp / "t" / "eval.jsonl", files["eval.jsonl"]), task, "eval")
        refuse_overlap(tr, ev)
        group_train(tr)
        case("the written dir passes data.py's load_task / load_rows / refuse_overlap / group_train", True)
        case("eval and train are disjoint after nfc-trim-ws-v1",
             not ({normalized_sha256(t) for t, _ in tr} & {normalized_sha256(t) for t, _ in ev}))
        case("both labels in eval", {y for _, y in ev} == {0, 1})
        counts_all = {c: sum(1 for _, y in tr if y == c) + sum(1 for _, y in ev if y == c) for c in (0, 1)}
        case("class totals are the distinct kept groups", counts_all == {0: want["classes"]["not_antisemitic"], 1: want["classes"]["antisemitic"]}, str(counts_all))
        case("per-class eval count == ceil(0.2 x class groups)",
             all(sum(1 for _, y in ev if y == c) == math.ceil(0.2 * counts_all[c]) for c in (0, 1)),
             str({c: sum(1 for _, y in ev if y == c) for c in (0, 1)}))
        case("split seed is the contract's declared seed", SPLIT_SEED == contract.declared_seed() == 13)
        files2, s2 = build(good, guide)
        case("two builds are byte-identical (determinism)", files == files2 and s == s2)
        case("no shift.jsonl is built (D-04)", "shift.jsonl" not in files)

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
        _synthetic_csv(bad_header, header=["ID", "Username", "Biased", "Text"])
        refuses("csv-header", "csv-header", lambda: read_rows(bad_header))
        refuses("csv-missing", "csv-missing", lambda: read_rows(tmp / "absent.csv"))
        bad_label = tmp / "bad-label.csv"
        _mutate(good, bad_label, lambda r: r.__setitem__(3, "2") if r[0] == "4" else None)
        refuses("csv-label", "csv-label", lambda: read_rows(bad_label))
        empty_text = tmp / "empty-text.csv"
        _mutate(good, empty_text, lambda r: r.__setitem__(5, "  ") if r[0] == "4" else None)
        refuses("csv-text", "csv-text", lambda: read_rows(empty_text))
        short = tmp / "short.csv"
        short.write_text(",".join(CSV_HEADER) + "\n1,u,2020-01-01,0\n", encoding="utf-8")
        refuses("csv-row (a short record)", "csv-row", lambda: read_rows(short))
        not_utf8 = tmp / "not-utf8.csv"
        not_utf8.write_bytes((",".join(CSV_HEADER) + "\n1,u,d,0,k,\xff\xfe\n").encode("latin-1"))
        refuses("csv-encoding", "csv-encoding", lambda: read_rows(not_utf8))
        tiny = tmp / "tiny.csv"
        with open(tiny, "w", newline="", encoding="utf-8") as f:
            w = csv.writer(f)
            w.writerow(CSV_HEADER)
            for i in range(10):
                w.writerow([str(i), "u", "d", str(i % 2), "k", "tiny made-up text %d" % i])
        try:
            build(tiny, guide)
            case("a dataset whose largest class is 16 or fewer is not the production size", False, "accepted")
        except PrepareFailed:
            case("a dataset whose largest class is 16 or fewer is not the production size", True)
        try:
            check_pin(s)
            case("the pin refuses a CSV that is not the partner file", False, "accepted")
        except PrepareFailed:
            case("the pin refuses a CSV that is not the partner file", True)

        print("the door arithmetic (a pure function of P, N and the total):")
        for p, n, fits, ceil_ in ((24, 8, True, 8), (100, 8, True, 8), (101, 8, False, 7), (135, 8, False, 5), (196, 8, False, 4), (196, 4, True, 4), (57, 8, True, 8)):
            d = door_fields(p, n, 800, False)
            case("P %d at door %d" % (p, n), d["fits_door"] is fits and d["max_texts_that_fit"] == ceil_ and d["door_total"] == p * n, json.dumps(d))

        print("no text on stdout or stderr (sentinel %s):" % SENTINEL)
        rc, o, e = _capture(["--csv", str(good), "--out", str(tmp / "out-a"), "--no-pin"])
        case("success path: exit 0, PREPARE OK, no sentinel", rc == 0 and "PREPARE OK" in o and SENTINEL not in o + e, "exit %s" % rc)
        rc, o, e = _capture(["--csv", str(good), "--out", str(tmp / "out-pin")])
        case("a CSV that is not the pinned file: exit 1, no sentinel, nothing written",
             rc == 1 and "PREPARE FAILED" in e and SENTINEL not in o + e and not (tmp / "out-pin").exists(), "exit %s" % rc)
        sentinel_header = tmp / "sentinel-header.csv"
        sentinel_header.write_text("%s,not,a,header\n1,u,d,0,k,t\n" % SENTINEL, encoding="utf-8")
        rc, o, e = _capture(["--csv", str(sentinel_header), "--out", str(tmp / "out-b"), "--no-pin"])
        case("header path (the header holds the sentinel): exit 2, no sentinel", rc == 2 and "REFUSED csv-header" in e and SENTINEL not in o + e, "exit %s" % rc)
        bad2 = tmp / "bad2.csv"
        _mutate(good, bad2, lambda r: r.__setitem__(3, "2") if r[0] == "4" else None)
        rc, o, e = _capture(["--csv", str(bad2), "--out", str(tmp / "out-c"), "--no-pin"])
        case("label path (Biased 2): exit 2, no sentinel", rc == 2 and "REFUSED csv-label" in e and SENTINEL not in o + e, "exit %s" % rc)
        bad3 = tmp / "bad3.csv"
        _mutate(good, bad3, lambda r: r.__setitem__(5, "") if r[0] == "4" else None)
        rc, o, e = _capture(["--csv", str(bad3), "--out", str(tmp / "out-d"), "--no-pin"])
        case("empty-text path: exit 2, no sentinel", rc == 2 and "REFUSED csv-text" in e and SENTINEL not in o + e, "exit %s" % rc)
        rc, o, e = _capture(["--csv", str(tmp / "absent.csv"), "--out", str(tmp / "out-e"), "--no-pin"])
        case("missing-file path: exit 2, no sentinel", rc == 2 and "REFUSED csv-missing" in e and SENTINEL not in o + e, "exit %s" % rc)
        # The detector itself: prove the capture sees both streams, so the cases above cannot pass vacuously.
        probe_out, probe_err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(probe_out), contextlib.redirect_stderr(probe_err):
            print(SENTINEL)
            print(SENTINEL, file=sys.stderr)
        case("control: the capture sees a sentinel on stdout and on stderr", SENTINEL in probe_out.getvalue() and SENTINEL in probe_err.getvalue())

    print("%d case(s) failed" % len(failures) if failures else "all cases ok")
    if not failures:
        print("PREPARE SELFTEST OK")
    return len(failures)


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


def selftest_tokenizer():
    """The measurement against the SERVED tokenizer (no model): exactly 48 option tokens is kept and 49 is a cut, an
    over-long head is a head cut, and the stance task measures the contract's 57 with 3 markers."""
    tok = load_tokenizer()
    failures = []

    def case(name, ok, detail=""):
        print("  %-4s %-72s %s" % ("ok" if ok else "FAIL", name, detail))
        if not ok:
            failures.append(name)

    def two(desc, instructions="Is this tweet antisemitic?"):
        return measure_task(tok, instructions, {"not_antisemitic": "a", "antisemitic": desc})

    n48 = next((n for n in range(1, 120) if two(" ".join(["the"] * n))["options"][1]["raw_tokens"] == 48), None)
    case("a repeated one-token word reaches an option of exactly 48 raw tokens", n48 is not None, "words %s" % n48)
    if n48 is not None:
        m48, m49 = two(" ".join(["the"] * n48)), two(" ".join(["the"] * (n48 + 1)))
        o48, o49 = m48["options"][1], m49["options"][1]
        case("exactly 48 tokens: kept 48, not a cut", o48["raw_tokens"] == 48 and o48["kept_tokens"] == 48 and o48["cut"] is False, json.dumps(o48))
        case("49 tokens: kept 48, a cut", o49["raw_tokens"] == 49 and o49["kept_tokens"] == 48 and o49["cut"] is True, json.dumps(o49))
    long_head = two("b", instructions=" ".join(["word"] * 400))
    case("a 400-word instruction is a head cut", long_head["head_cut"] is True and long_head["head_kept_tokens"] < long_head["head_raw_tokens"],
         "raw %d kept %d" % (long_head["head_raw_tokens"], long_head["head_kept_tokens"]))
    case("the cap build of a two-option task is 196", long_head["p_cap"] == 196, str(long_head["p_cap"]))
    stance = measure_task(tok, prepare_stance.INSTRUCTIONS, prepare_stance.DESCRIPTIONS)
    case("the stance task measures its prefix of record, STANCE_CONTROL_P 57, with 3 markers",
         stance["P"] == STANCE_CONTROL_P and stance["markers"] == 3, "P %d markers %d" % (stance["P"], stance["markers"]))
    case("the stance task has no head or option cut", not stance["head_cut"] and not any(o["cut"] for o in stance["options"]))
    print("%d case(s) failed" % len(failures) if failures else "all cases ok")
    if not failures:
        print("PREPARE TOKENIZER SELFTEST OK")
    return len(failures)


# ------------------------------------------------------------------------------------------------ main

def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--csv", default=None, help="the partner CSV (prepare mode default: the partner repo path; --measure reads a CSV only when given)")
    ap.add_argument("--out", default=None, help="data dir (default data/decide/goldstandard-2024)")
    ap.add_argument("--task", default=None, help="--measure: a task.json file instead of the approved task")
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
        files, summary = build(csv_path, TASK)
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
        instructions, criteria = INSTRUCTIONS, DESCRIPTIONS
    if args.max_texts is not None and args.max_texts < 1:
        ap.error("--max-texts must be at least 1")
    tok = load_tokenizer()
    csv_path = Path(args.csv) if args.csv else None
    print(json.dumps(report(tok, instructions, criteria, args.max_texts, csv_path)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
