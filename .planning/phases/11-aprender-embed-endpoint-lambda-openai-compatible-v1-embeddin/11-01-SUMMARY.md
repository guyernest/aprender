---
phase: 11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin
plan: 01
subsystem: infra
tags: [lambda, graviton, arm64, provided-al2023, cdk, ingress-scan, measurement, rust, lambda-runtime, setfit]

# Dependency graph
requires:
  - phase: 10-antisemitism-v1-decide-server-for-experiment-coach
    provides: the owner-run deploy-and-measure pattern (refuse ladder, DRY_RUN and PREFLIGHT modes, raw record then evidence) that this plan mirrors
provides:
  - A standalone arm64 measurement probe that calls the production SetFitBert engine (load once, identity hash at start, no_grad micro-batched encode)
  - A deterministic Lambda package (zip plus package record whose code_sha256_b64 is the base64 SHA-256 of the zip)
  - A no-ingress CDK stack (one arm64 function, no URL, API, load balancer, permission or event source) for the probe and the endpoint variant
  - An allowlist-first ingress scanner with a planted-violation selftest
  - The embed-graviton-v1 normalizer with a 49-case selftest, and the owner-run driver for the live measurement
  - Six embed-probe recipes and an offline gate that prints AWS CALLS: 0 only when its recorder is empty
affects: [11-02 (register decision), 11-04 (owner-run measurement), 11-06 (product tracer), 11-08 (identity hashing at start), 11-10 (endpoint stack), 11-12 (live readback)]

# Actuals (#2632) — chars/4 over the realized added diff of the plan's three commits
actuals:
  tokens: 27890
  tasks: 2
  commits: 3

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Standalone measurement crate with its own [workspace] table, lock seeded from the root Cargo.lock and gated for containment"
    - "Per-kind response validity (embedding kinds vs tokenize kinds) instead of one rule for every sample"
    - "Deterministic zip: sorted entries, fixed 1980 timestamps, explicit modes, no directory entries, symlink refusal"
    - "Allowlist-first template scan, denylist with named messages, recursive principal scan"
    - "Offline gate with aws and npx shims whose recorder is checked empty, plus a negative control that proves the recorder can fire"

key-files:
  created:
    - deploy/embed-endpoint/graviton-probe/Cargo.toml
    - deploy/embed-endpoint/graviton-probe/src/main.rs
    - deploy/embed-endpoint/graviton-probe/corpus.txt
    - deploy-extensions/lib/embed-endpoint-stack.ts
    - scripts/embed_endpoint/glibc_floor.py
    - scripts/embed_endpoint/package_zip.py
    - scripts/embed_endpoint/ingress_scan.py
    - scripts/embed_endpoint/graviton_gate.py
    - scripts/embed_endpoint/graviton_run.py
  modified:
    - deploy-extensions/bin/app.ts
    - justfile
    - .gitignore

key-decisions:
  - "The probe runs the handler on a blocking thread in --local mode too, as the Lambda path does: the grad tape is thread-local, and a load on the main thread counted 72 load-time tape entries as encode-time growth"
  - "The probe builds responses with Value::from and Map, not json!, because json! expands to unwrap at each call site and the repository's clippy policy bans unwrap"
  - "The package record names the zip by basename and carries code_sha256_b64 (base64 of the zip's SHA-256), the form Lambda reports as CodeSha256"
  - "The owner-run driver is a separate file (graviton_run.py) so graviton_gate.py stays free of AWS calls and fully selftestable"
  - "The stack context defaults to training; the default synth is byte-identical to the pre-change template"
  - "The gate's npx shim passes cdk synth and the ts-node app runner that cdk.json names; every other npx call is recorded and refused"
  - "The probe is a standalone project outside the workspace: no workspace member, no register edit, no change to root Cargo.toml, Cargo.lock or crates/"

patterns-established:
  - "Pattern 1: every AWS-touching recipe starts with the same refuse ladder (refuse <check> <message>, exit 2) and never reaches the AWS CLI on a refused input"
  - "Pattern 2: the evidence key names are asserted by the normalizer selftest, because plans 11-04 and 11-08 read them by name"
  - "Pattern 3: a raw record and an evidence file are written only after a 12-digit guard passes; console text is scrubbed of account ids and ARNs"

requirements-completed: [R6, R4, R1]

