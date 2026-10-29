# Phase 14: Laya training MCP server: S3 dataset in, .apr and reports out - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-10-28
**Phase:** 14-laya-training-mcp-server-s3-dataset-in-apr-and-reports-out
**Areas discussed:** Caller knobs, Access and spend limits, x86_64/CUDA acceptance, Result hand-off

Carried in and not re-asked: ze-kasher-dev; SageMaker first with ECS fallback; one role-read bucket; full output;
Python/torch training (Phase 8 D-01).

Todos: four keyword matches presented (GPU ledger tests, Qwen3.5-9B forward, Laya calibration spike, renacer
workspace gate). User's choice: **Fold none**.

---

## Caller knobs

| Option | Description | Selected |
|--------|-------------|----------|
| Dataset URI only | Everything else from laya-finetune-gate-v1 | ✓ |
| Plus epochs and stopping | Expose the two CLI knobs within contract bounds | |
| You decide | Claude picks during planning | |

| Option | Description | Selected |
|--------|-------------|----------|
| Prepared data dir only | task.json + train/eval jsonl (+ shift) | ✓ |
| Also raw CSV + named rule | Run prepare_* inside the job | |

| Option | Description | Selected |
|--------|-------------|----------|
| Sync validate tool | `validate_dataset`, same check in `train` | ✓ |
| Check inside the job only | Exit 2 after the container starts | |

| Option | Description | Selected |
|--------|-------------|----------|
| Caller-given name + task id | `runs/<name>/<task_id>/` | ✓ |
| Task id only | Caller keeps the mapping | |

---

## Access and spend limits

| Option | Description | Selected |
|--------|-------------|----------|
| pmcp.run OAuth | Token required, as on augury since Phase 13 | ✓ |
| Open on dev, like SetFit train | No auth | |

| Option | Description | Selected |
|--------|-------------|----------|
| One at a time | Single-flight, typed refusal | ✓ |
| A small fixed cap | 2–3 concurrent, needs quota | |

| Option | Description | Selected |
|--------|-------------|----------|
| 6 hours | Covers Phase 10's 4.7 h MPS run | ✓ |
| 2 hours | Tighter cost bound | |
| Set after the probe | Measure first | |

| Option | Description | Selected |
|--------|-------------|----------|
| ml.g5.xlarge, A10G 24 GB | Common, quota usually easy | ✓ |
| ml.g6.xlarge, L4 24 GB | Newer, often cheaper | |
| You decide after the probe | Pick by measured cost | |

---

## x86_64/CUDA acceptance

| Option | Description | Selected |
|--------|-------------|----------|
| arm64 step after training | Pack/verify on Graviton CPU | ✓ |
| Inside the x86 GPU container | One job, first x86 pack measurement | |

| Option | Description | Selected |
|--------|-------------|----------|
| Failed run, owner decides | No .apr; amendment only by owner | ✓ |
| Pre-authorize CUDA bars | Phase may amend laya-parity-v1 | |
| Switch to ECS Graviton GPU | Treat as the D-02 trigger | |

| Option | Description | Selected |
|--------|-------------|----------|
| Reproduce motivation-v1 | Retrain on SageMaker, compare to Phase 12 | ✓ |
| Synthetic lifecycle only | Tiny checkpoint, cheap | |
| Both | Lifecycle then reproduction | |

| Option | Description | Selected |
|--------|-------------|----------|
| Gate-outcome level | Same verdict, metrics in a band | ✓ |
| Force deterministic torch | Slower, some ops raise | |

---

## Result hand-off

| Option | Description | Selected |
|--------|-------------|----------|
| Summary inline + links | Verdict inline, manifest with read-time links | ✓ |
| Links only | Caller downloads gate-report.json | |

| Option | Description | Selected |
|--------|-------------|----------|
| Keep passes, expire fails | Fails expire after 30 days | ✓ |
| Keep everything | No lifecycle rule | |
| Expire all after 90 days | One rule | |

| Option | Description | Selected |
|--------|-------------|----------|
| Stops at S3 + fetch recipe | `just` pull into models/decide/<name>/ with sha256 checks | ✓ |
| Stops at S3, nothing more | Manual aws s3 cp | |

---

## Claude's Discretion

Server id and crate names; base-snapshot staging; image build and digest pinning; cancel semantics; finalizer
mechanism; task TTL and key layout.

## Deferred Ideas

Upload-through-MCP; raw-CSV preparation on the server; caller-set knobs; auto-deploy; ECS Graviton GPU tier; CUDA
parity bars.
