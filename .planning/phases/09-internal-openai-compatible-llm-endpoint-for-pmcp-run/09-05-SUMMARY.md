---
phase: 09-internal-openai-compatible-llm-endpoint-for-pmcp-run
plan: 05
subsystem: infra
tags: [conformance, probe, interim, re-probe, apr-serve, docker, auth-gate]

requires:
  - phase: 09-internal-openai-compatible-llm-endpoint-for-pmcp-run
    provides: "09-01 probe + record contract + reader test; 09-02/09-03 upstream-only fix commits on pushed branches; 09-04 image aprender-llm-endpoint:cpu-v0.71.4 and pin.env; 09-13 `just llm-endpoint-probe`"
provides:
  - "conformance-v0.71.4-release-local.json: the release pin re-probed from the real image, baseline profile, the spike-029 four crosses reproduced"
  - "auth-table-v0.71.4-release-local.md: the release serves the GGUF path OPEN (every row 200, even with APR_API_KEY_HASH set)"
  - "conformance-interim-upstream-main-plus-fixes.json: all three fixes on one build, fixed profile, zero crosses, label interim"
  - "auth-table-interim.md: 401 / 401 / 200 on POST, 401 on /health and /v1/models"
  - "a kept interim worktree + release binary for plan 09-12 (identity recorded below)"
  - "the per-release-tag fix-containment table that plan 09-08's re-pin predicate reads"
affects: [09-07, 09-08, 09-09, 09-12]

plan_head_before: 1f456918013c98a0bc37ccb0ab8b08b091dc435a
plan_head_after: 043a2e2c1bdf7455f7662eafa1985945f618e85a
actuals:
  tokens: 16800
  tasks: 2
  commits: 2

tech-stack:
  added: []
  patterns:
    - "an interim build is proved equal to the verified commits by git patch-id --stable plus an exact changed-file set, not by trusting the cherry-pick"
    - "a container memory cap is only evidence for the cgroup; read the VM's real RAM beside it (docker info MemTotal, docker stats limit)"

key-files:
  created:
    - deploy/llm-endpoint/probe/results/conformance-v0.71.4-release-local.json
    - deploy/llm-endpoint/probe/results/auth-table-v0.71.4-release-local.md
    - deploy/llm-endpoint/probe/results/conformance-interim-upstream-main-plus-fixes.json
    - deploy/llm-endpoint/probe/results/auth-table-interim.md
  modified: []

key-decisions:
  - "The interim record's server_version is the binary's own --version line plus the words `interim upstream-main+PRs`, because the reader refuses an interim record whose version has no `+`, `-g<sha>` or `interim`, and the bare line `apr 0.71.3 (bd85d728f)` also reads as the released v0.71.3"
  - "The D-02 claim 'admitted under 10 GB' is recorded as PARTIAL, not proven: the cap was set in the cgroup but the Docker VM has about 4 GB"

requirements-completed: [IR-1, IR-2, SC-2, SC-3]

