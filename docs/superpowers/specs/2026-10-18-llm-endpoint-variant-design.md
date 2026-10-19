# Design: several sizes of the in-account LLM endpoint at the same time (the `variant` change)

Date: 2026-10-18. Status: approved in substance by the operator on 2026-10-18 ("Make it so, plan the
variant change"), through the reply draft `.planning/phases/09-internal-openai-compatible-llm-endpoint-for-pmcp-run/09-PMCP-RUN-MULTI-SIZE-REPLY.md`.
That reply is the agreement with pmcp.run; this document is the builder-facing version of it. Where
this document goes beyond the reply, the item is marked **(builder call)**.

## 1. Intent and why

pmcp.run selects an in-account model by Lambda function name (one provider row per function, no
router, allowlist prefix `aprender-llm-`). To compare sizes and offer customers a choice, each size must
stay deployed under its own function name at the same time. Today the `just llm-endpoint-*` recipes
(`justfile`, `llm-endpoint-image` through `llm-endpoint-selftest`) name every resource from one stem,
`aprender-llm-endpoint-<env>`, and a second deploy in the same environment updates the existing
function instead of adding one. pmcp.run's change request (stored verbatim beside the reply) asks for
a parametrized name, model and sizing inputs, and a 2B smoke test.

## 2. Goals, non-goals, anti-goals

Goals
- A deployment under a new name creates its own function, role, repository, log group, bucket and
  CodeBuild resources, and touches nothing of any other name.
- A teardown removes only the resources of the name it is given.
- With no variant given, every recipe behaves and names exactly as today, with one exception
  stated in §3: the CodeBuild role, which has never been created in this account. The existing
  `aprender-llm-endpoint-dev` deployment is not redeployed by this change.
- The 2B stays deployed after the acceptance test: pmcp.run selects it by name for its own
  measured tables. The teardown half of the isolation proof therefore uses a throwaway variant.
- The model per variant is selected by the two existing pin keys in a per-variant pin file.
- The 2B is deployed as `aprender-llm-endpoint-dev-2b` and one tool-calling chat completion is
  measured through the recipe's own probe and the API Gateway v2 event.
- Isolation is proved by a measured acceptance test, not by a code reading.

Non-goals
- Memory, ephemeral storage, timeout, architecture and thread count stay the literals they are
  (10,240 MB, 10,240 MB, 900 s, arm64, 6) with the read-back refusal unchanged. The reply explains
  why; this document does not reopen it.
- No Fargate recipe, no GPU recipe, no SageMaker shim, no change to the auth posture (D-12 and the
  401 proof stay as they are).
- The dev/prod collision in the default ids directory (both environments record into
  `deploy/llm-endpoint/.ids/` today) is not fixed here; it is noted in the README.

Anti-goals (would make this a failure even if it works)
- A variant that can produce a name outside pmcp.run's allowlist or outside AWS's own limits.
- A teardown that can read another name's records and delete that name's resources.
- Two implementations of the naming rule that can drift apart.

## 3. Constraints

- Every name derives from one suffix. With `S = <env>-<variant>` when a variant is given and
  `S = <env>` otherwise:

  | Resource | Name |
  |---|---|
  | function, IAM role, ECR repository | `aprender-llm-endpoint-S` |
  | log group | `/aws/lambda/aprender-llm-endpoint-S` |
  | weights bucket | `aprender-llm-endpoint-weights-<account>-S` |
  | CodeBuild project | `aprender-llm-endpoint-weights-S` |
  | CodeBuild role | `aprender-llm-endpoint-S-codebuild` **(builder call, see below)** |

  The CodeBuild role is the one name whose shape changes for every variant, the empty one
  included (today it is `aprender-llm-endpoint-codebuild-<env>`). IAM roles share one namespace,
  so with the old shape `variant=codebuild-2b` would name a function role identical to variant
  `2b`'s CodeBuild role and `variant=codebuild` identical to the empty variant's, and `up` would
  attach the Lambda policies to a CodeBuild-trust role while a teardown of either name deleted the
  other's role. With the discriminator after `S`, `aprender-llm-endpoint-S1` can equal
  `aprender-llm-endpoint-S2-codebuild` only if the environment were the word `codebuild`, which the
  `env` check excludes. No CodeBuild role has ever been created in this account (the dev weights
  were staged with `mode=copy`; `.ids/weights.json` records an empty `codebuild_role`), so the rename
  changes no live resource. Readers take the role name from `weights.json`, never re-derive it, so
  a record written before the change still tears down.

- Variant rule: 1 to 14 characters, `[a-z0-9]` and `-`, no leading or trailing hyphen; regex
  `^[a-z0-9]([a-z0-9-]{0,12}[a-z0-9])?$`. The binding limit is the S3 bucket name (63 characters,
  lowercase, no underscores): the 30-character stem `aprender-llm-endpoint-weights-` plus the
  12-digit account and a hyphen leave 20 for `S`, and `S = <variant>-prod` needs 5 of them.
  Environment stays `dev` or `prod`. A variant outside the rule is refused before any AWS call, with
  the refusal class `variant`.
- Tags: every tagged resource gains `variant=<variant>` beside the existing `project`, `phase` and
  `env` tags; the empty variant tags `variant=` as an empty string **(builder call)**, so one tag
  filter lists all variants of one environment.
- Local records are per name. Default ids directory: `deploy/llm-endpoint/.ids/` for the empty
  variant (unchanged), `deploy/llm-endpoint/.ids/S/` otherwise. Default pin file:
  `deploy/llm-endpoint/pin.env` for the empty variant (unchanged), `deploy/llm-endpoint/pin-<variant>.env`
  otherwise. The existing seams `LLM_ENDPOINT_IDS_DIR` and `LLM_ENDPOINT_PIN_FILE` still override both
  **(builder call)**. Pin files are committed; ids directories stay gitignored (the existing
  `/deploy/llm-endpoint/.ids/` rule covers the subdirectories).
- The name rule has exactly one implementation, a sourced, option-neutral library
  `deploy/llm-endpoint/lib/names.sh` (per the project's sourced-library rule: it fails by return
  status, never `set -e`), used by every recipe. It ships a must-match / must-not-match case table
  under `--self-test` (Verification Discipline rule 7), run by the recipes' self-test. It is linted by
  `bashrs lint` like every script in the tree **(builder call)**.
- `up` writes `name`, `variant` and `env` into `aws.json`. Every later recipe that reads
  `aws.json` or `function.json` refuses with class `ids` when the recorded name differs from the one
  derived from its own `variant` and `env` arguments. This is the local half of isolation: a teardown
  invoked for one name cannot act on records written for another **(builder call)**.
- The image is shared: one local build per pin tag. `llm-endpoint-image 1 <env> variant=X` (positional; `variant=` is the last argument of every recipe) pushes the
  already-built local image to variant X's repository and records `image-pushed.json` in X's ids
  directory; the build step itself is unchanged **(builder call)**.
- Weights for the 2B are staged with `mode=copy` from the spike bucket: the object
  `s3://llm-spike-031-<account>/Qwen3.5-2B-Q4_K_M.gguf` carries `Metadata.sha256` equal to the ladder
  pin (read on 2026-10-18), which is what the copy mode already requires.
- Cost ceiling for the live work: pmcp.run's own cap, $3 for the 2B smoke test; the isolation test
  spends only API calls plus one 2B deploy that the smoke test needs anyway.
- Nothing live runs without the operator's go for that step: the deploy, the weights copy and the
  teardown are side effects outside the worktree. The offline self-test and the name case table run
  freely. The live 2B deploy waits only for the operator's go. The name order is env before variant
  (`aprender-llm-endpoint-dev-2b`), chosen to match the name pmcp.run's own plan writes; the function name
  is the only thing their provider row keys on. A variant may not be `codebuild` or end in `-codebuild`
  (its function role would equal another name's CodeBuild role).

## 4. The parts, as decided

### 4.1 Recipes that take a variant

All nine recipes that act on a deployment gain a trailing parameter `variant=""`:
`llm-endpoint-image`, `llm-endpoint-deploy`, `llm-endpoint-up`, `llm-endpoint-weights`,
`llm-endpoint-prove-401`, `llm-endpoint-url`, `llm-endpoint-warm`, `llm-endpoint-teardown`,
`llm-endpoint-measure`. `llm-endpoint-image` has no `env` parameter today (its ids directory is the
literal `deploy/llm-endpoint/.ids`), so it gains `env="dev"` before `variant=""`; its build step
ignores both and only `push=1` uses them to pick the repository and the ids directory.
`llm-endpoint-probe` takes none (it is given its target explicitly) and `llm-endpoint-selftest`
takes none. Trailing keeps every existing invocation valid.

Each recipe, right after its `env` check, sources the library and calls
`llm_endpoint_names "$VARIANT" "$ENV" "$ACCOUNT_OR_EMPTY"`, which sets `SUFFIX`, `FN`, `REPO_NAME`,
`LOG_GROUP`, `BUCKET` (empty until the account is known), `CB_PROJECT`, `CB_ROLE`, `IDS_DEFAULT`,
`PIN_DEFAULT`, or returns non-zero with a one-line reason the recipe turns into `REFUSED variant: ...`.
Recipes that print the function name in their OK and DRY-RUN lines print `$FN`, so a variant shows
in every receipt line.

### 4.2 The library

`deploy/llm-endpoint/lib/names.sh`:
- `llm_endpoint_variant_ok <variant>`: returns 0 when the variant is empty or matches the rule.
- `llm_endpoint_names <variant> <env> [account]`: as above.
- `--self-test` when executed (not sourced): a case table of at least the rows in §6, printing one
  PASS/FAIL line per row and exiting non-zero on any FAIL. Executed by the recipes' self-test as its
  first rows.

### 4.3 Records

- `aws.json` (written by `up`) gains `name` (the function name), `variant`, `suffix`. `env` stays.
- `function.json` (written by `deploy`) gains `variant` and `suffix`; `function` already holds the name.
- Readers (`deploy`, `weights`, `prove-401`, `url`, `warm`, `teardown`, `measure`, `image push=1`) compare
  the recorded `name` with the derived `FN` and refuse on mismatch. A record without a `name` key
  (written before this change) is accepted only when the variant is empty and `env` matches, so the
  existing dev records keep working **(builder call)**.

### 4.4 Teardown

Derives `FN` from the library (today it is a literal), checks the record as in 4.3, and otherwise
runs unchanged. The repository is still never deleted by the recipe (printed under `NOT RUN`).

### 4.5 Read-back and the isolation proof

A new recipe `llm-endpoint-readback env profile variant` writes
`$IDS/readback-<utc timestamp>.json` holding, for the derived function name: `FunctionName`,
`State`, `MemorySize`, `Timeout`, `EphemeralStorage.Size`, `Architectures`, `PackageType`,
`CodeSha256`, `ImageUri` (from `get-function`), `LastModified`, `RevisionId`, the sorted environment
variable keys (never the values), the reserved concurrency, and the `get-function-url-config`
outcome (`absent` or the URL's host). It makes only read calls.

A script `deploy/llm-endpoint/readback_equal.py a.json b.json [c.json ...]` exits 0 when every file
equals the first on every field above except the timestamp, and prints the first differing field
otherwise. It has its own unit test (two equal records pass; one changed field fails, naming the
field).

The baseline. The 0.8B function's `LastModified` was read on 2026-10-18 with
`aws lambda get-function-configuration` before any variant work: `2026-10-16T18:22:53.000+0000`,
which is the reserved-concurrency step of the 2026-10-16 deploy (`.ids/function.json` records
`deployed_at` 18:18:32Z for the create and the concurrency step followed). R0 must carry that
value; no `RevisionId` was recorded on 2026-10-16, so R0's `RevisionId` is the baseline for R1 to R3.

The acceptance sequence, recorded as `09-VARIANT-ISOLATION-EVIDENCE.json` in the Phase 9 directory.
The 2B stays deployed; the teardown half uses a throwaway variant `iso` (one more up, push, copy and
deploy, no invocation, cents):
1. `readback` of the 0.8B (`variant=""`) → R0, with `LastModified` equal to the baseline.
2. `up`, `image push=1`, `weights mode=copy`, `deploy` for `variant=2b` (the smoke test's deploy).
3. `readback` of the 0.8B → R1.
4. `up`, `image push=1`, `weights mode=copy`, `deploy` for `variant=iso`.
5. `readback` of the 0.8B → R2 and of the 2B → S0.
6. `teardown` for `variant=iso`.
7. `readback` of the 0.8B → R3 and of the 2B → S1; `aws lambda get-function` on
   `aprender-llm-endpoint-dev-iso` returns not found.
8. `readback_equal.py R0 R1 R2 R3` and `readback_equal.py S0 S1` both exit 0. The evidence file
   carries the six records, both exit statuses and outputs, the not-found read, and the recipe
   sequence with timestamps.

### 4.6 The 2B smoke test

With `deploy/llm-endpoint/pin-2b.env` (a copy of `pin.env` with `GGUF_RUNG=qwen35-2b-q4km` and
`GGUF_OBJECT=Qwen3.5-2B-Q4_K_M.gguf`; every other key identical, so the same image serves):
`up`, `image push=1`, `weights mode=copy`, `deploy` with `RESERVED_CONCURRENCY=1`, then
`prove-401` (expected: `401 NOT PROVEN`, exit 3, recorded, as on the 0.8B at this pin), then
`measure label=smoke cold=1 long=0 conc=1 budget=3`, which sends the recipe's API Gateway v2 event
through `aws lambda invoke` and records client timings, the server log lines, the cold-start
timeline and the CPU part. The evidence file `09-2B-SMOKE-EVIDENCE.json` carries the measure
record, the tool-call function names the probe already records (`bench.py` keeps names and the
first 160 characters of content, not arguments; capturing arguments is out of scope and the
evidence says so), the image digest and size, the weights size, the `get-function-url-config`
not-found read, and the CPU part with the sentence that it is not chosen by anyone.

### 4.7 Documentation

`deploy/llm-endpoint/README.md`: the "Environment-specific names" section becomes "Names:
environment and variant" with the table in §3, the rule, the record paths and the dev/prod ids
collision note; "Teardown" gains the record check; "The recipes' own test" lists the new rows; the
"Recipes" section shows the trailing `variant` parameter. The reply to pmcp.run is updated with
the evidence file names once they exist (the reply is otherwise already written).

## 5. What is left to the builder

- Exact wording of refusal messages, within the existing `REFUSED <class>: <reason>` shape.
- Whether the library's functions use `printf` assignments or `eval`-free `read` to return values
  (no `eval`).
- The order and numbers of the new self-test rows after the existing ones, and their log file
  names.
- How `deploy/llm-endpoint/lib/names.sh` is linted: `bashrs lint` on it is run as a self-test row,
  since the Makefile's lint and the sourced-library guard glob only `scripts/*.sh`.
- The layout of the evidence JSON beyond the fields named in §4.5 and §4.6 (follow
  `09-LIVE-DEPLOY-EVIDENCE.json`).

## 6. Case table for the name rule (minimum rows)

Must match (accepted): `2b`, `4b`, `2b-q4km`, `x`, `a1b2c3d4e5f6g7` (14 chars), empty (the default).
Must not match (refused): `2B`, `2_b`, `-2b`, `2b-`, `a1b2c3d4e5f6g7h` (15 chars), `2b.q4`, `2b dev`,
`aprender-llm-endpoint-2b` (the stem is not the caller's to write).
Derived names for `2b`/`dev`/account `816069129666`: function `aprender-llm-endpoint-dev-2b`, bucket
`aprender-llm-endpoint-weights-816069129666-dev-2b`, CodeBuild project `aprender-llm-endpoint-weights-dev-2b`,
CodeBuild role `aprender-llm-endpoint-dev-2b-codebuild`.
Derived names for empty/`dev`: exactly today's, except the CodeBuild role `aprender-llm-endpoint-dev-codebuild`.
Collision rows (must hold): `variant=codebuild-2b` derives a function role that equals no other
derived role for any variant; `variant=codebuild` likewise.

## 7. Risks

- A recipe that still carries a literal `aprender-llm-endpoint-$ENV` after the change would deploy
  or delete the wrong name. The self-test's grep for literals in the recipe bodies (a must-not-match
  row) and the DRY-RUN receipt line per variant are the guards.
- The 2B measurement is a Graviton lottery (spike 031 landed on Graviton2 at 1.9 prefill tok/s); the
  smoke test records the CPU part and makes no timing claim beyond the one environment it got.
- The spike bucket is the copy source for every size; if it is torn down before the 2B is staged,
  `mode=codebuild` is the fallback (slower, already supported).
