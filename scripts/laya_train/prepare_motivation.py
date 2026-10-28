"""The motivation CSV (the partner file) -> a Laya data dir, and the measured prefix of the condensed task (plan 12-01).

    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_motivation.py [--csv PATH] [--out DIR] [--no-pin]
    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_motivation.py --enrichment PATH [--enrichment PATH ...] [--enrichment-sha256 HEX ...] --out DIR [--csv PATH] [--no-pin]
    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_motivation.py --measure [--task PATH] [--csv PATH] [--max-texts N]
    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_motivation.py --selftest | --selftest-tokenizer
    (or: just laya-prepare-motivation <the same arguments>)

PILOT (D-01): motivation-v1 is trained on Dr. Iris Yuster's classification guide, the 25 October 2026 draft (docx sha256
0e1d5a35ac3ccd455c5556fecbc5c1d1edec0e0a1100e2feac8bd16a42f29167). The guide is not frozen; the model is expected to be
retrained after the freeze. Nothing here is an accuracy claim: whether a fuller instruction helps is a HYPOTHESIS that only
the gate run measures.

APPROVED 2026-10-26 by the owner (plan 12-01 Task 2): variant a; door 8; measured P 97; task.json sha256
f74a3ad3cfc00c160cc2959d95beb219aec859b394b883e87577a56dc14e7ea3. The owner quoted the first 12 hex of that sha256 (D-10),
so the approval is bound to the exact bytes INSTRUCTIONS and DESCRIPTIONS render to; the script refuses to write any other
bytes (PREPARE FAILED) and a change to either constant is a new owner approval, never an edit.

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
    RULE v1-identity (Phase 13 D-10)
                  a run on the pinned CSV builds exactly v1's bytes (task.json f74a3ad3..., train.jsonl f1d84f19...,
                  eval.jsonl ee95d71f...); check_v1_identity enforces it in every pinned run (PREPARE FAILED otherwise),
                  so a run without --enrichment writes exactly what data/decide/motivation-v1 holds.

ENRICHMENT (Phase 13 D-09; the format, mapping, merge rules and 17 refusal names are declared in
contracts/motivation-enrichment-v1.yaml, which the selftest mirrors value for value):

    in place      each --enrichment file (format motivation-enrichment-v1) is READ IN PLACE in the partner repository
                  and never copied: a path whose resolved form (symlinks followed) lies under the resolved repo root is
                  REFUSED enrichment-in-repo. The files are named source-1, source-2, ... in command-line order; a file
                  name is never printed or written (it could carry a post fragment), only the generated id and the
                  file's sha256. --enrichment-sha256 (one per --enrichment, in order) pins the expected bytes.
    RULE enrichment-train-only (D-11)
                  the v1 build runs first and unchanged (base-build-unchanged); enriched rows go to train.jsonl only,
                  after the surviving partner train rows (in v1 order), in file order with the files in command-line
                  order; eval.jsonl and task.json are the base build's bytes. --out is required and must not resolve
                  to data/decide/motivation-v1 (REFUSED enrichment-out).
    RULE enrichment-eval-overlap (D-11)
                  an enriched text whose nfc-trim-ws-v1 identity matches an eval text is dropped, whatever its label
                  (dropped_eval_overlap); eval rows are never modified.
    RULE enrichment-conflict (D-12)
                  an enriched text matching a partner group the v1 build dropped as mixed stays dropped
                  (dropped_partner_mixed_group). A normalized-text group across the partner train rows and the enriched
                  rows carrying more than one MAPPED label is dropped WHOLE, the partner row included, with no source
                  precedence (dropped_conflict_groups, dropped_conflict_rows_partner, dropped_conflict_rows_enriched).
                  A group with one mapped label becomes one row, the partner's exact text when the partner has one,
                  else the first enriched text (collapsed_same_label counts the rows collapsed away).
    RULE enrichment-mapping (D-15, with the contract's d15_amendment)
                  (expert label, evidence for the recorded function) -> served label:
                      social_affiliative + clear -> social_affiliative    social_affiliative + weak | mixed -> other
                      conspiratorial + clear     -> conspiratorial        conspiratorial + weak | mixed     -> other
                      other + none | weak | mixed -> other
                      other + clear, social_affiliative + none, conspiratorial + none -> REFUSED enrichment-label-evidence
                  from the guide: "The class Other covers three situations ... no evidence of either motivational
                  function, weak evidence only ..., and clear evidence of both (mixed)"; "Weak evidence ... does not,
                  on its own, assign a class"; "Posts with weak evidence of the other function keep their class".
                  Every (label, evidence) cell is counted, so weak/mixed -> other stays open for the post-freeze retrain.
    RULE enrichment-annotation-mode (D-16)
                  the header's annotation_mode (prediction_shown | blind) is validated, recorded and counted in the
                  manifest, and NEVER acted on: it changes no row. With prediction_shown the labels may be anchored to
                  motivation-v1's predictions, which the hand-off of any retrain must say.
    manifest      enrichment-manifest.json (schema motivation-enrichment-manifest-v1) beside the three files: the
                  base pins, per source {id, sha256, rows_in, per_label, per_evidence, cells, per_slice,
                  annotation_mode}, the totals, train_sha256 and train_counts. Counts and sha256 values only, never a
                  text or a file name. A refusal names the source id, a row number (header row 0, posts 1 to N), a
                  field and at most a count, never a value. After the merge, data.py's group_train, refuse_overlap and
                  calibration_split run on the new train rows; their refusals pass through under their own names.

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

The condensed guide (D-09): INSTRUCTIONS and DESCRIPTIONS condense Sections 4-6 of the guide into the short variant the
owner approved, short enough to keep the served door at 8 texts (P at most 100); the contract door is unchanged.
"""
import argparse
import contextlib
import csv
import io
import json
import math
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