coverage:
  - id: D1
    description: "v0.71.4 release image, run under docker --memory 10g with the ladder-pinned GGUF, answers /health in 2 s and reproduces the spike-029 baseline (the same four crosses); recorded as conformance-v0.71.4-release-local.json and reader-checked"
    requirement: IR-1
    verification:
      - kind: integration
        ref: "just llm-endpoint-probe http://127.0.0.1:8029/v1 Qwen3.5-0.8B-Q4_K_M release-local 'apr 0.71.4 (5027da624)' -> PROBE OK conformance-v0.71.4-release-local.json pin=v0.71.4 profile=baseline"
        status: pass
      - kind: unit
        ref: "cargo test -p aprender-serve --lib api::llm_endpoint_conformance_tests::conformance_record_matches_contract -> 1 passed"
        status: pass
    human_judgment: false
  - id: D2
    description: "The release image serves the GGUF path with no auth: every row of auth_probe.sh --expect open is 200 although APR_API_KEY_HASH was set"
    requirement: SC-3
    verification:
      - kind: integration
        ref: "bash deploy/llm-endpoint/probe/auth_probe.sh 8029 $KEY --expect open -> PASS: open"
        status: pass
    human_judgment: false
  - id: D3
    description: "D-02: the 0.8B is admitted under a 10 GB memory limit"
    requirement: IR-2
    verification:
      - kind: other
        ref: "docker run --memory 10g: in-container cgroup memory.max=10737418240; docker info MemTotal=4106346496; docker stats limit 3.824GiB; cgroup memory.peak 1368408064; the container started and served. The cap was not physically backed."
        status: unknown
    human_judgment: true
    rationale: "The Docker Desktop VM has about 4.0 GB of RAM, so the 10 GB cap is a cgroup setting, not a proof that 10 GB of memory admitted the model. The measured footprint (peak 1.27 GiB) fits in the VM, which says the model is small, not that the Lambda 10,240 MB tier admits it. The user must decide whether this is enough for D-15 or whether Docker's memory should be raised."
  - id: D4
    description: "SC3 pre-merge: the interim build (upstream/main + content + alias + auth) answers 401 without the bearer, 401 with a wrong bearer, 200 with the right one and 401 on /health; the fixed-profile probe has zero crosses"
    requirement: SC-3
    verification:
      - kind: integration
        ref: "bash deploy/llm-endpoint/probe/auth_probe.sh 8030 $KEY --expect gated -> PASS: gated"
        status: pass
      - kind: integration
        ref: "APR_PIN_PROFILE=fixed just llm-endpoint-probe http://127.0.0.1:8030/v1 Qwen3.5-0.8B-Q4_K_M interim ... -> PROBE OK conformance-interim-upstream-main-plus-fixes.json pin=upstream-main+PRs profile=fixed"
        status: pass
    human_judgment: false
  - id: D5
    description: "The interim build is exactly the three verified commits: three commits over an upstream/main ancestor, the four expected files, patch-ids equal to the pushed branch commits"
    requirement: IR-2
    verification:
      - kind: other
        ref: "bash scratchpad t2_ident.sh -> 'interim = exactly the three verified commits'"
        status: pass
    human_judgment: false
  - id: D6
    description: "SC2 server half: the content fix lets the pmcp-agent turn-2 body through (tool result -> final answer passes on the interim build; 422 on the release)"
    requirement: SC-2
    verification:
      - kind: integration
        ref: "release-local: `tool result -> final answer` 422; interim: 200, finish=stop, content='The weather in Paris is currently light rain at a temperature of 17C.'"
        status: pass
    human_judgment: false

duration: 19 min
completed: 2026-10-16
status: complete
---

# Phase 9 Plan 05: Release re-probe and interim fixed-profile proof Summary

**The v0.71.4 release image reproduces the spike-029 four crosses and serves the GGUF path open, while a build of upstream/main plus the three draft-PR commits answers 401/401/200 with `/health` gated and a probe table with zero crosses; the 10 GB admission claim is only partly proven because Docker's VM has about 4 GB.**

## Performance

- **Duration:** 19 min (about 15:14Z to 15:33Z; the start time was reconstructed from file times, not captured)
- **Tasks:** 2 (Task 1 tracer, Task 2 auto)
- **Files created:** 4 (two conformance records, two auth tables)
- **Commits:** 2 task commits (`fbb49d905`, `043a2e2c1`) plus this plan-metadata commit
- **AWS calls, PRs opened, pushes:** none

## Task Commits

1. **Task 1: release pin re-probed end to end** - `fbb49d905` (test)
2. **Task 2: interim build, 401 table, fixed-profile probe** - `043a2e2c1` (test)

## Task 1 evidence: v0.71.4 release image, local

- **GGUF:** `/Users/guy/.cache/huggingface/hub/blobs/de/de9bcb3f1b16e6e33ab42b3851e80945d4153631418c8494a77bfa46b3ac70ec` (the `Qwen3.5-0.8B-Q4_K_M.gguf` snapshot symlink resolves to it), 532,517,120 bytes, sha256 `bd258782e35f7f458f8aced1adc053e6e92e89bc735ba3be89d38a06121dc517`. `ladder_pin.py contracts/model-capability-ladder-v1.yaml qwen35-0.8b-q4km` printed the same value; the entrypoint's own `MODEL_SHA256` check passed too.
- **Image / version:** `docker run --rm --entrypoint /usr/local/bin/apr aprender-llm-endpoint:cpu-v0.71.4 --version` -> `apr 0.71.4 (5027da624)`. Image id `sha256:210b15b0736813cc3c4b10cd54b023d6b7f4950474adc8faed44b0b25f7a61f7`, arm64.
- **Run:** `docker run -d --memory 10g --cpus 6 -p 127.0.0.1:8029:8080` (loopback only), GGUF mounted read-only, `APR_API_KEY_HASH` = sha256 of a throwaway key (never recorded).
- **Seconds to health:** 2 s from `docker run` to `GET /health` 200 (the loop checks once a second, so 1 to 2 s).
- **`[entry]` lines** (from `docker logs`):

