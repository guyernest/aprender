"""Spike 033 back-office half: export Kev-4B for the Rust path and write the parity fixture (adapted from spike 017's
export_and_oracle.py).

ONE structural change from 017 (Phase 12 plan 12-14, review concern): 017 held the merged Kev model AND a second fp32
AutoModelForCausalLM at once (about 2 x 16 GB). Here the merged backbone's state dict (`m.lm.state_dict()`) is written
DIRECTLY as safetensors shards (keys prefixed `model.`, an index json, the tokenizer and a CausalLM-shaped config beside
them); lm_head.weight is copied from the base shards, one tensor, only when the base ties no embeddings.

Run with the kev vendor venv, offline:
  HF_HUB_OFFLINE=1 .planning/spikes/015-kev-vs-setfit-few-shot/vendor/kev/.venv/bin/python \
      .planning/spikes/033-motivation-kev-4b-arm/tools/export_kev4b.py --run jaredpalmer/kev-4b --name kev-4b

Outputs (gitignored /models/): models/kev/<name>/{merged/, head.safetensors, parity_fixture.json, export_status.json}.
Fixture: six public stance rows (spike 015's stance-abortion, cardiffnlp/tweet_eval) and the 3-question ticket, plus ONE long
synthetic row of about 6,800 Qwen tokens (seeded filler from a fixed word list, no motivation data, no guide text).
"""
import argparse, json, sys, time, random
from pathlib import Path
import torch
from safetensors import safe_open
from safetensors.torch import save_file

HERE = Path(__file__).resolve().parent; SPIKE = HERE.parent; SPIKES = SPIKE.parent
REPO = SPIKES.parent.parent
sys.path.insert(0, str(SPIKES / "015-kev-vs-setfit-few-shot" / "vendor" / "kev"))
sys.path.insert(0, str(SPIKES / "015-kev-vs-setfit-few-shot" / "tools"))
from kev.api import SystemOneRequest, to_record
from kev.checkpoint import Checkpoint, LoadOptions
from kev.model import rows_of
from tasks import load, request

SHARD_BYTES = 5 * 10**9
LONG_TARGET = 6800
WORDS = ("river stone market lantern copper window garden thread harbor engine paper violet anchor meadow ribbon "
         "compass winter basket mirror candle forest pillow bridge saddle marble orchard feather kettle valley "
         "button ladder cobble tunnel pepper blanket sparrow quarry lighthouse barrel meadow ember trellis "
         "pebble satchel furnace cradle thimble rooftop clover bobbin teapot lattice spindle gable chimney").split()


def filler(rng, n_words):
    """Made-up sentences from a fixed word list (no real text)."""
    out, left = [], n_words
    while left > 0:
        k = min(left, rng.randint(8, 14))
        s = " ".join(rng.choice(WORDS) for _ in range(k))
        out.append(s[0].upper() + s[1:] + ".")
        left -= k
    return " ".join(out)


def long_request(tok, rng_seed=13):
    """A choice request whose encoded row (state + instruction branch) is about LONG_TARGET Qwen tokens."""
    rng = random.Random(rng_seed)
    state = filler(rng, 70)
    criteria = {"alpha": filler(rng, 14), "beta": filler(rng, 14), "gamma": filler(rng, 14)}
    n = 4300
    for _ in range(8):
        req = {"state": state, "questions": {"filler": {"type": "choice", "instructions": filler(random.Random(rng_seed + 1), n), "criteria": criteria}}}
        rec, qmeta = to_record(SystemOneRequest(**req))
        from kev.model import encode
        enc = encode(tok, rec, max_state=8192, max_branch=8192, strict=True)
        total = len(enc["ids"])
        if abs(total - LONG_TARGET) <= 150:
            return req
        n = max(200, int(n * LONG_TARGET / total))
    raise SystemExit(f"could not size the long synthetic row near {LONG_TARGET} tokens (last {total})")


