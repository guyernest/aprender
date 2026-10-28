"""Which scored motivation-v1 posts go to the expert: the seeded selection rule of Phase 13 D-17 (plan 13-05).

    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/select_motivation.py \\
        --records PATH [--records PATH ...] --out PATH [--budget N] [--seed S] [--exclude-data DIR | --no-exclude] \\
        [--expect-sha256 HEX]
    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/select_motivation.py --print-rule
    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/select_motivation.py --selftest

INPUT: one or more motivation-record-v1 files (contracts/motivation-enrichment-v1.yaml `formats.motivation-record-v1`,
D-14): JSON Lines, one object per post the partner ran through the motivation-v1 server. They live in the partner
repository and are READ IN PLACE: a path whose resolved form (symlinks followed) lies under the aprender repository is
refused. The files are named source-1, source-2, ... in command-line order; a file name is never printed or written (it
could carry a post fragment), only the generated id and the file's sha256.

DECLARED RULES (written before any run; the code applies them and --print-rule prints them from these constants):

    budget        DEFAULT_BUDGET 300; any integer from BUDGET_MIN 1 to BUDGET_MAX 20000, the row cap of a
                  motivation-enrichment-v1 file (contract caps.rows). The partner may change the budget; the shares
                  scale by the integer formula below.
    shares        SHARES control 25 / uncertainty 40 / quota 35 percent of a budget B:
                      control = B * 25 // 100, uncertainty = B * 40 // 100, quota = B - control - uncertainty
                  so B = 300 gives 75 / 120 / 105 and B = 100 gives 25 / 40 / 35.
    quota split   QUOTA_SPLIT by PREDICTED label over QUOTA_DENOMINATOR 7, conspiratorial over-sampled (the thin
                  class: 41 of the 295 v1 train rows): conspiratorial = quota * 3 // 7, social_affiliative =
                  quota * 2 // 7, other = quota - the two (105 -> 45 / 30 / 30; 35 -> 15 / 10 / 10).
    band          BAND: a post is band-eligible when p1 - p2 < 0.20 OR 0.45 <= p1 <= 0.75, p1 and p2 its largest and
                  second-largest probabilities. The interval straddles experiment-coach's motivation_min_prob 0.6.
                  Probabilities are parsed as exact decimals (json parse_float=Decimal), so the band edges and the
                  sum tolerance compare exact values, never binary floats. With three labels summing to one, a post
                  with p1 < 0.45 always has p1 - p2 < 0.20 (p2 >= (1 - p1) / 2 > 0.275), and p1 > 0.75 always has
                  p1 - p2 > 0.5, so in practice the band is p1 <= 0.75; band_share_of_pool in the header reports
                  how selective it was on the actual pool.
    predicted     the argmax of the probabilities, a tie going to the lower index (served order social_affiliative,
                  conspiratorial, other).
    seed, rank    DEFAULT_SEED "motivation-selection-v1"; a post's rank key is the lower-case hex sha256 of the UTF-8
                  bytes of "<seed>:<post_id>", and smaller keys are drawn first. The rank does not depend on file or
                  line order, so a shuffled input selects the same posts.
    pool          (1) a record whose nfc-trim-ws-v1 text identity (data.normalized_sha256, the one normalizer)
                  equals a text of the --exclude-data dir's train.jsonl or eval.jsonl (default
                  data/decide/motivation-v1) is excluded (excluded_known_text). --no-exclude skips this step; a
                  missing dir is REFUSED, so a machine without the v1 data dir (the partner's) must choose
                  --no-exclude explicitly. (2) Records with one identity are deduplicated, the smaller rank key kept
                  (duplicate_text_skipped). What remains is the pool.
    draw order    1 control: the smallest rank keys of the WHOLE pool; 2 uncertainty: the smallest rank keys among
                  the band-eligible posts not yet drawn; 3 quota: per predicted label, the smallest rank keys among
                  that label's posts not yet drawn. No post twice. No backfill: a slice that finds too few posts
                  takes what it finds, the header reports the shortfall, and no other slice makes up the difference.
    control       CONTROL_POPULATION: a uniform random sample of the eligible unique-text pool of this scoring round
                  (after exclusion and dedupe), an unbiased reference for that pool and round only, and no longer an
                  independent evaluation of any model later trained on these rows. The uncertainty and quota slices
                  are biased by design; a population estimate from them needs inverse-inclusion weights.

VALIDATION (motivation-record-v1; every limit is a constant that the selftest's contract mirror compares with the
contract's structured value, and the boundary cases read the limits from the contract): exactly RECORD_KEYS; post_id
and source match their patterns; text a string of at most TEXT_MAX_CHARS characters with at least one left after
trimming; labels equal to LABELS; probabilities a list of PROB_LENGTH numbers in PROB_RANGE summing to one within
SUM_TOLERANCE; artifact_sha256 equal to --expect-sha256 (default the served motivation-v1 artifact); deployment exactly
DEPLOYMENT_KEYS, each matching its pattern, and the same object on every row of a file; tokens an integer >= TOKENS_MIN;
truncated a boolean; scored_at RFC 3339 UTC ending in Z; post_id unique across all files.

OUTPUT (format motivation-selection-v1, write-once: --out is refused under the repository or when it exists): line 1 is
a header {format, seed, budget, shares, quota_split, band, targets, control_population, records [{id, sha256}], counts
{pool, band_eligible, band_share_of_pool, duplicate_text_skipped, excluded_known_text, per_slice,
per_predicted_label_in_quota, shortfall}}; then one line per selected post {post_id, selection_slice, rank_key}, sorted
by slice (control, uncertainty, quota) then rank key. There is no text, probability or label column (D-03): no model
prediction can become a silver label through this file.

NO TEXT: stdout carries the counts as one JSON line and `SELECT OK`. A refusal (exit 2) prints one line
`REFUSED select-<rule>: ...` naming the source id, a 1-based row number, a field and at most a character count, never a
value. This script collects no post; it has been exercised on synthetic records only.
"""
import argparse
import contextlib
import hashlib
import io
import json
import random
import re
import sys
import tempfile
from decimal import Decimal
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import contract  # noqa: E402
import data  # noqa: E402
from common import sha256_bytes, sha256_file  # noqa: E402,F401
from data import DataError, normalized_sha256  # noqa: E402,F401
# OPS-03: the in-repo test and the duplicate-key JSON hook are the ingest's own, not a second copy.
from prepare_motivation import _DuplicateKey, _object_no_duplicates, _under_repo  # noqa: E402,F401

REPO = contract.REPO
CONTRACT = REPO / "contracts" / "motivation-enrichment-v1.yaml"
DEFAULT_EXCLUDE = REPO / "data" / "decide" / "motivation-v1"
EXCLUDE_FILES = ("train.jsonl", "eval.jsonl")