coverage:
  - id: D1
    description: "Standalone Graviton probe builds natively and runs the production engine call on the pinned weights: 8 inputs of 700 words give truncated_count 8, consumed_positions 2048, tape_len_after 0, dim 384; a 5-word input is not truncated; an empty model directory returns an error body with exit 0"
    requirement: "R6"
    verification:
      - kind: other
        ref: "deploy/embed-endpoint/graviton-probe --local with the pinned model (long, short, empty-directory events); host run"
        status: pass
    human_judgment: false
  - id: D2
    description: "Probe lock containment: its lock links 222 packages, every (name, version, source, checksum) present in the root Cargo.lock; no workspace file changed"
    requirement: "R6"
    verification:
      - kind: other
        ref: "lock containment check in the plan's verify block (tomllib comparison of both locks)"
        status: pass
    human_judgment: false
  - id: D3
    description: "arm64 probe asset: bootstrap at glibc floor 2.34, pinned tokenizer bytes, model subset without safetensors; deterministic package zip with a record whose code_sha256_b64 is the base64 SHA-256 of the zip"
    requirement: "R6"
    verification:
      - kind: unit
        ref: "scripts/embed_endpoint/package_zip.py selftest (12 cases)"
        status: pass
      - kind: other
        ref: "just embed-probe-asset; package_zip.py verify deploy-extensions/assets/embed-probe.zip deploy/embed-endpoint/.ids/package-embed-probe.json"
        status: pass
    human_judgment: false
  - id: D4
    description: "No-ingress stack: the probe and the endpoint variant each synthesize to one arm64 provided.al2023 function at 10,240 MB with a URL-free, permission-free template; the allowlist-first scanner passes both"
    requirement: "R4"
    verification:
      - kind: integration
        ref: "just embed-probe-synth dev; endpoint synth with temporary package copies (removed afterwards); ingress_scan.py on both templates"
        status: pass
      - kind: unit
        ref: "scripts/embed_endpoint/ingress_scan.py --selftest (10 cases: 8 must-fail, 2 must-pass)"
        status: pass
    human_judgment: false
  - id: D5
    description: "embed-graviton-v1 normalizer: per-kind response validity, proven-cold from Init Duration, 30-second derivations, fixed key names, 12-digit refusal"
    requirement: "R6"
    verification:
      - kind: unit
        ref: "scripts/embed_endpoint/graviton_gate.py selftest (49 cases)"
        status: pass
    human_judgment: false
  - id: D6
    description: "Owner-run recipes refuse before any AWS call on a bad environment, a mis-positioned argument, a memory outside 128 to 10240, a missing asset, a package zip that does not verify, and both mode flags; DRY_RUN synthesizes and scans offline"
    requirement: "R4"
    verification:
      - kind: integration
        ref: "just embed-gate-selftest (8 refusal cases, DRY_RUN, scanner, package and normalizer selftests, AWS CALLS: 0, negative control)"
        status: pass
    human_judgment: false
  - id: D7
    description: "Default training stack synthesizes byte-identically before and after the app.ts stack context change"
    verification:
      - kind: integration
        ref: "just synth-training dev, then cmp against the pre-edit template"
        status: pass
    human_judgment: false
  - id: D8
    description: "Live Graviton number (cold and warm latency, memory, CPU part, provisioned-concurrency need) is not measured by this plan"
    verification: []
    human_judgment: true
    rationale: "The live measurement is an owner-run AWS step in plan 11-04; this plan produces the harness and its offline proof only, and reports no Graviton figure."
---

# Phase 11 Plan 01: Graviton probe, no-ingress stack, scanner and owner-run recipes Summary

**Standalone arm64 probe that calls the production SetFit engine, a no-ingress CDK stack gated by an allowlist-first scanner, and an offline-proven owner-run recipe set that makes zero AWS calls**

## Performance

- **Duration:** unrecorded (the start timestamp was not captured before work began)
- **Start head (plan_head_before):** d49ff5e70e999ed9de5161b989f21403fd01125a
- **Close-out head (plan_head_after):** 7ba19353c33872c86ee73a3d5285d21d03991d97
- **Tasks:** 2 of 2 (plus one probe fix commit found by clippy)
- **Commits:** 3 (measured with `git rev-list --count d49ff5e70..HEAD`)
- **Files:** 12 files created or changed across the three commits

## Accomplishments

