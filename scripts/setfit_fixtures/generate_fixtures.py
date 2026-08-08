#!/usr/bin/env python3
"""Freeze the entire ENC-01..06 fixture corpus in ONE deterministic pass (D-15).

Run:  uv run python slice_model.py && uv run python generate_fixtures.py
Developer workflow only -- never wired into CI (D-12).

WHY ONE PASS
------------
Every slice-driven fixture must come from the SAME sliced torch model that produced the
committed slice_model.apr, otherwise "Rust disagrees with Python" becomes ambiguous
between a real defect and two different reference models. So this script loads
build/slice_model.safetensors -- the exact bytes slice_model.py converted to APR -- and
derives every slice fixture from that one object.

DROPOUT IS INERT (D-16)
-----------------------
Both dropout probabilities are set to 0.0 AND the model is put in eval() mode. Python and
Rust RNG streams are never compared bit-for-bit; Rust-side dropout placement,
determinism and statistics are tested separately.

ASSUMPTION A1, RESOLVED FROM THE LOCKED SOURCE (not from memory)
-----------------------------------------------------------------
Read out of sentence_transformers 5.7.0 in this very environment:

  Pooling._forward_padded (sentence_transformer/modules/pooling.py):
      mean_sum  = (token_embeddings * mask).sum(dim=1)
      mean_mask = torch.clamp(mask.sum(dim=1), min=1e-9)      <-- clamp constant 1e-9
      mean      = mean_sum / mean_mask

  Normalize.forward (sentence_transformer/modules/normalize.py):
      F.normalize(sentence_embedding, p=2, dim=1)             <-- torch default eps 1e-12

So the clamp constant is 1e-9 and the normalize eps is 1e-12, both CONFIRMED rather than
assumed. ``full_model_reference`` additionally cross-checks this pipeline against a real
SentenceTransformer forward, so a wrong constant would fail here rather than in wave 6.
"""

from __future__ import annotations

import hashlib
import json
import math
import subprocess
import sys
from pathlib import Path

import torch
import torch.nn.functional as F
from safetensors.torch import load_file
from tokenizers import Tokenizer
from transformers import BertConfig, BertModel

import corpus
import jsonfmt
from slice_model import (
    BUILD_DIR,
    FIXTURE_DIR,
    REPO_ID,
    REVISION,
    fetch_pinned,
    sha256_file,
)

SEED = 0

# ST pooling/normalize constants, verified from the locked 5.7.0 source (see module docstring).
ST_POOLING_CLAMP_MIN = 1e-9
ST_NORMALIZE_EPS = 1e-12

# AdamW hyperparameters for the single controlled step (ENC-04).
ADAMW = {"lr": 2e-5, "betas": [0.9, 0.999], "eps": 1e-8, "weight_decay": 0.01}

# D55 — steps in the multi-step trajectory obligation. At step 1 bias correction makes
# the update beta-INDEPENDENT (m_hat = g, v_hat = g^2 for every beta1/beta2), so no
# single-step fixture at any tolerance can constrain the betas. The moments only start
# to carry history from step 2 onward. 20 is where the measured beta separation is
# ~5 orders of magnitude above f32 noise while the trajectory is still cheap to replay
# in Rust; `assert_separation` re-measures it on every regeneration rather than trusting
# this comment.
MULTISTEP_N = 20

# --- tolerance floors ---------------------------------------------------------------
# WHY A FLOOR IS MANDATORY: `10 x observed f32/f64 delta` alone can produce ZERO (the two
# paths coincide on a small case) or a value far tighter than legitimate Rust/PyTorch
# REDUCTION-ORDER differences -- which an f32/f64 round trip does not measure at all,
# because both paths sum in the same order. A zero tolerance silently disables the
# comparison it is supposed to gate (T-1-07).
#
# DERIVATION: differing summation orders over W terms behave like a random walk, so the
# error grows as sqrt(W) * eps_f32 * |x|. With K = 8 as a safety factor on |x| and on the
# walk constant:
#
#     floor(W) = K * sqrt(W) * EPS_F32
#
# W is the characteristic reduction width of the family: 1 for a pointwise activation,
# the sequence length (<=64) for a pooled mean, the projection width for a forward pass
# (256 = the slice FFN intermediate), and a whole-batch backward accumulation for
# gradients / optimizer state. full_model_reference uses the FULL model's 1536-wide FFN.
#
# D55 — `optimizer_step` is W = 1, NOT 1024, and that is not a typo. A post-step
# PARAMETER is not a 1024-wide reduction of anything. At step 1 bias correction gives
# m_hat = g and v_hat = g^2, so the update is lr*g/(|g|+eps) -- it SATURATES to
# lr*sign(g), and gradient reduction-order noise therefore does not propagate into it at
# all. Inheriting the gradient family's W made the floor 3.05e-05 while the entire
# displacement being gated is ~lr = 2.01e-05: a floor 1.5x the signal, i.e. a gate that
# cannot fail. The residual really is pointwise f32 representation noise at the
# parameter magnitude, which is why this family also passes an explicit `scale`.
EPS_F32 = 1.1920928955078125e-07
FLOOR_K = 8.0
FAMILY_REDUCTION_WIDTH = {
    "activation": 1,
    "forward_per_layer": 256,
    "pooling_normalize": 64,
    "loss_pair": 64,
    "gradients": 1024,
    "optimizer_step": 1,
    # A trajectory of pair losses: each entry is one loss_pair reduction.
    "optimizer_multistep": 64,
    "batch_invariance": 64,
    "full_model_reference": 1536,
}