# ------------------------------------------------------------------------------------------------ the declared rule
SELECTION_FORMAT = "motivation-selection-v1"
DEFAULT_SEED = "motivation-selection-v1"
DEFAULT_BUDGET = 300
RULE_EXAMPLE_BUDGET = 100                               # the second column of --print-rule's tables
BUDGET_MIN, BUDGET_MAX = 1, 20000                       # BUDGET_MAX = motivation-enrichment-v1 caps.rows
PERCENT = 100
SLICES = ("control", "uncertainty", "quota")            # draw order; a subset of the enrichment selection_slice enum
SHARES = {"control": 25, "uncertainty": 40, "quota": 35}
QUOTA_DENOMINATOR = 7
QUOTA_SPLIT = {"conspiratorial": 3, "social_affiliative": 2, "other": None}   # None: the rest of the quota
BAND = {"top_two_margin_below": Decimal("0.20"), "top_probability_min": Decimal("0.45"),
        "top_probability_max": Decimal("0.75")}
MOTIVATION_MIN_PROB = Decimal("0.6")                    # experiment-coach's threshold the band straddles
CONTROL_POPULATION = ("a uniform random sample of the eligible unique-text pool of this scoring round (after exclusion "
                      "and dedupe): an unbiased reference for that pool and round only, and no longer an independent "
                      "evaluation of any model later trained on these rows")
TARGET_KEYS = ("control", "uncertainty", "quota", "quota_conspiratorial", "quota_social_affiliative", "quota_other")
ROW_KEYS = ("post_id", "selection_slice", "rank_key")

# ------------------------------------------------------------------------------------------------ motivation-record-v1
# Every value below is declared in contracts/motivation-enrichment-v1.yaml; the selftest's contract mirror compares them.
LABELS = ("social_affiliative", "conspiratorial", "other")
RECORD_KEYS = ("post_id", "source", "text", "labels", "probabilities", "artifact_sha256", "deployment", "tokens",
               "truncated", "scored_at")
DEPLOYMENT_KEYS = ("pmcp_target", "aws_account", "region", "endpoint")
POST_ID_PATTERN = r"^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$"
SOURCE_PATTERN = r"^[a-z0-9][a-z0-9_-]{0,63}$"
SHA256_PATTERN = r"^[0-9a-f]{64}$"
DEPLOYMENT_PATTERNS = {
    "pmcp_target": r"^[a-z0-9][a-z0-9-]{0,62}$",
    "aws_account": r"^[0-9]{12}$",
    "region": r"^[a-z]{2}(-[a-z]+)+-[0-9]$",
    "endpoint": r"^https://[a-z0-9]([a-z0-9.-]{0,251}[a-z0-9])?/mcp$",
}
SCORED_AT_PATTERN = r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]{1,9})?Z$"
TEXT_MIN_CHARS, TEXT_MAX_CHARS = 1, 20000               # min after trim (nfc-trim-ws-v1)
PROB_LENGTH = 3
PROB_RANGE = (Decimal("0.0"), Decimal("1.0"))
SUM_TOLERANCE = Decimal("1E-5")
TOKENS_MIN = 1
DEFAULT_EXPECT_SHA256 = "d202f4956cd6bbaabdc853afaffef2595e28a78c7054816687267292d9e1aecd"
REFUSALS = (
    "select-missing", "select-in-repo", "select-json", "select-record", "select-labels", "select-probabilities",
    "select-sha", "select-deployment", "select-duplicate-post-id", "select-budget", "select-out-in-repo",
    "select-out-exists", "select-exclude-missing")
SENTINEL = "ZQXJ-sentinel-select-4qp"                   # planted in the selftest's synthetic records only


# ------------------------------------------------------------------------------------------------ the rule
# RED (plan 13-05 Task 1): the selftest below is written first. These stubs return neutral values, so every behaviour
# case fails on its assertion rather than on a crash.

def read_records(path, source_id, expect_sha256):
    return {"id": source_id, "sha256": "", "records": []}


def known_identities(dirpath):
    return set()


def refuse_duplicate_post_ids(sources):
    return None


def parse_budget(text):
    return DEFAULT_BUDGET


def targets(budget):
    return {k: 0 for k in TARGET_KEYS}


def top_two(probs):
    return Decimal(0), Decimal(0)


def band_eligible(p1, p2):
    return False


def predicted(probs):
    return 0


def rank_key(seed, post_id):
    return ""


def select(sources, known, seed, budget):
    return {"counts": {}}, []


def render(header, rows):
    return b""


def rule_values():
    return {}


def rule_markdown():
    return ""


# ------------------------------------------------------------------------------------------------ selftest fixtures

FIXTURE_DEPLOYMENT = {"pmcp_target": "augury-dev", "aws_account": "783960389328", "region": "eu-west-1",
                      "endpoint": "https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/mcp"}
_DROP = object()                                        # _record: remove this key from the default
_P1_CYCLE = ("0.92", "0.85", "0.78", "0.70", "0.60", "0.50", "0.40")


def _probs(idx, p1):
    """A made-up probability vector: `p1` on label `idx`, the rest split 60 / 40 over the next two labels in served
    order. Exact decimals (written as their shortest float repr), so they sum to exactly one."""
    p1 = Decimal(p1)
    rest = Decimal(1) - p1
    second = (rest * Decimal("0.6")).quantize(Decimal("0.0001"))
    v = [None, None, None]
    v[idx], v[(idx + 1) % 3], v[(idx + 2) % 3] = p1, second, rest - second
    return [float(x) for x in v]