```
[entry] t=4ms start pid=1 apr=apr 0.71.4 (5027da624)
[entry] t=13ms host: nproc=10 threads=6 cpu_part=0x000 model_name=
[entry] t=16ms mem: total_kb=4010104 cgroup_max=10737418240
[entry] t=19ms APR_API_KEY_HASH=set
[entry] t=23ms model already present bytes=532517120
[entry] t=1174ms model sha256 OK bytes=532517120
[entry] t=1175ms apr serve start
Model ready: Qwen3.5 hybrid, 24 layers resident on the CPU, declared context 262144 tokens
chat template: Qwen3NoThink (thinking off)
```

  `cpu_part=0x000` and an empty `model_name`: on the Mac the container's `/proc/cpuinfo` has no `CPU part`/`model name` lines the awk can read, so this is not a Graviton measurement and says nothing about the Lambda CPU.
- **Auth table (`--expect open`):** PASS. POST with no header, with a wrong key and with the right key were all 200; `GET /health` 200; `GET /v1/models` 200. The container was started with a valid `APR_API_KEY_HASH` and still served open: v0.71.4 has no server-side gate on the GGUF path, which is why the D-04 PR exists.
- **Probe (baseline profile):** the four crosses are exactly the spike's: `tool result -> final answer` (422), `tool_choice=required`, `max_completion_tokens honoured` (222 tokens, finish=stop), `error envelope` (422). `foreign model id`, `no Authorization header` and `cpu_part in /health` read `ℹ`. No difference from the baseline set, so nothing goes to D-15 as a pin deviation (and no `UNEXPECTED-*` file exists). The v0.71.4 `response_format` note: the `extra OpenAI fields tolerated` probe (it sends `{"type":"text"}`) still passes.
- **Reader test:** `conformance_record_matches_contract ... ok`, `1 passed; 0 failed` (run by the wrapper, and again after I added `notes`).
- **Container:** `llm-ep-release` removed; `docker ps` shows none.

### D-02 memory: what was available versus what was claimed (NOT PROVEN as stated)

| Measure | Value | Source |
|---|---|---|
| Container limit requested | `--memory 10g` | docker run |
| In-container cgroup limit | 10,737,418,240 B (10 GiB) | `[entry]` line `cgroup_max=` |
| Docker Desktop VM RAM | 4,106,346,496 B (3.824 GiB) | `docker info --format '{{.MemTotal}}'` |
| Container's `/proc/meminfo` MemTotal | 4,010,104 kB | `[entry]` line `total_kb=` |
| Limit shown by `docker stats` | 3.824 GiB | `docker stats --no-stream` (it clamps to the VM) |
| Peak cgroup memory over the whole probe | 1,368,408,064 B (1.27 GiB) | `memory.peak`, sampled every 2 s through the probe |
| apr process peak RSS | 1,020,896 kB (0.97 GiB, VmHWM) | `/proc/1/status` after the probe |
| Idle RSS after load | 829 MiB | `docker stats` at health |
| Native interim server RSS (Task 2) | 1,373,120 kB, then 1,281,088 kB | `ps -o rss` |

The `--memory 10g` flag was accepted and the container started and served, but a limit above the VM's RAM is a bookkeeping number: the VM could not have supplied 10 GiB. What the run shows is that the 0.8B's real working set is about 1.3 GiB at peak, far below both the VM's 3.8 GiB and a 10 GiB cap. What it does not show is that the Lambda 10,240 MB tier admits the model through the v0.71.4 host-RAM guard, because the guard here read a cgroup limit that nothing backed, and I did not read the guard's code or see a line in the log saying it ran. I therefore did not record a green on "admitted under 10 GB". The claim is PARTIAL: it cannot be upgraded without either raising Docker's memory (Settings, Resources, to at least 12 GB) and re-running Task 1, or taking the answer from the live 10 GB function in plan 09-07. This is raised for D-15.

## Task 2 evidence: interim build