- A standalone probe (its own `[workspace]`, outside the aprender workspace, no register edit) that makes the production engine call the way the endpoint will: `SetFitBert::from_pretrained_dir` once at start, the sha256 of `full_model.apr` at start, and `encode_texts` in micro-batches inside one `no_grad` closure on the encoding thread.
- The probe reports load, identity, init and encode timings, truncation facts, tape length, the CPU part from `/proc/cpuinfo`, peak RSS from `VmHWM`, and the rayon pool width the encoding thread actually used.
- A deterministic arm64 package: `deploy-extensions/assets/embed-probe.zip`, with a record whose `code_sha256_b64` is the base64 SHA-256 of the zip, the form Lambda reports as `CodeSha256`.
- A no-ingress CDK stack (`EmbedEndpointStack`) with variant `probe` or `endpoint`: one arm64 `provided.al2023` function, an explicit log group, `retryAttempts: 0`, and no function URL, API, load balancer, permission or event source. The endpoint variant also publishes its name and ARN to SSM.
- An allowlist-first ingress scanner with a denylist, a recursive principal scan, and a planted-violation selftest.
- The `embed-graviton-v1` normalizer with a 49-case selftest. It derives every number from the samples and refuses any output containing a 12-digit sequence.
- The owner-run driver `graviton_run.py`, which refuses on the live account before measuring and writes only the raw record.
- Six `embed-probe-*` recipes and `embed-gate-selftest`. The gate scrubs AWS credentials, puts `aws` and `npx` shims on PATH, runs the refusal cases and the DRY_RUN path, and prints `AWS CALLS: 0` only when the recorder is empty.

## Task Commits

1. **Task 1: standalone probe, arm64 asset recipe and lock containment** - `f0e2f9345` (feat)
   - Fix found during verification - `7ba19353c` (fix): the probe's `json!` calls expanded to `unwrap`, which the repository's clippy policy bans. Rebuilt with `Value::from` and `Map`; `cargo clippy --all-targets -- -D warnings` now passes.
2. **Task 2: no-ingress stack, scanner, normalizer, owner-run driver and recipes** - `d7d20beb3` (feat)
3. **Plan metadata:** this SUMMARY commit (docs).

## Measurements (host, NOT Graviton)

The local run used the pinned weights (`full_model.apr`, sha256 `980becb4b853b5043b8c87f001eb3d4b5bc3a12d724b8ca2c04423bce7c59831`) on the macOS arm64 development host, release build, `available_parallelism` 14. The `/proc` fields are null on macOS, so no CPU part is reported. These are host numbers and say nothing about Lambda.

| Case | Result |
|---|---|
| 8 inputs of 700 words, micro_batch 8 | rows 8, dim 384, truncated_count 8, consumed_positions 2048, tape_len_after 0, first-row L2 norm 1.0 |
| same event, timings | load_ms 102.367, identity_ms 170.447, init_ms 272.814, tokenize_ms 4.509, encode_ms 1060.721, rayon_pool_threads 14 |
| 1 input of 5 words | truncated_count 0, consumed_positions 7, tape_len_after 0, encode_ms 15.108 |
| empty model directory (negative control) | error body `model load failed: ...`, exit 0 |

The first local run reported `tape_len_after` 72 (see Deviations). After the fix the value is 0 on every run.

## Lock containment

The probe lock (seeded as a copy of the root `Cargo.lock`, then pruned by cargo) links 222 packages. Every `(name, version, source, checksum)` record is present in the root `Cargo.lock`; the count of records absent from the root lock is 0. The root `Cargo.toml`, `Cargo.lock`, `crates/` and `scripts/duplicate_bin_names_allowlist.txt` are unchanged across the plan.

Resolved features, from `cargo tree -f '{p}|{f}'` on the probe:

- `aprender-core v0.69.3`: `conformance-fixtures, default, parallel, rayon, setfit` (the probe enables `conformance-fixtures` only to read truncation and mask facts; the product does not)
- `aprender-compute v0.69.3`: `default`
- `tokenizers v0.23.1`: `fancy-regex`
- `rayon v1.12.0`: no features listed

## Asset and package

- `deploy-extensions/assets/embed-probe/`: `bootstrap` 5,924,784 bytes, ELF ARM aarch64, glibc floor 2.34 (the `provided.al2023` ceiling is 2.34); `model/` holds `full_model.apr` (90,865,988 bytes), `tokenizer.json` (its sha256 equals `PINNED_TOKENIZER_SHA256`), `config.json`, `modules.json`, `full_manifest.json` and `1_Pooling/config.json`. `model.safetensors` is not copied. Unzipped size 93 MB against the 250 MB limit.
- `deploy-extensions/assets/embed-probe.zip`: 85,763,437 bytes; `zip_sha256` `649942aab41dc2ba82740af215d7332ee156f899f224fdf9b79a438b1f0d1234`; `code_sha256_b64` `ZJlCqrQdwrqCdAryFdczLuFW+JnyJP35t5pDix8NEjQ=`; `bootstrap_sha256` `b63bdfcf9f8fc0bc334e811254c34adb620e49363669adc075d8b5e9d7cda183`; `apr_sha256` `980becb4b853b5043b8c87f001eb3d4b5bc3a12d724b8ca2c04423bce7c59831`.
- The package record is at `deploy/embed-endpoint/.ids/package-embed-probe.json`, which is gitignored and not committed.
- Values from the first build (before the clippy fix) are superseded: that zip was 85,764,710 bytes with a different `code_sha256_b64`.

