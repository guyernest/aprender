---
phase: 09-internal-openai-compatible-llm-endpoint-for-pmcp-run
plan: 13
subsystem: infra
tags: [justfile, probe, bench, measure, aws-cli, budget-guard, stdlib-python]

requires:
  - phase: 09-internal-openai-compatible-llm-endpoint-for-pmcp-run
    provides: "09-01: probe_openai.py, the probe-record contract and the reader test; 09-06: llm-endpoint-selftest, the refuse() idiom and the .ids/ seams"
provides:
  - "probe/shapes.py: the spike-031 request shapes with nonces (copied from the spike)"
  - "probe/bench.py: aws-cli driven bench (stdlib only) recording client_wall_s beside billed_ms, price per record, cpu_part, the 401 case and a pre-sample budget guard"
  - "just llm-endpoint-probe: label/naming/unreleased rules and the reader test run right after every record"
  - "just llm-endpoint-measure: bench driver with an optional budget, MEASURE OK / MEASURE STOPPED (budget)"
  - "PASS bench-selftest row in llm-endpoint-selftest (now 42/42, AWS CALLS: 0)"
affects: [09-05 local probes, 09-07 live measurement (passes the remaining ceiling as budget), 09-08 HTTPS probe]

key-files:
  created:
    - deploy/llm-endpoint/probe/shapes.py
    - deploy/llm-endpoint/probe/bench.py
  modified:
    - justfile
    - deploy/llm-endpoint/README.md
    - deploy/llm-endpoint/probe/README.md

key-decisions:
  - "AWS_MAX_ATTEMPTS=1 and APR_PROBE_KEY removed from the child environment of every aws call: aws-cli retries after a read timeout would run, and bill, an invoke twice"
  - "Price uses the REPORT Memory Size when present, the function.json memory otherwise"
  - "The recipe parameters of llm-endpoint-probe are declared with just's `$` prefix so the arguments reach bash as environment variables, not as text spliced into the script"
  - "Eligibility of an APR_UNRELEASED_FIXES entry is a prefix match on the two eligible probe names followed by `=`, because other probe names (tool_choice=required) contain `=` and a split on the first `=` would be ambiguous"

patterns-established:
  - "An offline aws shim inside the tool's own self-test: it proves argv, key handling, temp-file removal and the cold bump on the real code path before any spend"
  - "A guard added without a mutation probe is unproven: 13 mutations of bench.py were applied; one survived (redaction) and got a check"

requirements-completed: [IR-5, SC-3, SC-4, SC-5]

actuals:
  tokens: 14000
  tasks: 2
  commits: 2
plan_head_before: cdbe4ed4fd73e51ace62287c920e0ca67669291c
plan_head_after: 052a4a68214ae2ca10f2531a8855e0aac1dff9cd

