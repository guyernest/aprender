"""Fine-tune Laya on a user's shots, calibrate, and gate -- the local back office (D-01..D-08).

    uv run --project scripts/laya_train --frozen python scripts/laya_train/train.py \
        --data DIR --out DIR [--epochs E] [--device mps|cuda|cpu]
    (or: just laya-train <data dir> <run dir> [args])

    --variant synthetic-fixture --base CKPT_DIR --base-sha256 HEX --epochs E
        the lifecycle self-test on a tiny synthetic checkpoint (just laya-train-lifecycle). The
        production variant ALWAYS uses the contract's pinned base; --base is refused without it.

The run, in order (every value from contracts/laya-finetune-gate-v1.yaml via contract.py):

  1. validate the data dir (data.py): task.json, train.jsonl, and a REQUIRED eval.jsonl; eval text
     overlapping train (NFC/trim/whitespace-normalized) is refused; conflicting train duplicates refused.
  2. resolve the recipe, write <out>/recipe.json (sort_keys, compact) and print RECIPE WRITTEN <sha256>
     BEFORE any model scores anything (recipe_before_scores, D-04).
  3. request the device mps -> cuda -> cpu, load Laya's own Agent on the pinned base with
     expected_sha256={"model.safetensors": <sha>} (the mapping laya.revisions.verify_digests requires),
     and record the device READ BACK from the parameters (Laya silently falls back to CPU, D-03).
  4. split fit / calibration by normalized-text GROUP (seeded, stratified), build rows with Laya's own
     Agent._encode_state, and run the spike-024 ft_laya.py loop.
  5. write the COMPLETE checkpoint dir (F16 weights with an F32 temperature, rl_agent_config.json,
     encoder/, tokenizer/ with tokenizer_config.json already in Laya's fixed form) and print
     CHECKPOINT COMPLETE -- before anything reloads it.
  6. RELOAD it through Agent(<ckpt>, device="cpu") in fp32 (sha256s asserted unchanged), print
     SCORING START, fit T by NLL on the calibration slice within [0.5, 5.0], write T into
     rl_agent_config.json, and reload AGAIN so every eval probability comes from Laya's own predict
     path with the saved temperature (f16_reload_scoring, Pitfall 5).
  7. zero-shot: the declared base on eval.jsonl -> zero-shot-probs.json; fine-tuned -> eval-probs.json;
     probes on the decide-apr-v1 probe task -> probes.json.
  8. gate.evaluate_gate -> gate-report.json; GATE PASS (exit 0) or GATE FAIL (exit 3).

Logs carry counts, hashes and timings only -- never input text.
"""
import argparse
import hashlib
import json
import math
import os
import random
import shutil
import sys
import time
import warnings
from pathlib import Path

os.environ.setdefault("TOKENIZERS_PARALLELISM", "false")

import numpy as np  # noqa: E402
import torch  # noqa: E402
import torch.nn.functional as F  # noqa: E402
from safetensors.torch import save_file  # noqa: E402

import laya.agent as laya_agent  # noqa: E402
from laya import Agent  # noqa: E402
from laya.common import QTYPES, proper_reward, temp_bucket  # noqa: E402

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import contract  # noqa: E402
import data  # noqa: E402
import gate  # noqa: E402

EXIT_PASS, EXIT_REFUSED, EXIT_GATE_FAIL = 0, 2, 3
BASE_FILES = ("rl_agent_config.json", "model.safetensors", "tokenizer/*", "encoder/*")


def log(msg):
    print(msg, flush=True)


def refuse(msg):
    print(msg if msg.startswith("REFUSED") else "REFUSED " + msg, file=sys.stderr, flush=True)
    sys.exit(EXIT_REFUSED)


def f32_list(t):
    return [float(x) for x in np.asarray(t, dtype=np.float32).reshape(-1)]