def _mixed_probs(i):
    return _probs(i % 3, _P1_CYCLE[(i // 3) % len(_P1_CYCLE)])


def _record(i, probs, prefix="syn", **over):
    """One made-up motivation-record-v1 object (never a post, D-03); its text carries the planted sentinel."""
    d = {"post_id": "%s-%05d" % (prefix, i), "source": "synthetic", "text": "made-up post %s %d %s" % (prefix, i, SENTINEL),
         "labels": list(LABELS), "probabilities": probs, "artifact_sha256": DEFAULT_EXPECT_SHA256,
         "deployment": dict(FIXTURE_DEPLOYMENT), "tokens": 40, "truncated": False, "scored_at": "2026-10-27T00:00:00Z"}
    d.update(over)
    return {k: v for k, v in d.items() if v is not _DROP}


def _write_items(path, items):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes("".join((x if isinstance(x, str) else json.dumps(x)) + "\n" for x in items).encode("utf-8"))
    return path


def _synthetic_records(path, n, probs_of=_mixed_probs, prefix="syn", extra=()):
    """Write n made-up records (probabilities from probs_of(i)) plus `extra` items (dicts, or raw str lines); return the
    written items."""
    items = [_record(i, probs_of(i), prefix) for i in range(n)] + list(extra)
    _write_items(path, items)
    return items


def _ref_band(probs):
    """The selftest's own reading of the band, with the PLANNED literals (not the constants): an independent oracle."""
    s = sorted((Decimal(repr(x)) if isinstance(x, float) else Decimal(x) for x in probs), reverse=True)
    return s[0] - s[1] < Decimal("0.20") or Decimal("0.45") <= s[0] <= Decimal("0.75")


def _ref_pred(probs):
    best = max(probs)
    return next(i for i, p in enumerate(probs) if p == best)


def _ref_rank(seed, post_id):
    return hashlib.sha256(("%s:%s" % (seed, post_id)).encode("utf-8")).hexdigest()


def _capture(argv):
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        try:
            rc = main(argv)
        except SystemExit as e:
            rc = e.code
        except Exception as e:                          # a crash is a FAIL of the case, never a traceback
            rc = "crash:%s" % type(e).__name__
    return rc, out.getvalue(), err.getvalue()


def _read_selection(path):
    """(header, rows, lines, body) of a selection file; (None, [], [], "") when it is absent or unreadable."""
    path = Path(path)
    if not path.is_file():
        return None, [], [], ""
    body = path.read_text(encoding="utf-8")
    lines = body.splitlines()
    try:
        return json.loads(lines[0]), [json.loads(x) for x in lines[1:]], lines, body
    except (ValueError, IndexError):
        return None, [], lines, body


def _per_slice(rows):
    per = {}
    for r in rows:
        per[r.get("selection_slice")] = per.get(r.get("selection_slice"), 0) + 1
    return per


def selftest():
    """The declared rule over synthetic records (never posts): the counts, the draw order, the band edges, determinism
    under shuffling, the seed, budget scaling, shortfall without backfill, the all-band pool, dedupe, exclusion, the
    file-name sentinel, every refusal, the structured contract mirror with its boundary cases, and --print-rule."""
    import yaml
    failures = []

    def case(name, ok, detail=""):
        print("  %-4s %-72s %s" % ("ok" if ok else "FAIL", name, detail))
        if not ok:
            failures.append(name)

    with tempfile.TemporaryDirectory(prefix="select-motivation-selftest-") as tmpd:
        tmp = Path(tmpd)
        state = {}
        outs = iter(range(1, 10 ** 6))

        def new_out(tag="o"):
            return tmp / "out" / ("%s-%d.jsonl" % (tag, next(outs)))

        def run(argv):
            return _capture(argv)

        def run_sel(recs, *extra):
            out = new_out()
            argv = []
            for r in ([recs] if isinstance(recs, (str, Path)) else recs):
                argv += ["--records", str(r)]
            rc, o, e = run(argv + ["--out", str(out)] + list(extra))
            h, rows, lines, body = _read_selection(out)
            return rc, o, e, h or {}, rows, lines, body

        # ---------------------------------------------------------------------------------------- the declared rule
        def sec_declared():
            print("the declared rule on 2000 synthetic records, budget 300:")
            recs = tmp / "in" / "records-a.jsonl"
            items = _synthetic_records(recs, 2000)
            rc, o, e, h, rows, lines, body = run_sel(recs, "--no-exclude")
            state["a"] = (items, recs, lines)
            per = _per_slice(rows)
            c = h.get("counts", {})
            qlab = c.get("per_predicted_label_in_quota")
            case("budget 300: exit 0, SELECT OK, control 75, uncertainty 120",
                 rc == 0 and "SELECT OK" in o.splitlines() and per.get("control") == 75 and per.get("uncertainty") == 120,
                 "exit %s %s" % (rc, per))
            case("budget 300: quota 105 = conspiratorial 45 / social_affiliative 30 / other 30",
                 per.get("quota") == 105 and qlab == {"conspiratorial": 45, "social_affiliative": 30, "other": 30}, str(qlab))
            ids = [r.get("post_id") for r in rows]
            case("every post_id distinct across the three slices (no post twice)", len(ids) == 300 and len(set(ids)) == 300,
                 "%d rows" % len(ids))
            case("rows hold post_id, selection_slice, rank_key only: no text, probability or label (D-03)",
                 bool(rows) and all(tuple(r) == ROW_KEYS for r in rows))
            case("no sentinel on stdout, stderr or in the output file", rc == 0 and bool(body) and SENTINEL not in o + e + body)
            ol = o.splitlines()
            case("stdout: the counts as one JSON line, then SELECT OK",
                 len(ol) == 2 and ol[1] == "SELECT OK" and bool(c) and json.loads(ol[0]) == c, "%d lines" % len(ol))
            by_id = {x["post_id"]: x for x in items}
            ranked = sorted(by_id, key=lambda p: _ref_rank(DEFAULT_SEED, p))
            control = {r["post_id"] for r in rows if r.get("selection_slice") == "control"}
            case("control is drawn first: its 75 posts are the 75 smallest rank keys of the whole pool",
                 control == set(ranked[:75]))
            unc = [r["post_id"] for r in rows if r.get("selection_slice") == "uncertainty"]
            quota = [r["post_id"] for r in rows if r.get("selection_slice") == "quota"]
            qref = {lab: sum(1 for p in quota if _ref_pred(by_id[p]["probabilities"]) == LABELS.index(lab)) for lab in LABELS}
            case("uncertainty rows are band-eligible; quota rows counted under their predicted label",
                 bool(unc) and all(_ref_band(by_id[p]["probabilities"]) for p in unc) and bool(quota) and qref == (qlab or {}),
                 str(qref))
            sorted_ok = [(SLICES.index(r["selection_slice"]), r["rank_key"]) for r in rows] == sorted(
                (SLICES.index(r["selection_slice"]), r["rank_key"]) for r in rows) if rows else False
            case("rank_key is sha256('<seed>:<post_id>'); rows sorted by slice, then rank key",
                 sorted_ok and all(r["rank_key"] == _ref_rank(DEFAULT_SEED, r["post_id"]) for r in rows))
            case("header: format motivation-selection-v1, budget 300, the default seed, targets 75 / 120 / 105",
                 h.get("format") == "motivation-selection-v1" and h.get("budget") == 300 and h.get("seed") == DEFAULT_SEED
                 and h.get("targets", {}).get("control") == 75 and h.get("targets", {}).get("uncertainty") == 120
                 and h.get("targets", {}).get("quota") == 105)
            cp = h.get("control_population", "")
            case("header: control_population bounds the control's claim (pool and round only)",
                 cp == CONTROL_POPULATION and "eligible unique-text pool" in cp and "no longer an independent evaluation" in cp)
            case("header: the record file is source-1 with its sha256; its name is in no output",
                 h.get("records") == [{"id": "source-1", "sha256": sha256_file(recs)}] and recs.name not in o + e + body)
            nb = sum(1 for x in items if _ref_band(x["probabilities"]))
            case("header: pool 2000, band_eligible as the reference counts it, band_share_of_pool = band / pool",
                 c.get("pool") == 2000 and c.get("band_eligible") == nb and c.get("band_share_of_pool") == round(nb / 2000, 6),
                 "band %s share %s" % (c.get("band_eligible"), c.get("band_share_of_pool")))
            case("--no-exclude: excluded_known_text 0, duplicate_text_skipped 0, no shortfall",
                 c.get("excluded_known_text") == 0 and c.get("duplicate_text_skipped") == 0
                 and set((c.get("shortfall") or {"x": 1}).values()) == {0})

        # ---------------------------------------------------------------------------------------- band and rank
        def sec_band():
            print("band boundaries, predicted label and rank key:")
            D = Decimal
            case("band boundary: margin exactly 0.20 with p1 0.80 is NOT eligible", band_eligible(D("0.80"), D("0.60")) is False)
            case("band boundary: margin just below 0.20 with p1 0.80 is eligible", band_eligible(D("0.80"), D("0.6001")) is True)
            case("band boundary: p1 0.45 is eligible (margin 0.35)", band_eligible(D("0.45"), D("0.10")) is True)
            case("band boundary: p1 0.75 is eligible (margin 0.60)", band_eligible(D("0.75"), D("0.15")) is True)
            case("band boundary: p1 0.7501 with margin 0.30 is NOT eligible", band_eligible(D("0.7501"), D("0.4501")) is False)
            case("band boundary: p1 0.4499 with margin exactly 0.20 is NOT eligible", band_eligible(D("0.4499"), D("0.2499")) is False)
            rnd = random.Random(11)
            agree, n = 0, 3000
            for _ in range(n):
                a = [D(rnd.randint(1, 10 ** 6)) for _ in range(3)]
                s = sum(a)
                v = [(x / s).quantize(D("0.000001")) for x in a]
                v[2] = D(1) - v[0] - v[1]
                p1, p2 = top_two(v)
                agree += band_eligible(p1, p2) == (max(v) <= D("0.75"))
            case("band, three labels: equals p1 <= 0.75 on %d random vectors (the margin clause adds no post)" % n, agree == n,
                 "%d agree" % agree)
            case("top_two: the largest and second-largest, a tie kept", top_two([D("0.2"), D("0.4"), D("0.4")]) == (D("0.4"), D("0.4")))
            case("predicted label: argmax, a tie to the lower index",
                 predicted([D("0.4"), D("0.4"), D("0.2")]) == 0 and predicted([D("0.2"), D("0.4"), D("0.4")]) == 1
                 and predicted([D("0.1"), D("0.2"), D("0.7")]) == 2)
            case("rank key: lower-case hex sha256 of the UTF-8 bytes of '<seed>:<post_id>'",
                 rank_key("s", "p-1") == hashlib.sha256(b"s:p-1").hexdigest())

        # ---------------------------------------------------------------------------------------- determinism and seed
        def sec_shuffled():
            print("determinism (shuffled input) and the seed:")
            items, recs, lines_a = state["a"]
            mixed = list(items)
            random.Random(5).shuffle(mixed)
            recs_b = _write_items(tmp / "in" / "records-shuffled.jsonl", mixed)
            rc, o, e, h, rows, lines_b, body = run_sel(recs_b, "--no-exclude")
            case("shuffled file order: the selected rows (every line after the header) are byte-identical",
                 rc == 0 and len(lines_a) == 301 and lines_a[1:] == lines_b[1:], "exit %s" % rc)
            h_a = json.loads(lines_a[0]) if lines_a else {}
            strip = lambda hh: {k: v for k, v in hh.items() if k != "records"}  # noqa: E731
            case("shuffled: header counts equal; only records[].sha256 may differ (the input bytes changed), so the whole "
                 "file is not compared",
                 bool(h) and h_a.get("counts") == h.get("counts") and strip(h_a) == strip(h)
                 and [r["id"] for r in h_a.get("records", [])] == [r["id"] for r in h.get("records", [])] == ["source-1"]
                 and h_a["records"][0]["sha256"] != h["records"][0]["sha256"])
            rc, o, e, h2, rows2, lines_c, body2 = run_sel(recs, "--no-exclude")
            case("the same input and seed, run twice: byte-identical output files",
                 rc == 0 and lines_c == lines_a and bool(lines_a))
            rc, o, e, h3, rows3, lines_d, body3 = run_sel(recs, "--no-exclude", "--seed", "another-seed")
            ctl_a = {json.loads(x)["post_id"] for x in lines_a[1:] if json.loads(x)["selection_slice"] == "control"}
            ctl_d = {r["post_id"] for r in rows3 if r["selection_slice"] == "control"}
            case("a different --seed gives a different control set, and the header records the seed",
                 rc == 0 and len(ctl_d) == 75 and ctl_d != ctl_a and h3.get("seed") == "another-seed", "%d shared" % len(ctl_a & ctl_d))

        # ---------------------------------------------------------------------------------------- budget scaling
        def sec_budget():
            print("budget scaling:")
            items, recs, _ = state["a"]
            rc, o, e, h, rows, lines, body = run_sel(recs, "--no-exclude", "--budget", "100")
            per = _per_slice(rows)
            qlab = h.get("counts", {}).get("per_predicted_label_in_quota")
            case("budget 100 scales by the formula: control 25, uncertainty 40, quota 35 = 15 / 10 / 10",
                 rc == 0 and per == {"control": 25, "uncertainty": 40, "quota": 35}
                 and qlab == {"conspiratorial": 15, "social_affiliative": 10, "other": 10}, "%s %s" % (per, qlab))
            ok = sum(SHARES.values()) == PERCENT
            for b in range(BUDGET_MIN, 1001):
                t = targets(b)
                ok = ok and t["control"] == b * 25 // 100 and t["uncertainty"] == b * 40 // 100 \
                    and t["control"] + t["uncertainty"] + t["quota"] == b \
                    and t["quota_conspiratorial"] == t["quota"] * 3 // 7 and t["quota_social_affiliative"] == t["quota"] * 2 // 7 \
                    and t["quota_conspiratorial"] + t["quota_social_affiliative"] + t["quota_other"] == t["quota"]
            case("targets(B), B = 1..1000: the integer formula, slices sum to B, the split sums to the quota", ok)

        # ---------------------------------------------------------------------------------------- shortfall
        def sec_thin():
            print("a thin pool (shortfall, no backfill):")

            def thin(i):
                if i < 10:                               # the only conspiratorial posts, all confident (outside the band)
                    return _probs(1, "0.95")
                return _probs(0 if i % 2 else 2, _P1_CYCLE[(i // 2) % len(_P1_CYCLE)])
            recs = tmp / "in" / "thin.jsonl"
            items = _synthetic_records(recs, 400, thin, prefix="thin")
            rc, o, e, h, rows, lines, body = run_sel(recs, "--no-exclude")
            c = h.get("counts", {})
            sf = c.get("shortfall", {})
            consp = {x["post_id"] for x in items[:10]}
            drawn_before = {r["post_id"] for r in rows if r.get("selection_slice") in ("control", "uncertainty")}
            q_consp = {r["post_id"] for r in rows if r.get("selection_slice") == "quota" and r["post_id"] in consp}
            case("thin pool: fewer than 45 conspiratorial posts left -> all taken, the shortfall recorded",
                 rc == 0 and q_consp == consp - drawn_before and sf.get("quota_conspiratorial") == 45 - len(q_consp)
                 and len(q_consp) < 45, "taken %d shortfall %s" % (len(q_consp), sf.get("quota_conspiratorial")))
            qlab = c.get("per_predicted_label_in_quota", {})
            per = _per_slice(rows)
            case("thin pool: no backfill (other labels stay at 30 / 30, total = budget - shortfall)",
                 bool(rows) and qlab.get("social_affiliative") == 30 and qlab.get("other") == 30
                 and per.get("control") == 75 and per.get("uncertainty") == 120 and len(rows) == 300 - sum(sf.values()),
                 "%d rows" % len(rows))

        # ---------------------------------------------------------------------------------------- all-band
        def sec_all_band():
            print("an all-band pool:")
            recs = tmp / "in" / "all-band.jsonl"
            items = _synthetic_records(recs, 600, lambda i: _probs(i % 3, ("0.70", "0.60", "0.50", "0.45", "0.75")[(i // 3) % 5]),
                                       prefix="band")
            rc, o, e, h, rows, lines, body = run_sel(recs, "--no-exclude")
            c = h.get("counts", {})
            case("all-band pool: band_eligible == pool == 600 and band_share_of_pool 1.0",
                 rc == 0 and c.get("band_eligible") == 600 and c.get("pool") == 600 and c.get("band_share_of_pool") == 1.0)
            ranked = sorted((x["post_id"] for x in items), key=lambda p: _ref_rank(DEFAULT_SEED, p))
            got = lambda s: [r["post_id"] for r in rows if r.get("selection_slice") == s]  # noqa: E731
            case("all-band pool: control is still drawn first (the 75 smallest rank keys)", got("control") == ranked[:75])
            case("all-band pool: uncertainty takes the next 120 of the rest", got("uncertainty") == ranked[75:195])
            by_id = {x["post_id"]: x for x in items}
            rest = ranked[195:]
            want = []
            for lab, k in (("conspiratorial", 45), ("social_affiliative", 30), ("other", 30)):
                want += [p for p in rest if _ref_pred(by_id[p]["probabilities"]) == LABELS.index(lab)][:k]
            case("all-band pool: quota is filled from the remainder, 45 / 30 / 30 by predicted label",
                 sorted(got("quota")) == sorted(want) and len(want) == 105)

        # ---------------------------------------------------------------------------------------- dedupe, exclusion
        def sec_pool():
            global DEFAULT_EXCLUDE
            print("dedupe and exclusion:")
            a = _record(100, _probs(0, "0.60"), "dd", post_id="dup-a", text="same made-up text café %s" % SENTINEL)
            b = _record(101, _probs(1, "0.90"), "dd", post_id="dup-b", text="  same   made-up\ttext café %s \n" % SENTINEL)
            recs = tmp / "in" / "dedupe.jsonl"
            _synthetic_records(recs, 30, prefix="dd", extra=[a, b])
            rc, o, e, h, rows, lines, body = run_sel(recs, "--no-exclude")
            c = h.get("counts", {})
            kept, dropped = sorted(("dup-a", "dup-b"), key=lambda p: _ref_rank(DEFAULT_SEED, p))
            ids = {r["post_id"] for r in rows}
            case("dedupe: two post_ids with one normalized text (NFC, whitespace): the smaller rank key kept, "
                 "duplicate_text_skipped 1",
                 rc == 0 and c.get("duplicate_text_skipped") == 1 and c.get("pool") == 31 and kept in ids and dropped not in ids)
            v1 = tmp / "v1-data"
            items = _synthetic_records(tmp / "in" / "exclude.jsonl", 20, prefix="ex")
            _write_items(v1 / "train.jsonl", [{"text": "  " + items[3]["text"].replace(" ", "  ") + " ", "label": "other"},
                                              {"text": "an unrelated made-up train text", "label": "other"}])
            _write_items(v1 / "eval.jsonl", [{"text": items[5]["text"], "label": "conspiratorial"}])
            rc, o, e, h, rows, lines, body = run_sel(tmp / "in" / "exclude.jsonl", "--exclude-data", str(v1))
            c = h.get("counts", {})
            ids = {r["post_id"] for r in rows}
            case("exclusion: a text equal to a v1 train or eval text (nfc-trim-ws-v1) is excluded and counted",
                 rc == 0 and c.get("excluded_known_text") == 2 and c.get("pool") == 18 and "ex-00003" not in ids
                 and "ex-00005" not in ids and len(ids) == 18 and SENTINEL not in o + e + body, "exit %s %s" % (rc, c.get("excluded_known_text")))
            saved = DEFAULT_EXCLUDE
            DEFAULT_EXCLUDE = tmp / "no-such-v1-dir"
            try:
                out = new_out()
                rc, o, e = run(["--records", str(tmp / "in" / "exclude.jsonl"), "--out", str(out)])
            finally:
                DEFAULT_EXCLUDE = saved
            case("exclusion: the default --exclude-data dir absent (the partner's machine) -> select-exclude-missing, "
                 "naming --no-exclude",
                 rc == 2 and "REFUSED select-exclude-missing" in e and "--no-exclude" in e and not out.exists(), "exit %s" % rc)

        # ---------------------------------------------------------------------------------------- file name
        def sec_file_name():
            print("a file name carrying the sentinel:")
            recs = tmp / "in" / ("records-%s.jsonl" % SENTINEL)
            _synthetic_records(recs, 50, prefix="fn")
            out = tmp / "out" / ("selection-%s.jsonl" % SENTINEL)
            rc, o, e = run(["--records", str(recs), "--out", str(out), "--no-exclude"])
            h, rows, lines, body = _read_selection(out)
            case("file name: a record file whose NAME carries the sentinel succeeds as source-1, the sentinel absent "
                 "from stdout, stderr and the output",
                 rc == 0 and "SELECT OK" in o and [r["id"] for r in (h or {}).get("records", [])] == ["source-1"]
                 and bool(body) and SENTINEL not in o + e + body, "exit %s" % rc)

        # ---------------------------------------------------------------------------------------- refusals
        produced = set()
        nfx = iter(range(1, 10 ** 6))

        def fx(items, name="r"):
            return _write_items(tmp / "rf" / ("%s-%d-%s.jsonl" % (name, next(nfx), SENTINEL)), items)

        def good(i=0, **over):
            return _record(i, _probs(0, "0.60"), "rf", **over)

        def raw_probs(i, text):
            d = good(i)
            d["probabilities"] = "__P__"
            return json.dumps(d).replace('"__P__"', text)

        def refused(name, rule, argv, where=(), out=None):
            target = out or new_out("refused")
            before = target.read_bytes() if target.is_file() else None
            rc, o, e = run(list(argv) + ["--out", str(target)])
            m = re.search(r"REFUSED ([a-z0-9-]+):", e)
            got = m.group(1) if m else None
            if got:
                produced.add(got)
            after = target.read_bytes() if target.is_file() else None
            ok = (rc == 2 and got == rule and SENTINEL not in o + e and all(w in e for w in where) and after == before
                  and "SELECT OK" not in o)
            case("%s: %s" % (rule, name), ok, "exit %s got %s" % (rc, got))

        def accepted(name, items, *extra):
            rc, o, e, h, rows, lines, body = run_sel(fx(items, "ok"), "--no-exclude", *extra)
            case(name, rc == 0 and "SELECT OK" in o and SENTINEL not in o + e + body, "exit %s %s" % (rc, e.strip()[:60]))

        def R(*items, extra=("--no-exclude",)):
            return ["--records", str(fx(list(items)))] + list(extra)

        def sec_refusals():
            print("refusals (exit 2, one REFUSED line, row and field only, no sentinel, nothing written):")
            refused("an absent --records path (the sentinel in its name)", "select-missing",
                    ["--records", str(tmp / ("absent-%s.jsonl" % SENTINEL)), "--no-exclude"], ("source-1",))
            (tmp / ("a-dir-%s" % SENTINEL)).mkdir()
            refused("a directory given as --records", "select-missing", ["--records", str(tmp / ("a-dir-%s" % SENTINEL)), "--no-exclude"])
            refused("a record file inside the repository", "select-in-repo", ["--records", str(CONTRACT), "--no-exclude"], ("source-1",))
            link = tmp / ("link-%s.jsonl" % SENTINEL)
            link.symlink_to(CONTRACT)
            refused("a symlink outside the repository pointing into it", "select-in-repo", ["--records", str(link), "--no-exclude"])
            refused("a line that is not JSON", "select-json", R(good(0), "not json at all " + SENTINEL), ("source-1 row 2",))
            refused("an object that repeats a key", "select-json",
                    R('{"post_id": "a", "post_id": "%s"}' % SENTINEL), ("source-1 row 1",))
            refused("a NaN probability", "select-json", R(good(0, probabilities=[float("nan"), 0.5, 0.5])), ("source-1 row 1",))
            bad = tmp / "rf" / ("bad-utf8-%s.jsonl" % SENTINEL)
            bad.write_bytes((json.dumps(good(0)) + "\n").encode("utf-8") + b'{"text": "\xff\xfe"}\n')
            refused("invalid UTF-8", "select-json", ["--records", str(bad), "--no-exclude"], ("source-1 row 2",))
            refused("a record missing a field", "select-record", R(good(0, tokens=_DROP)), ("source-1 row 1", "tokens"))
            refused("a record with an unknown key", "select-record", R(good(0, **{SENTINEL: 1})), ("source-1 row 1", "1 unknown key"))
            refused("a post_id off the pattern", "select-record", R(good(0, post_id=SENTINEL + " x")), ("source-1 row 1 field post_id",))
            refused("a source off the pattern", "select-record", R(good(0, source="Upper " + SENTINEL)), ("row 1 field source",))
            refused("a text with nothing left after trimming", "select-record", R(good(0, text=" \t\n ")), ("row 1 field text",))
            refused("a text holding a lone surrogate", "select-record", R(good(0, text="made-up \ud800 " + SENTINEL)), ("row 1 field text",))
            refused("a text that is not a string", "select-record", R(good(0, text=[SENTINEL])), ("row 1 field text",))
            refused("tokens below the minimum", "select-record", R(good(0, tokens=TOKENS_MIN - 1)), ("row 1 field tokens",))
            refused("tokens given as a boolean", "select-record", R(good(0, tokens=True)), ("row 1 field tokens",))
            refused("truncated not a boolean", "select-record", R(good(0, truncated="no")), ("row 1 field truncated",))
            refused("an empty record file", "select-record", ["--records", str(fx([])), "--no-exclude"], ("source-1",))
            refused("labels out of served order", "select-labels", R(good(0, labels=list(reversed(LABELS)))), ("row 1 field labels",))
            refused("two probabilities", "select-probabilities", R(good(0, probabilities=[0.5, 0.5])), ("row 1 field probabilities",))
            refused("a probability given as a string", "select-probabilities", R(good(0, probabilities=["0.5", 0.25, 0.25])),
                    ("row 1 field probabilities",))
            refused("a probability given as a boolean", "select-probabilities", R(good(0, probabilities=[True, 0, 0])),
                    ("row 1 field probabilities",))
            refused("a probability above 1", "select-probabilities", R(good(0, probabilities=[1.5, -0.25, -0.25])),
                    ("row 1 field probabilities",))
            refused("another artifact's sha256", "select-sha", R(good(0, artifact_sha256="0" * 64)), ("row 1 field artifact_sha256",))
            refused("an artifact_sha256 off the pattern", "select-sha", R(good(0, artifact_sha256=SENTINEL)), ("row 1 field artifact_sha256",))
            refused("--expect-sha256 naming another artifact", "select-sha", R(good(0), extra=("--no-exclude", "--expect-sha256", "f" * 64)),
                    ("row 1 field artifact_sha256",))
            dep = dict(FIXTURE_DEPLOYMENT)
            del dep["endpoint"]
            refused("a deployment missing a key", "select-deployment", R(good(0, deployment=dep)), ("row 1 field deployment",))
            refused("a deployment that is not an object", "select-deployment", R(good(0, deployment=SENTINEL)), ("row 1 field deployment",))
            other = dict(FIXTURE_DEPLOYMENT, region="eu-west-2")
            refused("a second deployment in one file", "select-deployment", R(good(0), good(1, deployment=other)),
                    ("source-1 row 2 field deployment",))
            refused("a post_id repeated in one file", "select-duplicate-post-id", R(good(0), good(1), good(0)),
                    ("source-1 row 3 field post_id", "source-1 row 1"))
            refused("a post_id repeated across two files", "select-duplicate-post-id",
                    ["--records", str(fx([good(0), good(1)])), "--records", str(fx([good(1)])), "--no-exclude"],
                    ("source-2 row 1 field post_id", "source-1 row 2"))
            refused("budget 0", "select-budget", R(good(0), extra=("--no-exclude", "--budget", "0")))
            refused("budget BUDGET_MAX + 1", "select-budget", R(good(0), extra=("--no-exclude", "--budget", str(BUDGET_MAX + 1))))
            refused("a budget that is not an integer", "select-budget", R(good(0), extra=("--no-exclude", "--budget", "12x")))
            in_repo = REPO / "data" / ("selection-%s.jsonl" % SENTINEL)
            refused("--out inside the repository", "select-out-in-repo", R(good(0)), out=in_repo)
            case("select-out-in-repo: nothing was written in the repository", not in_repo.exists())
            exists = tmp / "out" / ("exists-%s.jsonl" % SENTINEL)
            exists.parent.mkdir(parents=True, exist_ok=True)
            exists.write_bytes(b"an earlier selection\n")
            refused("--out that already exists (left unchanged)", "select-out-exists", R(good(0)), out=exists)
            refused("an --exclude-data dir that does not exist", "select-exclude-missing",
                    R(good(0), extra=("--exclude-data", str(tmp / ("absent-dir-%s" % SENTINEL)))))
            half = tmp / "v1-half"
            _write_items(half / "train.jsonl", [{"text": "made-up", "label": "other"}])
            refused("an --exclude-data dir without eval.jsonl", "select-exclude-missing", R(good(0), extra=("--exclude-data", str(half))))
            case("every refusal rule: the cases produced all 13 declared names (%d) and no other" % len(produced),
                 produced == set(REFUSALS), "missing %s, outside %s" % (sorted(set(REFUSALS) - produced), sorted(produced - set(REFUSALS))))

        # ---------------------------------------------------------------------------------------- contract mirror
        def sec_mirror():
            print("contract mirror (contracts/motivation-enrichment-v1.yaml, structured values):")
            c = yaml.safe_load(CONTRACT.read_text(encoding="utf-8"))
            rec = c["formats"]["motivation-record-v1"]
            f = rec["fields"]
            dep = f["deployment"]
            enr = c["formats"]["motivation-enrichment-v1"]
            case("contract mirror: RECORD_KEYS are the record fields, in order, closed", RECORD_KEYS == tuple(f) and rec["keys_closed"] is True)
            case("contract mirror: DEPLOYMENT_KEYS are the deployment fields, closed, one value per file",
                 DEPLOYMENT_KEYS == tuple(dep["fields"]) and dep["keys_closed"] is True and dep["one_value_per_file"] is True)
            case("contract mirror: LABELS equal served_task.labels and the record's labels.equals",
                 list(LABELS) == c["served_task"]["labels"] == f["labels"]["equals"])
            pats = {"post_id": POST_ID_PATTERN, "source": SOURCE_PATTERN, "artifact_sha256": SHA256_PATTERN, "scored_at": SCORED_AT_PATTERN}
            case("contract mirror: the post_id, source, artifact_sha256 and scored_at patterns",
                 all(f[k]["pattern"] == v for k, v in pats.items()))
            case("contract mirror: the four deployment patterns",
                 {k: v["pattern"] for k, v in dep["fields"].items()} == DEPLOYMENT_PATTERNS)
            p = f["probabilities"]
            case("contract mirror: probabilities length, range and sum_tolerance",
                 p["length"] == PROB_LENGTH and [Decimal(repr(x)) for x in p["range"]] == list(PROB_RANGE)
                 and Decimal(repr(p["sum_tolerance"])) == SUM_TOLERANCE and p["aligned_to"] == "labels")
            case("contract mirror: text min/max chars (after trim) and tokens min",
                 f["text"]["min_chars"] == TEXT_MIN_CHARS and f["text"]["max_chars"] == TEXT_MAX_CHARS
                 and f["text"]["min_chars_after_trim"] is True and f["tokens"]["min"] == TOKENS_MIN
                 and f["truncated"]["type"] == "boolean")
            case("contract mirror: DEFAULT_EXPECT_SHA256 is this round's artifact",
                 DEFAULT_EXPECT_SHA256 == f["artifact_sha256"]["equals_this_round"] == c["served_task"]["artifact_sha256"])
            case("contract mirror: SLICES within the enrichment selection_slice enum; BUDGET_MAX is caps.rows",
                 list(SLICES) == enr["row"]["selection_slice"]["enum"][:3] and BUDGET_MAX == enr["caps"]["rows"])
            tr = dep["this_round"]
            case("contract mirror: the fixture deployment is this round's triple",
                 all(FIXTURE_DEPLOYMENT[k] == tr[k] for k in ("pmcp_target", "aws_account", "region")))
            state["fields"] = f

        # ---------------------------------------------------------------------------------------- boundaries
        def sec_boundaries():
            print("boundaries (limits read from the contract):")
            f = state.get("fields") or yaml.safe_load(CONTRACT.read_text(encoding="utf-8"))["formats"]["motivation-record-v1"]["fields"]
            tol = Decimal(repr(f["probabilities"]["sum_tolerance"]))
            quarter = Decimal("0.25")
            accepted("boundary: a probability sum above 1 by exactly sum_tolerance is accepted",
                     [raw_probs(0, "[0.5, 0.25, %s]" % (quarter + tol))])
            accepted("boundary: a probability sum below 1 by exactly sum_tolerance is accepted",
                     [raw_probs(0, "[0.5, 0.25, %s]" % (quarter - tol))])
            refused("boundary: a sum off by twice sum_tolerance", "select-probabilities",
                    R(raw_probs(0, "[0.5, 0.25, %s]" % (quarter + 2 * tol))), ("row 1 field probabilities",))
            accepted("boundary: scored_at 2026-10-27T00:00:00Z and a fractional second are accepted",
                     [good(0, scored_at="2026-10-27T00:00:00Z"), good(1, scored_at="2026-10-27T00:00:00.123456789Z")])
            refused("boundary: scored_at '2026-10-27 00:00:00'", "select-record", R(good(0, scored_at="2026-10-27 00:00:00")),
                    ("row 1 field scored_at",))
            refused("boundary: an 11-digit aws_account", "select-deployment",
                    R(good(0, deployment=dict(FIXTURE_DEPLOYMENT, aws_account="7" * 11))), ("row 1 field deployment.aws_account",))
            refused("boundary: an http endpoint", "select-deployment",
                    R(good(0, deployment=dict(FIXTURE_DEPLOYMENT, endpoint="http://example.pmcp.run/mcp"))),
                    ("row 1 field deployment.endpoint",))
            tmax = f["text"]["max_chars"]
            at = ("x" * (tmax - len(SENTINEL) - 1)) + " " + SENTINEL
            accepted("boundary: a text of exactly max_chars (%d) is accepted" % tmax, [good(0, text=at)])
            refused("boundary: a text of max_chars + 1", "select-record", R(good(0, text="y" + at)),
                    ("row 1 field text", "%d characters" % (tmax + 1)))
            accepted("boundary: tokens at the minimum and budgets %d and %d are accepted" % (BUDGET_MIN, BUDGET_MAX),
                     [good(0, tokens=f["tokens"]["min"])], "--budget", str(BUDGET_MIN))
            accepted("boundary: budget BUDGET_MAX (%d) is accepted" % BUDGET_MAX, [good(0)], "--budget", str(BUDGET_MAX))

        # ---------------------------------------------------------------------------------------- print-rule
        def sec_print_rule():
            print("--print-rule (the note's generated region):")
            rc, o, e = run(["--print-rule"])
            rc2, o2, e2 = run(["--print-rule"])
            m = re.search(r"```json\n(.*?)\n```\n\Z", o, re.S)
            j = json.loads(m.group(1)) if m else {}
            planned = {
                "format": "motivation-selection-v1", "default_budget": 300, "budget_range": [1, 20000],
                "shares_percent": {"control": 25, "uncertainty": 40, "quota": 35},
                "quota_split": {"denominator": 7, "conspiratorial": 3, "social_affiliative": 2, "other": "the rest"},
                "band": {"top_two_margin_below": "0.20", "top_probability_min": "0.45", "top_probability_max": "0.75",
                         "straddles_motivation_min_prob": "0.6"},
                "default_seed": "motivation-selection-v1",
                "rank_key": "sha256 of the UTF-8 bytes of <seed>:<post_id>, lower-case hex, ascending",
                "draw_order": ["control", "uncertainty", "quota"], "no_backfill": True,
                "targets": {"300": {"control": 75, "uncertainty": 120, "quota": 105, "quota_conspiratorial": 45,
                                    "quota_social_affiliative": 30, "quota_other": 30},
                            "100": {"control": 25, "uncertainty": 40, "quota": 35, "quota_conspiratorial": 15,
                                    "quota_social_affiliative": 10, "quota_other": 10}},
            }
            from_constants = {"default_budget": DEFAULT_BUDGET, "shares_percent": SHARES, "default_seed": DEFAULT_SEED,
                              "budget_range": [BUDGET_MIN, BUDGET_MAX],
                              "band": {k: str(v) for k, v in BAND.items()}}
            case("print-rule: its JSON block equals the planned values and the constants",
                 rc == 0 and j == planned and all(j.get(k) == v or (k == "band" and all(j["band"].get(x) == y for x, y in v.items()))
                                                  for k, v in from_constants.items())
                 and j.get("targets", {}).get("300") == targets(DEFAULT_BUDGET) and j.get("targets", {}).get("100") == targets(100)
                 and j == rule_values(), "exit %s" % rc)
            case("print-rule: byte-stable across two runs, one trailing newline, nothing on stderr",
                 rc2 == 0 and o == o2 and o.endswith("```\n") and not o.endswith("\n\n") and e == e2 == "")
            rows = ("| control | 25 % | B * 25 // 100 | 75 | 25 |", "| uncertainty | 40 % | B * 40 // 100 | 120 | 40 |",
                    "| quota | 35 % | B - control - uncertainty | 105 | 35 |", "| conspiratorial | quota * 3 // 7 | 45 | 15 |",
                    "| social_affiliative | quota * 2 // 7 | 30 | 10 |",
                    "| other | quota - conspiratorial - social_affiliative | 30 | 10 |")
            case("print-rule: the markdown tables hold the counts at budget 300 and 100",
                 all(r in o.splitlines() for r in rows), str([r for r in rows if r not in o.splitlines()])[:80])
            case("print-rule: the markdown states the band, motivation_min_prob 0.6, the seed and the draw order",
                 "`p1 - p2 < 0.20`" in o and "`0.45 <= p1 <= 0.75`" in o and "`motivation_min_prob` 0.6" in o
                 and "`motivation-selection-v1`" in o and "(1) control" in o and "(3) quota" in o)

        # ---------------------------------------------------------------------------------------- capture control
        def sec_control():
            po, pe = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(po), contextlib.redirect_stderr(pe):
                print(SENTINEL)
                print(SENTINEL, file=sys.stderr)
            case("control: the capture sees a sentinel on stdout and on stderr", SENTINEL in po.getvalue() and SENTINEL in pe.getvalue())

        for sec in (sec_declared, sec_band, sec_shuffled, sec_budget, sec_thin, sec_all_band, sec_pool, sec_file_name,
                    sec_refusals, sec_mirror, sec_boundaries, sec_print_rule, sec_control):
            try:
                sec()
            except Exception as ex:                      # a section that crashes is a FAIL, never a traceback with a text
                case("%s: completes without raising" % sec.__name__, False, "raised %s" % type(ex).__name__)

    print("%d case(s) failed" % len(failures) if failures else "all cases ok")
    if not failures:
        print("SELECT MOTIVATION SELFTEST OK")
    return 1 if failures else 0


# ------------------------------------------------------------------------------------------------ main

def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--records", action="append", default=[], metavar="PATH",
                    help="a motivation-record-v1 file, read in place (repeatable; source-1, source-2, ... in order)")
    ap.add_argument("--out", default=None, help="the selection file to write once, outside the repository")
    ap.add_argument("--budget", default=str(DEFAULT_BUDGET), help="posts to select (default %d)" % DEFAULT_BUDGET)
    ap.add_argument("--seed", default=DEFAULT_SEED, help="the rank-key seed (default %s)" % DEFAULT_SEED)
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--exclude-data", default=None, metavar="DIR",
                   help="a v1 data dir whose train.jsonl and eval.jsonl texts are excluded (default data/decide/motivation-v1)")
    g.add_argument("--no-exclude", action="store_true", help="exclude no known texts (the partner's machine)")
    ap.add_argument("--expect-sha256", default=DEFAULT_EXPECT_SHA256, metavar="HEX",
                    help="the artifact sha256 every record must carry (default the served motivation-v1 artifact)")
    ap.add_argument("--print-rule", action="store_true", help="print the rule section the selection note embeds")
    ap.add_argument("--selftest", action="store_true", help="the declared rule over synthetic records")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    if args.print_rule:
        sys.stdout.write(rule_markdown())
        return 0
    print("SELECT NOT IMPLEMENTED (plan 13-05 RED)", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