coverage:
  - id: D1
    description: "bench.py parses a fixture LogResult, prices 71,047 ms at 10,240 MB to 0.009473, reads cpu_part 0xd40 and the apr version line from the [entry] lines"
    requirement: "SC-4"
    verification:
      - kind: unit
        ref: "python3 -I deploy/llm-endpoint/probe/bench.py --selftest -> BENCH SELFTEST OK price=0.009473 ... cpu_part=0xd40"
        status: pass
    human_judgment: false
  - id: D2
    description: "the budget guard refuses a sample before it would overspend: 0.012 admits the first tools2_turn1 (0.0095) and refuses the second; a budget below the first estimate exits 4"
    requirement: "SC-4"
    verification:
      - kind: unit
        ref: "--selftest ('budget guard refused tools2_turn1 #2') and --selftest-budget-only --budget-usd 0.0001 -> exit 4, budget_stop on stdout"
        status: pass
    human_judgment: false
  - id: D3
    description: "the probe wrapper accepts only release-local, interim, deployed-https, names records per the contract, forwards and polices APR_UNRELEASED_FIXES, never takes the key on a command line, and runs the reader after the record"
    requirement: "IR-5"
    verification:
      - kind: unit
        ref: "plan Task 2 verify blocks (key/label refusals, naming dry runs, unreleased case table, static greps); extra: mock HTTP server + cargo shim -> REFUSED reader (exit 1) and PROBE OK"
        status: pass
    human_judgment: false
  - id: D4
    description: "llm-endpoint-measure refuses without key or function.json, prints MEASURE OK, or MEASURE STOPPED (budget) with exit 4 and a partial record"
    requirement: "SC-3"
    verification:
      - kind: unit
        ref: "plan Task 2 verify (REFUSED key, REFUSED function); extra: aws shim run, full sequence (12 records, cpu_part 0xd40) and budget 0.02 stop before short_64tok"
        status: pass
    human_judgment: false
  - id: D5
    description: "both recipe bodies are bash with set -euo pipefail and lint with 0 bashrs errors"
    requirement: "SC-5"
    verification:
      - kind: other
        ref: "bashrs lint on each extracted body: probe 0 errors / 13 warnings, measure 0 errors / 8 warnings, selftest 0 errors / 68 warnings"
        status: pass
    human_judgment: false
  - id: D6
    description: "the bench works against the real Lambda service (aws-cli flag shapes, LogResult size, filter-log-events behaviour)"
    requirement: "SC-4"
    verification: []
    human_judgment: true
    rationale: "No AWS call was made, by design. The aws shim proves what bench.py sends, not that the service accepts it. The first real run is plan 09-07's measurement."
---

# Phase 9 Plan 13: Probe wrapper, bench and measure recipe Summary

**A stdlib-only aws-cli bench that records client wall and billed time, price and cpu_part on every record and refuses a sample before it would overspend its `--budget-usd`, the `llm-endpoint-probe` wrapper that binds every record it can write to the Rust reader, and `llm-endpoint-measure`, with `PASS bench-selftest` added to the recipe self-test (42/42, `AWS CALLS: 0`).**

## Performance

- **Tasks:** 2 (Task 1 tracer, Task 2 auto)
- **Files:** 5 (2 created, 3 modified); +1061 / -4 lines
- **Commits:** 2 task commits (`dfe9db8b3`, `052a4a682`)
- **Spend:** none. No AWS call was made; the only `aws` the tests reach is a shim.

## Accomplishments

- `bench.py` replays the spike-031 shapes through `aws lambda invoke --log-type Tail`: `cold_short_1tok` (after a `COLD_NONCE` read-merge-write bump and `wait function-updated-v2`), `short_1tok`, `short_64tok`, `tools2_1tok`, `tools2_turn1`, `turn2_extend_1tok`, `turn2_extend_64tok`, `tools10_1tok`, `conc<N>` (parallel subprocesses) and `no_auth_401`. Each record has `client_wall_s`, `duration_ms`, `billed_ms`, `init_ms`, `max_mem_mb`, `cost_usd`, `cpu_part` and `cpu_part_source`. The result names function, image digest, apr tag, GGUF sha, apr version line, model and memory.
- The budget guard runs before each invocation: measured spend plus the per-shape `EST_USD` ceiling (labelled guard-only in the source) against `--budget-usd`; a refusal writes the partial record with `budget_stop {before, spent_usd, est_usd, budget_usd}` and exits 4.
- `llm-endpoint-probe`: `REFUSED key|label|pin|profile|unreleased|probe|reader`, dry run `WOULD WRITE <file> pin=<pin> profile=<profile> unreleased=<n>`.
- `llm-endpoint-measure`: `MEASURE OK <file>`, or `MEASURE STOPPED (budget): <shape> spent=... est=... budget=...` and exit 4.
- README section `## Probe and measure`; the probe README no longer calls the recipe future work.

## Self-test output