def f32_hex_list(t):
    h = np.ascontiguousarray(np.asarray(t, dtype=np.float32).reshape(-1)).astype(">f4").tobytes().hex()
    return [h[i:i + 8] for i in range(0, len(h), 8)]


def write_json(path, obj):
    Path(path).write_bytes((json.dumps(obj, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))


def tree_sha256(root):
    root = Path(root)
    return {str(p.relative_to(root)): data.sha256_file(p) for p in sorted(root.rglob("*")) if p.is_file()}


# ------------------------------------------------------------------------------------------ Laya glue

class _ConfidenceSpy:
    """Captures the unrounded `p` Laya's Agent._decode_answers computes (at its answer_confidence call).

    `predict` publishes round(p, 4); the gate needs the unrounded probabilities of that same path, so the
    spy records them and the caller asserts the public answer equals round(p, 4)."""

    def __init__(self):
        self.captured = []
        self._orig = None

    def __enter__(self):
        self._orig = laya_agent.answer_confidence

        def spy(p, k):
            self.captured.append(np.array(p, dtype=p.dtype, copy=True))
            return self._orig(p, k)
        laya_agent.answer_confidence = spy
        return self

    def __exit__(self, *exc):
        laya_agent.answer_confidence = self._orig
        return False


class Scorer:
    """Runs Laya's OWN `Agent.predict` for one (text, question) and returns (p, logits[:K], n_tokens)."""

    def __init__(self, agent):
        self.agent = agent
        self.logits = []
        orig_forward = agent._forward

        def forward(b):
            out = orig_forward(b)
            self.logits.append(np.array(out[0], copy=True))
            return out
        agent._forward = forward

    def score(self, text, question):
        agent = self.agent
        internal = {"q": Agent._to_internal(question)}
        it = agent._encode_state(text, ["q"], internal)[0]
        k = len(it["markers"])
        self.logits.clear()
        with _ConfidenceSpy() as spy:
            res = agent.predict(text, {"q": question})
        if len(spy.captured) != 1 or len(self.logits) != 1:
            raise RuntimeError("expected one decoded question and one forward, got %d / %d"
                               % (len(spy.captured), len(self.logits)))
        p = spy.captured[0]
        z = self.logits[0][0, :k]
        public = list(res["answers"]["q"]["probabilities"].values())
        if public != [round(float(v), 4) for v in p]:
            raise RuntimeError("predict's public answer is not round(p, 4): the spy captured another path")
        t = agent.temperature_by_options.get(temp_bucket(it["qtype"], k), agent.temperature[it["qtype"]])
        ref = gate.softmax(z[None, :], t)[0]
        if np.abs(ref - p.astype(np.float64)).max() > 1e-6:
            raise RuntimeError("captured p is not softmax(logits / T) at the applied temperature")
        return p, z, len(it["ids"])


def load_agent(src, device, digest, revision=None):
    """Laya's own loader with the digest MAPPING; returns the agent and the device READ BACK."""
    kw = {"device": device, "expected_sha256": {"model.safetensors": digest}}
    if revision is not None:
        kw["revision"] = revision
    agent = Agent(str(src), **kw)
    return agent, str(next(agent.model.parameters()).device)


def load_for_scoring(src, digest, revision=None):
    agent, used = load_agent(src, "cpu", digest, revision)
    agent.model.float().eval()
    if used != "cpu":
        raise RuntimeError("scoring agent landed on %s, expected cpu" % used)
    return agent


def request_device(forced):
    if forced:
        return forced
    for d in contract.device_order():
        if d == "mps" and hasattr(torch.backends, "mps") and torch.backends.mps.is_available():
            return "mps"
        if d == "cuda" and torch.cuda.is_available():
            return "cuda"
        if d == "cpu":
            return "cpu"
    return "cpu"


def fixed_tokenizer_config_bytes(src_bytes):
    """tokenizer_config.json in the form laya.agent._fix_tokenizer_config produces, so Laya's loader
    never rewrites it in place (a hashed checkpoint file changing after the fact, T-08-08-06)."""
    tcfg = json.loads(src_bytes)
    changed = False
    if tcfg.get("tokenizer_class") in (None, "TokenizersBackend"):
        tcfg["tokenizer_class"] = "PreTrainedTokenizerFast"
        tcfg.pop("backend", None)
        tcfg.pop("is_local", None)
        changed = True
    extra = tcfg.get("extra_special_tokens")
    if isinstance(extra, list):
        tcfg["extra_special_tokens"] = {"extra_%d" % i: t for i, t in enumerate(extra)}
        changed = True
    return json.dumps(tcfg, indent=2).encode("utf-8") if changed else src_bytes


def collate(items, pad):
    """spike-024 ft_laya.py collate."""
    n, L, km = len(items), max(len(i["ids"]) for i in items), max(len(i["markers"]) for i in items)
    ids = torch.full((n, L), pad, dtype=torch.long)
    att = torch.zeros((n, L), dtype=torch.long)
    mpos = torch.zeros((n, km), dtype=torch.long)
    mm = torch.zeros((n, km), dtype=torch.bool)
    for j, it in enumerate(items):
        ids[j, :len(it["ids"])] = torch.tensor(it["ids"])
        att[j, :len(it["ids"])] = 1
        mpos[j, :len(it["markers"])] = torch.tensor(it["markers"])
        mm[j, :len(it["markers"])] = True
    return ids, att, mpos, mm, torch.tensor([it["qtype"] for it in items])


# ------------------------------------------------------------------------------------------ stages

class Base:
    """Where the base checkpoint comes from and how it is pinned."""

    def __init__(self, args):
        if args.variant == "production":
            if args.base or args.base_sha256:
                refuse("REFUSED base: --base is accepted only with --variant synthetic-fixture; production "
                       "always uses the contract base")
            b = contract.base()
            self.src, self.revision, self.digest = b["repo"], b["revision"], b["model_safetensors_sha256"]
            self.block = contract.production_base_block()
            self._dir = None
        else:
            if not args.base or not args.base_sha256:
                refuse("REFUSED base: --variant synthetic-fixture needs --base CKPT_DIR and --base-sha256 HEX")
            self.src, self.revision, self.digest = str(Path(args.base).resolve()), None, args.base_sha256
            self.block = {"family": "laya", "repo": "synthetic", "revision": "local", "checkpoint": "tiny-synthetic",
                          "sha256": self.digest}
            self._dir = Path(self.src)

    def load(self, device):
        return load_agent(self.src, device, self.digest, self.revision)

    def directory(self):
        """The on-disk base snapshot (resolved after Agent has fetched and verified it)."""
        if self._dir is None:
            from huggingface_hub import snapshot_download
            self._dir = Path(snapshot_download(self.src, revision=self.revision, allow_patterns=list(BASE_FILES),
                                               local_files_only=True))
            got = data.sha256_file(self._dir / "model.safetensors")
            if got != self.digest:
                raise RuntimeError("base snapshot model.safetensors sha256 %s != contract %s" % (got, self.digest))
        return self._dir


def train_seed(seed, base, requested, question, fit_rows, k, epochs):
    """The spike-024 ft_laya.py loop on Laya's own rows; returns (model, info)."""
    rc = contract.recipe()
    torch.manual_seed(seed)
    random.seed(seed)
    np.random.seed(seed)
    agent, device_used = base.load(requested)
    log("DEVICE seed=%d requested=%s used=%s torch=%s" % (seed, requested, device_used, torch.__version__))
    if device_used == "cpu":
        log("WARNING: training on CPU (device_is_cpu true)%s"
            % ("" if requested == "cpu" else " -- %s was requested and Laya fell back" % requested))
    Agent._check_question("q", question)
    internal = {"q": Agent._to_internal(question)}
    items = [agent._encode_state(t, ["q"], internal)[0] for t, _ in fit_rows]
    if any(len(it["markers"]) != k for it in items):
        raise RuntimeError("a training row does not carry one marker per criterion")
    ys = torch.tensor([y for _, y in fit_rows])
    bucket = temp_bucket(QTYPES["choice"], k)
    t_loss = agent.temperature_by_options.get(bucket, agent.temperature[QTYPES["choice"]])
    model = agent.model
    model.train()
    enc = [p for n, p in model.named_parameters() if n.startswith("encoder.")]
    rest = [p for n, p in model.named_parameters() if not n.startswith("encoder.")]
    opt = torch.optim.AdamW([{"params": enc, "lr": float(rc["encoder_lr"])},
                             {"params": rest, "lr": float(rc["head_lr"])}], weight_decay=float(rc["weight_decay"]))
    bs = int(rc["batch_size"])
    steps = epochs * math.ceil(len(items) / bs)
    sched = torch.optim.lr_scheduler.CosineAnnealingLR(opt, T_max=max(1, steps), eta_min=float(rc["eta_min"]))
    pad, dev = agent.tok.pad_token_id, agent.device
    losses = []
    t0 = time.time()
    for _ in range(epochs):
        order = list(range(len(items)))
        random.shuffle(order)
        for b in range(0, len(order), bs):
            sel = order[b:b + bs]
            ids_t, att, mpos, mm, qt = (x.to(dev) for x in collate([items[i] for i in sel], pad))
            z, act = model(ids_t, att, mpos, mm, qt)
            z = z[:, :k]
            yb = ys[sel].to(dev)
            ce = F.cross_entropy(z, yb)
            q = torch.softmax(z / t_loss, -1)
            onehot = F.one_hot(yb, k).float()
            rl = -proper_reward(q, onehot, qt, mm[:, :k].float(),
                                w_sph=float(rc["proper_reward_w_sph"]), w_rps=float(rc["proper_reward_w_rps"])).mean()
            loss = ce + rl + 0.0 * act.sum()
            opt.zero_grad()
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), float(rc["grad_clip"]))
            opt.step()
            sched.step()
            losses.append(float(ce.item()))
    if dev.type == "mps":
        torch.mps.synchronize()
    train_s = time.time() - t0
    model.eval()
    info = {"seed": seed, "device_requested": requested, "device_used": device_used, "epochs": epochs,
            "steps": steps, "fit_rows": len(items), "train_seconds": round(train_s, 1), "loss_temperature": t_loss,
            "ce_first": round(losses[0], 4) if losses else None, "ce_last": round(losses[-1], 4) if losses else None}
    log("TRAIN seed=%d fit_rows=%d epochs=%d steps=%d seconds=%.1f ce_first=%s ce_last=%s"
        % (seed, len(items), epochs, steps, train_s, info["ce_first"], info["ce_last"]))
    return agent, info