# The condensed guide Sections 4-6 (D-09) as the owner approved them (plan 12-01 Task 2, variant a): the exact task.json
# bytes these render to are bound to APPROVED_TASK_SHA256.
INSTRUCTIONS = (
    "Which function does this post serve? Social affiliative: signals belonging to a group (we, our people, calls "
    "to join, mocking critics). Conspiratorial: explains events by hidden control or secret orchestration by a "
    "group. Other: neither clear, weak or both. If unsure, choose other.")
DESCRIPTIONS = {
    "social_affiliative": "group belonging or loyalty",
    "conspiratorial": "hidden control by a group",
    "other": "neither clear, weak or mixed",
}
APPROVED_TASK_SHA256 = "f74a3ad3cfc00c160cc2959d95beb219aec859b394b883e87577a56dc14e7ea3"


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


def build(csv_path):
    """({file name: bytes}, counts-only summary) for the CSV and the approved INSTRUCTIONS / DESCRIPTIONS."""
    files, summary, _ = _build_v1(csv_path)
    return files, summary


def _build_v1(csv_path):
    """build()'s statements, unchanged and in the same order, plus the internal context the enrichment merge needs:
    ({file name: bytes}, summary, {kept, eval_rows, eval_hashes, mixed_hashes, train_rows, csv_sha256})."""
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
    task_doc = {"type": "choice", "instructions": INSTRUCTIONS, "criteria": dict(DESCRIPTIONS)}
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
    ctx = {"kept": kept, "eval_rows": eval_rows, "train_rows": train_rows,
           "eval_hashes": {normalized_sha256(t) for t, _ in eval_rows},
           "mixed_hashes": {h for h, g in groups.items() if len(set(g[1])) > 1},
           "csv_sha256": summary["csv_sha256"]}
    return files, summary, ctx


def check_pin(summary):
    """The CSV must be the pinned partner file (T-12-03); PrepareFailed names the differing keys, never content."""
    bad = [k for k, v in PINNED.items() if summary[k] != v]
    if bad:
        raise PrepareFailed("the CSV is not the pinned partner file (differs in %s); pass --no-pin only for a deliberate "
                            "update of PINNED" % ", ".join(bad))


def check_approved(files):
    """The task.json bytes about to be written must be the bytes the owner approved (T-12-02)."""
    got = sha256_bytes(files["task.json"])
    if got != APPROVED_TASK_SHA256:
        raise PrepareFailed("task.json would hash to %s, not the approved %s" % (got[:12], APPROVED_TASK_SHA256[:12]))


# ------------------------------------------------------------------------------------------------ v1 identity (D-10)

V1_TASK_SHA256 = APPROVED_TASK_SHA256
V1_TRAIN_SHA256 = "f1d84f19ec38ed85356bcbcd1eae6ef8ad07fb97e014f61a17d37adc0d68d378"
V1_EVAL_SHA256 = "ee95d71fcb5d90d515b9fc6741e5df51087e763fa7f251ace83d9f11d5ccd575"