## Scanner and normalizer

- **Allowed resource types (6):** `AWS::Lambda::Function`, `AWS::Lambda::EventInvokeConfig`, `AWS::IAM::Role`, `AWS::Logs::LogGroup`, `AWS::SSM::Parameter`, `AWS::CDK::Metadata`. The probe template uses five of them (no SSM parameters); the endpoint template uses all six. The allowlist was derived from these templates, so the denylist and principal scan are the independent guards.
- **Denylist:** `Lambda::Url`, `Lambda::Permission`, `ApiGateway*`, `ApiGatewayV2*`, `ElasticLoadBalancing*`, `ElasticLoadBalancingV2*`, `CloudFront*`, `VpcLattice*`.
- **Ingress scanner selftest:** 10 cases, 8 must-fail (URL, `Principal: *` permission, named-principal permission, API Gateway service-principal permission, ApiGatewayV2 API, ELBv2 load balancer, nested `Principal: {AWS: *}` three levels deep, unknown type) and 2 must-pass (the clean template, and a role trusting `lambda.amazonaws.com`).
- **Normalizer selftest:** 49 cases, including the 5.1 s, 29.9 s and 30.1 s boundaries for both 30-second derivations, the per-kind response rule, the tokenize samples, the exact key names, rounding to three decimals, and the 12-digit refusal.
- **Package zip selftest:** 12 cases (byte-identical rebuilds, sorted entries, fixed timestamps, executable bit round trip, flipped byte named `zip_sha256`, edited record field, symlink and missing-bootstrap refusals).

## Training template

`just synth-training dev` was run before the `app.ts` edit (control, saved outside the repo) and after it. `cmp` returned 0: the default training template is byte-identical.

## No AWS calls

- The offline gate's `aws` recorder is empty, and it prints `AWS CALLS: 0 (aws shim recorder empty; npx passes through only cdk synth; credentials scrubbed)`. A negative control then proves the recorder fires on one deliberate call.
- Outside the gate, one `graviton_run.py run` was started with a nonexistent profile to exercise the refuse path. The AWS CLI failed at profile lookup, so no AWS API request was sent. The probe was never deployed.
- The owner-run recipes (`embed-probe-run`, `embed-probe-diff`, `embed-probe-destroy`, and the PREFLIGHT path of `embed-probe-deploy`) were not executed live. That is by design; plan 11-04 runs them.

## Deviations from Plan

**1. [Rule 1 - Bug] Local tape length was 72, not 0.** The model load on the main thread recorded autograd tape entries on that thread, and the handler ran there too, so `tape_len_after` counted load-time entries as encode-time growth. Fix: `--local` loads on the main thread and runs the event on a blocking thread, as the Lambda path does. Re-verified: 0 on the long and short runs. Commit `f0e2f9345`.

**2. [Rule 2 - Missing critical: repository policy] `json!` used `unwrap` at 35 call sites.** `serde_json::json!` expands to `to_value(..).unwrap()`, which the repository's clippy `disallowed-methods` policy rejects. Fix: the probe builds its responses with `Value::from` and `Map`; behavior unchanged, tests and local runs re-verified. Commit `7ba19353c`.

**3. [Rule 3 - Blocking] Package selftest design.** Two builds placed in different directories produced records differing in `zip_file`, a basename field, so "identical records" failed for a reason unrelated to the builder. Fix: the second build lands in a directory with the same basename. The product behavior is unchanged.

**4. [Rule 3 - Blocking] A literal 12-digit string in the normalizer's leak-guard case.** It would have failed the plan's own leak scan. Fix: the string is built at runtime.

**5. [Naming] `SetFitMiniLm` in the plan is `SetFitBert` in tree.** `from_pretrained_dir`, `encode_texts` and `tokenize` are methods of `SetFitBert`. The probe calls those methods as the plan describes.

**6. [Scope] The live driver is a new file.** `scripts/embed_endpoint/graviton_run.py` is not in the plan's file list. It holds the AWS calls, so that `graviton_gate.py` stays AWS-free and selftestable. The recipes call it.