def write_checkpoint(agent, base_dir, ck, provenance):
    """The COMPLETE checkpoint dir, before anything reloads it; returns {relpath: sha256}."""
    ck.mkdir(parents=True)
    sd = agent.model.state_dict()
    out = {name: t.detach().to(torch.float32 if name == "temperature" else torch.float16).contiguous().cpu()
           for name, t in sd.items()}
    save_file(out, str(ck / "model.safetensors"), metadata={"format": "pt"})
    cfg = json.loads((base_dir / "rl_agent_config.json").read_bytes())
    cfg["training"] = provenance            # Laya reads head/len/temperature keys; this is provenance only
    write_json(ck / "rl_agent_config.json", cfg)
    for sub in ("encoder", "tokenizer"):
        for src in sorted((base_dir / sub).rglob("*")):
            if src.is_file():
                dst = ck / sub / src.relative_to(base_dir / sub)
                dst.parent.mkdir(parents=True, exist_ok=True)
                raw = src.read_bytes()
                if dst.name == "tokenizer_config.json":
                    raw = fixed_tokenizer_config_bytes(raw)
                dst.write_bytes(raw)
    for need in ("encoder/config.json", "tokenizer/tokenizer.json", "tokenizer/tokenizer_config.json"):
        if not (ck / need).is_file():
            raise RuntimeError("base snapshot has no %s" % need)
    return tree_sha256(ck)