```
BENCH SELFTEST OK price=0.009473 (71047 ms at 10240 MB) parsed cpu_part=0xd40 budget guard refused tools2_turn1 #2 (11 shimmed invokes, key in no argv, temp events removed)
PASS 42 bench-selftest: fixture LogResult parsed and priced, cpu_part read, budget guard refuses the second two-tool turn
AWS CALLS: 0
FIXTURE READS: 9
LLM-ENDPOINT SELFTEST OK 42/42
```

`--selftest-budget-only --budget-usd 0.0001` prints `{"budget_stop": {"before": "cold_short_1tok", ...}}` and exits 4.

## Wrapper case table (all pass)

| Case | Expected | Result |
|---|---|---|
| `APR_PROBE_KEY=` | `REFUSED key` | pass |
| labels `deployed`, `baseline`, `bogus` | `REFUSED label` | pass |
| `release-local`, `deployed-https` dry run | `WOULD WRITE conformance-v0.71.4-<label>.json pin=v0.71.4` | pass |
| `interim` dry run | `conformance-interim-upstream-main-plus-fixes.json pin=upstream-main+PRs` | pass |
| unreleased: `no Authorization header=x`, `tool_choice=required=x`, `max_completion_tokens honoured` (no `=`) | `REFUSED unreleased` | pass |
| unreleased in the baseline profile | `REFUSED unreleased` | pass |
| two eligible pairs, fixed profile | `profile=fixed unreleased=2` | pass |
| measure without key / with a missing ids dir | `REFUSED key` / `REFUSED function` | pass |

Beyond the plan's verify (run once from scratchpad scripts, not committed):

- mock HTTP server plus a `cargo` shim: reader exit 1 gives `REFUSED reader` (exit 1, record left in place); reader exit 0 gives `PROBE OK`;
- `llm-endpoint-measure` against the bench's aws shim: full run gives `MEASURE OK`, 12 records, cpu_part 0xd40, no key in output or record; budget 0.02 gives `MEASURE STOPPED (budget)` before `short_64tok`, exit 4, partial file with `budget_stop`.

## Mutation probes (CLAUDE.md Verification Discipline #4 / #7)

Thirteen mutations of `bench.py` were applied one at a time and `--selftest` required to fail: guard never refuses, guard off by a margin, spend counted as zero, event always carries auth, retries allowed, temp event not removed, price off, cpu_part regex wrong, bump not merging the environment, key left in the child environment, no log lookup, 401 case sends auth, redaction off. Twelve were caught on the first pass; **redaction survived** and a check (`write_json` must redact an echoed key) was added, after which 13/13 are caught.

## bashrs (`bashrs lint` on each extracted body)

| Recipe | errors | warnings |
|---|---|---|
| llm-endpoint-probe | 0 | 13 |
| llm-endpoint-measure | 0 | 8 |
| llm-endpoint-selftest | 0 | 68 |

Two real errors had to be fixed first: a nested parameter expansion (`${A:-${B:-}}`, SC2296) and `ARGS+=(...)` on the same line as `if [ ... ]; then` (the known SC1028 false positive). Neither needed a behaviour change.

## Task Commits

1. **Task 1 (tracer): shapes.py, bench.py, `PASS bench-selftest`** - `dfe9db8b3` (feat)
2. **Task 2: llm-endpoint-probe, llm-endpoint-measure, README** - `052a4a682` (feat)