def write_shards(sd, out):
    out.mkdir(parents=True, exist_ok=True)
    shards, cur, size = [], {}, 0
    for k, v in sd.items():
        t = v.detach().to(torch.float32).contiguous()
        nb = t.numel() * 4
        if cur and size + nb > SHARD_BYTES:
            shards.append(cur); cur, size = {}, 0
        cur[k] = t; size += nb
    if cur: shards.append(cur)
    wm, total = {}, 0
    for i, sh in enumerate(shards, 1):
        name = f"model-{i:05d}-of-{len(shards):05d}.safetensors"
        save_file(sh, str(out / name), metadata={"format": "pt"})
        for k, t in sh.items(): wm[k] = name; total += t.numel() * 4
    (out / "model.safetensors.index.json").write_text(json.dumps({"metadata": {"total_size": total}, "weight_map": wm}, indent=2))
    return wm, len(shards), total


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--run", default="jaredpalmer/kev-4b"); ap.add_argument("--name", default="kev-4b")
    ap.add_argument("--out", default=None, help="default models/kev/<name> under the repo root")
    a = ap.parse_args()
    torch.manual_seed(0)
    root = Path(a.out) if a.out else REPO / "models" / "kev" / a.name
    merged = root / "merged"
    status = {"run": a.run, "name": a.name}
    t0 = time.perf_counter()
    ck = Checkpoint(a.run); meta = ck.meta
    tok, m = ck.load("cpu", LoadOptions(backend="torch", dtype=torch.float32, merge=True)); m.eval()
    T = float(m.head.temperature)
    print(f"loaded + merged {a.run} in {time.perf_counter()-t0:.1f}s, T {T}", flush=True)

    # ---- write the merged text backbone directly (no second resident model)
    from huggingface_hub import constants
    from transformers import AutoConfig
    # the cached snapshot lacks README/LICENSE, so snapshot_download(local_files_only) calls it incomplete: read the dir
    base_dir = Path(constants.HF_HUB_CACHE) / ("models--" + meta.base.replace("/", "--")) / "snapshots" / meta.base_revision
    assert (base_dir / "model.safetensors.index.json").exists(), base_dir
    cfg = AutoConfig.from_pretrained(base_dir).get_text_config()
    tied = bool(getattr(cfg, "tie_word_embeddings", False))
    sd = {"model." + k: v for k, v in m.lm.state_dict().items()}
    base_ix = json.loads((base_dir / "model.safetensors.index.json").read_text())["weight_map"]
    # The base is multimodal (model.language_model.* / model.visual.* / mtp.*); the text backbone's CausalLM names are
    # model.language_model.X -> model.X. The written set must equal exactly that image of the base's text keys.
    base_text = {"model." + k[len("model.language_model."):] for k in base_ix if k.startswith("model.language_model.")}
    if not tied: base_text.add("lm_head.weight")
    if not tied:
        k = "lm_head.weight"
        with safe_open(str(base_dir / base_ix[k]), framework="pt") as f: sd[k] = f.get_tensor(k).to(torch.float32)
    equal = set(sd) == base_text
    status["merged_tensor_keys_equal_base"] = equal
    status["base_keys"] = {"total": len(base_ix), "text": len(base_text), "dropped_visual": sum(k.startswith("model.visual.") for k in base_ix),
                           "dropped_mtp": sum(k.startswith("mtp.") for k in base_ix), "tie_word_embeddings": tied}
    status["only_in_written"] = sorted(set(sd) - base_text)[:10]; status["only_in_base"] = sorted(base_text - set(sd))[:10]
    assert equal, status
    t1 = time.perf_counter()
    wm, n_shards, total = write_shards(sd, merged)
    del sd
    cfg.architectures = ["Qwen3_5ForCausalLM"]; cfg.dtype = "float32"
    cfg.save_pretrained(merged)
    tok.save_pretrained(merged)
    gc = base_dir / "generation_config.json"
    if gc.exists(): (merged / "generation_config.json").write_bytes(gc.read_bytes())
    status.update({"shards": n_shards, "merged_bytes": total, "export_s": time.perf_counter() - t1, "merged_dir": str(merged)})
    print(f"export: {n_shards} shards, {total/1e9:.2f} GB, {len(wm)} tensors -> {merged} in {time.perf_counter()-t1:.1f}s", flush=True)
    root.mkdir(parents=True, exist_ok=True)
    save_file({k: v.detach().float().contiguous() for k, v in m.head.state_dict().items()}, str(root / "head.safetensors"),
              metadata={"temperature": repr(T), "run": a.run, "base": meta.base, "base_revision": meta.base_revision})
    (root / "export_status.json").write_text(json.dumps(status, indent=2))

    # ---- fixture records: six public stance rows, a 3-question ticket
    recs = [request("stance-abortion", t) for t, _ in load("stance-abortion", "test")[:6]]
    recs.append({"state": "Shoes arrived two weeks late and in the wrong size. Also I see two charges on my card.", "questions": {
        "department": {"type": "choice", "instructions": "Which team should handle this?", "criteria": {
            "returns": "Exchanges, refunds, wrong or damaged items", "shipping": "Delivery status, delays, lost packages",
            "billing": "Charges, invoices, payment problems"}},
        "escalate": {"type": "noul", "instructions": "Does this need urgent human attention?"},
        "frustration": {"type": "score", "instructions": "How frustrated is the customer?", "criteria": ["Calm", "Frustrated", "Very angry"]}}})
    fixture = {"run": a.run, "base": meta.base, "base_revision": meta.base_revision, "temperature": T,
               "hidden_size": m.lm.config.hidden_size, "records": []}

    def oracle(req, check_probs):
        rec, qmeta = to_record(SystemOneRequest(**req))
        enc = m.encode(tok, rec, max_state=8192, max_branch=8192, strict=True)
        S, Sp, brs = rows_of(enc)
        probs = None; t0 = time.perf_counter()
        if check_probs: probs = [p.tolist() for p in m.probs(enc)]
        torch_ms = 1000 * (time.perf_counter() - t0)
        rows = []
        for qi, r in enumerate(brs):
            ids, pos = S + r["ids"], Sp + r["pos"]
            assert pos == list(range(len(ids))), "row positions are not contiguous"
            t1 = time.perf_counter()
            h = m.lm(input_ids=torch.tensor([ids]), position_ids=torch.tensor([pos])).last_hidden_state[0].float()
            row_ms = 1000 * (time.perf_counter() - t1)
            d, oi = len(S) + r["decide"], [len(S) + o for o in r["opts"]]
            z = m.head.k(h[oi]) @ m.head.q(h[d]) * m.head.scale
            rows.append({"ids": ids, "decide": d, "opts": oi, "h_decide": h[d].tolist(), "h_opts": h[oi].tolist(),
                         "logits_raw": z.tolist(), "probs": torch.softmax(z / T, -1).tolist(), "torch_ms": row_ms})
        if check_probs:
            assert all(abs(p - q) < 1e-5 for r, pp in zip(rows, probs) for p, q in zip(r["probs"], pp)), "row recompute != m.probs"
        return {"request": req if check_probs else {"synthetic": True}, "keys": [q["keys"] for q in qmeta], "n_state": len(S), "rows": rows, "torch_cpu_ms": torch_ms}

    with torch.no_grad():
        for ri, req in enumerate(recs):
            rec_out = oracle(req, True); fixture["records"].append(rec_out)
            print(f"record {ri}: {len(rec_out['rows'])} q, state {rec_out['n_state']} tok, rows {[len(r['ids']) for r in rec_out['rows']]}", flush=True)
        json.dump(fixture, open(root / "parity_fixture.json", "w"))
        status["short_fixture_written"] = True
        (root / "export_status.json").write_text(json.dumps(status, indent=2))
        print("short-row fixture written", flush=True)

        # ---- the long synthetic row (about 6,800 tokens): covers the operating regime; an OOM is recorded, not fatal
        req = long_request(tok)
        status["parity_long"] = {"status": "pending"}
        (root / "export_status.json").write_text(json.dumps(status, indent=2))   # survives a kill of this process
        try:
            t2 = time.perf_counter()
            rec_out = oracle(req, False)
            n = len(rec_out["rows"][0]["ids"])
            fixture["records"].append(rec_out)
            json.dump(fixture, open(root / "parity_fixture.json", "w"))
            status["parity_long"] = {"status": "measured_oracle", "n_tokens": n, "oracle_s": time.perf_counter() - t2}
            print(f"long synthetic row: {n} tokens, torch oracle {time.perf_counter()-t2:.0f}s; fixture rewritten", flush=True)
        except (RuntimeError, MemoryError) as e:
            status["parity_long"] = {"status": "not_measured", "reason": "memory", "error": str(e)[:300]}
            print("long row oracle failed:", str(e)[:300], flush=True)
    (root / "export_status.json").write_text(json.dumps(status, indent=2))
    print("export complete", flush=True)


if __name__ == "__main__":
    main()