def family_floor(family: str, scale: float = 1.0) -> float:
    return FLOOR_K * math.sqrt(FAMILY_REDUCTION_WIDTH[family]) * EPS_F32 * scale


def record_tolerance(
    tolerances: dict, family: str, delta: float, scale: float = 1.0
) -> None:
    """Record one family's measured delta, floor and recommended tolerance.

    The family name is spelled ONCE per call site. The previous form repeated it
    three times per block (dict key plus two `family_floor` arguments) across eight
    near-identical blocks, so a mismatch between the key and the floor lookup would
    silently record the wrong reduction width -- and `recommended_tolerance` is
    exactly what the Rust conformance gates load.

    `scale` is the MAGNITUDE the family's comparison actually lives at. It is 1.0 for
    every family that compares normalized or O(1) quantities, and is passed explicitly
    by the optimizer families, which compare raw parameter values. Leaving it implicit
    is what produced the vacuous D55 tolerance.
    """
    floor = family_floor(family, scale)
    tolerances[family] = {
        "max_abs_f32_f64_delta": delta,
        "floor": floor,
        "recommended_tolerance": max(10 * delta, floor),
    }


def assert_separation(tolerances: dict, family: str, signal: float, what: str) -> None:
    """Fail generation unless `family`'s tolerance sits 10x BELOW the signal it gates.

    A tolerance at or above the effect it is supposed to resolve is not a loose gate,
    it is an absent one -- the D55 defect in one line. The `activation` family has had
    this guard since 01-04; every family whose tolerance must SEPARATE two behaviours
    (rather than merely absorb round-off) needs it, and the optimizer families are
    exactly that case.
    """
    tol = tolerances[family]["recommended_tolerance"]
    if tol * 10 > signal:
        sys.exit(
            f"FATAL: {family} tolerance {tol:.6e} does not sit 10x below {what} "
            f"({signal:.6e}); margin is {signal / tol:.2f}x. A gate at this tolerance "
            "cannot distinguish the behaviour it claims to gate."
        )
    print(f"  {family}: tol {tol:.6e} separates {what} ({signal:.6e}) by {signal / tol:.1f}x")


def flat(t: torch.Tensor) -> list[float]:
    """Row-major flattened f32 values."""
    return [float(v) for v in t.detach().to(torch.float32).reshape(-1).tolist()]


def max_abs_delta(a: torch.Tensor, b: torch.Tensor) -> float:
    return float((a.detach().to(torch.float64) - b.detach().to(torch.float64)).abs().max())


# ------------------------------------------------------------------ tokenizer ------
def load_tokenizer() -> Tokenizer:
    return Tokenizer.from_file(str(FIXTURE_DIR / "tokenizer.json"))


def encode_case(tok: Tokenizer, texts: list[str], max_length: int) -> dict:
    """Tokenize a batch, padding to LONGEST-IN-BATCH (never padding='max_length').

    max_length is a TRUNCATION bound only. 01-05 Task 2 asserts exact integer equality
    against these arrays with no tolerance, so a max_length-padded corpus would fail at
    wave 3 and force a wave-2 fixture + manifest + tolerance regeneration.
    """
    tok.no_truncation()
    tok.no_padding()
    raw = tok.encode_batch(list(texts))
    original_counts = [len(e.ids) for e in raw]

    tok.enable_truncation(max_length=max_length)
    tok.enable_padding(pad_id=0, pad_token="[PAD]", pad_type_id=0)  # length=None => longest
    enc = tok.encode_batch(list(texts))
    tok.no_truncation()
    tok.no_padding()

    return {
        "texts": list(texts),
        "max_length": max_length,
        "input_ids": [list(e.ids) for e in enc],
        "token_type_ids": [list(e.type_ids) for e in enc],
        "attention_mask": [list(e.attention_mask) for e in enc],
        "truncated": [c > max_length for c in original_counts],
        "original_token_counts": original_counts,
    }


# ------------------------------------------------------------------ slice model ----
def slice_config() -> dict:
    return json.loads((FIXTURE_DIR / "slice_config.json").read_text())


def vocab_remap() -> dict[str, int]:
    return json.loads((FIXTURE_DIR / "vocab_remap.json").read_text())["orig_to_slice"]


def build_slice_model(dtype: torch.dtype = torch.float32) -> BertModel:
    """Fresh slice model from the EXACT bytes slice_model.py converted to APR."""
    cfg_json = slice_config()
    cfg = BertConfig(
        vocab_size=cfg_json["vocab"],
        hidden_size=cfg_json["hidden"],
        num_hidden_layers=cfg_json["num_layers"],
        num_attention_heads=cfg_json["heads"],
        intermediate_size=cfg_json["intermediate"],
        max_position_embeddings=cfg_json["positions"],
        type_vocab_size=cfg_json["type_vocab_size"],
        layer_norm_eps=cfg_json["layer_norm_eps"],
        hidden_act=cfg_json["hidden_act"],
        pad_token_id=cfg_json["pad_token_id"],
        position_embedding_type="absolute",
        # D-16: dropout inert. eval() below would suffice; 0.0 removes all doubt.
        hidden_dropout_prob=0.0,
        attention_probs_dropout_prob=0.0,
    )
    torch.manual_seed(SEED)
    model = BertModel(cfg, add_pooling_layer=False)
    sd = load_file(str(BUILD_DIR / "slice_model.safetensors"))
    missing, unexpected = model.load_state_dict(sd, strict=False)
    real_missing = [k for k in missing if not k.endswith("position_ids")]
    if real_missing or unexpected:
        sys.exit(f"FATAL: slice state_dict mismatch\n  missing={real_missing}\n  unexpected={unexpected}")
    model.eval()
    return model.to(dtype)