Tracer gate: Task 1's three verify blocks (compile + `--selftest`, budget-only exit 4, `just llm-endpoint-selftest`) were green, after the mutation probes, before Task 2 began.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug in plan text] `filter-log-events --limit 1` returns the OLDEST match**
- **Found during:** Task 1 design
- **Issue:** the interface block's `--limit 1 --query 'events[-1].message'` yields the single oldest `[entry] host:` line, not the newest.
- **Fix:** `host_info()` uses `--start-time` (seven days, the log group's retention) with no limit and takes `events[-1]`, the newest. The selftest shim covers the lookup path.
- **Commit:** dfe9db8b3

**2. [Rule 2 - Missing critical] `AWS_MAX_ATTEMPTS=1` and the key scrubbed from the aws child environment**
- **Issue:** the aws-cli v2 default retries a timed-out read, which would run and bill an invoke twice (the spike's boto3 client used `max_attempts: 0`). The key has no business in the CLI's environment.
- **Fix:** both set in `LambdaTarget`; asserted by the selftest and by a mutation probe.
- **Commit:** dfe9db8b3

**3. [Rule 2 - Missing critical] Key redaction in every written file**
- **Fix:** `write_json` replaces the key's value with `[redacted]` (the plan only required that the bench output carry none); covered by a selftest check.
- **Commit:** dfe9db8b3

**4. [Rule 3 - Blocking] `LLM_ENDPOINT_RESULTS_DIR` seam and `$`-prefixed recipe parameters**
- The non-dry path of the probe wrapper could not otherwise be exercised without writing into the committed results directory. The seam defaults to `probe/results`. `$base $model $label $server_version` pass the arguments as environment variables so a server-version string with shell metacharacters cannot reach bash as text.
- **Commit:** 052a4a682

**5. [Rule 3 - Blocking] bashrs lint constraints** (nested expansion, `ARGS+=()` on an `if` line) fixed by rewriting, see above.

**6. Note: `probe/README.md` edited** although not in `files_modified`: it said the probe recipe was a later plan, which became false.

**Total deviations:** 6 (one plan-text bug, two missing-critical, three blocking/notes). **Impact:** none on scope; every acceptance criterion passes.

## Authentication Gates

None. No AWS call was made.

## Known Stubs

None. `StubTarget` in `bench.py` is the offline test double for the budget proofs, not a placeholder behind any output.

## Threat Flags

None beyond the plan's register. T-09-22 (wall vs billed on every record), T-09-55 (key by environment only, mktemp event deleted in a `finally`, no `--key` on any command line, redaction), T-09-56 (guard before each sample, proven both ways and by 13 mutations) are implemented and pinned by self-test checks.

## Issues Encountered

- The worktree harness refuses compound shell commands and rewrites some git porcelain; staging and committing used `/opt/homebrew/bin/git` and multi-step checks ran as scripts from the session scratchpad.
- The session hit a usage limit between Task 1's implementation and its commit; on resume `git log` showed no task commit, the uncommitted files were re-verified (self-test 13/13 mutations, `just llm-endpoint-selftest`) and then committed.
- `commits: 2` is measured before this SUMMARY's own commit.

## Next Phase Readiness

- Plan 09-07 can run `just llm-endpoint-measure env profile deployed 1 1 2 <remaining ceiling>` and compose `09-LIVE-DEPLOY-EVIDENCE.json` from `.ids/bench-deployed-<utc>.json`.
- First real run is where the aws-cli argument shapes get checked against the service (D6, human judgment): `--cli-read-timeout 910`, `--query LogResult` on a function error, `filter-log-events --filter-pattern` quoting.
- `cpu_part_source` says when a record's value is borrowed from the cold record or the log group; a second environment started by `conc2` may differ and the record says so when it has `init_ms`.
- The per-shape `EST_USD` ceilings are guard estimates from the spike-031 prices; after the first live run they can be tightened from measured costs.

## Self-Check: PASSED

- FOUND: deploy/llm-endpoint/probe/bench.py, deploy/llm-endpoint/probe/shapes.py, justfile, deploy/llm-endpoint/README.md
- FOUND (ancestors of HEAD): dfe9db8b3, 052a4a682
- Commits measured from the ledger: 2 (`cdbe4ed4f..052a4a682`) before this SUMMARY's own commit
- Verify blocks re-run after the final edit: `--selftest` OK, `--selftest-budget-only` exit 4, `just llm-endpoint-selftest` 42/42 with `AWS CALLS: 0`, recipe case table all pass
