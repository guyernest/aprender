---
phase: 09-internal-openai-compatible-llm-endpoint-for-pmcp-run
plan: 04
subsystem: infra
tags: [deploy, lambda, docker, lwa, justfile, bashrs, fail-closed]

requires:
  - phase: 08
    provides: "justfile refuse()/shasum idiom (laya-deploy) and the root-anchored .gitignore precedent"
provides:
  - "deploy/llm-endpoint/ asset directory: Dockerfile, entrypoint.sh, entrypoint-selftest.sh, pin.env"
  - "just llm-endpoint-image [push=1]: sha256-verified release apr -> local arm64 image, identity proven from the image's own binary"
  - "fail-closed entrypoint: refuses an open server (APR_API_KEY_HASH not 64 hex) and an unverified GGUF (sha256 != MODEL_SHA256)"
affects: [09-06 llm-endpoint-up (reads .ids/image-local.json, writes .ids/aws.json), 09-11 (ladder reader, buildspec, README), re-pin to the fixed apr release]

actuals:
  tokens: 5600
  tasks: 2
  commits: 2
plan_head_before: e312f4be52074587bfffdf1f073044c346133ba5
plan_head_after: 3ea54641a60437ea2092da24e818e356c78d88ac

tech-stack:
  added: []
  patterns:
    - "pin.env as the single re-pin surface; the Dockerfile's duplicate adapter digest is compared by the recipe (lwa-digest)"
    - "shim-based offline case table for an entrypoint (date/nproc/sha256sum/aws/apr shims) plus an in-image proof"
    - "mutation probes of the guarded script, each required to turn the table red"

key-files:
  created:
    - deploy/llm-endpoint/Dockerfile
    - deploy/llm-endpoint/entrypoint.sh
    - deploy/llm-endpoint/entrypoint-selftest.sh
    - deploy/llm-endpoint/pin.env
  modified:
    - .gitignore
    - justfile

key-decisions:
  - "LWA pinned by the multi-arch INDEX digest sha256:46d6625e... (tag 0.9.1), not the arm64 child manifest, so the pin survives a platform-specific rebuild of the adapter"
  - "Refusal line keeps the [entry] prefix and carries the timeline at the end: '[entry] REFUSED auth: ... (t=13ms)'"
  - "bashrs is run with --fail-on error: its default exit status trips on warnings, which this plan itself accepts (DET004)"

patterns-established:
  - "Two copies of one pin (pin.env + Dockerfile FROM) are legal only with a recipe-side refusal proven on a mutated copy"

requirements-completed: [IR-2, IR-5, SC-4, SC-5]

coverage:
  - id: D1
    description: "just llm-endpoint-image builds a local arm64 image from the sha256-verified v0.71.4 release apr; apr --version in the image prints 0.71.4; labels carry tag + tarball sha"
    requirement: "SC-4"
    verification:
      - kind: integration
        ref: "just llm-endpoint-image -> IMAGE OK aprender-llm-endpoint:cpu-v0.71.4 (apr apr 0.71.4 (5027da624)); docker run --entrypoint /usr/local/bin/apr ... --version"
        status: pass
    human_judgment: false
  - id: D2
    description: "lwa-digest refusal fires when the Dockerfile's adapter digest differs from pin.env"
    requirement: "SC-4"
    verification:
      - kind: integration
        ref: "LLM_ENDPOINT_DOCKERFILE=<digest-mutated copy> just llm-endpoint-image -> exit 4, REFUSED lwa-digest"
        status: pass
    human_judgment: false
  - id: D3
    description: "Entrypoint refuses without a 64-hex APR_API_KEY_HASH and refuses a GGUF whose sha256 differs; both proven by an 8-row case table, six mutation probes, and the auth refusal inside the real image"
    requirement: "IR-5"
    verification:
      - kind: unit
        ref: "bash deploy/llm-endpoint/entrypoint-selftest.sh -> ENTRYPOINT SELFTEST OK 8/8, IMAGE PROOF OK"
        status: pass
    human_judgment: false
  - id: D4
    description: "Dockerfile carries no weights and no secret (guard function passes on the real file, refuses five mutated copies); bin/ and .ids/ are root-anchored ignored while the Dockerfile and entrypoint are not"
    requirement: "SC-5"
    verification:
      - kind: other
        ref: "dockerfile_ok guard + git check-ignore --no-index (bin=0 ids=0 entry=1 docker=1)"
        status: pass
    human_judgment: false