def check_v1_identity(files, enriched):
    """On the pinned CSV the build must be v1's bytes (D-10): task.json and eval.jsonl always, train.jsonl too unless
    the run is enriched. PrepareFailed names each differing file with sha256 prefixes only."""
    want = {"task.json": V1_TASK_SHA256, "eval.jsonl": V1_EVAL_SHA256}
    if not enriched:
        want["train.jsonl"] = V1_TRAIN_SHA256
    bad = ["%s (%s, not v1's %s)" % (n, sha256_bytes(files.get(n, b""))[:12], w[:12])
           for n, w in sorted(want.items()) if sha256_bytes(files.get(n, b"")) != w]
    if bad:
        raise PrepareFailed("the pinned CSV no longer builds v1's bytes (D-10): %s" % "; ".join(bad))


# ------------------------------------------------------------------------------------------------ the enrichment (D-09)
# Every value below is declared in contracts/motivation-enrichment-v1.yaml; the selftest's contract mirror compares them.

ENRICHMENT_CONTRACT = REPO / "contracts" / "motivation-enrichment-v1.yaml"
ENRICHMENT_FORMAT = "motivation-enrichment-v1"
HEADER_KEYS = ("format", "annotation_mode", "artifact_sha256", "guide")
ROW_KEYS_ENRICHMENT = ("post_id", "text", "label", "evidence", "selection_slice")
EVIDENCE = ("clear", "weak", "mixed", "none")
SLICES = ("control", "uncertainty", "quota", "unspecified")
ANNOTATION_MODES = ("prediction_shown", "blind")
POST_ID_PATTERN = r"^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$"
SHA256_PATTERN = r"^[0-9a-f]{64}$"
TEXT_MIN_CHARS, TEXT_MAX_CHARS = 1, 20000               # min after trim (nfc-trim-ws-v1)
GUIDE_MIN_CHARS, GUIDE_MAX_CHARS = 1, 200
CAPS = {"file_bytes": 16777216, "rows": 20000, "text_chars": 20000}
REFUSED_PAIR = "enrichment-label-evidence"
MAPPING = {                                             # RULE enrichment-mapping: (label, evidence) -> served, None refused
    ("social_affiliative", "clear"): "social_affiliative",
    ("social_affiliative", "weak"): "other",
    ("social_affiliative", "mixed"): "other",
    ("social_affiliative", "none"): None,
    ("conspiratorial", "clear"): "conspiratorial",
    ("conspiratorial", "weak"): "other",
    ("conspiratorial", "mixed"): "other",
    ("conspiratorial", "none"): None,
    ("other", "none"): "other",
    ("other", "weak"): "other",
    ("other", "mixed"): "other",
    ("other", "clear"): None,
}
MERGE_RULES = ("base-build-unchanged", "dropped_eval_overlap", "dropped_partner_mixed_group", "dropped_conflict_groups",
               "collapsed_same_label")
TOTALS_KEYS = ("dropped_eval_overlap", "dropped_partner_mixed_group", "dropped_conflict_groups",
               "dropped_conflict_rows_partner", "dropped_conflict_rows_enriched", "collapsed_same_label", "rows_added")
ENRICHMENT_REFUSALS = (
    "enrichment-missing", "enrichment-in-repo", "enrichment-size", "enrichment-encoding", "enrichment-json",
    "enrichment-header", "enrichment-row", "enrichment-field", "enrichment-label", "enrichment-evidence",
    "enrichment-slice", "enrichment-annotation-mode", "enrichment-label-evidence", "enrichment-duplicate-post-id",
    "enrichment-empty", "enrichment-sha256", "enrichment-out")
MANIFEST_NAME = "enrichment-manifest.json"
MANIFEST_SCHEMA = "motivation-enrichment-manifest-v1"
SOURCE_COUNT_KEYS = ("rows_in", "per_label", "per_evidence", "cells", "per_slice", "annotation_mode")


class _DuplicateKey(ValueError):
    pass


def _object_no_duplicates(pairs):
    keys = [k for k, _ in pairs]
    if len(set(keys)) != len(keys):
        raise _DuplicateKey()
    return dict(pairs)


def _under_repo(path):
    """True when `path`, resolved with symlinks followed, lies under the resolved repo root (both sides resolved)."""
    try:
        Path(path).resolve().relative_to(REPO.resolve())
        return True
    except ValueError:
        return False


def _cell(label, evidence):
    return "%s+%s" % (label, evidence)