- **INTERIM_BASE** (freshly fetched `upstream/main`): `d7ac5f7829ad1d260f72620a55d29dbbc6a8ba04` (`fix(guard): check_row_pr_write_set.sh feeds grep -q without a pipe`). It had moved since 09-02/09-03 cut their branches (`5b1fd7b08`, `abf66f908`); all three cherry-picks still applied with no conflict.
- **Three commits** (cherry-pick -x onto a detached worktree; patch-ids from `git show <sha> | git patch-id --stable`):

| Fix | Source SHA (pushed branch) | Applied SHA | patch-id (equal) | PR |
|---|---|---|---|---|
| content | `de62ce561768fa05010ceb4d84517803c659b3ea` (`fix/chat-message-content-optional`) | `6b3fe115e335ce874d48c55de24b604f3d3616cd` | `6b36e54a8210b7b6cac65639640b422f2c6a2281` | paiml/aprender#5411 |
| alias | `31db0c39a9466faaec209181c031f11a0852c361` (`fix/max-completion-tokens-alias`) | `2380f6cb53d87ef4e749a79f7963043e8dcf7582` | `b04f50dc080f2436f6c38c5ab28d4c8cd73f00d7` | paiml/aprender#5412 |
| auth | `f4ad4c06da14df1992aabf88667bd64c6b1f9015` (`fix/gguf-router-auth-gate`) | `bd85d728f5354f7b42f2ee85114ab14639c4e12a` | `7192be970b94caf73502a8a7a3d0ead4b7d372bd` | paiml/aprender#5451 |

- **Identity proof:** `git rev-list --count BASE..HEAD` = 3; BASE is an ancestor of `upstream/main` (it is `upstream/main`); `git diff --name-only BASE..HEAD` is exactly `crates/apr-cli/src/commands/serve/server.rs`, `crates/aprender-serve/src/api/mod_create_demo.rs`, `.../tests/chat_message_content_pmat801.rs`, `.../tests/max_completion_tokens_alias.rs`; the sorted source and applied patch-id lists are `cmp`-equal. Output: `interim = exactly the three verified commits`.
- **Build:** `CARGO_INCREMENTAL=0 cargo build --release --bin apr` in the worktree's own target dir (its `.cargo/config.toml` is gitignored and absent, so the build did not go to the main checkout's target). Last lines: `Compiling aprender v0.71.3 (...aprender-interim)` / `Finished release profile [optimized] target(s) in 2m 39s` / `build rc=0`. The plan estimated 10 to 25 minutes; dependencies were mostly cached.
- **Binary identity:** `target/release/apr --version` -> `apr 0.71.3 (bd85d728f)`; worktree `git rev-parse --short HEAD` -> `bd85d728f`. Equal. Note the Cargo version is `0.71.3` on upstream/main even though the tag `v0.71.4` exists; the record therefore adds the words `interim upstream-main+PRs` to `server_version` (see Deviations). I did not use `scripts/apr_bin.sh` in the worktree; the `--version` sha check is the identity proof.
- **Serve:** `APR_API_KEY=<throwaway> apr serve run <gguf> --host 127.0.0.1 --port 8030 --backend cpu`; `lsof` showed the listener on `127.0.0.1:8030` only. `/health` answered 401 after 1 s (the auth commit is present). `serve.log`: `warning: Qwen3.5-0.8B is not CRUX-certified. Known issue #4030: ...`, `chat template: Qwen3NoThink (thinking off)`, `gpu-layers: requested=none resolved=0 total=24 (backend=cpu)`.
- **Auth table (`--expect gated`):** PASS. no header 401, wrong key 401, right key 200, `GET /health` 401, `GET /v1/models` 401; body `{"error":"unauthorized","message":"Missing or invalid Authorization: Bearer <key> header"}`.
- **Fixed-profile probe:** zero crosses. `tool result -> final answer` passes (200, `finish=stop`, content "The weather in Paris is currently light rain at a temperature of 17 C"), `max_completion_tokens honoured` passes (`completion_tokens=12 finish=length`), `no Authorization header` passes (401). `tool_choice=none` also passes (no leak on this build, so it is not a limitation). KNOWN LIMITATION: `tool_choice=required`, `error envelope {error:{message,type}}`, `foreign model id`, `cpu_part in /health` (all four are in `known_limitations`; `unreleased_at_pin` is empty). `GET /health` carries no `cpu_part` key on this build.
- **Reader test** over baseline + release-local + interim: `1 passed; 0 failed`.
- **Server stopped:** the process was killed; `lsof` shows no listener on 8030 and `pgrep -fl 'apr serve'` is empty.

