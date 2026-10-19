# GPU probe: one private A10G, to fill the one measurement nobody has

STATUS: PREPARED, NOT LAUNCHED. Nothing here has run. Launching needs the operator's explicit word.

## What it is, and what it is not

It is a measurement host: one `g5.xlarge` (A10G, sm_86, 4 vCPU, 16 GiB) that runs the released
`apr` v0.71.4 **x86_64 CUDA** build on the same two GGUFs the Lambda endpoints serve (0.8B and 2B
Q4_K_M) and records start time, a plain chat, a tool call (cold, then warm), the GPU process and the
server's own device lines. It is not a deployment of the endpoint: no Function URL, no load balancer,
no public route, and nothing pmcp.run could point a provider row at.

Why it exists: the repo certifies RTX 4090 (sm_89) and GB10 (sm_121) only. No record shows apr on an
A10G (sm_86) or a T4 (sm_75), which is what SageMaker `ml.g5` and `g4dn` would give pmcp.run
(`09-PMCP-RUN-MODEL-LADDER-REPLY.md`, Question 3). This run turns "UNKNOWN" into a measurement or a
recorded failure.

## Why the recorded GPU position still holds

The position (`deferred-items.md` D-09-03): no GPU-backed *deployment* of this endpoint until the CUDA
routers are gated (#5478) or the 401 proof is repeated on that path. The CUDA routers ignore
`APR_API_KEY_HASH`, so this probe is private by construction instead of by key:

| Guard | Where |
|-------|-------|
| Security group with zero ingress rules; `up.sh` refuses if it finds any | `up.sh` |
| Server binds `127.0.0.1` only | `probe-remote.sh` |
| No SSH key, no key pair: access is SSM Session Manager / Run Command only | `up.sh` |
| IMDSv2 required, hop limit 1 | `up.sh` |
| Instance role: SSM core + `s3:GetObject` on the two weights buckets, nothing else | `up.sh` |
| Powers off and terminates itself at `CAP_MINUTES` (default 150) | `userdata.sh` + `--instance-initiated-shutdown-behavior terminate` |
| Release tarball and both GGUFs verified by sha256 before use; setup leaves a REFUSED marker on a mismatch and the probe refuses to start | `userdata.sh`, `probe-remote.sh` |
| Instance type limited to `g5.xlarge` / `g4dn.xlarge` (4 vCPU each; quota is 8 vCPU) | `up.sh` |

The probe does not run the 401 proof and does not claim the CUDA path is gated. Expect it to show the
ungated behaviour; that is a result, not a failure of the probe.

## Cost

`g5.xlarge` on-demand is about $1.01/h in us-east-1 (check the current price before launch), so the
150-minute cap bounds the instance at about $2.50, plus 120 GB gp3 for the same window (cents).
A normal run needs 15-25 minutes: setup about 5 minutes (the CUDA tarball and the two GGUFs, 0.5 GB and 1.3 GB), probe about 10.

## Run it (you run the mutating steps; Claude does read-only checks)

```
! assume ze-kasher-dev && deploy/llm-endpoint/gpu-probe/up.sh ze-kasher-dev
# wait ~5 minutes; check setup finished
! assume ze-kasher-dev && deploy/llm-endpoint/gpu-probe/probe.sh ze-kasher-dev
! assume ze-kasher-dev && deploy/llm-endpoint/gpu-probe/down.sh ze-kasher-dev    # prints "0 0 1 1"
```

`probe.sh` leaves its output in `deploy/llm-endpoint/.ids/gpu-probe/probe-<ts>.out` (gitignored).
`down.sh` is idempotent and removes the instance, security group, role and instance profile.

## What the result can and cannot say

Can: whether the x86_64 CUDA build starts, loads and serves both models on sm_86 (JIT from PTX is a
code reading today); start time from a warm S3 pull; one tool-call round trip; GPU memory used;
whether the server reports a CUDA device (mechanism engaged, not assumed).

Cannot: agent-turn cost on GPU (that needs the platform's real turns, not one request), concurrency,
a T4/`g4dn` or Graviton `g5g` result (`GPU_TYPE=g4dn.xlarge` runs the same script on a T4; `g5g` is
arm64 and needs the aarch64 CUDA tarball, not wired here), or anything about auth on this path.
Single environment, one sample per shape, like the Lambda smoke test.

## Known gaps before launch

- `probe-remote.sh` uses a minimal request pair, not the 12-message replay set; it is a smoke
  measurement, not the table pmcp.run asked for.
- The tarball layout was not inspected; `userdata.sh` locates `apr` with `find`.
- The `--dry-run` of `run-instances` needs the role and security group to exist, so it can only run
  after `up.sh` has made them; it was not run.