def remap_ids(ids: list[list[int]], remap: dict[str, int]) -> list[list[int]]:
    out = []
    for row in ids:
        remapped = []
        for i in row:
            key = str(i)
            if key not in remap:
                sys.exit(
                    f"FATAL: canonical id {i} is outside the slice vocabulary closure. "
                    "The corpus changed without re-running slice_model.py."
                )
            remapped.append(remap[key])
        out.append(remapped)
    return out


def run_slice(model: BertModel, case: dict, remap: dict[str, int], dtype: torch.dtype):
    """Forward the slice model on a case; returns (hidden_states tuple, mask tensor)."""
    ids_slice = remap_ids(case["input_ids"], remap)
    input_ids = torch.tensor(ids_slice, dtype=torch.long)
    mask = torch.tensor(case["attention_mask"], dtype=torch.long)
    type_ids = torch.tensor(case["token_type_ids"], dtype=torch.long)
    with torch.no_grad():
        out = model(
            input_ids=input_ids,
            attention_mask=mask,
            token_type_ids=type_ids,
            output_hidden_states=True,
        )
    return out.hidden_states, mask.to(dtype)


def masked_mean(token_emb: torch.Tensor, mask: torch.Tensor) -> torch.Tensor:
    m = mask.unsqueeze(-1).expand(token_emb.size()).to(token_emb.dtype)
    summed = (token_emb * m).sum(dim=1)
    denom = torch.clamp(m.sum(dim=1), min=ST_POOLING_CLAMP_MIN)
    return summed / denom


def l2_normalize(x: torch.Tensor) -> torch.Tensor:
    return F.normalize(x, p=2, dim=1, eps=ST_NORMALIZE_EPS)


def encode_slice(model: BertModel, case: dict, remap: dict[str, int], dtype: torch.dtype):
    """Graph-connected encode used by the loss/gradient fixtures (no no_grad)."""
    ids_slice = remap_ids(case["input_ids"], remap)
    out = model(
        input_ids=torch.tensor(ids_slice, dtype=torch.long),
        attention_mask=torch.tensor(case["attention_mask"], dtype=torch.long),
        token_type_ids=torch.tensor(case["token_type_ids"], dtype=torch.long),
    )
    mask = torch.tensor(case["attention_mask"], dtype=torch.long).to(dtype)
    return l2_normalize(masked_mean(out.last_hidden_state, mask))


def pair_loss(model: BertModel, ca: dict, cb: dict, remap, dtype) -> torch.Tensor:
    za = encode_slice(model, ca, remap, dtype)
    zb = encode_slice(model, cb, remap, dtype)
    cos = F.cosine_similarity(za, zb, dim=1)
    labels = torch.tensor(corpus.LOSS_PAIR_LABELS, dtype=dtype)
    return F.mse_loss(cos, labels)


def adamw_trajectory(steps, ca, cb, remap, dtype, **overrides):
    """Run `steps` AdamW steps from a FRESH slice model on the recorded pair batch.

    Returns `(post_step, losses)` where `losses[i]` is the loss measured BEFORE step i
    and `losses[steps]` is the loss after the last step, so a trajectory of length
    `steps + 1` brackets every update.

    `overrides` replaces individual ADAMW hyperparameters. The generator uses that to
    run the SAME mutations the conformance gate claims to detect (decay deleted, wrong
    betas) and measure how far each one moves the trajectory -- so the separation this
    fixture can prove is measured, never asserted from a comment.
    """
    hp = {**ADAMW, **overrides}
    model = build_slice_model(dtype)
    params = [p for n, p in model.named_parameters() if not n.startswith("pooler.")]
    opt = torch.optim.AdamW(
        params,
        lr=hp["lr"],
        betas=tuple(hp["betas"]),
        eps=hp["eps"],
        weight_decay=hp["weight_decay"],
    )
    losses = []
    for _ in range(steps):
        opt.zero_grad(set_to_none=True)
        loss = pair_loss(model, ca, cb, remap, dtype)
        losses.append(float(loss.detach().to(torch.float64)))
        loss.backward()
        opt.step()
    with torch.no_grad():
        losses.append(float(pair_loss(model, ca, cb, remap, dtype).detach().to(torch.float64)))
    post = {n: p for n, p in model.named_parameters() if not n.startswith("pooler.")}
    return post, losses