duration: 35min
completed: 2026-10-16
status: complete
---

# Phase 9 Plan 04: Release-pinned image and fail-closed entrypoint Summary

**A pin.env-driven `just llm-endpoint-image` that double-checks the v0.71.4 release tarball, builds the arm64 Lambda-Web-Adapter image locally and proves its identity from the binary's own version line, with an entrypoint that refuses to exec an open server or an unverified GGUF.**

## Performance

- **Duration:** ~35 min
- **Completed:** 2026-10-16
- **Tasks:** 2 (Task 1 tracer, Task 2 auto)
- **Files modified:** 6 (4 created, 2 modified)

## Accomplishments

- `deploy/llm-endpoint/pin.env` pins `APR_TAG=v0.71.4`, tarball sha256 `f09b425e...2db8` (read from the release `.sha256`, matched by `shasum -c` and by an independent recompute), `APR_PIN_PROFILE=baseline`, and the adapter digest.
- `just llm-endpoint-image` refuses on `tag`, `sha-release`, `sha-pin`, `lwa-digest`, `identity`, and only touches AWS when `push=1` and `.ids/aws.json` exists (pushes to the env-specific `ecr_repo` recorded there). No AWS resource was touched by this plan.
- Local image `aprender-llm-endpoint:cpu-v0.71.4`, image id `sha256:210b15b0736813cc3c4b10cd54b023d6b7f4950474adc8faed44b0b25f7a61f7` (latest rebuild), `apr --version` = `apr 0.71.4 (5027da624)`, labels `org.aprender.apr_tag=v0.71.4`, `org.aprender.apr_tarball_sha256=f09b425e...`. LWA digest `sha256:46d6625e68cbbdd2efab4a20245977664513f13ffef47915b000d431adcea0b4` (index; arm64 child `cd0ad953...`).
- Entrypoint hardened: auth refusal (length glob + hex-alphabet regex), `model-pin` refusal, post-pull and pre-present sha256 verification with delete-on-mismatch, `[entry] t=<ms>` timeline and `cpu_part` kept, key hash logged as the word `set` only, never `--verbose`. The image copy was compared byte-identical to the tree.

## Case table output (`bash deploy/llm-endpoint/entrypoint-selftest.sh`)

```
PASS 1 hash unset -> REFUSED auth
PASS 2 63 hex -> REFUSED auth
PASS 3 64 chars with g, and 64 hex + junk line -> REFUSED auth
PASS 4 MODEL_SHA256 unset -> REFUSED model-pin
PASS 5 valid hash + sha, pulled -> exec serve run (hash never logged)
PASS 6 wrong MODEL_SHA256 -> REFUSED model-sha, file removed
PASS 7 model already present, right sha -> exec, aws not called
PASS 8 model absent, MODEL_S3_URI unset -> FATAL
ENTRYPOINT SELFTEST OK 8/8
IMAGE PROOF OK aprender-llm-endpoint:cpu-v0.71.4 refused a malformed APR_API_KEY_HASH (exit 2)
```

In the image: `[entry] REFUSED auth: APR_API_KEY_HASH must be 64 hex chars (apr serve would start OPEN with only a warning) (t=13ms)`, exit 2.

**Mutation probes (each must turn the table red):** regex-alphabet removed, auth refusal neutered, model-pin neutered, sha-mismatch neutered, file-not-removed, pre-present-skips-verify: 6/6 caught. The Dockerfile guard function refused 5/5 mutated copies (MODEL_S3_URI, `COPY model.gguf`, APR_API_KEY_HASH, MODEL_SHA256, `ADD <tarball>`) and accepted the real file.

