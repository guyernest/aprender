# LLM endpoint `variant` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let several sizes of the in-account LLM endpoint be deployed at the same time under their own names, prove that one name's deploy and teardown leave the others untouched, and deploy the 2B as `aprender-llm-endpoint-dev-2b` with a measured smoke test.

**Architecture:** One sourced bash library derives every resource name, the default ids directory and the default pin file from `(variant, env, account)`; the nine `just llm-endpoint-*` recipes that act on a deployment call it instead of carrying literals, and every reader of a record refuses when the recorded name is not the derived one. A read-only `readback` recipe plus a comparison script turn "untouched" into a measured claim. Live steps are gated and come last.

**Tech Stack:** `just` recipes (bash, `set -euo pipefail`), a sourced option-neutral bash library, `python3 -I` one-liners and one script, the existing offline self-test with its `aws` shim, `bashrs lint`, AWS CLI v2 (Lambda, IAM, ECR, S3, Logs, CodeBuild).

**Spec:** `docs/superpowers/specs/2026-10-18-llm-endpoint-variant-design.md` (its §3 table, §6 case table and §4.5/§4.6 sequences are the values below).

## Global Constraints

- Variant rule, verbatim from the spec: regex `^[a-z0-9]([a-z0-9-]{0,12}[a-z0-9])?$`; empty means the default. Environment stays `dev` or `prod`. A bad variant is refused with class `variant` before any AWS call.
- Names, with `S=<env>-<variant>` when a variant is given and `S=<env>` otherwise: function, IAM role and ECR repository `aprender-llm-endpoint-S`; log group `/aws/lambda/aprender-llm-endpoint-S`; bucket `aprender-llm-endpoint-weights-<account>-S`; CodeBuild project `aprender-llm-endpoint-weights-S`; CodeBuild role `aprender-llm-endpoint-S-codebuild`.
- Default records: ids directory `deploy/llm-endpoint/.ids` for the empty variant, `deploy/llm-endpoint/.ids/S` otherwise; pin file `deploy/llm-endpoint/pin.env` for the empty variant, `deploy/llm-endpoint/pin-<variant>.env` otherwise. `LLM_ENDPOINT_IDS_DIR` and `LLM_ENDPOINT_PIN_FILE` override both, as today.
- Every tagged resource adds `variant=<variant>` (empty string for the default) beside `project=aprender-llm-endpoint`, `phase=09`, `env=<env>`.
- Memory 10,240 MB, ephemeral 10,240 MB, timeout 900 s, arm64, `RAYON_NUM_THREADS` 6 and the read-back refusal are not touched.
- No literal `aprender-llm-endpoint-$ENV` or `aprender-llm-endpoint-dev` remains in any of the nine recipe bodies after Task 4 (the self-test proves it with `just --show`).
- A sourced library is option-neutral: no `set` at file scope, failure by return status (`scripts/check_sourced_libs_option_neutral.sh` is the rule; it globs only `scripts/*.sh`, so the self-test lints the library itself).
- Live steps (Task 7) run only after the operator's go for that task. The 2B name `aprender-llm-endpoint-dev-2b` is the one pmcp.run's own plan writes (ruled 2026-10-18: the environment comes before the variant), so no name confirmation is awaited. Invocation spend on the 2B is capped at $3 (`budget=3`). Nothing in Tasks 1 to 6 touches AWS: the self-test's `aws` shim is first on PATH and refuses in `SHIM_MODE=refuse`.
- Every self-test row reads its status after a redirect, never through a pipe (Verification Discipline rule 1), and every new guard pattern ships both polarities (rule 7).
- Commit messages: `type(llm-endpoint): subject`, body in prose, ending with the attribution line the session carries.

## Review Focus

1. A variant that is valid by the regex but collides with another derived IAM role: `codebuild-2b` and `codebuild` must derive roles equal to no other variant's function or CodeBuild role. Pinned by Task 1's collision rows.
2. A teardown invoked with one variant while the ids directory holds another name's records must refuse before any delete call; a reader given a pre-change record (no `name` key) with the empty variant must still work. Pinned by Task 2's rows V5/V6 and Task 4's rows V10/V11.
3. `image push=1` with a variant must push to that variant's repository and write `image-pushed.json` in that variant's ids directory, never in `.ids/`. Pinned by Task 3's row V8.
4. The default invocation (`variant` omitted) must produce byte-identical receipts to today: every existing self-test row keeps passing unchanged after each task. Pinned by the full self-test run in every task's completion command.
5. The read-back comparison must fail on a changed field and name it, so the isolation evidence cannot pass vacuously. Pinned by Task 5's unit test `test_one_changed_field_fails_and_names_it`.