**7. [Scope] The gate's npx shim also passes `ts-node`.** `cdk.json` names `npx ts-node` as the app runner, and `cdk synth` spawns it, so the gate could not synthesize without passing it. Every other npx call is recorded and refused, and the required `AWS CALLS: 0` sentence is printed as specified. The claim is precisely: no AWS call, npx passes only `cdk synth` and the app runner it spawns.

**8. [Verification scope] The 12-digit scan over whole files flags pre-existing content.** At HEAD, `justfile` already has nine lines matching `\b\d{12}\b` (existing hashes and a twelve-digit placeholder account id). The scan was run on every new file and on every added line of `justfile` and `.gitignore`: zero matches. Pre-existing content was not touched.

**9. [Environment] Temporary paths.** The plan's `/tmp` scratch paths were replaced with the session scratchpad.

**10. [Verification scope] Endpoint variant verified by synth only.** `assets/embed/` and `assets/embed.zip` were created as temporary copies of the probe package to synthesize the endpoint variant, then removed. No endpoint package exists in the tree (plan 11-10 builds it).

**Total deviations:** 2 auto-fixed bugs or policy fixes (items 1 and 2), 2 blocking test or literal fixes (items 3 and 4), and 6 scope or naming notes (items 5 to 10). **Impact on plan:** None of the deviations changes the plan's outcome. Items 1 and 2 changed probe code and were re-verified; items 3 and 4 changed only test code.

## Issues Encountered

- **The rtk hook rewrites `npx` in Bash-tool calls and drops `--context` arguments.** While debugging, several direct `npx cdk synth` calls silently produced the training stack, and an earlier `cdk.out` left a stale probe template that briefly looked like a successful synth. The justfile recipes are not rewritten: they honor the context (verified with `env=prod` through `just synth-training prod`, and with `rtk proxy npx` directly). Guidance for later plans: run synths through `just` or `rtk proxy npx`, and clear `cdk.out` before trusting a template.
- **The CodeSha256 assumption is partly checked.** CDK's staged copy of the zip (`deploy-extensions/cdk.out/asset.<hash>.zip`) was byte-identical to the source zip on the first build (sha256 equal). The live `CodeSha256` of the deployed function has not been observed; it is plan 11-04's check, and a mismatch there is a decision for the owner, not something to work around.
- **No Graviton number exists yet.** The local figures above are host numbers only.

## User Setup Required

None for this plan. Plan 11-04 needs owner AWS credentials (profile `ze-kasher-dev`) to run the deploy and measurement recipes.

## Next Phase Readiness

- **Plan 11-02 (register decision):** the probe is outside the workspace, so the FALSIFY-MONO-011 register is untouched and that decision is still open.
- **Plan 11-04 (owner-run):** run `just embed-probe-synth dev` and `just embed-probe-deploy dev ze-kasher-dev 10240` with `PREFLIGHT=1` first, then `just embed-probe-run dev ze-kasher-dev <label> 10240`. The driver refuses if the live `CodeSha256` differs from `ZJlCqrQdwrqCdAryFdczLuFW+JnyJP35t5pDix8NEjQ=`, so a rebuild of the zip requires a fresh deploy. Then normalize the raw record with `graviton_gate.py normalize`, and destroy the probe.
- **Requirements:** `requirements-completed` copies the plan frontmatter verbatim (R6, R4, R1). Only the probe, scanner and no-ingress stack parts of R1 are in place; the endpoint itself is plan 11-06 onward. R6's live half is plan 11-04.
- **Known Stubs:** none.
- **Threat Flags:** none beyond the plan's threat model. The owner-run driver makes AWS calls, refuses before measuring, scrubs error text, and writes no ARN, account id or role name.

---
*Phase: 11-aprender-embed-endpoint-lambda-openai-compatible-v1-embeddin*
*Completed: 2026-10-23*

## Self-Check: PASSED

- Created files present on disk: the probe manifest, source and corpus; the stack; the four Python scripts; the package zip and its record (11 checked).
- Commits present on the branch: `f0e2f9345`, `7ba19353c`, `d7d20beb3`.
- The current staged CDK copy (`cdk.out/asset.af39a57c…zip`) is byte-identical to `deploy-extensions/assets/embed-probe.zip`. The first build's staged copy was a stale leftover and has been removed.
- The workspace (`Cargo.toml`, `Cargo.lock`, `crates/`, the bin allowlist) is unchanged across the plan.