### Fix containment per upstream release tag (input to plan 09-08's re-pin predicate)

Read-only: `gh pr view <n> --repo paiml/aprender` (all three are OPEN and draft, no merge commit) and `git ls-remote --tags upstream` (newest release tag is `v0.71.4`; `gh release list` agrees, plus the rolling `nightly` pre-release). `git merge-base --is-ancestor <merge commit> <tag>` could not be run for any fix because none has a merge commit.

| Tag | auth (#5451, the ONLY gating one: D-04, D-12) | content (#5411) | alias (#5412) |
|---|---|---|---|
| v0.71.4 (newest, 2026-10-14) | PR not merged | PR not merged | PR not merged |
| v0.71.3 | PR not merged | PR not merged | PR not merged |
| v0.71.2, v0.71.1, v0.71.0, v0.70.x | PR not merged | PR not merged | PR not merged |

No upstream release tag newer than v0.71.4 exists at execution time. This table is the whole input: nothing released contains any fix, so a deployed pin on any tag currently has `unreleased_at_pin` entries for the content and alias fixes and no auth gate at all. The interim record is the all-three-fixes proof and is NOT a statement that a pin needs all three; only the D-04 auth merge gates the ingress and the end-to-end turn.

## Hand-off to plan 09-12 and cleanup rule

KEPT on disk (do not delete before 09-12):

- Worktree: `/private/tmp/claude-501/-Users-guy-Development-machine-learning-aprender/d0054c91-c40d-4435-b1d4-b59c1eea11f9/scratchpad/aprender-interim` (also written to `/tmp/p09-05-interim-wt`; it is a detached worktree of the main repo, listed by `git worktree list`)
- Upstream SHA: `d7ac5f7829ad1d260f72620a55d29dbbc6a8ba04`; cherry-picked: `6b3fe115e335ce874d48c55de24b604f3d3616cd`, `2380f6cb53d87ef4e749a79f7963043e8dcf7582`, `bd85d728f5354f7b42f2ee85114ab14639c4e12a` (worktree HEAD is the last)
- Binary: `.../scratchpad/aprender-interim/target/release/apr`, 68.8 MB, sha256 `a633be3a663591c37b3fb37988967aa8083d62b00ec5522376addcbeaf3a25fd`, `apr --version` -> `apr 0.71.3 (bd85d728f)`
- The target directory is 2.2 GB.
- The throwaway probe key is in `<scratchpad>/probe.key` (mode 600). It is not in any committed file; both records and both tables were checked for it and for `Bearer ` and contain neither. The scratchpad is session-scoped: if it is gone, rebuild from the recorded upstream SHA plus the three cherry-picks.

Cleanup: plan 09-12 Task 2 removes `<worktree>/target` when its matrix is done. If 09-12 is skipped or paused beyond the session, run:

`rm -rf /private/tmp/claude-501/-Users-guy-Development-machine-learning-aprender/d0054c91-c40d-4435-b1d4-b59c1eea11f9/scratchpad/aprender-interim/target`

and `git worktree remove --force` the worktree. Nothing deletes it automatically, because 09-12 serves that exact binary.

## Deviations from Plan

**1. [Rule 1 - Plan vs reader] Interim `server_version` carries extra words.**
- **Found during:** Task 2, after the build.
- **Issue:** the plan says record the `--version` line. `apr 0.71.3 (bd85d728f)` contains no `+`, `-g` or `interim`, which `is_non_release_server_version` in `llm_endpoint_conformance.rs` (line 112) requires, so the reader would reject the record. The line also reads as the released v0.71.3.
- **Fix:** `server_version` = `apr 0.71.3 (bd85d728f) interim upstream-main+PRs` (the `--version` line verbatim, then the marker). The reader and the plan's verify (`not a bare release`) pass.
- **Files:** `conformance-interim-upstream-main-plus-fixes.json`. **Commit:** `043a2e2c1`.

**2. [Plan inaccuracy] The verify greps `correct key> | 200 |`, but `auth_probe.sh` prints `Bearer <the key> | 200 |`.**
- **Found during:** Task 2 verify block 2. The literal grep fails; the right-key 200 row is present under its real label (`grep -q 'Bearer <the key> | 200 |'` matches) and `--expect gated` asserted it. I did not edit `auth_probe.sh` (a 09-01 artifact already used by the committed release table) to satisfy the string. 09-01's table label and this plan's verify disagree; a later edit should align them.

**3. [Plan inaccuracy] `foreign model id` reads KNOWN LIMITATION in the fixed profile, not `ℹ`.** The plan expected `ℹ`; the 09-01 probe converts it in the fixed profile and puts it in `known_limitations`, which the contract allows. No ✗ is involved.

**4. [Plan inaccuracy] `[entry]` `cpu_part` is read as `0x000` with an empty `model_name`**, not nothing: Docker on the Mac exposes no readable `CPU part` line in the way the awk expects. Recorded as asked; not a Graviton fact.

**5. [Environment] The 10 GB cap was not physically backed** (see the memory table). Not routed as a blocker because no acceptance criterion requires physical backing, but the D-02 admission claim is PARTIAL and is for D-15.

**6. [Process] upstream/main moved** (`d7ac5f782`) past the SHAs 09-02/09-03 recorded; the three commits applied cleanly, so the plan's "conflict is a finding" branch did not fire.

**7. [Process] No `scripts/apr_bin.sh`.** I used the plan's fallback (the `--version` sha equals the worktree HEAD) instead of sourcing it in the worktree.

**8. [Process] rtk hook.** `git commit` output was rewritten (`ok 2 files changed`); I verified commits with `/opt/homebrew/bin/git log` and `rev-parse`. The reader's libtest summary lines were readable.

**Total deviations:** 3 plan/reader mismatches fixed or documented (1, 2, 3), 1 environment finding (5), 4 process notes. **Impact:** none of the four records says more than the runs showed.

## Observations for later plans

- D-09-02-2 (does the apr-cli `ChatCompletionRequest` need the alias?) is answered for this build: on the served CPU GGUF route `max_completion_tokens` is honoured (`completion_tokens=12 finish=length`) with only the aprender-serve alias commit, so no follow-up on that type is needed for this deployment.
- The release image's `/health` answered 200 in 2 s because `/health` is open on v0.71.4; with the D-04 gate it is 401, and 09-08 depends on the Lambda Web Adapter counting 401 as ready (09-03 key decision).
- Probe timings on this laptop (tools turns 41 s and 91 s on the release container; 24 s and 60 s native) are context only (D-14).
- Upstream's `apr serve` now lists a new `POST /v1/chat/prompt-ids` route that the release does not; the probe does not touch it.

## Issues Encountered

None that blocked the plan. The `gsd-tools state.*` writes could not run (below).

## State updates

- `roadmap.update-plan-progress 09`: run after this SUMMARY was written (result in the completion report).
- `state.advance-plan`, `state.record-metric`, `state.record-session`: FAILED on the pre-existing Phase 4 file (`04-VERIFICATION.md`, `status: gaps_acknowledged`, outside the tool's closed set). Per the dispatch I did not edit or re-key it. STATE.md is unchanged by this plan; the orchestrator reconciles it. `state.update-progress` answered "phase scope is unscoped".
- `requirements.mark-complete IR-1 IR-2 SC-2 SC-3`: phase-local IDs, not in REQUIREMENTS.md (the same result 09-02 reported).

## User Setup Required

None. For a physically backed 10 GB proof: raise Docker Desktop's memory to at least 12 GB and re-run Task 1 (`just llm-endpoint-probe` etc.), or take the answer from the live function in plan 09-07.

## Known Stubs

None.

## Threat Flags

None. Ports 8029 and 8030 were bound to loopback only (`docker -p 127.0.0.1:...`, `--host 127.0.0.1`); the HF GGUF was sha-checked before any server started; the throwaway key is in no committed file; no package was installed.

## Self-Check: PASSED

- Files exist: both conformance records and both auth tables under `deploy/llm-endpoint/probe/results/`.
- Commits `fbb49d905` and `043a2e2c1` are ancestors of HEAD.
- Reader test green over baseline + release-local + interim; container `llm-ep-release` removed; no server listening on 8029 or 8030.
- Acceptance: all criteria met except the literal `correct key>` grep (Deviation 2) and the physically-backed 10 GB claim (Deviation 5, recorded as PARTIAL).

---
*Phase: 09-internal-openai-compatible-llm-endpoint-for-pmcp-run*
*Completed: 2026-10-16*
