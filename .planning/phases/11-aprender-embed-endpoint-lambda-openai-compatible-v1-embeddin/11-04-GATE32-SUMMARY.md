# Phase 11 Plan 04: Graviton gate, 32-input run (gate32) Summary

**A second owner-run measurement of the same probe, with 32-input samples added (D-04 cap, D-05 admission on Lambda Duration). The 32-input Lambda Duration is 14.30 to 14.59 s on all five samples, about 0.45 s per input. Every 64-input sample also ran in this record. This is a separate run from gate1.**

status: complete
evidence: `.planning/phases/11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin/11-GRAVITON-EVIDENCE-gate32.json` (schema `embed-graviton-v1`, outcome `measured`, reported_not_promised true)
gate1 evidence (unchanged): `.planning/phases/11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin/11-GRAVITON-EVIDENCE.json`

## Run

- **Raw record:** `deploy/embed-endpoint/.ids/graviton-gate32-20261024T041003Z.json` (gitignored; sha256 in `source.raw_sha256`).
- **Owner reply (verbatim, in `source.owner_reply`):** `gate-done deploy/embed-endpoint/.ids/graviton-gate32-20261024T041003Z.json`
- **Function:** arm64, 10,240 MB, us-east-1, RAYON_NUM_THREADS 6, timeout 120 s.
- **Live CodeSha256 vs package record:** equal, `ZJlCqrQdwrqCdAryFdczLuFW+JnyJP35t5pDix8NEjQ=`.
- **Ingress:** no function URL, no resource policy (as recorded by the owner's run).
- **Invokes:** 20 nominal, 20 attempts, no retries (every sample accepted on its first attempt).
- **Probe destroyed:** the owner reported `aprender-embed-probe-dev: destroyed`. A read-only `get-function` (the one AWS call this step made) returned `ResourceNotFoundException`, so `probe_function_after` is `absent`. Only the error code was recorded.
- **Pre-record:** `deploy/embed-endpoint/.ids/graviton-gate32-pre.json` (gitignored). It was written after the run, from the package record and the asset on disk (bootstrap and apr hashes checked against the record). Its `git_head` is `f5ab8806b`, the last commit before the run (committed 04:02:27Z, run at 04:10:03Z), and no commit came after it. The file says it was written after the run, so it is not a pre-run record.

## Per-sample table (all 20)

Duration is the Lambda REPORT Duration. Client wall is measured on the client. Init is the REPORT Init Duration, present only on cold samples. CPU part is the `/proc/cpuinfo` part on the serving instance, read from the probe's response.

| Kind | Proven cold | CPU part | n | Rows / truncated | Client wall (ms) | Duration (ms) | Init (ms) | Max mem (MB) | Billed (ms) |
|---|---|---|---|---|---|---|---|---|---|
| cold-1 | yes | 0xd0c | 1 | 1 / 1 | 4170.2 | 501.8 | 1816.7 | 426 | 2319 |
| cold-1 | yes | 0xd0c | 1 | 1 / 1 | 4235.7 | 494.2 | 1818.1 | 426 | 2313 |
| cold-1 | yes | 0xd0c | 1 | 1 / 1 | 4347.3 | 507.8 | 1908.3 | 430 | 2417 |
| warm-1 | no | 0xd0c | 1 | 1 / 1 | 1645.8 | 485.6 | - | 430 | 486 |
| warm-1 | no | 0xd0c | 1 | 1 / 1 | 1581.3 | 510.0 | - | 430 | 511 |
| warm-1 | no | 0xd0c | 1 | 1 / 1 | 1266.9 | 499.7 | - | 430 | 500 |
| warm-1 | no | 0xd0c | 1 | 1 / 1 | 1353.0 | 495.7 | - | 430 | 496 |
| warm-1 | no | 0xd0c | 1 | 1 / 1 | 1281.9 | 495.6 | - | 430 | 496 |
| **cold-32** | yes | 0xd0c | 32 | 32 / 32 | 18234.9 | **14590.8** | 1823.4 | 548 | 16415 |
| **cold-32** | yes | 0xd0c | 32 | 32 / 32 | 18132.4 | **14501.2** | 1785.7 | 544 | 16287 |
| **warm-32** | no | 0xd0c | 32 | 32 / 32 | 15129.2 | **14330.3** | - | 544 | 14331 |
| **warm-32** | no | 0xd0c | 32 | 32 / 32 | 15143.5 | **14339.8** | - | 544 | 14340 |
| **warm-32** | no | 0xd0c | 32 | 32 / 32 | 15114.3 | **14299.1** | - | 544 | 14300 |
| cold-64 | yes | 0xd40 | 64 | 64 / 64 | 20668.2 | 17035.8 | 1374.3 | 550 | 18411 |
| cold-64 | yes | 0xd0c | 64 | 64 / 64 | 32664.6 | 29110.4 | 1860.0 | 550 | 30971 |
| warm-64 | no | 0xd0c | 64 | 64 / 64 | 29660.8 | 28847.1 | - | 550 | 28848 |
| warm-64 | no | 0xd0c | 64 | 64 / 64 | 29755.9 | 28977.0 | - | 550 | 28977 |
| warm-64 | no | 0xd0c | 64 | 64 / 64 | 29563.0 | 28734.8 | - | 550 | 28735 |
| tokenize-1mib (prose) | no | 0xd0c | - | - | 1907.8 | 1086.9 | - | 550 | 1087 |
| tokenize-1mib (adversarial) | no | 0xd0c | - | - | 1677.6 | 900.5 | - | 550 | 901 |

Every batch sample truncated all of its inputs (32 of 32, 64 of 64) and left the autograd tape at 0. No FunctionError. Every embedding sample has 384-dimensional rows.

## The 32-input figures (admission measure: Lambda Duration, D-05)

- **Duration:** cold 14,590.8 and 14,501.2 ms; warm 14,330.3, 14,339.8 and 14,299.1 ms. Max 14,590.8 ms, which is 14.59 s, inside 30 s.
- **Per input:** Duration divided by 32 gives about 447 to 456 ms per 256-token input, close to the 64-input figure of about 449 ms.
- **Client wall (not the admission measure):** 15.11 to 15.14 s warm and 18.13 to 18.23 s cold. Both are under 30 s.
- **CPU part:** all five 32-input samples ran on `0xd0c` (Neoverse N1 by my MIDR-part reading, not confirmed against the instance type).
- **Cold start:** Init 1.79 to 1.82 s on the 32-input cold samples.

Caveat: the 32-input samples are the only 32-input data. D-04 chose 32, and this run measured it. The derived block has no 32-specific keys (see gaps), so these numbers come from the per-sample rows.

## Derived block (normalizer output, as produced)

| Key | Value |
|---|---|
| batch64_within_30s_cold | true (Lambda Duration) |
| batch64_within_30s_warm | true (Lambda Duration) |
| canary_within_30s | true (cold-1 client wall max 4.35 s) |
| cold_1_client_wall_ms_max | 4347.277 |
| cold_64_duration_ms_max | 29110.35 |
| warm_64_encode_ms_median | 28795.87 |
| warm_64_encode_ms_max | 28928.561 |
| warm_ms_per_input_256_max | 452.009 |
| measured_max_inputs | 64 |
| invoke_attempts_total | 20 |
| billed_ms_total | 223141 |
| tokenize_ms_per_mib_max | 1080.336 |
| max_memory_used_mb_max | 550 |
| research_bracket_s | [5.3, 50] |
| vs_bracket | within |

The normalizer's derived block has no 32-specific keys. `measured_max_inputs` is 64 because the 64-input samples are valid and larger; the 32-input samples count toward it but do not change the maximum. The 64-input cold maximum (29.11 s) is on `0xd0c`.

## Relation to gate1

These are two separate runs of the same probe: the same package and the same CodeSha256, a different run, and a later pre-record. Differences to keep in view:

- **64-input client wall.** Gate1 had warm-64 client wall at 31.7 to 35.7 s. In gate32 it was 29.6 to 29.8 s, under 30 s. Cold-64 was 32.7 s on `0xd0c` and 20.7 s on `0xd40`. Since D-05 admits on Lambda Duration, this matters only as information.
- **CPU part.** One cold-64 sample ran on `0xd40` at 17.0 s Duration. The `0xd0c` cold-64 samples were 28.7 to 29.1 s across both runs. Gate1 also had a `0xd40` cold-1 sample at 321 ms Duration (vs 497 to 518 ms on `0xd0c` in gate1), and its five warm-1 samples on `0xd40` ran 291 to 303 ms (vs 486 to 510 ms on `0xd0c` in this run). Across both runs, the `0xd40` samples were about 1.7 times faster. That is an observation from one `0xd40` 64-input sample and five `0xd40` warm-1 samples. It is not a measured Graviton 3 benchmark.
- **Cold canary.** Client wall 4.17 to 4.35 s here, against 5.99 to 6.81 s in gate1. Init was 1.82 to 1.91 s on `0xd0c` in this run (1.43 to 1.80 s in gate1).

## Cost

Billed 223,141 ms, which is 2,231 GB-s at 10 GB. At the us-east-1 arm64 rate of $0.0000133334 per GB-s (recalled, not checked against the live price list), that is about **$0.030**. Gate1 and gate32 together are about **$0.051** on the same basis. Request charges for 20 invokes are negligible.

## Gaps and open points

1. **No 32-specific derived keys.** The 32-input figures are per sample, as the owner asked. A `cold_32_duration_ms_max` or `batch32_within_30s_*` would change the `embed-graviton-v1` key set, which needs approval first.
2. **The pre-record was written after the run.** It is not a pre-run record. Its `git_head` is the last commit before the run, and the raw record's CodeSha256 equals the package record, which is what ties the run to the tested code.
3. **Driver on disk.** `scripts/embed_endpoint/graviton_run.py` on disk matched HEAD `f5ab8806b` before this step (`git diff` empty, file sha256 `5c42404355bcad03e10c35e2d8d520ea6eb86bf2`). The raw record's PLAN (20 attempts, the 32-input labels) matches that version. I cannot confirm which driver version the owner executed, only that the record matches it.
4. **The justfile comment** at `embed-probe-run` still says "15 nominal invokes". It is now 20. This was outside the earlier commit scope and was not changed here.
5. **CPU-part effect is observed, not measured as a benchmark.** It rests on one `0xd40` 64-input sample and five `0xd40` warm-1 samples across two runs. Plan 11-08 should not treat it as a Graviton 3 figure.

## Self-check

- The gate32 evidence was rescanned: 0 matches for a 12-digit sequence or `arn:aws`.
- Verification passed: 20 samples; outcome measured with no failed checks; all batch samples truncated fully; warm samples carry no Init; the source pre-record matches the evidence's code and asset hashes; re-normalizing the raw record reproduces the derived block; the probe is recorded absent.
- `GRAVITON GATE SELFTEST OK 62 cases` and `GATE SELFTEST OK` under `just embed-gate-selftest` with `AWS CALLS: 0`.
- The gate1 evidence file is unchanged.
