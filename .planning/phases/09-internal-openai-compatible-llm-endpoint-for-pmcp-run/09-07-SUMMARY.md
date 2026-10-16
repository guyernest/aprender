---
phase: 09-internal-openai-compatible-llm-endpoint-for-pmcp-run
plan: 07
subsystem: infra
tags: [aws, lambda, live, go-no-go, measurement, evidence, d-15, 401-not-proven]

requires:
  - phase: 09-internal-openai-compatible-llm-endpoint-for-pmcp-run
    provides: "09-05 release re-probe (baseline four crosses, release serves the GGUF path open); 09-06 lifecycle recipes and self-test; 09-13 probe wrapper, bench, measure recipe and budget guard"
provides:
  - "the deployment of record: Lambda function aprender-llm-endpoint-dev (10,240 MB / 900 s / 10,240 MB / arm64 image, reserved concurrency 1) in 816069129666 us-east-1, with its role, bucket, ECR repository and log group, and NO Function URL"
  - "09-LIVE-DEPLOY-EVIDENCE.json: outcome deployed-401-not-proven-no-url, identity, 401 verdict, cold and warm samples with wall and billed, derived figures, cost 0.154 USD against 0.50"
  - "conformance-v0.71.4-deployed-invoke.json: the deployed pin's table through lambda:Invoke (label deployed, transport lambda-invoke), reader-green"
  - "bench.py --conformance and an importable probe_openai transport seam"
affects: [09-08, 09-09, 09-10]

plan_head_before: f6279dbd4a8f22e588491b1e064119f7349f4463
plan_head_after: 97a1af5701e4eb23ca001601ac9cb989b19e4a0d
actuals:
  tokens: 70000
  tasks: 3
  commits: 5

key-files:
  created:
    - .planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-LIVE-DEPLOY-EVIDENCE.json
    - deploy/llm-endpoint/probe/results/conformance-v0.71.4-deployed-invoke.json
  modified:
    - justfile
    - deploy/llm-endpoint/probe/bench.py
    - deploy/llm-endpoint/probe/probe_openai.py

key-decisions:
  - "The D-15 answer was the option id go-dev-copy-weights alone; the 0.50 USD ceiling was proposed by the orchestrator and is recorded as such, not attributed to the operator"
  - "401 NOT PROVEN is recorded and acted on exactly as the plan says: no Function URL, and 09-08 waits on a release carrying the D-04 auth fix (paiml/aprender#5451) and only that fix"
  - "Reserved concurrency 1 was kept, so the two-at-once sample measures throttling (TooManyRequestsException), not a second environment; the record says so instead of substituting the spike's number"
  - "The conformance replay reuses probe_openai.main through a transport seam rather than a second copy of the verdict rules; HTTP output is byte-identical (stub-server comparison in both profiles)"

requirements-completed: [IR-2, IR-4, IR-5, SC-3, SC-4]