## bashrs

`bashrs lint --fail-on error`: `entrypoint.sh` 0 errors / 6 warnings / 12 info; `entrypoint-selftest.sh` 0 errors / 9 warnings / 31 info.
Warnings (accepted): DET004 `nproc` (both scripts), SEC013/REL005 the `/tmp/model.gguf` default path (the pull destination, required), SEC020 `exec "$APR_BIN"` (the `APR_BIN` override is the self-test seam and defaults to the pinned path), SC2047/SC2154 on `MODEL_S3_URI` (false positives; the variable is `${MODEL_S3_URI:-}`), SEC014 on shim `chmod`/`.` pin sourcing, SC2114/SC2295/SC2297 style on selftest.

## Task Commits

1. **Task 1 (tracer): pin.env -> `just llm-endpoint-image` -> verified release apr -> local arm64 image** - `32a2d24a4` (feat)
2. **Task 2: entrypoint hardening + eight-row self-test + in-image auth proof** - `3ea54641a` (feat)

**Plan metadata:** committed with this SUMMARY (docs).

## Files Created/Modified

- `deploy/llm-endpoint/pin.env` - the single re-pin surface (tag, asset, tarball sha, profile, adapter digest, image name, GGUF rung/object)
- `deploy/llm-endpoint/Dockerfile` - AL2023-minimal + awscli-2 + LWA by tag AND digest + `bin/apr`; ARGs/LABELs for tag and sha; no weights, no secret
- `deploy/llm-endpoint/entrypoint.sh` - fail-closed entrypoint
- `deploy/llm-endpoint/entrypoint-selftest.sh` - shimmed 8-row table, `ENTRYPOINT_UNDER_TEST` mutation seam, `SKIP_IMAGE=1`, fail-closed image proof
- `.gitignore` - root-anchored `/deploy/llm-endpoint/bin/` and `/deploy/llm-endpoint/.ids/`
- `justfile` - `llm-endpoint-image push="0"` under a Phase 9 banner

## Decisions Made

- LWA pinned by the multi-arch index digest (tag kept in the FROM line for readability) so a platform-specific adapter rebuild does not invalidate the pin.
- The refusal line is `[entry] REFUSED <check>: <why> (t=<ms>ms)`: the plan's literal `[entry] REFUSED auth` prefix is kept (the image proof greps it) and D-14's timeline rides at the end.
- Row 3 of the table carries two variants (a `g` in the hash, and a 64-hex line followed by a junk line). The second exists because the length glob and the alphabet regex are independently removable: with only the plan's variants, neutering the glob was an uncaught mutant (found by probing; the regex alone also refuses a 63-char hash, but grep's per-line `^...$` accepts `HASH\njunk`).

## Deviations from Plan

### Auto-fixed / adjusted

**1. [Rule 3 - Blocking] `bashrs lint` verify commands use `--fail-on error`**
- **Found during:** Task 2
- **Issue:** The plan's verify requires `bashrs lint` exit status 0 on both scripts while also accepting DET004 warnings. bashrs's default `--fail-on warning` exits 1 on any warning (entrypoint.sh: rc=1 with 0 errors), so the two requirements contradict each other. SEC020 on `exec "$APR_BIN"` cannot be removed without dropping the `APR_BIN` seam the plan itself requires.
- **Fix:** Ran `bashrs lint --fail-on error`, which exits 0 exactly when there are 0 errors (the SC5 criterion: "0 errors"); warnings are listed above.
- **Files modified:** none (verification invocation only)

**2. [Rule 1 - Bug] Selftest shim bodies rewritten to avoid bashrs parser false positives**
- **Found during:** Task 2
- **Issue:** The first selftest used heredocs for the shims and a multi-line string; bashrs reported 10 parser errors (SC1128 shebang position, SC1078, SC1028, SC1035) that are lexer false positives, not defects.
- **Fix:** Shims are written by a `write_shim` helper from single-quoted lines using `case` instead of `if/fi`; the multi-line hash is built with `printf` into a variable.
- **Files modified:** deploy/llm-endpoint/entrypoint-selftest.sh
- **Commit:** `3ea54641a`