def read_enrichment(path, source_id, expected_sha256=None):
    """One motivation-enrichment-v1 file, read in place: {id, sha256, annotation_mode, rows, counts} with rows
    [(row number, text, mapped label index, label, evidence, selection_slice)].

    Every DataError names the source id, the row number (header row 0), the field and at most a count; never a value
    and never the file name."""
    sid = source_id
    path = Path(path)
    if not path.is_file():
        raise DataError("enrichment-missing", "%s: the --enrichment path does not exist or is not a regular file" % sid)
    if _under_repo(path):
        raise DataError("enrichment-in-repo", "%s: the --enrichment path resolves inside the aprender repository; an "
                        "enrichment file is read in place in the partner repository and never copied here" % sid)
    cap = CAPS["file_bytes"]
    size = path.stat().st_size
    if size > cap:
        raise DataError("enrichment-size", "%s: %d bytes, more than caps.file_bytes %d" % (sid, size, cap))
    with open(path, "rb") as f:
        raw = f.read(cap + 1)
    if len(raw) > cap:
        raise DataError("enrichment-size", "%s: more than caps.file_bytes %d" % (sid, cap))
    sha = sha256_bytes(raw)
    if expected_sha256 is not None and sha != expected_sha256:
        raise DataError("enrichment-sha256", "%s hashes to %s..., not the --enrichment-sha256 given for it" % (sid, sha[:12]))
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as e:
        raise DataError("enrichment-encoding", "%s row %d: not valid UTF-8 (byte %d)"
                        % (sid, raw[:e.start].count(b"\n"), e.start))
    lines = data.jsonl_lines(text)

    def obj_of(n, line):
        try:
            obj = json.loads(line, object_pairs_hook=_object_no_duplicates)
        except _DuplicateKey:
            raise DataError("enrichment-json", "%s row %d: an object repeats a key" % (sid, n))
        except (ValueError, RecursionError):
            raise DataError("enrichment-json", "%s row %d: not one JSON object" % (sid, n))
        if not isinstance(obj, dict):
            raise DataError("enrichment-json", "%s row %d: not one JSON object" % (sid, n))
        return obj

    def key_set(n, obj, want, rule):
        if sorted(obj) != sorted(want):
            missing = [k for k in want if k not in obj]
            unknown = len(set(obj) - set(want))
            raise DataError(rule, "%s row %d: missing field(s) %s, %d unknown key(s); expected exactly %s"
                            % (sid, n, missing, unknown, list(want)))

    if not lines:
        raise DataError("enrichment-header", "%s row 0: the header line is missing" % sid)
    head = obj_of(0, lines[0])
    key_set(0, head, HEADER_KEYS, "enrichment-header")
    if not isinstance(head["format"], str) or head["format"] != ENRICHMENT_FORMAT:
        raise DataError("enrichment-header", "%s row 0 field format: not %s" % (sid, ENRICHMENT_FORMAT))
    mode = head["annotation_mode"]
    if not isinstance(mode, str) or mode not in ANNOTATION_MODES:
        raise DataError("enrichment-annotation-mode", "%s row 0 field annotation_mode: not one of %s"
                        % (sid, ", ".join(ANNOTATION_MODES)))
    art = head["artifact_sha256"]
    if not isinstance(art, str) or not re.fullmatch(SHA256_PATTERN, art):
        raise DataError("enrichment-header", "%s row 0 field artifact_sha256: not 64 lower-case hex digits" % sid)
    guide = head["guide"]
    if not isinstance(guide, str) or not GUIDE_MIN_CHARS <= len(guide) <= GUIDE_MAX_CHARS:
        raise DataError("enrichment-header", "%s row 0 field guide: %s, expected a string of %d to %d characters"
                        % (sid, "%d characters" % len(guide) if isinstance(guide, str) else "not a string",
                           GUIDE_MIN_CHARS, GUIDE_MAX_CHARS))
    posts = lines[1:]
    if not posts:
        raise DataError("enrichment-empty", "%s: a header and no post rows" % sid)
    if len(posts) > CAPS["rows"]:
        raise DataError("enrichment-size", "%s: %d post rows, more than caps.rows %d" % (sid, len(posts), CAPS["rows"]))

    def enum(n, d, field, values, rule):
        v = d[field]
        if not isinstance(v, str):
            raise DataError("enrichment-field", "%s row %d field %s: not a string" % (sid, n, field))
        if v not in values:
            raise DataError(rule, "%s row %d field %s: a value of %d characters, not one of %s"
                            % (sid, n, field, len(v), ", ".join(values)))
        return v

    rows, seen = [], {}
    for n, line in enumerate(posts, 1):
        d = obj_of(n, line)
        key_set(n, d, ROW_KEYS_ENRICHMENT, "enrichment-row")
        pid = d["post_id"]
        if not isinstance(pid, str):
            raise DataError("enrichment-field", "%s row %d field post_id: not a string" % (sid, n))
        if not re.fullmatch(POST_ID_PATTERN, pid):
            raise DataError("enrichment-field", "%s row %d field post_id: %d characters, not matching the declared "
                            "pattern" % (sid, n, len(pid)))
        t = d["text"]
        if not isinstance(t, str):
            raise DataError("enrichment-field", "%s row %d field text: not a string" % (sid, n))
        try:
            data.refuse_unencodable(t, "enrichment", sid, n, "text")
        except DataError:                                # its own rule name and its code point never escape
            raise DataError("enrichment-encoding", "%s row %d field text: not encodable as UTF-8" % (sid, n))
        if len(data.normalize(t)) < TEXT_MIN_CHARS:
            raise DataError("enrichment-field", "%s row %d field text: %d characters, none left after trimming"
                            % (sid, n, len(t)))
        if len(t) > TEXT_MAX_CHARS:
            raise DataError("enrichment-field", "%s row %d field text: %d characters, at most %d"
                            % (sid, n, len(t), TEXT_MAX_CHARS))
        label = enum(n, d, "label", LABELS, "enrichment-label")
        evidence = enum(n, d, "evidence", EVIDENCE, "enrichment-evidence")
        sl = enum(n, d, "selection_slice", SLICES, "enrichment-slice")
        if pid in seen:
            raise DataError("enrichment-duplicate-post-id", "%s row %d field post_id: repeats the post_id of row %d"
                            % (sid, n, seen[pid]))
        seen[pid] = n
        served = MAPPING[(label, evidence)]
        if served is None:
            raise DataError(REFUSED_PAIR, "%s row %d fields label and evidence: a pair the declared mapping refuses"
                            % (sid, n))
        rows.append((n, t, LABELS.index(served), label, evidence, sl))
    counts = {
        "rows_in": len(rows),
        "per_label": {lab: sum(1 for r in rows if r[3] == lab) for lab in LABELS},
        "per_evidence": {ev: sum(1 for r in rows if r[4] == ev) for ev in EVIDENCE},
        "cells": {_cell(lab, ev): sum(1 for r in rows if (r[3], r[4]) == (lab, ev))
                  for (lab, ev), served in MAPPING.items() if served is not None},
        "per_slice": {s: sum(1 for r in rows if r[5] == s) for s in SLICES},
        "annotation_mode": {m: (len(rows) if m == mode else 0) for m in ANNOTATION_MODES},
    }
    return {"id": sid, "sha256": sha, "annotation_mode": mode, "rows": rows, "counts": counts}