coverage:
  - id: D1
    description: "D-15 go: up, weights, image push, DRY-RUN OK, PREFLIGHT OK, DEPLOY OK; function Active at 10240/900/10240/arm64, reserved concurrency 1, resolved image digest equals the pushed digest, no URL config"
    requirement: IR-4
    verification:
      - kind: integration
        ref: "just llm-endpoint-deploy -> DEPLOY OK; get-function-configuration/get-function-concurrency/get-function readback; get-function-url-config -> ResourceNotFoundException"
        status: pass
    human_judgment: false
  - id: D2
    description: "identity from the function's own first [entry] log: apr 0.71.4 (5027da624) equals the image's apr --version; gguf sha256 OK; cpu_part 0xd40"
    requirement: IR-2
    verification:
      - kind: integration
        ref: "aws logs filter-log-events '[entry]' on /aws/lambda/aprender-llm-endpoint-dev"
        status: pass
    human_judgment: false
  - id: D3
    description: "SC3: the unauthenticated request is answered 200 by the v0.71.4 pin, so 401 is NOT PROVEN and no URL was created; the HTTPS half waits on a release with the D-04 fix"
    requirement: SC-3
    verification:
      - kind: integration
        ref: "just llm-endpoint-prove-401 -> exit 3, 401 NOT PROVEN: statusCode=200; bench no_auth_401 200 twice; conformance P12 200"
        status: pass
    human_judgment: true
    rationale: "That the outcome is honest and the consequence for 09-08 is right is a decision for the operator; the measurement itself is mechanical"
  - id: D4
    description: "SC4: wall and billed duration, cost, cpu_part per sample on the deployed function: cold, two-tool turn twice, turn-2 reuse, ten-tool prefill, two-at-once, no-auth case"
    requirement: SC-4
    verification:
      - kind: integration
        ref: "just llm-endpoint-measure x2 -> MEASURE OK (both), budget guard never fired; evidence verify block 1 passed"
        status: pass
    human_judgment: false
  - id: D5
    description: "D-07: the deployed pin's conformance table through lambda:Invoke, label deployed, reader-green, with the probe refactor not changing the HTTP output"
    requirement: IR-5
    verification:
      - kind: unit
        ref: "cargo test -p aprender-serve --lib api::llm_endpoint_conformance_tests -> 10 passed; python3 -I bench.py --selftest OK; stub-server cmp of probe_openai before/after in both profiles identical"
        status: pass
    human_judgment: false
  - id: D6
    description: "the 120 s demo-turn statement (D-10): the two-tool turn fits, a ten-tool prompt does not"
    requirement: SC-4
    verification:
      - kind: other
        ref: "warm tools2_turn1 72.3 s and 71.5 s; turn 2 4.6 s and 4.5 s; tools10_1tok 148.8 s"
        status: pass
    human_judgment: true
    rationale: "Whether 148.8 s for a ten-tool prompt is acceptable for the platform is a product decision; the figures are measured on one cpu_part and one model"

duration: 40 min
completed: 2026-10-16
status: complete
---

# Phase 9 Plan 07: D-15 go and the live deployment of record Summary

**The 0.8B endpoint runs on default Lambda at the 10,240 MB tier behind IAM-only invoke, the v0.71.4 pin answers an unauthenticated request 200 (401 NOT PROVEN, no URL created), and every figure in the evidence record comes from a run made here: a two-tool turn is 72.3 s and 71.5 s at 0.0095 USD, turn 2 reuses the session at 8.1 s against 66.0 s, a ten-tool prompt is 148.8 s, for 0.154 USD of a 0.50 USD ceiling.**

## Performance

- **Duration:** 40 min (2026-10-16T18:05:06Z to about 18:46Z)
- **Tasks:** 3 (Task 1 the D-15 checkpoint, answered before this run; Task 2 tracer; Task 3 auto)
- **Commits:** 5 (`8dfa5cf65`, `0f394e6be`, `758229a7f`, `a4fccac29`, `97a1af570`), measured from the ledger before this SUMMARY's own commit
- **Spend:** 0.154 USD against 0.50 (0.1436 measured invocation cost plus a 0.01 rounded-up allowance for storage, estimate)

## Task 1: the D-15 answer

The operator answered the blocking-human checkpoint with the option id `go-dev-copy-weights` (reply of 2026-10-16): env dev, profile `ze-kasher-dev` (account 816069129666, us-east-1), pin as in `pin.env` (v0.71.4, profile baseline), weights by server-side copy from the spike bucket, reserved concurrency 1, spike-031 leftovers left as they are. The USD 0.50 cost ceiling was proposed by the orchestrator in the question and accepted implicitly through the option id; the evidence records it as `cost_ceiling_source: orchestrator-proposed default` and does not attribute the number to the operator. `aws sts get-caller-identity` returned account 816069129666 before any write.

## Task 2: the deployment, by recipe

