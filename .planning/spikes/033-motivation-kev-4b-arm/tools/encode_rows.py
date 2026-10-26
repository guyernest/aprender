"""Encode the motivation-v1 rows for the Kev-4B arm (Phase 12 plan 12-14, D-24 / D-11 / OPS-03).

Reads data/decide/motivation-v1/{task.json, train.jsonl, eval.jsonl} AS WRITTEN through scripts/laya_train/data.py (never
re-split), builds one Kev request per post

    {state: post, questions: {motivation: {type: "choice", instructions: <the guide text>, criteria: task.json's criteria in
     document order with their descriptions}}}

encodes each with `kev.model.encode(strict=True, max_state=8192, max_branch=8192)` and writes token IDS ONLY (no text) to
models/kev/motivation-v1/rows-train.json and rows-eval.json: {row, split, label, text_sha256, ids, decide, opts}.

Leakage (D-11), by text match under data.normalize (nfc-trim-ws-v1): how many normalized eval posts occur in the normalized
guide text (expected 0); how many fitting (train) rows are eval rows (the normalized sha256 sets must be disjoint). When an
eval post occurs in the guide, the REMOVAL BRANCH runs: extract_guide.py --drop-paragraphs-matching drops every guide
paragraph that contains one, the rows are re-encoded and the before/after counts are recorded. A train row that is an eval
row is a split defect, not a prompt edit: the arm is blocked (failure_class leakage).

Run with the kev vendor venv, offline:  HF_HUB_OFFLINE=1 <venv python> -I tools/encode_rows.py --evidence <12-KEV-ARM-EVIDENCE.json>
It prints counts and hashes only, never post text or guide text.
"""
import argparse, hashlib, importlib.util, json, subprocess, sys
from pathlib import Path

HERE = Path(__file__).resolve().parent; SPIKE = HERE.parent; SPIKES = SPIKE.parent; REPO = SPIKES.parent.parent
sys.path.insert(0, str(SPIKES / "015-kev-vs-setfit-few-shot" / "vendor" / "kev"))
from kev.api import SystemOneRequest, to_record
from kev.checkpoint import Checkpoint
from kev.model import ContextOverflow, encode, load_tokenizer, rows_of, user_tokens

spec = importlib.util.spec_from_file_location("laya_data", REPO / "scripts" / "laya_train" / "data.py")
data = importlib.util.module_from_spec(spec); spec.loader.exec_module(data)

DATA = REPO / "data" / "decide" / "motivation-v1"
GUIDE = REPO / "data" / "kev" / "motivation-v1" / "guide-s4-6-no-4-5.txt"
OUT = REPO / "models" / "kev" / "motivation-v1"
BOUNDS = (6500, 7200)
OMITTED = ("4.5 (the derivation narrative): with it Sections 4-6 are 9,648 Qwen tokens, above Kev's 8,192-token row limit (D-24)")
REGIME = "about 6.6x Kev's 1,024-token training branch, outside its training distribution"


def merge(path, kv):
    e = json.load(open(path)) if Path(path).exists() else {}
    for k, v in kv.items():
        d = e; parts = k.split(".")
        for x in parts[:-1]: d = d.setdefault(x, {})
        d[parts[-1]] = v
    json.dump(e, open(path, "w"), indent=2); open(path, "a").write("\n")


