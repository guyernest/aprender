# Phase 11 Plan 04: Graviton gate (R6) Summary

**Owner-run Graviton measurement on a 10,240 MB arm64 probe: 64 x 256-token embedding ran about 28.7 s warm in Lambda Duration (inside 30 s), but every 64-input client wall exceeded 30 s (31.7 to 38.9 s). Reported, not promised.**

status: complete
evidence: `.planning/phases/11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin/11-GRAVITON-EVIDENCE.json` (schema `embed-graviton-v1`, outcome `measured`, reported_not_promised true)
commit: `50d155f7f` docs(11-04): Graviton gate evidence, reported not promised (R6)

## Run

- **Tier and region:** 10,240 MB, arm64 (`provided.al2023`), us-east-1. The memory ceiling was not a problem, so no fallback tier was used.
- **Owner reply (verbatim, recorded in `source.owner_reply`):** `gate-done deploy/embed-endpoint/.ids/graviton-gate1-20261024T013546Z.json`
- **Raw record:** `deploy/embed-endpoint/.ids/graviton-gate1-20261024T013546Z.json` (gitignored; sha256 recorded in `source.raw_sha256`).
- **Invoke attempts:** 15 (no retries, so no discarded attempts).
- **Live CodeSha256 vs package record:** equal, `ZJlCqrQdwrqCdAryFdczLuFW+JnyJP35t5pDix8NEjQ=`. The record was prepared before the run, and the measured code is the tested code.
- **Ingress:** no function URL, no resource policy, as recorded by the owner's run.
- **Probe destroyed:** the owner reported the stack destroyed. A read-only `get-function` (the single AWS call this task made, with the region from the raw record) returned `ResourceNotFoundException`, so `probe_function_after` is `absent`. Only the error code was captured.

## Samples

| Kind | Proven cold | CPU part | Client wall (ms) | Lambda Duration (ms) | Init Duration (ms) | Max memory (MB) | Billed (ms) |
|---|---|---|---|---|---|---|---|
| cold-1 | yes | 0xd0c | 6808.7 | 496.6 | 1804.6 | 426 | 2302 |
| cold-1 | yes | 0xd0c | 6455.0 | 517.7 | 1805.0 | 426 | 2323 |
| cold-1 | yes | 0xd40 | 5990.5 | 321.4 | 1429.4 | 426 | 1751 |
| warm-1 | no | 0xd40 | 3630.6 | 293.9 | - | 426 | 294 |
| warm-1 | no | 0xd40 | 3723.1 | 291.4 | - | 426 | 292 |
| warm-1 | no | 0xd40 | 3635.1 | 292.8 | - | 426 | 293 |
| warm-1 | no | 0xd40 | 3666.2 | 293.1 | - | 426 | 294 |
| warm-1 | no | 0xd40 | 3545.5 | 303.0 | - | 426 | 304 |
| cold-64 | yes | 0xd0c | 34890.9 | 28669.0 | 1778.7 | 550 | 30448 |
| cold-64 | yes | 0xd0c | 38947.0 | 29089.3 | 1750.5 | 549 | 30840 |
| warm-64 | no | 0xd0c | 32842.9 | 28703.9 | - | 549 | 28704 |
| warm-64 | no | 0xd0c | 35747.7 | 28624.8 | - | 549 | 28625 |
| warm-64 | no | 0xd0c | 31736.0 | 28795.5 | - | 549 | 28796 |
| tokenize-1mib (prose) | no | 0xd0c | 3329.8 | 1124.2 | - | 549 | 1125 |
| tokenize-1mib (adversarial) | no | 0xd0c | 3952.4 | 901.4 | - | 549 | 902 |

Every 64-input sample truncated all 64 inputs, left the autograd tape at 0, and returned no FunctionError. Every embedding sample has 384-dimensional rows. Both tokenize kinds were measured.

## Derived block, against the research bracket

Values are the normalizer's output, reproduced from the raw record. They are reported, not promised.

| Derived key | Value | Note |
|---|---|---|
| warm_64_encode_ms_median | 28654.2 | encode inside the handler, 64 inputs |
| warm_64_encode_ms_max | 28737.3 | |
| warm_ms_per_input_256_max | 449.0 | warm encode per input at the 256-token cap |
| cold_64_duration_ms_max | 29089.3 | cold 64-input Lambda Duration |
| cold_1_client_wall_ms_max | 6808.7 | cold canary, client wall |
| canary_within_30s | true | |
| batch64_within_30s_warm | true | Lambda Duration, see the gap below |
| batch64_within_30s_cold | true | Lambda Duration, see the gap below |
| tokenize_ms_per_mib_max | 1117.4 | prose; adversarial 899.4 |
| max_memory_used_mb_max | 550 | |
| measured_max_inputs | 64 | count of valid 64-input calls that completed |
| invoke_attempts_total | 15 | |
| billed_ms_total | 157293 | |
| research_bracket_s | [5.3, 50] | from the research, context only |
| vs_bracket | within | cold 64 duration 29.1 s |