def merge_enrichment(ctx, base_files, sources):
    """The contract's merge, in its declared order, over the unchanged base build: eval overlap, partner mixed group,
    conflict groups dropped whole (no precedence), same-label collapse keeping the partner's exact text, then the append
    order. Returns ({file name: bytes} with the new train.jsonl and MANIFEST_NAME, the manifest). annotation_mode is
    never read here (D-16)."""
    train_rows = ctx["train_rows"]
    partner = {}
    for i, (t, _) in enumerate(train_rows):
        partner.setdefault(normalized_sha256(t), i)
    totals = dict.fromkeys(TOTALS_KEYS, 0)
    groups = {}                                         # normalized hash -> [(text, y)] in file / command-line order
    for src in sources:
        for _, text, y, _, _, _ in src["rows"]:
            h = normalized_sha256(text)
            if h in ctx["eval_hashes"]:
                totals["dropped_eval_overlap"] += 1
            elif h in ctx["mixed_hashes"]:
                totals["dropped_partner_mixed_group"] += 1
            else:
                groups.setdefault(h, []).append((text, y))
    conflict, added = set(), []
    for h, members in groups.items():
        labs = {y for _, y in members}
        if h in partner:
            labs.add(train_rows[partner[h]][1])
        if len(labs) > 1:
            conflict.add(h)
            totals["dropped_conflict_groups"] += 1
            totals["dropped_conflict_rows_enriched"] += len(members)
        elif h in partner:
            totals["collapsed_same_label"] += len(members)
        else:
            totals["collapsed_same_label"] += len(members) - 1
            added.append(members[0])
    kept_partner = [r for r in train_rows if normalized_sha256(r[0]) not in conflict]
    totals["dropped_conflict_rows_partner"] = len(train_rows) - len(kept_partner)
    totals["rows_added"] = len(added)
    new_train = kept_partner + added
    # What train.py asserts, on the rows about to be written; a refusal passes through under its own data.py name.
    group_train(new_train)
    refuse_overlap(new_train, ctx["eval_rows"])
    train_counts = {LABELS[c]: sum(1 for _, y in new_train if y == c) for c in range(len(LABELS))}
    if min(train_counts.values()) == 0:
        raise PrepareFailed("the enriched train.jsonl would hold no row of some criterion: %s" % train_counts)
    calibration_split(new_train, contract.constant("calibration_slice_fraction", "float"),
                      contract.constant("calibration_slice_min_per_class", "int"), SPLIT_SEED, len(LABELS))
    named = lambda rs: [(t, LABELS[y]) for t, y in rs]  # noqa: E731
    if jsonl_bytes(named(ctx["eval_rows"])) != base_files["eval.jsonl"]:
        raise PrepareFailed("the enriched run's eval.jsonl is not the base build's bytes (D-11)")
    train_bytes = jsonl_bytes(named(new_train))
    manifest = {
        "schema": MANIFEST_SCHEMA,
        "format": ENRICHMENT_FORMAT,
        "base": {"csv_sha256": ctx["csv_sha256"], "task_sha256": sha256_bytes(base_files["task.json"]),
                 "base_train_sha256": sha256_bytes(base_files["train.jsonl"]),
                 "eval_sha256": sha256_bytes(base_files["eval.jsonl"])},
        "sources": [dict({"id": s["id"], "sha256": s["sha256"]}, **s["counts"]) for s in sources],
        "totals": totals,
        "train_rows": len(new_train),
        "train_sha256": sha256_bytes(train_bytes),
        "train_counts": train_counts,
    }
    files = {"task.json": base_files["task.json"], "train.jsonl": train_bytes, "eval.jsonl": base_files["eval.jsonl"],
             MANIFEST_NAME: (json.dumps(manifest, sort_keys=True, indent=2) + "\n").encode("utf-8")}
    return files, manifest


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