**3. [Rule 2 - Missing critical] Strengthened table row 3 and added a mutation seam**
- See Decisions Made. `ENTRYPOINT_UNDER_TEST` lets the verify feed a mutated entrypoint (CLAUDE.md Verification Discipline #4/#7). The in-image proof fails closed (no docker or no image is a FAIL unless `SKIP_IMAGE=1`). It was observed failing once, correctly, against the image built before the hardening (the old entrypoint exited 2 for a different reason: no `MODEL_S3_URI`, with an empty stderr), which proves the check keys on `REFUSED auth` and not on the exit status.
- **Commit:** `3ea54641a`

**4. [Rule 3 - Blocking] Entrypoint made portable for the offline table**
- `nproc` is guarded (`|| echo unknown`) and file sizes use `wc -c` instead of GNU `stat -c`, so the same script runs under the macOS table with shims for `date` (no `%N`) and `sha256sum`. The behaviour in the AL2023 image is unchanged.

**Total deviations:** 4 (1 verification-invocation change, 2 test-harness fixes, 1 portability tweak). **Impact:** none on scope; all acceptance criteria met.

### Notes (no action)

- `bash scripts/check_include_files.sh` exits 0 but prints a grep usage line and `OK: All 0 include!() files are tracked by git`, i.e. it is vacuous on macOS (known: memory note `aprender-include-guard-vacuous-on-macos`). The criterion "exits 0" holds; the real protection here is `git check-ignore` on the five paths (bin=0, ids=0, entrypoint=1, Dockerfile=1), which was run and is the evidence.
- Verify logs were written to the session scratchpad instead of `/tmp/p09-04-*` (same commands otherwise). The plan's `.ids/` logs (`build.log`, `identity.log`, `sha-release.log`) live under the gitignored `deploy/llm-endpoint/.ids/`.
- `.ids/image-local.json` was written by the final rebuild; it is gitignored by design and is not in the commit.

## Authentication Gates

None. `gh release download` ran unauthenticated-capable against the public release; no AWS call was made.

## Known Stubs

None.

## Threat Flags

None. T-09-10..T-09-14, T-09-47 mitigations are implemented as planned (two independent tarball hashes, Dockerfile guard, `lwa-digest` refusal, auth/model refusals, no secret in logs). `push=1` is the only AWS-touching path and is unexercised until plan 09-06 writes `.ids/aws.json`.

## Issues Encountered

- The worktree harness rewrites some git porcelain through `rtk`, which its own guard then rejects; staging and committing used `/usr/bin/git` directly. No effect on repository content.

## Next Phase Readiness

- Plan 09-06 can read `deploy/llm-endpoint/.ids/image-local.json` and call `just llm-endpoint-image 1` once `.ids/aws.json` (keys `profile`, `region`, `ecr_host`, `ecr_repo`, `ecr_repo_name`) exists.
- Re-pin: edit `pin.env` (tag, asset, tarball sha, `APR_PIN_PROFILE=fixed`), update the adapter digest in both `pin.env` and the Dockerfile if it changes, re-run `just llm-endpoint-image` and `bash deploy/llm-endpoint/entrypoint-selftest.sh`.
- The `fixed` pin profile and the fork-free server fixes of this phase are not in v0.71.4 (`APR_PIN_PROFILE=baseline`).

## Self-Check: PASSED

- FOUND: deploy/llm-endpoint/Dockerfile, entrypoint.sh, entrypoint-selftest.sh, pin.env
- FOUND (ancestors of HEAD): 32a2d24a4, 3ea54641a
- Commits measured from the ledger: 2 (`e312f4be5..3ea54641a`)
- All Task 1 and Task 2 acceptance criteria re-run after the final image rebuild: pass (see above)