**Against the bracket:** the measurement lands inside the research bracket of 5.3 to 50 s, at about 29 s. That is on the slow side of the bracket, not the fast end.

## Which CPU parts served

The probe reports `cpu_part` from `/proc/cpuinfo`. Per the MIDR part numbers, `0xd0c` is Neoverse N1 (Graviton 2) and `0xd40` is Neoverse V1 (Graviton 3). This mapping is my reading of the part numbers. It was not confirmed against the instance type, which the record does not state.

- **All three 64-input samples (cold and warm) ran on `0xd0c`.** All tokenize samples also ran on `0xd0c`.
- **Most one-input warm samples ran on `0xd40`.** The one cold-1 on `0xd40` was the third canary.

The 64-input numbers are therefore a Graviton 2 (N1) result, not a Graviton 3 result. Cross-kind comparisons, such as warm-1 per input against warm-64 per input, mix two CPU parts and should not be read as like-for-like.

## Does 64 x 256 fit 30 s?

- **Lambda Duration:** yes, warm and cold. Warm median 28.65 s, warm max 28.74 s, cold max 29.09 s.
- **Client wall:** no. Every 64-input sample exceeded 30 s by client wall: cold 34.9 and 38.9 s, warm 31.7 to 35.7 s.

**Gap for plan 11-08:** the normalizer's `batch64_within_30s_*` booleans, and `measured_max_inputs`, are based on Lambda Duration. A gateway cap of 30 s counts client wall, not Duration. On this measurement, 64 inputs do not fit a 30 s client budget. The normalizer was not changed, because the plan says derived numbers are not to be edited by hand and the plan does not choose a timeout or batch limit. Plan 11-08 needs to decide which measure governs admission. Until it does, `measured_max_inputs` = 64 should not be read as "64 inputs are admissible under a 30 s gateway".

## Cold canary

Cold one-input client wall: 5.99 to 6.81 s, Init Duration 1.43 to 1.80 s. Warm one-input client wall was about 3.5 to 3.7 s, with Duration about 0.29 s.

## Cost

Estimated from billed milliseconds (157,293 ms at 10 GB, so 1,572.9 GB-s), at the us-east-1 arm64 rate of $0.0000133334 per GB-s. That rate was recalled and not checked against a live price list in this run. Request charges for 15 invokes are negligible. The estimate is about **$0.021**. Billed milliseconds for the cold samples include Init (for example, 2302 billed = 496.6 Duration + 1804.6 Init, rounded), so the figure includes init billing. The plan's expectation of "well under one US dollar" holds.

## Deviations and gaps

1. **Driver change not committed.** The owner's first run refused to write its record because of a 12-digit false positive in float timings. `scripts/embed_endpoint/graviton_run.py` was changed to round float fields to six decimal places (`round_floats`, +14/-1). That change is uncommitted, as the coordinator instructed. The raw record was written by this uncommitted driver. The probe package is unchanged (CodeSha256 equal to the record), and the gate selftest was green after the change. A reviewer who wants the record reproducible from HEAD must first commit the driver change.
2. **Client wall exceeds the 30 s budget** for every 64-input sample. Reported above; this is the main open question for plan 11-08.
3. **Mixed CPU parts across samples.** The 64-input numbers are all N1 (`0xd0c`); the one-input warm numbers are mostly V1 (`0xd40`). Recorded above.
4. **`/tmp` paths in the plan's verify block** were replaced with the session scratchpad.
5. **Pre-record HEAD:** `graviton-pre.json` recorded git HEAD `9ba13403c`. HEAD was the same at normalize time (checked before writing the evidence).
6. **Destroy evidence:** the read-only `get-function` (the plan's one allowed AWS call) confirmed absence, so the owner's destroy report was not needed as the confirmation.
7. **No AWS write by the executor.** Deploy, run and destroy were owner-run. The executor made one read-only `get-function`. No `sts` call was made.

## Self-Check

- The committed evidence file exists at the plan path and was rescanned: 0 matches for a 12-digit sequence or `arn:aws`.
- The plan's verification block passed on the committed file (outcome measured, with provenance checks, reproducible derived block and probe absent).
- The normalizer selftest passed (`GRAVITON GATE SELFTEST OK 49 cases`).
- Commit `50d155f7f` contains only the evidence file. The driver change and `.planning/config.json` were left uncommitted.
- STATE.md and ROADMAP.md were not touched.