# ------------------------------------------------------------------ main -----------
def main() -> None:
    torch.manual_seed(SEED)
    torch.use_deterministic_algorithms(True)

    cfg_json = slice_config()
    if cfg_json["source_revision"] != REVISION:
        sys.exit("FATAL: slice_config.json pins a different revision than slice_model.py")

    verify_upstream_digests()

    tok = load_tokenizer()
    remap = vocab_remap()
    tolerances: dict[str, dict] = {}

    # ---------------------------------------------------------- corpus of record ---
    cases = {cid: encode_case(tok, texts, corpus.MAX_LENGTH) for cid, texts in corpus.CASES.items()}

    tokenizer_cases = {
        "revision": REVISION,
        "tokenizer_sha256": cfg_json["tokenizer_sha256"],
        "cases": [{"id": cid, **cases[cid]} for cid in corpus.CASES],
    }

    # The slice keeps only `positions` position embeddings; anything longer cannot be
    # driven through it. Asserted, not left to the corpus comment.
    for cid in corpus.SLICE_DRIVEN_CASES:
        seq = len(cases[cid]["input_ids"][0])
        if seq > cfg_json["positions"]:
            sys.exit(
                f"FATAL: case '{cid}' is {seq} tokens but the slice has only "
                f"{cfg_json['positions']} positions."
            )
    if not any(any(c["truncated"]) for c in cases.values()):
        sys.exit("FATAL: no case exercises truncation; the >256-token case is missing.")

    def joined(cid: str) -> dict:
        """Fixture-side view of a case: case_id + verbatim texts + canonical ids (B6)."""
        c = cases[cid]
        return {
            "case_id": cid,
            "texts": list(c["texts"]),
            "input_ids_canonical": [list(r) for r in c["input_ids"]],
            "attention_mask": [list(r) for r in c["attention_mask"]],
        }

    # ---------------------------------------------------------- activation --------
    grid = torch.linspace(-6.0, 6.0, 2401, dtype=torch.float32)
    y_exact = F.gelu(grid, approximate="none")
    y_tanh = F.gelu(grid, approximate="tanh")
    tanh_delta = float((y_exact - y_tanh).abs().max())
    y64 = F.gelu(grid.to(torch.float64), approximate="none")
    act_delta = max_abs_delta(y_exact, y64)

    # The gate must SEPARATE the exact erf form from the tanh approximation, so the
    # activation tolerance has to sit far below the gap between them. MEASURED here at
    # ~4.7e-4 near x = -2.7 -- note this is BELOW the 1e-3 the plan predicted, so the
    # measured value is recorded and the assertion is set from it rather than the other
    # way round (CLAUDE.md: never label a run by intent).
    if tanh_delta <= 1e-4:
        sys.exit(f"FATAL: exact-vs-tanh GELU gap {tanh_delta:.3e} is too small to gate on")

    record_tolerance(tolerances, "activation", act_delta)
    if tolerances["activation"]["recommended_tolerance"] >= tanh_delta / 10:
        sys.exit(
            "FATAL: activation tolerance is not far enough below the exact-vs-tanh gap; "
            "a tanh implementation could pass the gate."
        )

    jsonfmt.write(
        FIXTURE_DIR / "activation_reference.json",
        {
            "op": "gelu_exact",
            "note": (
                "torch.nn.functional.gelu(x, approximate='none') -- the EXACT erf form "
                "0.5*x*(1+erf(x/sqrt(2))), matching the pinned config's hidden_act='gelu'. "
                f"MEASURED max|exact - tanh_approx| over this grid is {tanh_delta:.6e} at the "
                "grid point nearest x=-2.699. The tanh approximation is a DIFFERENT function, "
                "not an acceptable implementation: this gap is ~3 orders of magnitude above "
                "f32 round-trip noise, so the gate separates them instead of absorbing the "
                "difference into tolerance."
            ),
            "approximate": "none",
            "tanh_vs_exact_max_delta": tanh_delta,
            "x": flat(grid),
            "y": flat(y_exact),
            "max_abs_f32_f64_delta": act_delta,
        },
    )

    # ---------------------------------------------------------- forward per layer --
    model32 = build_slice_model(torch.float32)
    model64 = build_slice_model(torch.float64)

    fwd_cases, fwd_delta = [], 0.0
    for cid in ("single_short", "mixed_length_pair"):
        c = cases[cid]
        hs32, _ = run_slice(model32, c, remap, torch.float32)
        hs64, _ = run_slice(model64, c, remap, torch.float64)
        fwd_delta = max(fwd_delta, max_abs_delta(hs32[-1], hs64[-1]))
        b, s, h = hs32[-1].shape
        if len(hs32) != cfg_json["num_layers"] + 1:
            sys.exit("FATAL: expected num_layers+1 hidden states")
        fwd_cases.append(
            {
                **joined(cid),
                "input_ids_slice": remap_ids(c["input_ids"], remap),
                "shape": {"batch": b, "seq": s, "hidden": h},
                "embeddings_out": flat(hs32[0]),
                "layer_outputs": [flat(hs32[i + 1]) for i in range(cfg_json["num_layers"])],
                # Duplicates layer_outputs[-1] on purpose: the duplication is what lets a
                # Rust mismatch name embedding-vs-layer-N-vs-final instead of "the encoder".
                "final_tokens": flat(hs32[-1]),
            }
        )
    record_tolerance(tolerances, "forward_per_layer", fwd_delta)
    jsonfmt.write(FIXTURE_DIR / "forward_per_layer.json", {"cases": fwd_cases})

    # ---------------------------------------------------------- pooling/normalize --
    pool_cases, pool_delta = [], 0.0
    for cid in ("mixed_length_pair", "pooling_batch"):
        c = cases[cid]
        hs32, m32 = run_slice(model32, c, remap, torch.float32)
        hs64, m64 = run_slice(model64, c, remap, torch.float64)
        pooled32 = masked_mean(hs32[-1], m32)
        norm32 = l2_normalize(pooled32)
        pooled64 = masked_mean(hs64[-1], m64)
        pool_delta = max(pool_delta, max_abs_delta(norm32, l2_normalize(pooled64)))
        pool_cases.append(
            {
                **joined(cid),
                "shape": {"batch": pooled32.shape[0], "hidden": pooled32.shape[1]},
                "pooled": flat(pooled32),
                "normalized": flat(norm32),
            }
        )
    record_tolerance(tolerances, "pooling_normalize", pool_delta)
    jsonfmt.write(FIXTURE_DIR / "pooling_normalize.json", {"cases": pool_cases})

    # ---------------------------------------------------------- pair loss ---------
    ca, cb = cases["loss_pair_a"], cases["loss_pair_b"]
    za = encode_slice(model32, ca, remap, torch.float32)
    zb = encode_slice(model32, cb, remap, torch.float32)
    cos32 = F.cosine_similarity(za, zb, dim=1)
    labels = torch.tensor(corpus.LOSS_PAIR_LABELS, dtype=torch.float32)
    mse32 = F.mse_loss(cos32, labels)
    with torch.no_grad():
        za64 = encode_slice(model64, ca, remap, torch.float64)
        zb64 = encode_slice(model64, cb, remap, torch.float64)
        cos64 = F.cosine_similarity(za64, zb64, dim=1)
    loss_delta = max(
        max_abs_delta(cos32, cos64),
        max_abs_delta(mse32, F.mse_loss(cos64, labels.to(torch.float64))),
    )
    record_tolerance(tolerances, "loss_pair", loss_delta)
    jsonfmt.write(
        FIXTURE_DIR / "loss_pair.json",
        {
            "pair": {
                "a_case_id": "loss_pair_a",
                "a_texts": list(ca["texts"]),
                "a_ids_canonical": [list(r) for r in ca["input_ids"]],
                "a_ids_slice": remap_ids(ca["input_ids"], remap),
                "a_mask": [list(r) for r in ca["attention_mask"]],
                "b_case_id": "loss_pair_b",
                "b_texts": list(cb["texts"]),
                "b_ids_canonical": [list(r) for r in cb["input_ids"]],
                "b_ids_slice": remap_ids(cb["input_ids"], remap),
                "b_mask": [list(r) for r in cb["attention_mask"]],
                "labels": list(corpus.LOSS_PAIR_LABELS),
            },
            "cosine": flat(cos32),
            "mse": float(mse32.detach()),
        },
    )

    # ---------------------------------------------------------- gradients ---------
    gmodel = build_slice_model(torch.float32)
    gmodel.zero_grad(set_to_none=True)
    loss = pair_loss(gmodel, ca, cb, remap, torch.float32)
    loss.backward()

    named = [(n, p) for n, p in gmodel.named_parameters() if not n.startswith("pooler.")]
    parameter_order = [n for n, _ in named]
    grads = {}
    max_abs = {}
    for n, p in named:
        if p.grad is None:
            sys.exit(f"FATAL: parameter {n} received no gradient at all")
        g = p.grad.detach()
        if not torch.isfinite(g).all():
            sys.exit(f"FATAL: non-finite gradient on {n}")
        grads[n] = {"shape": list(p.shape), "grad": flat(g)}
        max_abs[n] = float(g.abs().max())

    # zero_grad_floor derivation: the reference gradients separate into a cluster that is
    # numerically zero (|g| ~ 1e-10 and below, i.e. analytically zero perturbed by f32
    # rounding) and a cluster of genuinely non-zero components many orders of magnitude
    # larger. The floor is placed one order of magnitude BELOW the smallest genuinely
    # non-zero max|grad|, so it cannot swallow a real gradient, and the split is read off
    # the observed distribution rather than assumed.
    ordered = sorted(max_abs.values())
    gap_idx = max(
        range(1, len(ordered)),
        key=lambda i: (math.log10(ordered[i] + 1e-300) - math.log10(ordered[i - 1] + 1e-300)),
    )
    smallest_nonzero = ordered[gap_idx]
    zero_grad_floor = smallest_nonzero / 10.0

    analytically_zero = []
    for n in parameter_order:
        if max_abs[n] <= zero_grad_floor:
            analytically_zero.append(
                {
                    "name": n,
                    "max_abs_grad": max_abs[n],
                    "justification": (
                        "the key bias adds the same constant to every key, so for a fixed "
                        "query q_i the term q_i . b_k is identical across all keys j; softmax "
                        "is invariant under a constant shift of all logits in a row, therefore "
                        "dL/db_k = 0 in exact arithmetic"
                        if n.endswith("attention.self.key.bias")
                        else "measured analytically-zero gradient under this loss and batch"
                    ),
                }
            )
    if not analytically_zero:
        sys.exit("FATAL: analytically_zero is empty; the ENC-04 exemption gate would be vacuous")
    if not any(e["name"].endswith("attention.self.key.bias") for e in analytically_zero):
        sys.exit(
            "FATAL: no attention.self.key.bias in analytically_zero. Either the slice or the "
            "loss is degenerate, or the reference gradient is wrong. Do NOT proceed by "
            "deleting the expectation."
        )

    g64 = build_slice_model(torch.float64)
    g64.zero_grad(set_to_none=True)
    pair_loss(g64, ca, cb, remap, torch.float64).backward()
    grad_delta = 0.0
    for (n, p), (_, p64) in zip(named, [(n, p) for n, p in g64.named_parameters() if not n.startswith("pooler.")]):
        grad_delta = max(grad_delta, max_abs_delta(p.grad, p64.grad))
    record_tolerance(tolerances, "gradients", grad_delta)

    source_block = {"fixture": "loss_pair.json", "a_case_id": "loss_pair_a", "b_case_id": "loss_pair_b"}
    jsonfmt.write(
        FIXTURE_DIR / "gradients.json",
        {
            "source": source_block,
            "note": (
                "Gradients of the cosine-similarity/MSE pair loss w.r.t. every slice "
                "parameter, pooler.* excluded. `source` names the exact batch these were "
                "recorded on so the Rust gate rebuilds the SAME batch by tokenizing those "
                "texts. `parameter_order` is torch's named_parameters() iteration order -- "
                "compare against IT, not against JSON object key order, which carries no "
                "ordering guarantee."
            ),
            "parameter_order": parameter_order,
            "zero_grad_floor": zero_grad_floor,
            "analytically_zero": analytically_zero,
            "grads": grads,
        },
    )

    # ---------------------------------------------------------- optimizer step ----
    omodel = build_slice_model(torch.float32)
    oparams = [p for n, p in omodel.named_parameters() if not n.startswith("pooler.")]
    before_step = {
        n: p.detach().clone()
        for n, p in omodel.named_parameters()
        if not n.startswith("pooler.")
    }
    opt = torch.optim.AdamW(
        oparams,
        lr=ADAMW["lr"],
        betas=tuple(ADAMW["betas"]),
        eps=ADAMW["eps"],
        weight_decay=ADAMW["weight_decay"],
    )
    opt.zero_grad(set_to_none=True)
    loss_before = pair_loss(omodel, ca, cb, remap, torch.float32)
    loss_before.backward()
    opt.step()
    with torch.no_grad():
        loss_after = pair_loss(omodel, ca, cb, remap, torch.float32)

    post_step = {
        n: flat(p) for n, p in omodel.named_parameters() if not n.startswith("pooler.")
    }

    # D55 (CLOSED) — the optimizer family measures its OWN f32/f64 delta.
    # It previously reused `grad_delta`, so no f64 optimizer step was ever run and the
    # recorded tolerance (3.052e-05) exceeded the entire step-1 displacement (~lr =
    # 2.01e-05) it was supposed to resolve. The `scale` is the parameter magnitude the
    # comparison actually lives at; see the FAMILY_REDUCTION_WIDTH note on why W = 1.
    post64, _ = adamw_trajectory(1, ca, cb, remap, torch.float64)
    step_delta = 0.0
    for n, p in omodel.named_parameters():
        if n.startswith("pooler."):
            continue
        step_delta = max(step_delta, max_abs_delta(p, post64[n]))
    max_abs_param = max(
        float(p.detach().abs().max())
        for n, p in omodel.named_parameters()
        if not n.startswith("pooler.")
    )
    record_tolerance(tolerances, "optimizer_step", step_delta, scale=max_abs_param)

    # The displacement this gate must resolve. Measured against the pre-step snapshot,
    # not predicted from lr: a step that silently did nothing would otherwise be gated
    # by a tolerance derived from the step it failed to take.
    max_displacement = max(
        float((p.detach() - before_step[n]).abs().max())
        for n, p in omodel.named_parameters()
        if not n.startswith("pooler.")
    )
    assert_separation(
        tolerances, "optimizer_step", max_displacement, "the step-1 displacement"
    )
    jsonfmt.write(
        FIXTURE_DIR / "optimizer_step.json",
        {
            "source": source_block,
            "note": (
                "ALL parameters are trainable in this fixture -- no frozen groups, matching "
                "SetFit's full-body fine-tuning default (D-20). It therefore CANNOT validate "
                "a Rust model configured with frozen groups; 01-08 uses a second, clean model "
                "for the frozen-byte-identity proof. loss_before/loss_after let the "
                "loss-decrease assertion compare against a recorded reference instead of an "
                "unpinned expectation."
            ),
            "adamw": ADAMW,
            "all_trainable": True,
            "loss_before": float(loss_before.detach()),
            "loss_after": float(loss_after.detach()),
            "post_step": post_step,
        },
    )

    # ------------------------------------------------------ optimizer multi-step --
    # D55 — the obligation that makes the BETAS falsifiable.
    #
    # Why a loss TRAJECTORY and not a second post-step parameter dump: the discriminating
    # power of a max-abs parameter comparison is set by its noisiest single element, and
    # both mutations below stay inside that noise at every step count measured (the
    # weight-decay term peaks at 2.6x the f32/f64 delta at N = 50). The loss contracts
    # every parameter into one number and the trajectory accumulates the divergence
    # coherently, which buys ~5 orders of magnitude on the betas -- and it costs 21
    # floats instead of 1.6 MB.
    _, traj32 = adamw_trajectory(MULTISTEP_N, ca, cb, remap, torch.float32)
    _, traj64 = adamw_trajectory(MULTISTEP_N, ca, cb, remap, torch.float64)
    traj_delta = max(abs(a - b) for a, b in zip(traj32, traj64))
    record_tolerance(tolerances, "optimizer_multistep", traj_delta)

    # Run the mutations THIS obligation claims to detect and measure the separation.
    # `assert_separation` then fails generation if the recorded tolerance could not
    # actually tell them apart -- the check that D55 was missing.
    _, traj_betas = adamw_trajectory(
        MULTISTEP_N, ca, cb, remap, torch.float32, betas=[0.5, 0.5]
    )
    betas_signal = max(abs(a - b) for a, b in zip(traj32, traj_betas))
    assert_separation(
        tolerances, "optimizer_multistep", betas_signal, "a betas (0.5, 0.5) trajectory"
    )

    # The decay control is measured and RECORDED but deliberately not asserted here.
    # At this lr/weight_decay the decay term is ~3 f32 ulp of the parameters it acts on,
    # so no tolerance over this fixture can separate it. Deleting decoupled decay is
    # caught instead by `falsify_aw_001_decoupled_weight_decay` (adamw-kernel-v1), which
    # compares AdamW against Adam algebraically rather than against a f32 reference.
    _, traj_nodecay = adamw_trajectory(
        MULTISTEP_N, ca, cb, remap, torch.float32, weight_decay=0.0
    )
    decay_signal = max(abs(a - b) for a, b in zip(traj32, traj_nodecay))

    jsonfmt.write(
        FIXTURE_DIR / "optimizer_multistep.json",
        {
            "source": source_block,
            "note": (
                f"{MULTISTEP_N} consecutive AdamW steps on the SAME recorded pair batch, "
                "from a fresh all-trainable slice model. `losses[i]` is the loss measured "
                "BEFORE step i, and the final entry is the loss after the last step, so the "
                "trajectory brackets every update. This is the only obligation in the phase "
                "that constrains beta1/beta2: at step 1 bias correction makes the update "
                "beta-independent (m_hat = g, v_hat = g^2), so a single-step fixture cannot "
                "constrain them at ANY tolerance. `separation` records what the mutations "
                "actually move, measured during generation."
            ),
            "adamw": ADAMW,
            "all_trainable": True,
            "steps": MULTISTEP_N,
            "losses": traj32,
            "separation": {
                "f32_f64_noise": traj_delta,
                "betas_0.5_0.5": betas_signal,
                "weight_decay_0": decay_signal,
                "decay_not_gated_here": (
                    "the decay term is ~3 f32 ulp of the parameters it acts on at this "
                    "lr/weight_decay, so no tolerance over this fixture separates it; "
                    "adamw-kernel-v1's falsify_aw_001 owns that defect"
                ),
            },
        },
    )

    # ---------------------------------------------------------- batch invariance --
    cs, cbatch = cases["invariance_single"], cases["invariance_batch3"]
    hs_s, m_s = run_slice(model32, cs, remap, torch.float32)
    hs_b, m_b = run_slice(model32, cbatch, remap, torch.float32)
    emb_single = l2_normalize(masked_mean(hs_s[-1], m_s))
    emb_batch = l2_normalize(masked_mean(hs_b[-1], m_b))
    row = corpus.INVARIANCE_TARGET_ROW
    inv_delta = float((emb_single[0].to(torch.float64) - emb_batch[row].to(torch.float64)).abs().max())
    record_tolerance(tolerances, "batch_invariance", inv_delta)
    jsonfmt.write(
        FIXTURE_DIR / "batch_invariance.json",
        {
            "note": (
                "The same sentence encoded at batch 1 and inside a padded batch of 3. "
                "`case_id` lives on EACH of `single` and `padded_batch` (mirroring loss_pair's "
                "a_case_id/b_case_id) because the two batches hold different text sets and "
                "cannot resolve to one tokenizer case. `target_row` says which padded row is "
                "the sentence that also appears in `single`."
            ),
            "single": {**joined("invariance_single"), "embedding": flat(emb_single)},
            "padded_batch": {
                **joined("invariance_batch3"),
                "embeddings": flat(emb_batch),
                "target_row": row,
            },
            "observed_max_abs_delta": inv_delta,
        },
    )

    # ---------------------------------------------------------- full model --------
    from sentence_transformers import SentenceTransformer

    st = SentenceTransformer(REPO_ID, revision=REVISION, device="cpu")
    st.eval()
    (full_case_id,) = corpus.FULL_MODEL_CASES
    trio = corpus.CASES[full_case_id]
    with torch.no_grad():
        st_emb = torch.tensor(st.encode(trio, convert_to_numpy=True, normalize_embeddings=True))

    # Cross-check: our manual masked-mean + L2 pipeline must reproduce the real
    # SentenceTransformer forward. This is what turns A1 from an assumption into a
    # verified fact -- a wrong clamp constant or eps fails HERE, not in wave 6.
    full_case = cases[full_case_id]
    bert = st[0].auto_model.eval()
    with torch.no_grad():
        out = bert(
            input_ids=torch.tensor(full_case["input_ids"], dtype=torch.long),
            attention_mask=torch.tensor(full_case["attention_mask"], dtype=torch.long),
            token_type_ids=torch.tensor(full_case["token_type_ids"], dtype=torch.long),
        )
        manual = l2_normalize(
            masked_mean(out.last_hidden_state, torch.tensor(full_case["attention_mask"], dtype=torch.float32))
        )
    st_vs_manual = max_abs_delta(st_emb, manual)
    if st_vs_manual > 1e-5:
        sys.exit(
            f"FATAL: manual masked-mean/L2 pipeline diverges from SentenceTransformer by "
            f"{st_vs_manual:.3e}. The ST pooling clamp / normalize eps read from source do not "
            "describe what the library actually does."
        )
    record_tolerance(tolerances, "full_model_reference", st_vs_manual)
    jsonfmt.write(
        FIXTURE_DIR / "full_model_reference.json",
        {
            "case_id": full_case_id,
            "texts": list(trio),
            "shape": {"batch": st_emb.shape[0], "hidden": st_emb.shape[1]},
            "embeddings": flat(st_emb),
            "st_vs_manual_pipeline_max_delta": st_vs_manual,
        },
    )

    # ---------------------------------------------------------- tokenizer + tol ---
    tolerances["tokenizer"] = {
        "max_abs_f32_f64_delta": 0.0,
        "floor": 0.0,
        "recommended_tolerance": 0.0,
        "exact": True,
        "note": "integer equality -- token ids/type ids/masks admit NO tolerance",
    }
    jsonfmt.write(FIXTURE_DIR / "tokenizer_cases.json", tokenizer_cases)
    jsonfmt.write(FIXTURE_DIR / "tolerances_measured.json", tolerances)

    for fam, t in tolerances.items():
        if fam == "tokenizer":
            continue
        if t["recommended_tolerance"] <= 0.0 or t["floor"] <= 0.0:
            sys.exit(f"FATAL: family '{fam}' produced a zero tolerance/floor")

    # ---------------------------------------------------------- join integrity ----
    verify_joins()

    # ---------------------------------------------------------- manifest ----------
    write_manifest()
    print("\nfixture corpus complete.")