| Step | Result |
|---|---|
| `just llm-endpoint-up dev ze-kasher-dev` | `UP OK aprender-llm-endpoint-dev (account 816069129666, bucket aprender-llm-endpoint-weights-816069129666-dev, repository aprender-llm-endpoint-dev)` |
| `just llm-endpoint-weights copy dev ze-kasher-dev` (run 1) | `REFUSED s3-sha` (see Deviation 1) |
| same, after the fix | `WEIGHTS OK s3://aprender-llm-endpoint-weights-816069129666-dev/Qwen3.5-0.8B-Q4_K_M.gguf sha256 bd258782e35f7f458f8aced1adc053e6e92e89bc735ba3be89d38a06121dc517` |
| `just llm-endpoint-image 1` (runs 1 and 2) | `REFUSED push` rc 8 (see Deviation 3) |
| `just llm-endpoint-image 1` (run 3) | `PUSHED 816069129666.dkr.ecr.us-east-1.amazonaws.com/aprender-llm-endpoint-dev:cpu-v0.71.4 sha256:f291c0a77f17d9f203953bba748f1a3f7287780d36628883f933d5b069a1dd15` then `IMAGE OK aprender-llm-endpoint:cpu-v0.71.4 (apr apr 0.71.4 (5027da624))` |
| `DRY_RUN=1 just llm-endpoint-deploy` | `DRY-RUN OK aprender-llm-endpoint-dev (offline rungs 1-7; account, s3-sha and digest NOT checked; apr v0.71.4, gguf bd258782...; nothing deployed)` |
| `PREFLIGHT=1 just llm-endpoint-deploy` | `PREFLIGHT OK aprender-llm-endpoint-dev (rungs 1-10; read-only AWS calls only; apr v0.71.4; nothing deployed)` |
| `RESERVED_CONCURRENCY=1 just llm-endpoint-deploy` | `readback: state=Active memory=10240 timeout=900 ephemeral=10240 arch=arm64` then `DEPLOY OK aprender-llm-endpoint-dev (image sha256:f291c0a7..., apr v0.71.4)` |

Read back independently: State Active, MemorySize 10240, Timeout 900, EphemeralStorage 10240, arm64, PackageType Image, environment variable names only (`APR_API_KEY_HASH`, `MODEL_S3_URI`, `MODEL_SHA256`, `PLATFORM`, `RAYON_NUM_THREADS`), `ReservedConcurrentExecutions` 1, `Code.ResolvedImageUri` digest equal to `.ids/image-pushed.json`, and `get-function-url-config` answering `ResourceNotFoundException` (re-checked at the end of the plan).