---

### Task 1: The name library and its case table

**Files:**
- Create: `deploy/llm-endpoint/lib/names.sh`
- Modify: `justfile` (`llm-endpoint-selftest`, after the shim setup and before row 1: two new rows V1, V2)
- Test: `deploy/llm-endpoint/lib/names.sh --self-test` (the case table is the test), plus `bashrs lint deploy/llm-endpoint/lib/names.sh`

**Interfaces:**
- Consumes: nothing.
- Produces, when sourced (`. deploy/llm-endpoint/lib/names.sh || exit 1`):
  - `llm_endpoint_variant_ok VARIANT` → return 0 if empty or matching the rule, 1 otherwise.
  - `llm_endpoint_names VARIANT ENV [ACCOUNT]` → return 0 and set `SUFFIX`, `FN`, `REPO_NAME`, `LOG_GROUP`, `CB_PROJECT`, `CB_ROLE`, `IDS_DEFAULT`, `PIN_DEFAULT`, `TAG_VARIANT` (the string `variant=<variant>`), and `BUCKET` (set only when ACCOUNT is given, else empty); return 1 and set `LLM_ENDPOINT_NAMES_ERR` to a one-line reason otherwise (bad variant, or ENV not `dev`/`prod`). Paths in `IDS_DEFAULT`/`PIN_DEFAULT` are relative to the repo root (`deploy/llm-endpoint/...`).
  - Executed (not sourced) with `--self-test`: prints one `PASS`/`FAIL` line per row and exits 1 on any FAIL, 0 otherwise.

- [ ] **Step 1: Write the case table first, as the file's `--self-test` body, calling the two functions that do not exist yet**

Rows (each a `row <label> <expected rc> <command>` line):
- accepted variants: `2b`, `4b`, `2b-q4km`, `x`, `a1b2c3d4e5f6g7`, `` (empty)
- refused variants: `2B`, `2_b`, `-2b`, `2b-`, `a1b2c3d4e5f6g7h`, `2b.q4`, `2b dev`, `aprender-llm-endpoint-2b`
- refused env: `llm_endpoint_names 2b staging` → rc 1
- derived for `2b dev 816069129666`: `FN=aprender-llm-endpoint-dev-2b`, `BUCKET=aprender-llm-endpoint-weights-816069129666-dev-2b`, `CB_PROJECT=aprender-llm-endpoint-weights-dev-2b`, `CB_ROLE=aprender-llm-endpoint-dev-2b-codebuild`, `LOG_GROUP=/aws/lambda/aprender-llm-endpoint-dev-2b`, `IDS_DEFAULT=deploy/llm-endpoint/.ids/dev-2b`, `PIN_DEFAULT=deploy/llm-endpoint/pin-2b.env`, `TAG_VARIANT=variant=2b`
- derived for `"" dev`: `FN=aprender-llm-endpoint-dev`, `IDS_DEFAULT=deploy/llm-endpoint/.ids`, `PIN_DEFAULT=deploy/llm-endpoint/pin.env`, `CB_ROLE=aprender-llm-endpoint-dev-codebuild`, `TAG_VARIANT=variant=`, `BUCKET` empty when no account is given
- collision rows: the set `{FN, CB_ROLE}` for `codebuild-2b dev` is disjoint from the set for `2b dev` and from the set for `"" dev`; likewise `codebuild dev` against `"" dev`
- longest legal bucket: `a1b2c3d4e5f6g7 prod 816069129666` → `${#BUCKET}` is 62

- [ ] **Step 2: Run it to verify it fails**

Run: `bash deploy/llm-endpoint/lib/names.sh --self-test; echo rc=$?`
Expected: FAIL rows (functions undefined) and `rc=1`

- [ ] **Step 3: Implement the two functions above the self-test block; no `set` at file scope; the self-test block runs only when `${BASH_SOURCE[0]}` is `$0`**

- [ ] **Step 4: Run it to verify it passes, and lint it**

