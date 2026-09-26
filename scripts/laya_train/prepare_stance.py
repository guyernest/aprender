"""TweetEval stance_abortion -> data/decide/tweet-stance-16/{task.json, train.jsonl, eval.jsonl} (D-19).

    uv run --project scripts/laya_train --frozen python scripts/laya_train/prepare_stance.py
    (or: just laya-prepare-stance)

Source: the gitignored local dataset `apr data tweet-eval-stance --output data/tweet-eval-stance`
(train.jsonl / test.jsonl rows {id, input, label, label_text, source_split}).

Selection: the contract's demo selection manifest (laya-finetune-gate-v1 `demo.selection`, s16-seed13).
Every `ordered_examples[].id` is looked up in the source train split and REFUSED unless sha256 of its
input equals the manifest `exact_hash`, its nfc-trim-ws-v1 hash equals `normalized_hash`, and its
label index equals the manifest label. `label_names` must equal the contract's `demo.criteria_order`.

Output lands under the root-anchored, gitignored /data/: tweet text is never committed and never
printed (counts and hashes only).
"""
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import contract  # noqa: E402
from data import exact_sha256, normalized_sha256  # noqa: E402

REPO = contract.REPO
SRC = REPO / "data" / "tweet-eval-stance"
OUT = REPO / "data" / "decide" / "tweet-stance-16"

# The stance-abortion task exactly as spike 024 asked it (tools/tasks.py), criteria in label order.
INSTRUCTIONS = "What stance does the author of this tweet take on abortion?"
DESCRIPTIONS = {
    "none": "No stance on abortion, or the tweet is not about abortion",
    "against": "Opposes abortion (pro-life)",
    "favor": "Supports abortion rights (pro-choice)",
}


def fail(msg):
    print("PREPARE FAILED: " + msg, file=sys.stderr)
    sys.exit(1)


def read_jsonl(path):
    rows = {}
    order = []
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        r = json.loads(line)
        if r["id"] in rows:
            fail("%s line %d repeats id %s" % (path.name, n, r["id"]))
        rows[r["id"]] = r
        order.append(r["id"])
    return rows, order


def main():
    demo = contract.demo()
    order = list(demo["criteria_order"])
    if list(DESCRIPTIONS) != order:
        fail("criteria descriptions %s are not in the contract order %s" % (list(DESCRIPTIONS), order))
    for name in ("train.jsonl", "test.jsonl"):
        if not (SRC / name).is_file():
            fail("%s is missing; run: apr data tweet-eval-stance --output data/tweet-eval-stance" % (SRC / name))
    manifest = json.loads((REPO / demo["selection"]).read_text())
    payload = manifest["payload"]
    if payload["label_names"] != order:
        fail("selection label_names %s != contract criteria_order %s" % (payload["label_names"], order))
    if int(payload["shots_per_class"]) != int(demo["shots_per_class"]):
        fail("selection shots_per_class %s != contract %s" % (payload["shots_per_class"], demo["shots_per_class"]))

    train, _ = read_jsonl(SRC / "train.jsonl")
    test, test_order = read_jsonl(SRC / "test.jsonl")

    shots = []
    for ex in payload["ordered_examples"]:
        r = train.get(ex["id"])
        if r is None:
            fail("selection id %s is not in %s" % (ex["id"], SRC / "train.jsonl"))
        if exact_sha256(r["input"]) != ex["exact_hash"]:
            fail("selection id %s: sha256(input) != manifest exact_hash (the local dataset differs)" % ex["id"])
        if normalized_sha256(r["input"]) != ex["normalized_hash"]:
            fail("selection id %s: nfc-trim-ws-v1 hash != manifest normalized_hash" % ex["id"])
        if int(r["label"]) != int(ex["label"]) or r["label_text"] != order[int(ex["label"])]:
            fail("selection id %s: label %s/%s disagrees with manifest label %s"
                 % (ex["id"], r["label"], r["label_text"], ex["label"]))
        shots.append((r["input"], order[int(ex["label"])]))

    eval_rows = []
    for tid in test_order:
        r = test[tid]
        if r["label_text"] != order[int(r["label"])]:
            fail("test id %s: label %s/%s is not in the contract order" % (tid, r["label"], r["label_text"]))
        eval_rows.append((r["input"], r["label_text"]))
    if len(eval_rows) != int(demo["eval_rows"]):
        fail("test split has %d rows, the contract demo declares %s" % (len(eval_rows), demo["eval_rows"]))

    OUT.mkdir(parents=True, exist_ok=True)
    task = {"type": "choice", "instructions": INSTRUCTIONS, "criteria": DESCRIPTIONS}
    (OUT / "task.json").write_bytes((json.dumps(task, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))
    for name, rows in (("train.jsonl", shots), ("eval.jsonl", eval_rows)):
        body = "".join(json.dumps({"text": t, "label": lab}, ensure_ascii=False) + "\n" for t, lab in rows)
        (OUT / name).write_bytes(body.encode("utf-8"))

    per_class = {lab: sum(1 for _, x in shots if x == lab) for lab in order}
    eval_class = {lab: sum(1 for _, x in eval_rows if x == lab) for lab in order}
    print("prepared %s: train %d rows %s (verified against %s), eval %d rows %s"
          % (OUT.relative_to(REPO), len(shots), per_class, demo["selection"], len(eval_rows), eval_class))
    print("PREPARE OK")


if __name__ == "__main__":
    main()