def leak_counts(guide_text, train, ev):
    g = data.normalize(guide_text)
    ev_norm = [data.normalize(t) for t, _ in ev]
    tr_hash = {data.normalized_sha256(t) for t, _ in train}
    ev_hash = {data.normalized_sha256(t) for t, _ in ev}
    return {
        "eval_posts_in_guide": sum(1 for p in ev_norm if p in g),
        "fit_rows_in_eval": len(tr_hash & ev_hash),
        # informational (not the D-11 rule): a 50-character prefix of a post quoted with different punctuation after it
        "eval_post_prefix50_in_guide": sum(1 for p in ev_norm if len(p) >= 50 and p[:50] in g),
        "train_posts_in_guide": sum(1 for t, _ in train if data.normalize(t) in g),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--evidence", required=True)
    ap.add_argument("--guide", default=str(GUIDE))
    a = ap.parse_args()
    task = data.load_task(DATA / "task.json")
    train = data.load_rows(DATA / "train.jsonl", task, "train")
    ev = data.load_rows(DATA / "eval.jsonl", task, "eval")
    labels = task["labels"]
    crit = dict(task["criteria"])

    guide_text = Path(a.guide).read_text(encoding="utf-8")
    before = leak_counts(guide_text, train, ev)
    removed = None
    if before["fit_rows_in_eval"] > 0:
        merge(a.evidence, {"outcome": "blocked", "failure_class": "leakage",
                           "failure_line": "%d fitting row(s) are eval rows (split defect, D-11)" % before["fit_rows_in_eval"],
                           "leakage": {**before, "method": "nfc-trim-ws-v1"}})
        sys.exit("BLOCKED leakage: a fitting row is an eval row")
    if before["eval_posts_in_guide"] > 0:
        out = subprocess.run([sys.executable, "-I", str(HERE / "extract_guide.py"), "--out", a.guide,
                              "--drop-paragraphs-matching", str(DATA / "eval.jsonl")], capture_output=True, text=True, check=True).stdout
        removed = json.loads(out)["removed_paragraphs"]
        guide_text = Path(a.guide).read_text(encoding="utf-8")
    after = leak_counts(guide_text, train, ev)
    leakage = {"method": "nfc-trim-ws-v1 (scripts/laya_train/data.normalize): substring match of each normalized eval post in the "
                         "normalized guide text; sha256 intersection of the normalized train and eval texts",
               "eval_posts_in_guide": after["eval_posts_in_guide"], "fit_rows_in_eval": after["fit_rows_in_eval"],
               "eval_post_prefix50_in_guide": after["eval_post_prefix50_in_guide"], "train_posts_in_guide": after["train_posts_in_guide"]}
    if removed is not None:
        leakage.update({"eval_posts_in_guide_before": before["eval_posts_in_guide"], "removed_paragraphs": removed})
    assert leakage["eval_posts_in_guide"] == 0, "eval posts remain in the guide after the D-11 removal"

    ck = Checkpoint("jaredpalmer/kev-4b")
    tok = load_tokenizer(ck.meta.base, revision=ck.meta.base_revision)
    guide_tokens = len(user_tokens(tok, guide_text))
    if not BOUNDS[0] <= guide_tokens <= BOUNDS[1]:
        sys.exit("STOP: the guide text is %d Qwen tokens, outside %s: the section boundaries differ from the research's reading" % (guide_tokens, BOUNDS))

    def encode_split(split, rows):
        out_rows, mx = [], 0
        for i, (text, y) in enumerate(rows):
            req = {"state": text, "questions": {"motivation": {"type": "choice", "instructions": guide_text, "criteria": crit}}}
            rec, _ = to_record(SystemOneRequest(**req))
            try:
                enc = encode(tok, rec, max_state=8192, max_branch=8192, strict=True)
            except ContextOverflow as e:
                merge(a.evidence, {"outcome": "blocked", "failure_class": "context", "failure_line": "%s row %d: %s" % (split, i, e)})
                sys.exit("BLOCKED context: %s row %d: %s" % (split, i, e))
            S, _, brs = rows_of(enc)
            r = brs[0]
            ids = S + r["ids"]
            assert len(ids) == len(enc["ids"])
            mx = max(mx, len(ids))
            out_rows.append({"row": i, "split": split, "label": labels[y], "text_sha256": data.normalized_sha256(text),
                             "ids": ids, "decide": len(S) + r["decide"], "opts": [len(S) + o for o in r["opts"]]})
        return out_rows, mx

    OUT.mkdir(parents=True, exist_ok=True)
    rt, m1 = encode_split("train", train)
    re_, m2 = encode_split("eval", ev)
    for name, rows in (("rows-train.json", rt), ("rows-eval.json", re_)):
        (OUT / name).write_text(json.dumps(rows, separators=(",", ":")))
    row_tokens = [len(r["ids"]) for r in rt + re_]
    prompt = {"guide_sha256": hashlib.sha256(guide_text.encode("utf-8")).hexdigest(), "guide_words": len(guide_text.split()),
              "qwen_tokens": guide_tokens, "instruction_branch_tokens": guide_tokens + 1, "max_row_tokens": max(m1, m2),
              "min_row_tokens": min(row_tokens), "sections_kept": ["4.1", "4.2", "4.3", "4.4", "5", "6"],
              "section_4_lead_in_included": True, "omitted": OMITTED, "regime_note": REGIME, "options": labels,
              "rows": {"train": len(rt), "eval": len(re_)}}
    merge(a.evidence, {"prompt": prompt, "leakage": leakage})
    print(json.dumps({"prompt": prompt, "leakage": leakage}))


if __name__ == "__main__":
    main()