_DROP = object()                                        # _synthetic_enrichment: remove this key from the default
FIXTURE_ARTIFACT_SHA256 = "d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd"


def _synthetic_enrichment(path, rows, header=None):
    """A made-up motivation-enrichment-v1 file (never posts, D-03: synthetic fixtures only). `rows` items are dicts merged
    over a default row whose text carries the planted sentinel, or raw str lines; `header` is a dict merged over the
    default header, or a raw str line. A value of _DROP removes that key. Returns the path."""
    head = {"format": "motivation-enrichment-v1", "annotation_mode": "prediction_shown",
            "artifact_sha256": FIXTURE_ARTIFACT_SHA256, "guide": GUIDE_DESIGNATION}

    def merged(base, over):
        d = dict(base)
        d.update(over)
        return {k: v for k, v in d.items() if v is not _DROP}

    lines = [header if isinstance(header, str) else json.dumps(merged(head, header or {}))]
    for i, r in enumerate(rows):
        if isinstance(r, str):
            lines.append(r)
            continue
        default = {"post_id": "fx-%d" % i, "text": "made-up enrichment post %d %s" % (i, SENTINEL), "label": "other",
                   "evidence": "none", "selection_slice": "control"}
        lines.append(json.dumps(merged(default, r)))
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes("".join(line + "\n" for line in lines).encode("utf-8"))
    return path