def reload_checked(ck, before):
    """Agent(<ckpt>, device="cpu") in fp32; the reload must not change a single checkpoint file."""
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", RuntimeWarning)       # the base ships choice:11+ = 0.10 (clamped)
        agent = load_for_scoring(ck, before["model.safetensors"])
    after = tree_sha256(ck)
    if after != before:
        raise RuntimeError("Laya's loader changed checkpoint file(s): %s"
                           % sorted(k for k in set(before) | set(after) if before.get(k) != after.get(k)))
    return agent


def score_rows(agent, rows, question):
    sc = Scorer(agent)
    P, Z = [], []
    for t, _ in rows:
        p, z, _ = sc.score(t, question)
        P.append(p)
        Z.append(z)
    return np.array(P, dtype=np.float32), np.array(Z, dtype=np.float32)


def calibrate_and_score(ck, sha_before, calib_rows, eval_rows, question, k):
    """F16 reload -> calibration fit -> T written -> reload again -> eval probabilities."""
    c = contract.constants()
    agent = reload_checked(ck, sha_before)
    bucket = temp_bucket(QTYPES["choice"], k)
    t_pre = agent.temperature_by_options.get(bucket, agent.temperature[QTYPES["choice"]])
    _, zc = score_rows(agent, calib_rows, question)
    yc = np.array([y for _, y in calib_rows])
    t_fit, t_applied, clamp_hit = gate.fit_temperature(zc, yc, c["calibration_temp_min"], c["calibration_temp_max"])
    del agent
    cfg_path = ck / "rl_agent_config.json"
    cfg = json.loads(cfg_path.read_bytes())
    cfg.setdefault("temperature_by_options", {})[bucket] = t_applied
    write_json(cfg_path, cfg)
    sha_cal = tree_sha256(ck)
    agent = reload_checked(ck, sha_cal)
    if agent.temperature_by_options.get(bucket) != t_applied:
        raise RuntimeError("the reloaded checkpoint does not apply T %r to %s" % (t_applied, bucket))
    P, Z = score_rows(agent, eval_rows, question)
    pre = gate.softmax(Z, t_pre)
    calib = {"bucket": bucket, "t_pre": t_pre, "t_fitted": t_fit, "t_applied": t_applied, "clamp_hit": clamp_hit}
    return agent, P, pre, calib, sha_cal