def verify_upstream_digests() -> None:
    """Re-verify the pinned upstream digests, fail closed (T-1-06).

    slice_model.py checks these too, but this generator can be re-run on its own, and a
    re-pointed upstream artifact must not be able to reach a fixture through THAT door
    either. A guard that does not cover the surface where the decision is made is theater.
    """
    manifest_path = FIXTURE_DIR / "upstream_manifest.json"
    if not manifest_path.exists():
        sys.exit("FATAL: upstream_manifest.json missing; run slice_model.py first")
    recorded = json.loads(manifest_path.read_text())
    if recorded.get("revision") != REVISION:
        sys.exit(
            f"FATAL: upstream_manifest.json pins {recorded.get('revision')} but this "
            f"generator pins {REVISION}"
        )
    for name, path in fetch_pinned().items():
        got = sha256_file(path)
        want = recorded["files"].get(name)
        if got != want:
            sys.exit(
                f"FATAL: upstream digest mismatch for {name}\n  recorded: {want}\n  fetched : {got}"
            )
    print(f"upstream digests re-verified ({len(recorded['files'])} files) @ {REVISION[:12]}")


def verify_joins() -> None:
    """Assert B6 over the COMMITTED files, not over in-memory state."""
    tc = json.loads((FIXTURE_DIR / "tokenizer_cases.json").read_text())
    by_id = {c["id"]: c for c in tc["cases"]}

    def check(cid, texts, ids, where):
        if cid not in by_id:
            sys.exit(f"FATAL: {where} case_id '{cid}' does not resolve in tokenizer_cases.json")
        if by_id[cid]["texts"] != texts:
            sys.exit(f"FATAL: {where} texts differ from tokenizer_cases.json case '{cid}'")
        if ids is not None and by_id[cid]["input_ids"] != ids:
            sys.exit(f"FATAL: {where} input_ids_canonical != tokenizer_cases.json case '{cid}'")

    for c in json.loads((FIXTURE_DIR / "forward_per_layer.json").read_text())["cases"]:
        check(c["case_id"], c["texts"], c["input_ids_canonical"], "forward_per_layer")
    for c in json.loads((FIXTURE_DIR / "pooling_normalize.json").read_text())["cases"]:
        check(c["case_id"], c["texts"], c["input_ids_canonical"], "pooling_normalize")
    lp = json.loads((FIXTURE_DIR / "loss_pair.json").read_text())["pair"]
    check(lp["a_case_id"], lp["a_texts"], lp["a_ids_canonical"], "loss_pair.a")
    check(lp["b_case_id"], lp["b_texts"], lp["b_ids_canonical"], "loss_pair.b")
    bi = json.loads((FIXTURE_DIR / "batch_invariance.json").read_text())
    check(bi["single"]["case_id"], bi["single"]["texts"], bi["single"]["input_ids_canonical"], "batch_invariance.single")
    check(
        bi["padded_batch"]["case_id"],
        bi["padded_batch"]["texts"],
        bi["padded_batch"]["input_ids_canonical"],
        "batch_invariance.padded_batch",
    )
    fm = json.loads((FIXTURE_DIR / "full_model_reference.json").read_text())
    check(fm["case_id"], fm["texts"], None, "full_model_reference")
    print("join integrity: every fixture case_id resolves with matching texts and ids")


def write_manifest() -> None:
    files = sorted(p for p in FIXTURE_DIR.iterdir() if p.is_file() and p.name != "manifest.sha256")
    lines = []
    for p in files:
        h = hashlib.sha256(p.read_bytes()).hexdigest()
        lines.append(f"{h}  {p.name}")
    (FIXTURE_DIR / "manifest.sha256").write_text("\n".join(lines) + "\n")

    proc = subprocess.run(
        ["shasum", "-a", "256", "-c", "manifest.sha256"],
        cwd=FIXTURE_DIR,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        sys.exit(f"FATAL: manifest self-verification failed\n{proc.stdout}\n{proc.stderr}")
    print(f"manifest.sha256 covers {len(files)} files; shasum -c passed")


if __name__ == "__main__":
    main()