def _enrichment_cases(tmp, good, base_files, case):
    """The enrichment rules (D-09..D-12, D-15, D-16) over synthetic fixtures and the synthetic CSV, through main() with
    captured streams. Every fixture text AND every fixture file name carries the planted sentinel."""
    produced = set()

    def run(argv):
        return _capture(["--csv", str(good), "--no-pin"] + argv)

    def rule_of(err):
        m = re.search(r"REFUSED ([A-Za-z0-9_-]+):", err)
        return m.group(1) if m else None

    def refused(name, rule, argv, where=()):
        rc, o, e = run(argv)
        got = rule_of(e)
        if got:
            produced.add(got)
        ok = rc == 2 and got == rule and SENTINEL not in o + e and all(w in e for w in where)
        case(name, ok, "exit %s %s" % (rc, got))
        return e

    def fx(name, rows, header=None):
        return _synthetic_enrichment(tmp / "enr" / ("%s-%s.jsonl" % (name, SENTINEL)), rows, header)

    outs = iter(range(1, 10 ** 6))

    def fresh():
        return tmp / ("enr-out-%d" % next(outs))

    print("enrichment (tracer: D-09, D-10, D-11):")
    try:
        check_v1_identity(base_files, False)
        case("check_v1_identity refuses a synthetic build (train.jsonl, eval.jsonl named; task.json not)", False, "accepted")
    except PrepareFailed as e:
        case("check_v1_identity refuses a synthetic build (train.jsonl, eval.jsonl named; task.json not)",
             "train.jsonl" in str(e) and "eval.jsonl" in str(e) and "task.json" not in str(e), str(e)[:90])
    except Exception as e:                               # a missing function is a FAIL, never a crash
        case("check_v1_identity refuses a synthetic build (train.jsonl, eval.jsonl named; task.json not)", False, type(e).__name__)
    try:
        check_v1_identity(base_files, True)
        case("check_v1_identity with enrichment names eval.jsonl only (train may differ)", False, "accepted")
    except PrepareFailed as e:
        case("check_v1_identity with enrichment names eval.jsonl only (train may differ)",
             "eval.jsonl" in str(e) and "train.jsonl" not in str(e) and "task.json" not in str(e), str(e)[:90])
    except Exception as e:
        case("check_v1_identity with enrichment names eval.jsonl only (train may differ)", False, type(e).__name__)
    try:
        check_v1_identity(dict(base_files, **{"task.json": base_files["task.json"] + b" "}), True)
        case("check_v1_identity on tampered task.json bytes names task.json", False, "accepted")
    except PrepareFailed as e:
        case("check_v1_identity on tampered task.json bytes names task.json", "task.json" in str(e), str(e)[:90])
    except Exception as e:
        case("check_v1_identity on tampered task.json bytes names task.json", False, type(e).__name__)

    tracer = fx("tracer", [{"label": "conspiratorial", "evidence": "clear"}, {"label": "social_affiliative", "evidence": "clear"},
                           {"label": "other", "evidence": "none"}, {"label": "conspiratorial", "evidence": "weak"}])
    out = fresh()
    rc, o, e = run(["--enrichment", str(tracer), "--out", str(out)])
    case("one synthetic file: exit 0, PREPARE OK, no sentinel on stdout or stderr",
         rc == 0 and "PREPARE OK" in o and SENTINEL not in o + e, "exit %s" % rc)
    got = {n: (out / n).read_bytes() for n in ("task.json", "train.jsonl", "eval.jsonl", "enrichment-manifest.json")
           if (out / n).is_file()}
    case("eval.jsonl and task.json are the base build's bytes (D-11)",
         got.get("eval.jsonl") == base_files["eval.jsonl"] and got.get("task.json") == base_files["task.json"])
    tr = got.get("train.jsonl", b"")
    case("train.jsonl is the base train rows, then the 4 mapped rows",
         tr.startswith(base_files["train.jsonl"]) and tr.count(b"\n") == base_files["train.jsonl"].count(b"\n") + 4)
    tail = [json.loads(x)["label"] for x in tr[len(base_files["train.jsonl"]):].decode("utf-8").splitlines()]
    case("the added rows carry the mapped labels (conspiratorial+weak -> other)",
         tail == ["conspiratorial", "social_affiliative", "other", "other"], str(tail))
    mbody = got.get("enrichment-manifest.json", b"").decode("utf-8")
    m = json.loads(mbody) if mbody else {}
    case("enrichment-manifest.json: schema, source-1, sha256 of the file, no text and no file name",
         m.get("schema") == "motivation-enrichment-manifest-v1" and SENTINEL not in mbody
         and [s.get("id") for s in m.get("sources", [])] == ["source-1"]
         and m["sources"][0].get("sha256") == sha256_file(tracer) and "tracer" not in mbody)
    case("the manifest pins the base eval and train and counts rows_added 4",
         m.get("base", {}).get("eval_sha256") == sha256_bytes(base_files["eval.jsonl"])
         and m.get("base", {}).get("base_train_sha256") == sha256_bytes(base_files["train.jsonl"])
         and m.get("totals", {}).get("rows_added") == 4 and m.get("train_sha256") == sha256_bytes(tr))
    refused("--enrichment without --out: REFUSED enrichment-out", "enrichment-out", ["--enrichment", str(tracer)])
    refused("--out data/decide/motivation-v1 with --enrichment: REFUSED enrichment-out", "enrichment-out",
            ["--enrichment", str(tracer), "--out", str(DEFAULT_OUT) + "/."])
    refused("an --enrichment path under the repo: REFUSED enrichment-in-repo", "enrichment-in-repo",
            ["--enrichment", str(HERE / "data.py"), "--out", str(fresh())])
    return produced


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

    with tempfile.TemporaryDirectory(prefix="motivation-selftest-") as tmp:
        tmp = Path(tmp)
        good = tmp / "good.csv"
        want = _synthetic_csv(good)
        print("the declared rule:")
        files, s = build(good)
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
        files2, s2 = build(good)
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
        tampered = dict(files, **{"task.json": files["task.json"] + b" "})
        try:
            check_approved(tampered)
            case("task.json bytes other than the approved ones are refused", False, "accepted")
        except PrepareFailed:
            case("task.json bytes other than the approved ones are refused", True)
        check_approved(files)
        case("the written task.json hashes to the owner-approved sha256", sha256_bytes(files["task.json"]) == APPROVED_TASK_SHA256)

        print("no text on stdout or stderr (sentinel %s):" % SENTINEL)
        base = ["--no-pin"]
        rc, o, e = _capture(["--csv", str(good), "--out", str(tmp / "out-a")] + base)
        case("success path: exit 0, PREPARE OK, no sentinel", rc == 0 and "PREPARE OK" in o and SENTINEL not in o + e, "exit %s" % rc)
        rc, o, e = _capture(["--csv", str(good), "--out", str(tmp / "out-pin")])
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

        _enrichment_cases(tmp, good, files, case)

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
    ap.add_argument("--task", default=None, help="--measure: a task.json file instead of the approved task")
    ap.add_argument("--measure", action="store_true", help="measure the task's prefix with the served tokenizer and print ONE JSON line")
    ap.add_argument("--max-texts", type=int, default=None, help="--measure: ENFORCE this served door (REFUSED prefix-budget when N x P exceeds the total)")
    ap.add_argument("--no-pin", action="store_true", help="accept a CSV that is not the pinned partner file")
    ap.add_argument("--enrichment", action="append", default=[], metavar="PATH",
                    help="a motivation-enrichment-v1 file, read in place (repeatable; source-1, source-2, ... in order)")
    ap.add_argument("--enrichment-sha256", action="append", default=[], metavar="HEX",
                    help="the expected sha256 of each --enrichment file, one per file in the same order")
    ap.add_argument("--selftest", action="store_true", help="the declared rule over a synthetic CSV, torch-free")
    ap.add_argument("--selftest-tokenizer", action="store_true", help="the measurement against the served tokenizer (no model)")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    try:
        if args.selftest_tokenizer:
            return selftest_tokenizer()
        if args.measure:
            if args.enrichment or args.enrichment_sha256:
                ap.error("--enrichment is a prepare-mode option, not a --measure one")
            return measure_main(ap, args)
        csv_path = Path(args.csv) if args.csv else DEFAULT_CSV
        out = Path(args.out) if args.out else DEFAULT_OUT
        pins = list(args.enrichment_sha256)
        if args.enrichment:
            if args.out is None:
                raise DataError("enrichment-out", "--enrichment needs an explicit --out; data/decide/motivation-v1 is "
                                "never written with enrichment")
            if out.resolve() == DEFAULT_OUT.resolve():
                raise DataError("enrichment-out", "--out resolves to data/decide/motivation-v1, the v1 data dir; an "
                                "enriched run writes elsewhere")
            if pins and len(pins) != len(args.enrichment):
                raise DataError("enrichment-sha256", "%d --enrichment-sha256 value(s) for %d --enrichment file(s); give "
                                "one per file, in the same order" % (len(pins), len(args.enrichment)))
        elif pins:
            raise DataError("enrichment-sha256", "--enrichment-sha256 was given without --enrichment")
        files, summary, ctx = _build_v1(csv_path)
        if not args.no_pin:
            check_pin(summary)
        check_approved(files)
        if not args.no_pin:
            check_v1_identity(files, False)
        manifest = None
        if args.enrichment:
            sources = [read_enrichment(p, "source-%d" % k, pins[k - 1] if pins else None)
                       for k, p in enumerate(args.enrichment, 1)]
            files, manifest = merge_enrichment(ctx, files, sources)
            if not args.no_pin:
                check_v1_identity(files, True)
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
    if manifest is not None:                            # the generated id and the sha256, never the file name
        for s in manifest["sources"]:
            print("enrichment %s sha256 %s rows %d %s" % (s["id"], s["sha256"], s["rows_in"], json.dumps(
                {k: s[k] for k in SOURCE_COUNT_KEYS if k != "rows_in"}, sort_keys=True)))
        print("enrichment totals %s train_counts %s" % (json.dumps(manifest["totals"], sort_keys=True),
                                                        json.dumps(manifest["train_counts"], sort_keys=True)))
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
    print(json.dumps(measure_report(tok, instructions, criteria, args.max_texts, csv_path)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