def eval_probs_obj(labels, eval_rows, P):
    return {"labels": list(labels), "rows": [
        {"row": i, "text_sha256": data.exact_sha256(t), "probabilities": f32_list(P[i])}
        for i, (t, _) in enumerate(eval_rows)]}


def probes_obj(agent):
    task, inputs, max_tokens = contract.probe_policy()
    labels = list(task["criteria"])
    sc = Scorer(agent)
    probes = []
    for i, text in enumerate(inputs):
        p, _, n = sc.score(text, task)
        if n > max_tokens:
            raise RuntimeError("probe %d built a %d-token row, over decide-apr-v1 probe_max_row_tokens %d"
                               % (i, n, max_tokens))
        probes.append({"input_index": i, "tokens": n, "label": labels[int(np.argmax(p))],
                       "probabilities_f32_hex": f32_hex_list(p)})
    return {"probes": probes}


# ------------------------------------------------------------------------------------------ main

def parse_args(argv):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--data", required=True, help="data dir: task.json, train.jsonl, eval.jsonl")
    ap.add_argument("--out", required=True, help="run dir to create (must not exist or be empty)")
    ap.add_argument("--epochs", type=int, default=None)
    ap.add_argument("--device", choices=("mps", "cuda", "cpu"), default=None,
                    help="force a device (default: the contract's device_order, first available)")
    ap.add_argument("--variant", choices=("production", "synthetic-fixture"), default="production")
    ap.add_argument("--base", default=None, help="synthetic-fixture only: a local Laya checkpoint dir")
    ap.add_argument("--base-sha256", default=None, help="synthetic-fixture only: its model.safetensors sha256")
    return ap.parse_args(argv)