Run: `bash deploy/llm-endpoint/lib/names.sh --self-test; echo rc=$?` then `bashrs lint deploy/llm-endpoint/lib/names.sh; echo rc=$?`
Expected: every row `PASS`, `rc=0` twice

- [ ] **Step 5: Add rows V1 and V2 to `llm-endpoint-selftest`**

V1: `bash deploy/llm-endpoint/lib/names.sh --self-test > "$T/v1.log" 2>&1` → `row V1 "names.sh case table" "$T/v1.log" zero '^PASS' ` with the extra check that the log holds no `^FAIL` line.
V2: `bashrs lint deploy/llm-endpoint/lib/names.sh > "$T/v2.log" 2>&1` → zero.

- [ ] **Step 6: Run the self-test and commit**

Run: `just llm-endpoint-selftest > /tmp/selftest-t1.log 2>&1; echo rc=$?; tail -3 /tmp/selftest-t1.log`
Expected: `rc=0`, the summary line shows the two new rows plus every existing row passing

```bash
git add deploy/llm-endpoint/lib/names.sh justfile
git commit -m "feat(llm-endpoint): one library derives every endpoint name from (variant, env, account)"
```

---

### Task 2: `up` and `deploy` take a variant; records carry the name; readers check it

**Files:**
- Modify: `justfile` recipes `llm-endpoint-up` (signature line 4081; names at 4115-4120; tags at 4134, 4140, 4145, 4154; `aws.json` writer at 4171-4172) and `llm-endpoint-deploy` (signature 3907; ids/pin defaults 3913-3914; env check 3926-3929; ids rung 3966-3976; `FN`/`TAGS` 4010-4011; DRY-RUN and PREFLIGHT lines 3978, 4006; `function.json` writer 4073-4074)
- Modify: `justfile` `llm-endpoint-selftest` (fixture `aws.json` near line 5041-5056; new rows V3 to V7)
- Test: self-test rows V3 to V7

**Interfaces:**
- Consumes: Task 1's `llm_endpoint_names`.
- Produces:
  - Recipe signatures `llm-endpoint-up env="dev" profile="ze-kasher-dev" variant=""` and `llm-endpoint-deploy env="dev" profile="ze-kasher-dev" variant=""`.
  - `aws.json` gains keys `name` (= `FN`), `variant`, `suffix`; `function.json` gains `variant`, `suffix`.
  - A shared check every later task copies: `check_record FILE` refuses with class `ids` when the file's `name` key exists and differs from `FN`; when the key is absent it refuses unless `VARIANT` is empty and the file's `env` equals `ENV` (legacy records).
  - Refusal classes: `variant` (bad variant), `ids` (record mismatch).

- [ ] **Step 1: Write rows V3 to V7 in the self-test, against recipes that do not accept `variant` yet**