**Identity lines** (first `[entry]` of the function's own log, the cold start the proof invocation caused):

```
[entry] t=1848ms start pid=9 apr=apr 0.71.4 (5027da624)
[entry] t=1893ms host: nproc=6 threads=6 cpu_part=0xd40 model_name=
[entry] t=1901ms mem: total_kb=10972280 cgroup_max=unknown
[entry] t=1911ms APR_API_KEY_HASH=set
[entry] t=1918ms s3 pull start
[entry] t=21358ms s3 pull done bytes=532517120
[entry] t=23118ms model sha256 OK bytes=532517120
[entry] t=23120ms apr serve start
REPORT ... Duration: 19201.83 ms  Billed Duration: 29110 ms  Memory Size: 10240 MB  Max Memory Used: 1522 MB  Init Duration: 9907.55 ms
```

The image's `apr --version` is the same line, so `identity.matches` is true. The 10,240 MB tier admitted the model (host MemTotal 10,972,280 kB, peak Max Memory Used 1,602 MB over every request); this answers the D-02 admission claim that plan 09-05 left PARTIAL because the Docker VM had 4 GB.

**The 401 verdict (D-12):** `just llm-endpoint-prove-401 dev ze-kasher-dev` exited 3: `401 NOT PROVEN: statusCode=200, invoke rc=0. The deployed pin does not enforce the key; no URL may be created (D-12)`. This is the expected outcome for v0.71.4, which lacks the D-04 fix (paiml/aprender#5451, open at this date). The outcome is `deployed-401-not-proven-no-url`: no Function URL was created at any point, `ACCEPT_UNRESERVED_URL` was not used, and plan 09-08 waits on a release that carries the D-04 auth fix and only that fix (D-05). If none ships, 09-08 and 09-10 close the phase as incomplete with SC1 and SC3 not delivered and no ingress created.

## Task 3: D-14 measurement (`MEASURE OK` lines and tables)

`just llm-endpoint-measure dev ze-kasher-dev deployed 1 1 2 0.49` printed `MEASURE OK deploy/llm-endpoint/.ids/bench-deployed-20261016T182251Z.json` (spent_usd 0.049903, budget 0.49, no budget stop, cpu_part 0xd40):

| shape | status | wall s | billed ms | init ms | max MB | $ | cpu_part |
|---|---|---|---|---|---|---|---|
| cold_short_1tok | 200 | 20.134 | 18954 | 7489.01 | 1521 | 0.002527 | 0xd40 |
| short_1tok | 200 | 12.051 | 11261 | - | 1563 | 0.001501 | 0xd40 |
| short_64tok | 200 | 21.542 | 20817 | - | 1570 | 0.002776 | 0xd40 |
| tools2_1tok | 200 | 66.028 | 65264 | - | 1584 | 0.008702 | 0xd40 |
| tools2_turn1 | 200 | 72.34 | 71452 | - | 1591 | 0.009527 | 0xd40 |
| turn2_extend_1tok | 200 | 8.104 | 7298 | - | 1592 | 0.000973 | 0xd40 |
| turn2_extend_64tok | 200 | 4.607 | 3813 | - | 1592 | 0.000508 | 0xd40 |
| tools10_1tok | 200 | 148.819 | 148082 | - | 1602 | 0.019744 | 0xd40 |
| conc2_0 | 200 | 16.834 | 16020 | - | 1602 | 0.002136 | 0xd40 |
| conc2_1 | 0 | 0.786 | - | - | - | - | 0xd40 |
| conc2_wall | - | 16.838 | - | - | - | - | 0xd40 |
| no_auth_401 | 200 | 12.15 | 11316 | - | 1602 | 0.001509 | 0xd40 |

`just llm-endpoint-measure dev ze-kasher-dev deployed-sample2 0 0 0 0.446` printed `MEASURE OK deploy/llm-endpoint/.ids/bench-deployed-sample2-20261016T182943Z.json` (spent_usd 0.025339, budget 0.446, no budget stop):

| shape | status | wall s | billed ms | init ms | max MB | $ | cpu_part |
|---|---|---|---|---|---|---|---|
| short_1tok | 200 | 12.095 | 11284 | - | 1602 | 0.001505 | null (see Deviation 2) |
| short_64tok | 200 | 21.787 | 20947 | - | 1602 | 0.002793 | null |
| tools2_1tok | 200 | 65.592 | 64817 | - | 1602 | 0.008642 | null |
| tools2_turn1 | 200 | 71.492 | 70683 | - | 1602 | 0.009424 | null |
| turn2_extend_1tok | 200 | 8.052 | 7202 | - | 1602 | 0.00096 | null |
| turn2_extend_64tok | 200 | 4.533 | 3818 | - | 1602 | 0.000509 | null |
| no_auth_401 | 200 | 12.095 | 11295 | - | 1602 | 0.001506 | null |

What the figures say (all on one cpu_part, 0xd40, and one model):

- **Two-tool turn:** 72.34 s and 71.492 s wall, 71.452 s and 70.683 s billed, 0.009527 and 0.009424 USD. The warm turn is dominated by prefill of about 421 prompt tokens at roughly 6.4 tokens/s (decode about 6.5 to 6.6 tokens/s).
- **Cold start to first answer:** 20.1 s wall, init 7.49 s, billed 18.95 s; the first start after deploy billed 29.1 s with init 9.91 s.
- **Turn-2 reuse:** extending the same conversation by one token took 8.1 s (8.05 s) against 66.0 s (65.6 s) for the same prompt without the cached prefix.
- **Ten-tool prefill:** 148.8 s wall, 148.1 s billed, 0.0197 USD (one sample; the conformance probe's ten-tool request took 155.1 s).
- **Two at once:** the first request was served in 16.8 s; the second was refused in 0.79 s with `TooManyRequestsException` because reserved concurrency is 1. No second environment started, so there is no second-environment cold start; the function serves one conversation at a time and a second caller is throttled, not queued.
- **No-auth case:** 200 in both runs and in the conformance probe, equal to the 401 verdict's status.
- **D-10 (120 s):** the demo two-tool turn fits: turn 1 is 72.3 s and 71.5 s and turn 2 about 4.6 s, roughly 77 s, plus about 20 s if the first request is cold; each request is under 120 s. A ten-tool prompt does not fit (148.8 s).

Spike-031 numbers appear in the record only under `spike_031_context` with `context_not_measurement: true`.

**Deployed conformance table** (`bench.py --conformance`, 16 invokes, 0.064479 USD, `conformance invokes=16 spent_usd=0.064479 budget_usd=0.42`): `conformance-v0.71.4-deployed-invoke.json`, label `deployed`, profile baseline, pin v0.71.4, `transport: lambda-invoke`, streaming probes `[buffered-through-adapter]`. The crosses are exactly the spike-029 four: `tool result -> final answer` (422), `tool_choice=required`, `max_completion_tokens honoured` (222 tokens, finish=stop) and `error envelope` (422); `foreign model id`, `no Authorization header` (200, the server is open) and `cpu_part in /health` read info. The reader test passed: 10 passed, 0 failed.

## Task Commits

1. **Recipe fix found by the first live run** - `8dfa5cf65` (fix): `llm-endpoint-weights copy` uses `s3api copy-object`
2. **Task 2 (tracer): deployment record, 401 NOT PROVEN** - `0f394e6be` (test)
3. **Task 3a: `bench.py --conformance`, probe transport seam** - `758229a7f` (feat)
4. **Task 3b: host lookup fix found by the first live measure run** - `a4fccac29` (fix)
5. **Task 3c: measurements and the deployed conformance record** - `97a1af570` (test)

Tracer gate: the Task 2 verify blocks (record shape, function configuration, no URL config, live configuration Active at the tier) passed before any measurement ran.

## Deviations from Plan

### Auto-fixed issues

**1. [Rule 1 - Bug] `llm-endpoint-weights copy` dropped the sha256 metadata**
- **Found during:** Task 2, first real run of the recipe
- **Issue:** `aws s3 cp` between buckets with `--metadata-directive COPY` produced an object with no `Metadata.sha256`, so `verify_object` refused `s3-sha` (the object had been copied; the recipe stopped).
- **Fix:** `aws s3api copy-object --metadata-directive COPY --server-side-encryption AES256` (the object is far under 5 GB); self-test case 43 asserts the `copy-object` argv with the COPY directive and no `s3 cp`, and the shim now answers `head-object` per bucket; self-test 43/43, `AWS CALLS: 0`. Re-run: metadata equals the ladder pin.
- **Files modified:** justfile. **Commit:** `8dfa5cf65`

**2. [Rule 1 - Bug] bench `host_info` returned nothing on the real log service**
- **Found during:** Task 3, run 2 (a run with no cold record): `cpu_part` came back null.
- **Issue:** the pattern `"[entry] host:"` matches nothing on CloudWatch Logs, and `--query events[-1].message` prints per result page, with a `None` before the message. The 09-13 shim answered any pattern, so its self-test could not see either.
- **Fix:** pattern `"cpu_part"`, take every message and the newest `[entry]` line; the shim now answers like the service (pattern-sensitive, paginated); the old pattern is caught by a mutation of the self-test; the live lookup returns `0xd40` and `apr 0.71.4 (5027da624)`. Run 2's records stay null in the bench file; the evidence fills `cpu_part` 0xd40 with the stated source (the same execution environment as run 1: no Init Duration in any run-2 REPORT).
- **Files modified:** deploy/llm-endpoint/probe/bench.py. **Commit:** `a4fccac29`

**3. [Environment] `docker push` through Docker Desktop's proxy failed with `write: broken pipe`**
- **Found during:** Task 2, `llm-endpoint-image 1` (rc 8, `REFUSED push`, twice).
- **Fix:** none in the recipe. Layers already pushed persist, so I repeated `docker push` of the recipe's own tag until it completed (5 attempts), then re-ran the recipe, which recorded the digest. A retry loop in `llm-endpoint-image` is a candidate follow-up; I did not change a 09-04 recipe for a network symptom.

### Plan text vs reality

**4. The plan's `bench.py --conformance` was written as an addition plus a probe refactor.** I made `probe_openai.main` take a transport and an extra header (default `HttpTransport`, so the verdict rules are not duplicated). The HTTP stdout, stderr and JSON are identical to the committed version on a stub server in both profiles (compared with timings normalised; only the `--out` file name differs). The reader test and the committed baseline record are unchanged.

**5. `just llm-endpoint-probe` cannot write label `deployed`, as the plan says; the bench conformance mode wrote the record.** The budget guard in conformance mode refuses before each of the 16 invokes (ceiling 0.025 USD each, guard estimate, not recorded) and, on a stop, writes nothing and exits 4; it did not fire.

**Total deviations:** 2 bugs auto-fixed (1, 2), 1 environment note (3), 2 plan-text notes (4, 5). **Impact:** none on scope or on the evidence; the two bugs sat in 09-06 and 09-13 code that had only ever run against shims.

## Authentication Gates

None. The AWS profile resolved on the first call; the key material was sourced from the session scratchpad inside the shells that ran the recipes and was never printed or written.

## Known Stubs

None.

## Threat Flags

None beyond the plan's register. T-09-23: no URL exists and the spend is 0.154 USD against 0.50. T-09-24: the spike-031 bucket, functions, roles and the rest were not touched (the bucket was only read; its object still carries its metadata). T-09-25: the deploy refused unless the account matched `.ids/aws.json`, the function's resolved image digest equals the pushed digest, and identity is read from the function's own `[entry]` apr line.

## Issues Encountered

- `gsd-tools state.*` writes fail on the pre-existing Phase 4 file (`04-VERIFICATION.md`, status `gaps_acknowledged`); not edited, per the dispatch (see State updates).
- The rtk hook rewrites bare `aws` output (`AWS: ? ?`); I called `/usr/local/bin/aws` by path for my own read-only checks. The recipes run inside `just` and are not affected.
- A recursive grep over `deploy/` for a secret scan was too slow (the agent-loop target directories); I scanned the five files this plan writes or commits instead. No key or hash value is in any of them, and the evidence and the conformance record contain only the known pins and digests as 64-hex strings.

## State updates

- `roadmap.update-plan-progress 09`, `state.*` and `requirements.mark-complete`: see the completion report; the `state.*` writes are expected to fail on the Phase 4 file and were not worked around.

## Resources left running (the deployment of record)

Function `aprender-llm-endpoint-dev` (reserved concurrency 1, no URL), role `aprender-llm-endpoint-dev`, bucket `aprender-llm-endpoint-weights-816069129666-dev` (one object, 532,517,120 bytes), ECR repository `aprender-llm-endpoint-dev` (tag `cpu-v0.71.4`), log group `/aws/lambda/aprender-llm-endpoint-dev` (7 days). Idle cost is cents per month. `teardown_commands_not_run`: `just llm-endpoint-teardown dev ze-kasher-dev`. The throwaway-free key hash is the function's `APR_API_KEY_HASH`; the key itself is only in the session scratchpad until plan 09-10.

## Next Phase Readiness

- 09-08 (HTTPS) is blocked on a release carrying the D-04 auth fix; nothing in this plan removes that dependency. If none ships, 09-08 and 09-10 close the phase incomplete (SC1 and SC3 not delivered, no ingress).
- The figures 09-09 and 09-10 need are in the evidence record: 120 s fits for two tools and not for ten, one conversation per function, throttling at concurrency 1.

## Self-Check: PASSED

- FOUND: 09-LIVE-DEPLOY-EVIDENCE.json, conformance-v0.71.4-deployed-invoke.json, bench.py, probe_openai.py, justfile, `.ids/function.json`, `.ids/image-pushed.json`, `.ids/prove-401.json`
- FOUND (ancestors of HEAD): 8dfa5cf65, 0f394e6be, 758229a7f, a4fccac29, 97a1af570
- Verify blocks re-run after the last edit: evidence verify (go record, no URL config, live configuration), Task 3 verify (evidence complete, deployed record, reader 10 passed, baseline untouched), `just llm-endpoint-selftest` 43/43 with `AWS CALLS: 0`, `bench.py --selftest` OK
- No key or key-hash value in the evidence, the conformance record, the SUMMARY or the code