def main(argv=None):
    args = parse_args(argv)
    t_start = time.time()
    c = contract.constants()
    sp = contract.seed_policy()
    seed = int(sp["declared_seed"])
    data_dir, out = Path(args.data), Path(args.out)

    # 1. data
    try:
        task = data.load_task(data_dir / "task.json")
        train_rows = data.load_rows(data_dir / "train.jsonl", task, "train")
        eval_rows = data.load_rows(data_dir / "eval.jsonl", task, "eval")
        data.refuse_overlap(train_rows, eval_rows)
        fit_ids, calib_ids, slice_ids, slice_sha = data.calibration_split(
            train_rows, c["calibration_slice_fraction"], c["calibration_slice_min_per_class"], seed,
            len(task["labels"]))
        epochs = None
        shots_per_class = max(data.class_counts(train_rows, len(task["labels"])))
        epochs = contract.resolve_epochs(args.variant, shots_per_class, args.epochs)
    except (data.DataError, contract.RecipeError) as e:
        refuse(str(e))
    base = Base(args)
    if out.exists() and any(out.iterdir()):
        refuse("REFUSED out-dir: %s exists and is not empty; a run dir is written once" % out)
    labels, k = task["labels"], len(task["labels"])
    question = data.laya_question(task)
    log("DATA task=%s K=%d train=%d eval=%d shots_per_class=%d fit=%d calibration=%d"
        % (data_dir / "task.json", k, len(train_rows), len(eval_rows), shots_per_class, len(fit_ids), len(calib_ids)))

    # 2. recipe first
    out.mkdir(parents=True, exist_ok=True)
    recipe = contract.recipe_json(args.variant, shots_per_class, epochs, seed, base.block)
    recipe_bytes = json.dumps(recipe, sort_keys=True, separators=(",", ":")).encode("utf-8")
    (out / "recipe.json").write_bytes(recipe_bytes)
    recipe_id = hashlib.sha256(recipe_bytes).hexdigest()
    log("RECIPE WRITTEN %s" % recipe_id)

    # 3-4. device, train
    requested = request_device(args.device)
    fit_rows = [train_rows[i] for i in fit_ids]
    calib_rows = [train_rows[i] for i in calib_ids]
    agent, info = train_seed(seed, base, requested, question, fit_rows, k, epochs)

    # 5. complete checkpoint before any reload
    ck = out / "checkpoint"
    provenance = {"fine_tuned_from": base.block, "recipe_id": recipe_id, "seed": seed, "epochs": epochs,
                  "steps": info["steps"], "fit_rows": info["fit_rows"], "device_used": info["device_used"]}
    sha_before = write_checkpoint(agent, base.directory(), ck, provenance)
    del agent
    if torch.backends.mps.is_available():
        torch.mps.empty_cache()
    log("CHECKPOINT COMPLETE %d files (model.safetensors %s)" % (len(sha_before), sha_before["model.safetensors"]))

    # 6. reload, calibrate, reload, score
    log("SCORING START")
    agent, P_ft, P_pre, calib, _ = calibrate_and_score(ck, sha_before, calib_rows, eval_rows, question, k)
    log("CALIBRATION bucket=%s t_pre=%.6f t_fitted=%.6f t_applied=%.6f clamp_hit=%s slice=%d"
        % (calib["bucket"], calib["t_pre"], calib["t_fitted"], calib["t_applied"], calib["clamp_hit"], len(slice_ids)))
    write_json(out / "eval-probs.json", eval_probs_obj(labels, eval_rows, P_ft))
    write_json(out / "probes.json", probes_obj(agent))
    del agent

    # 7. zero-shot: the declared base, loaded the same way
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", RuntimeWarning)
        zs_agent = load_for_scoring(base.src, base.digest, base.revision)
    P_zs, _ = score_rows(zs_agent, eval_rows, question)
    del zs_agent
    write_json(out / "zero-shot-probs.json", eval_probs_obj(labels, eval_rows, P_zs))
    (out / "task.json").write_bytes((data_dir / "task.json").read_bytes())

    # 8. gate on exactly the probabilities written
    y = np.array([lab for _, lab in eval_rows])
    as64 = lambda P: np.array([f32_list(r) for r in P], dtype=np.float64)  # noqa: E731
    demo = contract.demo()
    f_avg_labels = None
    if labels == list(demo["criteria_order"]):
        f_avg_labels = [labels.index("against"), labels.index("favor")]
    g = gate.evaluate_gate(as64(P_zs), as64(P_ft), P_pre, y, f_avg_labels)
    report = {
        "schema": "laya-gate-report-v1",
        "pass": g["pass"],
        "thresholds": g["thresholds"],
        "zero_shot": g["zero_shot"],
        "fine_tuned": g["fine_tuned"],
        "margin": g["margin"],
        "calibration": {"bucket": calib["bucket"], "t_fitted": calib["t_fitted"], "t_applied": calib["t_applied"],
                        "clamp_hit": calib["clamp_hit"], "slice_size": len(slice_ids), "slice_ids": slice_ids,
                        "slice_ids_sha256": slice_sha},
        "seeds": {"declared": seed, "n": 1, "label": contract.seeds_label(1)},
        "device_used": info["device_used"],
        "device_is_cpu": info["device_used"] == "cpu",
        "torch_version": torch.__version__,
        "recipe_id": recipe_id,
        "inputs_sha256": {"task_json": data.sha256_file(data_dir / "task.json"),
                          "train_jsonl": data.sha256_file(data_dir / "train.jsonl"),
                          "eval_jsonl": data.sha256_file(data_dir / "eval.jsonl"),
                          "base_model": base.digest,
                          "tokenizer_json": data.sha256_file(ck / "tokenizer" / "tokenizer.json")},
        "eval_probs_sha256": data.sha256_file(out / "eval-probs.json"),
        "zero_shot_probs_sha256": data.sha256_file(out / "zero-shot-probs.json"),
        "probes_sha256": data.sha256_file(out / "probes.json"),
    }
    write_json(out / "gate-report.json", report)

    zs, ft = g["zero_shot"], g["fine_tuned"]
    fmt = lambda v: "null" if v is None else "%.4f" % v  # noqa: E731
    log("RESULT zero_shot macro_f1=%s f_avg=%s ece=%s | fine_tuned macro_f1=%s f_avg=%s ece_pre=%s ece_post=%s "
        "nll=%s | margin=%s (need >= %s) ece_post (need <= %s)"
        % (fmt(zs["macro_f1"]), fmt(zs["f_avg"]), fmt(zs["ece"]), fmt(ft["macro_f1"]), fmt(ft["f_avg"]),
           fmt(ft["ece_pre"]), fmt(ft["ece_post"]), fmt(ft["nll"]), fmt(g["margin"]),
           g["thresholds"]["min_macro_f1_margin"], g["thresholds"]["max_ece"]))
    if f_avg_labels is not None and args.variant == "production":
        log("INFO spike 024 baseline F_avg @%d: %s" % (demo["shots_per_class"], demo["spike_baseline_f_avg"]))
    log("SEEDS %s | device_used=%s torch=%s train_seconds=%s total_seconds=%.1f"
        % (report["seeds"]["label"], info["device_used"], torch.__version__, info["train_seconds"],
           time.time() - t_start))
    if report["pass"]:
        log("GATE PASS")
        return EXIT_PASS
    log("GATE FAIL")
    return EXIT_GATE_FAIL


if __name__ == "__main__":
    sys.exit(main())