- V3 `up variant=2b` under the permissive shim with a fresh ids dir `$T/up2b`: rc 0; `$T/up2b/aws.json` has `name` = `aprender-llm-endpoint-dev-2b`, `variant` = `2b`, `suffix` = `dev-2b`, `bucket` ending `-dev-2b`, `ecr_repo_name` = `aprender-llm-endpoint-dev-2b`; the shim log shows `repository-names aprender-llm-endpoint-dev-2b` and `--tags` carrying `variant=2b`; the log holds no `aprender-llm-endpoint-dev( |$)` match.
- V4 `deploy DRY_RUN=1 variant=2b` with `LLM_ENDPOINT_IDS_DIR=$T/up2b` plus the image fixtures copied in, and `LLM_ENDPOINT_PIN_FILE` pointing at a copy of the fixture pin with `GGUF_RUNG=qwen35-2b-q4km` and `GGUF_OBJECT=Qwen3.5-2B-Q4_K_M.gguf`: rc 0; stdout matches `^DRY-RUN OK aprender-llm-endpoint-dev-2b `.
- V5 `deploy DRY_RUN=1 variant=2b` with `LLM_ENDPOINT_IDS_DIR=$IDS` (the dev fixture, whose `aws.json` has no `name` key): rc non-zero; stderr matches `^REFUSED ids:`; the shim log is empty.
- V6 `deploy DRY_RUN=1 variant=2B` (uppercase): rc non-zero; `^REFUSED variant:`; the shim log is empty; and the same for `variant=aprender-llm-endpoint-2b`.
- V7 `deploy DRY_RUN=1` with the dev fixture and no variant (legacy record, empty variant): rc 0 and `^DRY-RUN OK aprender-llm-endpoint-dev ` (this is row 8's path; keep row 8 as it is and let V7 use an `aws.json` that carries `name` = `aprender-llm-endpoint-dev` to cover the new-format path for the empty variant).

- [ ] **Step 2: Run the self-test to verify the new rows fail**

Run: `just llm-endpoint-selftest > /tmp/selftest-t2-red.log 2>&1; echo rc=$?; grep -E '^FAIL V' /tmp/selftest-t2-red.log`
Expected: `rc=1`; V3 to V6 listed as FAIL (just rejects the unknown `variant=` argument); V7 may pass already.

- [ ] **Step 3: Implement in `up` and `deploy`**

In both: add `variant=""` as the trailing parameter; `VARIANT="{{ trim_start_match(variant, "variant=") }}"`; after the existing env check, `. deploy/llm-endpoint/lib/names.sh || exit 1` then `llm_endpoint_names "$VARIANT" "$ENV" "${ACCOUNT:-}" || refuse variant "$LLM_ENDPOINT_NAMES_ERR"`; `IDS="${LLM_ENDPOINT_IDS_DIR:-$IDS_DEFAULT}"`, `PIN_FILE="${LLM_ENDPOINT_PIN_FILE:-$PIN_DEFAULT}"`. In `up`, call the library again with the account once `sts get-caller-identity` has returned, so `BUCKET` is set; replace the `NAME`/`BUCKET`/`REPO_NAME`/`LOG_GROUP` literals with the library's variables; append `$TAG_VARIANT` to every tag list; extend the `aws.json` writer with `name`, `variant`, `suffix`. In `deploy`, replace `FN="aprender-llm-endpoint-$ENV"` and the two receipt lines with `$FN`; append `,$TAG_VARIANT` to `TAGS`; add `check_record "$IDS/aws.json"` to rung 7; extend the `function.json` writer with `variant` and `suffix`. `check_record` is a bash function defined next to `jget`.

- [ ] **Step 4: Run the self-test to verify it passes**

Run: `just llm-endpoint-selftest > /tmp/selftest-t2.log 2>&1; echo rc=$?; tail -3 /tmp/selftest-t2.log`
Expected: `rc=0`; rows 1 to 27 and V1 to V7 all PASS

- [ ] **Step 5: Commit**

```bash
git add justfile
git commit -m "feat(llm-endpoint): up and deploy take a variant and refuse another name's records"
```

---

### Task 3: `image push=1` targets the variant's repository and ids directory

**Files:**
- Modify: `justfile` `llm-endpoint-image` (signature 3817; `IDS="$DIR/.ids"` at 3824; push block 3873-3906)
- Modify: `justfile` `llm-endpoint-selftest` (a `docker` shim beside the `aws` shim; row V8)
- Test: self-test row V8

**Interfaces:**
- Consumes: Task 1's library; Task 2's `check_record`.
- Produces: signature `llm-endpoint-image push="0" env="dev" variant=""`; the build step ignores `env` and `variant`; `push=1` reads `$IDS/aws.json` from the derived ids directory, checks it with `check_record`, pushes to its `ecr_repo`, and writes `$IDS/image-pushed.json` there.

- [ ] **Step 1: Write the `docker` shim and row V8**

The shim answers `run ... --version` with the fixture version line (`apr 0.71.4 (selftest)`), `image inspect` with a fixed id, and exits 0 for `buildx build`, `tag`, `push` and `login`, logging every argv to `$SHIM_LOG` like the `aws` shim. Row V8: `just llm-endpoint-image push=1 env=dev variant=2b` with `LLM_ENDPOINT_IDS_DIR=$T/up2b` (from V3), `LLM_ENDPOINT_BIN_DIR=$T/bin` holding the fixture tarball, and the permissive `aws` shim: rc 0; `$T/up2b/image-pushed.json` exists with `repo` ending `/aprender-llm-endpoint-dev-2b`; `$T/ids/image-pushed.json` is unchanged (compare its bytes before and after); the docker shim log shows `push` with a tag under `aprender-llm-endpoint-dev-2b`.

- [ ] **Step 2: Run to verify V8 fails**

Run: `just llm-endpoint-selftest > /tmp/selftest-t3-red.log 2>&1; echo rc=$?; grep -E '^FAIL V8' /tmp/selftest-t3-red.log`
Expected: `rc=1`, `FAIL V8` (unknown argument `env=`)

- [ ] **Step 3: Implement**

Add `env="dev" variant=""`, the env check (`dev|prod`), the library call and `IDS` derivation as in Task 2 (no `ACCOUNT` is needed: the repository comes from `aws.json`); keep `BIN` and the build unchanged; in the push block, `check_record "$IDS/aws.json"` before reading its fields.

- [ ] **Step 4: Run to verify it passes**

Run: `just llm-endpoint-selftest > /tmp/selftest-t3.log 2>&1; echo rc=$?; tail -3 /tmp/selftest-t3.log`
Expected: `rc=0`, all rows PASS

- [ ] **Step 5: Commit**

```bash
git add justfile
git commit -m "feat(llm-endpoint): image push=1 pushes to the variant's repository and records there"
```

---

### Task 4: The remaining recipes take the variant; no literal name survives

**Files:**
- Modify: `justfile` recipes `llm-endpoint-weights` (4181; names 4259-4260, tags 4267), `llm-endpoint-prove-401` (4316), `llm-endpoint-url` (4371), `llm-endpoint-warm` (4529), `llm-endpoint-teardown` (4582; `FN` literal at 4609; CodeBuild names read from `weights.json` at 4730-4731 stay), `llm-endpoint-measure` (4880)
- Modify: `justfile` `llm-endpoint-selftest` (rows V9 to V13)
- Test: self-test rows V9 to V13

**Interfaces:**
- Consumes: Task 1's library; Task 2's `check_record`.
- Produces: every recipe above ends its signature with `variant=""`; `teardown` derives `FN` from the library; `weights` names the CodeBuild project and role from the library and tags them with `$TAG_VARIANT`; `prove-401`, `url`, `warm`, `measure` apply `check_record` to `function.json` before using it.

- [ ] **Step 1: Write rows V9 to V13**

- V9 `teardown variant=2b` with an ids dir holding a `dev-2b` `aws.json` (from V3) plus a `function.json` naming `aprender-llm-endpoint-dev-2b`, permissive shim, `SHIM_CONCURRENCY=0`: rc 0; `^TEARDOWN OK aprender-llm-endpoint-dev-2b`; the shim log shows `--function-name aprender-llm-endpoint-dev-2b`, `--role-name aprender-llm-endpoint-dev-2b`, `--log-group-name /aws/lambda/aprender-llm-endpoint-dev-2b`, `--bucket aprender-llm-endpoint-weights-<acct>-dev-2b`; and no line matches `aprender-llm-endpoint-dev( |$)`.
- V10 `teardown variant=2b` with the dev fixture ids (no `name` key): rc non-zero; `^REFUSED ids:`; the shim log is empty.
- V11 `teardown` (no variant) with an ids dir whose `aws.json` carries `name` = `aprender-llm-endpoint-dev-2b`: rc non-zero; `^REFUSED ids:`; shim log empty.
- V12 `weights mode=codebuild env=dev variant=2b` under the permissive shim with the 2b ids and 2b pin: the shim log shows `create-role ... --role-name aprender-llm-endpoint-dev-2b-codebuild` and `create-project ... --name aprender-llm-endpoint-weights-dev-2b` with `variant=2b` in the tags (the shim must answer `codebuild batch-get-builds` with `SUCCEEDED`; extend the shim's canned answers as needed); `weights.json` records those two names.
- V13 literal guard: for each of the nine recipes, `just --show <recipe>` piped to a file, then `grep -cE 'aprender-llm-endpoint-(\$ENV|dev|prod)\b'` over the nine files must print 0 for each; a must-match control greps the same pattern over `just --show llm-endpoint-selftest` and must find at least one.

- [ ] **Step 2: Run to verify V9 to V13 fail**

Run: `just llm-endpoint-selftest > /tmp/selftest-t4-red.log 2>&1; echo rc=$?; grep -E '^FAIL V(9|1[0-3])' /tmp/selftest-t4-red.log`
Expected: `rc=1`; V9, V10, V12 fail on the unknown argument; V11 fails because teardown ignores `name` today; V13 fails on the literal at 4609 and on the receipt literals.

- [ ] **Step 3: Implement in the six recipes**

Same preamble as Task 2 in each. `teardown`: `FN="$FN"` from the library, `check_record "$IDS/aws.json"`, and `check_record "$IDS/function.json"` when that file exists. `weights`: `CB_ROLE`/`CB_PROJECT` from the library, `$TAG_VARIANT` in the CodeBuild tags, `check_record "$IDS/aws.json"`. `prove-401`, `url`, `warm`, `measure`: `check_record "$IDS/function.json"` after the file-exists check, before `FN` is read.

- [ ] **Step 4: Run to verify it passes**

Run: `just llm-endpoint-selftest > /tmp/selftest-t4.log 2>&1; echo rc=$?; tail -3 /tmp/selftest-t4.log`
Expected: `rc=0`, every row PASS, V13 prints 0 for each of the nine recipes

- [ ] **Step 5: Commit**

```bash
git add justfile
git commit -m "feat(llm-endpoint): every lifecycle recipe takes the variant; teardown derives its name"
```

---

### Task 5: `readback` and the comparison script

**Files:**
- Create: `deploy/llm-endpoint/readback_equal.py`
- Create: `deploy/llm-endpoint/tests/test_readback_equal.py`
- Modify: `justfile` (new recipe `llm-endpoint-readback env="dev" profile="ze-kasher-dev" variant=""` after `llm-endpoint-warm`; self-test row V14)
- Test: `python3 -I -m unittest deploy/llm-endpoint/tests/test_readback_equal.py`; self-test row V14

**Interfaces:**
- Consumes: Task 1's library; Task 2's `check_record`.
- Produces:
  - `readback_equal.py A.json B.json [C.json ...]` → exit 0 when every file equals A on the compared fields; exit 1 and print `DIFFERS <field>: <A value> != <file> <value>` for the first difference; exit 2 on a missing file or a file lacking a compared field. Compared fields, exactly: `function_name`, `state`, `memory_mb`, `timeout_s`, `ephemeral_mb`, `architecture`, `package_type`, `code_sha256`, `image_uri`, `last_modified`, `revision_id`, `env_keys` (sorted list), `reserved_concurrency`, `function_url`. Not compared: `read_at`.
  - `llm-endpoint-readback` writes `$IDS/readback-<UTC yyyymmddThhmmssZ>.json` with those fields plus `read_at`, from `aws lambda get-function` (configuration and code `ImageUri`), `get-function-concurrency` and `get-function-url-config` (`function_url` = `absent` on `ResourceNotFoundException`, else the URL's host). Read calls only; prints `READBACK OK <FN> <path>`.

- [ ] **Step 1: Write the unit tests**

`test_two_equal_records_pass` (two files differing only in `read_at` → rc 0), `test_one_changed_field_fails_and_names_it` (`memory_mb` 10240 vs 3008 → rc 1, stdout contains `DIFFERS memory_mb`), `test_missing_field_is_an_error` (a file without `revision_id` → rc 2). Run the script with `subprocess.run([sys.executable, "-I", SCRIPT, ...])`.

- [ ] **Step 2: Run to verify they fail**

Run: `python3 -I -m unittest deploy/llm-endpoint/tests/test_readback_equal.py 2>&1 | tail -3`
Expected: 3 errors (script not found)

- [ ] **Step 3: Implement `readback_equal.py`, then the recipe**

The recipe: preamble as Task 2, `check_record "$IDS/function.json"`, three read calls with `"${AP[@]}"`, one `python3 -I -c` that assembles the record from the three JSON outputs and writes the file.

- [ ] **Step 4: Run the unit tests, then write row V14 and run the self-test**

Run: `python3 -I -m unittest deploy/llm-endpoint/tests/test_readback_equal.py 2>&1 | tail -3`
Expected: `OK` with 3 tests.

Row V14: `just llm-endpoint-readback env=dev variant=2b` under the permissive shim (extend the shim's `lambda get-function` answer to carry `Configuration` and `Code.ImageUri`, and `get-function-url-config` to raise `ResourceNotFoundException`): rc 0; one `readback-*.json` in `$T/up2b` whose `function_url` is `absent` and whose `env_keys` has no key value in it (grep the file for the fixture hash: absent); the shim log holds no write verb (`create`, `update`, `put`, `delete`, `add`, `remove`).

Run: `just llm-endpoint-selftest > /tmp/selftest-t5.log 2>&1; echo rc=$?; tail -3 /tmp/selftest-t5.log`
Expected: `rc=0`, every row PASS

- [ ] **Step 5: Commit**

```bash
git add deploy/llm-endpoint/readback_equal.py deploy/llm-endpoint/tests/test_readback_equal.py justfile
git commit -m "feat(llm-endpoint): readback recipe and a comparison script for the isolation evidence"
```

---

### Task 6: The 2B pin file and the README

**Files:**
- Create: `deploy/llm-endpoint/pin-2b.env`
- Modify: `deploy/llm-endpoint/README.md` (sections "Recipes (the D-12 order)" at line 106, "Environment-specific names" at 158, "Teardown" at 173, "The recipes' own test" at 204)
- Test: self-test row V15; `python3 -I deploy/llm-endpoint/ladder_pin.py contracts/model-capability-ladder-v1.yaml qwen35-2b-q4km`

**Interfaces:**
- Consumes: Task 1's defaults (`PIN_DEFAULT` for `2b` is this file).
- Produces: `pin-2b.env` identical to `pin.env` except `GGUF_RUNG=qwen35-2b-q4km` and `GGUF_OBJECT=Qwen3.5-2B-Q4_K_M.gguf`.

- [ ] **Step 1: Write row V15, which fails until the pin file exists**

V15: `diff <(grep -vE '^GGUF_(RUNG|OBJECT)=' deploy/llm-endpoint/pin.env) <(grep -vE '^GGUF_(RUNG|OBJECT)=' deploy/llm-endpoint/pin-2b.env)` is empty, and `grep -c '^GGUF_RUNG=qwen35-2b-q4km$' deploy/llm-endpoint/pin-2b.env` prints 1, and `deploy DRY_RUN=1 variant=2b` with no `LLM_ENDPOINT_PIN_FILE` override and `LLM_ENDPOINT_IDS_DIR=$T/up2b` prints `^DRY-RUN OK aprender-llm-endpoint-dev-2b .* gguf aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223`.

- [ ] **Step 2: Run to verify V15 fails** — Expected: `FAIL V15` (file absent)

- [ ] **Step 3: Create `pin-2b.env`; rewrite the four README sections**

README: rename "Environment-specific names" to "Names: environment and variant" with the Global Constraints table, the variant rule and its reason (the bucket limit), the record paths, the `check_record` rule, and one sentence that dev and prod share `.ids/` for the empty variant today. "Teardown": add that it derives the name and refuses another name's records. "The recipes' own test": list V1 to V15 by label. "Recipes": show the trailing `variant` on the nine recipes and the new `readback`.

- [ ] **Step 4: Run the self-test and the ladder check**

Run: `just llm-endpoint-selftest > /tmp/selftest-t6.log 2>&1; echo rc=$?; python3 -I deploy/llm-endpoint/ladder_pin.py contracts/model-capability-ladder-v1.yaml qwen35-2b-q4km`
Expected: `rc=0`; the hash `aaf42c8b…9223`

- [ ] **Step 5: Commit**

```bash
git add deploy/llm-endpoint/pin-2b.env deploy/llm-endpoint/README.md justfile
git commit -m "docs(llm-endpoint): the variant rule, records and the 2B pin"
```

---

### Task 7 (gated, live): isolation evidence and the 2B smoke test

**Calling convention:** `variant=` is always the LAST positional argument; every command below passes all positionals (`just` binds them in order, so `env=dev variant=2b` would put the variant into `profile` and aim at the live 0.8B name). The recipes refuse `variant=` in any other slot.

**Gate:** operator's go for this task. Do not start a step without it. Profile `ze-kasher-dev`, region `us-east-1`, account `816069129666`.

**Files:**
- Create: `.planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-VARIANT-ISOLATION-EVIDENCE.json`
- Create: `.planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-2B-SMOKE-EVIDENCE.json`
- Modify: `.planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-PMCP-RUN-MULTI-SIZE-REPLY.md` ("What you get back": the two evidence file names and the measured numbers)
- Test: the evidence files' own pass lines (`readback_equal.py` exit 0 twice; the not-found read; `MEASURE OK`)

**Interfaces:**
- Consumes: everything above; `APR_API_KEY_HASH` and `APR_PROBE_KEY` from the operator's environment (never written).

- [ ] **Step 1: R0** — `just llm-endpoint-readback dev ze-kasher-dev` → `READBACK OK aprender-llm-endpoint-dev …`; the file's `last_modified` is `2026-10-16T18:22:53.000+0000`. If not, stop: the baseline moved and the operator decides.

- [ ] **Step 2: 2B up, push, weights, deploy** — `just llm-endpoint-up dev ze-kasher-dev variant=2b` → `UP OK aprender-llm-endpoint-dev-2b …`; `just llm-endpoint-image 1 dev variant=2b` → `PUSHED 816069129666.dkr.ecr.us-east-1.amazonaws.com/aprender-llm-endpoint-dev-2b:cpu-v0.71.4 sha256:…` then `IMAGE OK …` (the digest may differ from the dev image's `f291c0a7…`: the Dockerfile's base image is unpinned and the image is rebuilt, so a different digest is not a defect — record both); `just llm-endpoint-weights copy dev ze-kasher-dev variant=2b` → `WEIGHTS OK s3://aprender-llm-endpoint-weights-816069129666-dev-2b/Qwen3.5-2B-Q4_K_M.gguf sha256 aaf42c8b…`; `RESERVED_CONCURRENCY=1 just llm-endpoint-deploy dev ze-kasher-dev variant=2b` → `DEPLOY OK aprender-llm-endpoint-dev-2b …` with the read-back line `memory=10240 timeout=900 ephemeral=10240 arch=arm64`.

- [ ] **Step 3: R1** — `just llm-endpoint-readback dev ze-kasher-dev`.

- [ ] **Step 4: throwaway `iso`** — the four recipes of Step 2 with `variant=iso` in the last position (`up`, `image 1`, `weights copy`, `deploy`; same positional forms) (its pin file: copy `pin-2b.env` to `pin-iso.env` for the run; do not commit it).

- [ ] **Step 5: R2 and S0** — `just llm-endpoint-readback dev ze-kasher-dev` and `just llm-endpoint-readback dev ze-kasher-dev variant=2b`.

- [ ] **Step 6: tear `iso` down** — `just llm-endpoint-teardown dev ze-kasher-dev variant=iso` → `TEARDOWN OK aprender-llm-endpoint-dev-iso`; then `aws lambda get-function --function-name aprender-llm-endpoint-dev-iso` → `ResourceNotFoundException`. Delete `pin-iso.env` and `.ids/dev-iso/` locally afterwards; the `iso` ECR repository is printed under `NOT RUN` and is deleted by hand after the evidence is written.

- [ ] **Step 7: R3 and S1, then compare** — `python3 -I deploy/llm-endpoint/readback_equal.py R0 R1 R2 R3; echo rc=$?` → `rc=0`; `python3 -I deploy/llm-endpoint/readback_equal.py S0 S1; echo rc=$?` → `rc=0`. Write `09-VARIANT-ISOLATION-EVIDENCE.json`: the six records, both commands with exit status and output, the not-found read, the recipe sequence with timestamps, and the identity lines (account, region, profile, apr tag, image digest).

- [ ] **Step 8: 2B smoke test** — `just llm-endpoint-prove-401 dev ze-kasher-dev variant=2b; echo rc=$?` → `401 NOT PROVEN`, `rc=3` (expected at this pin; record it); `aws lambda get-function-url-config --function-name aprender-llm-endpoint-dev-2b` → `ResourceNotFoundException`; `just llm-endpoint-measure dev ze-kasher-dev smoke 1 0 1 3 variant=2b` (label smoke, cold 1, long 0, conc 1, budget 3) → `MEASURE OK <path>`. From the measure record and the function log: cold-start timeline, CPU part (`cpu_part`), the tool-call function names, prefill and decode tok/s, dollars. Write `09-2B-SMOKE-EVIDENCE.json` with those, the image digest and its ECR size (`aws ecr describe-images`), the weights size (`head-object`), and the sentence that the CPU part is not chosen by anyone.

- [ ] **Step 9: Update the reply draft and commit the evidence**

```bash
git add .planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-VARIANT-ISOLATION-EVIDENCE.json \
        .planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-2B-SMOKE-EVIDENCE.json \
        .planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-PMCP-RUN-MULTI-SIZE-REPLY.md
git commit -m "docs(09): variant isolation evidence and the 2B smoke test"
```

The 2B stays deployed (`aprender-llm-endpoint-dev-2b`, reserved concurrency 1). Its idle cost is storage only.
