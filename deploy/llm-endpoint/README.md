# deploy/llm-endpoint

The internal OpenAI-compatible LLM endpoint for pmcp.run (Phase 9): one arm64 container image
that runs an upstream release `apr serve` binary, weights pulled from S3 at start, hosted on
default AWS Lambda behind the Lambda Web Adapter, authenticated by a server-side Bearer key.

This directory is **not** part of the CDK app in `deploy/` (`deploy/bin`, `deploy/lib`,
`cdk.json`), and it ships **no Rust binary**: the OpenAI API is the server's own surface, so
the FALSIFY-MONO-011 register of crates allowed to ship a `[[bin]]` is untouched. The justfile
(`just llm-endpoint-*`) is the deployment surface, as `just laya-*` is for Phase 8; the
Makefile stays the quality surface.

The pattern is the Kev / Laya decider pattern (one image, weights in S3, default Lambda).
Spike 031 measured it on real Graviton hardware. **Those numbers are context only, never the
measurement (D-14)**: a 40-76 s first turn, 1-4 minute turns and cents per two-tool turn on
default Lambda tell you what to expect, not what this deployment does. Every figure this
phase reports is re-measured on the deployed function.

## Assets

Image half (plan 09-04):

| File | Role |
|---|---|
| `Dockerfile` | build args `APR_TAG`, `APR_TARBALL_SHA256`; no weights, no key, no URI baked in |
| `entrypoint.sh` | refuses without auth and pin, pulls the GGUF, re-hashes it, logs the `[entry]` timeline, execs `apr serve` |
| `entrypoint-selftest.sh` | offline proof of the entrypoint's refusals |
| `pin.env` | the one place the release pin lives: `APR_TAG`, `APR_ASSET`, `APR_TARBALL_SHA256`, `APR_PIN_PROFILE`, `LWA_IMAGE`, `LWA_IMAGE_DIGEST`, `IMAGE_NAME`, `GGUF_RUNG`, `GGUF_OBJECT` |
| `bin/`, `.ids/` | gitignored runtime directories (extracted binary, recorded ids and proofs) |

Staging half (plan 09-11, this README):

| File | Role |
|---|---|
| `ladder_pin.py` | prints the GGUF sha256 the capability ladder pins for a rung; the only place the hash is derived |
| `buildspec-weights.yaml` | the CodeBuild job: one Hugging Face fetch, sha256 check, S3 upload with the hash as metadata |
| `iam/lambda-trust.json`, `iam/lambda-s3-read.json` | the function role: trust, and S3 read on one bucket |
| `iam/codebuild-trust.json`, `iam/codebuild-s3-logs.json` | the CodeBuild role: trust, and S3 write on one bucket plus CloudWatch logs |

The two S3 policy files carry the placeholder `__BUCKET__`. A recipe substitutes it with `sed`
into a `mktemp` file and passes that as `--policy-document file://...`. No recipe inlines JSON.
Each policy keeps object-level actions (on the `/*` ARN) and bucket-level actions (on the bucket
ARN) in separate statements; the only wildcard resource is the CloudWatch logs statement. The
Lambda role also gets the AWS-managed `AWSLambdaBasicExecutionRole`; a default-networking
function needs no VPC execution policy.

## Requirements

- `python3` with **PyYAML importable under `python3 -I`** for `ladder_pin.py` (tested with
  PyYAML 6.0.3). This is the same dependency `scripts/check_model_ladder.sh` already has. When it
  is absent the reader exits 5 with a single `REFUSED yaml` line and no traceback.
- `just`, `docker` with `buildx` (linux/arm64), and the AWS CLI with the profile named by
  `APR_PIN_PROFILE` in `pin.env`.
- No PyPI install of `huggingface_hub` or `boto3`: the weights job fetches with `curl`.

## The pin

Two pins, each with exactly one source:

1. **The `apr` release** lives in `pin.env` (`APR_TAG`, `APR_TARBALL_SHA256`). The image build
   refuses a tarball whose hash is not the release's own sha256 file (`sha-release`) or not the
   pin (`sha-pin`).
2. **The GGUF** lives in `contracts/model-capability-ladder-v1.yaml`. `pin.env` names only the
   rung (`GGUF_RUNG`). Read the hash with:

   ```bash
   python3 -I deploy/llm-endpoint/ladder_pin.py contracts/model-capability-ladder-v1.yaml "$GGUF_RUNG"
   ```

   `ladder_pin.py` exit codes:

   | Exit | Message | Meaning |
   |---|---|---|
   | 0 | the hash on stdout | exactly one distinct sha256 is attributed to the rung |
   | 2 | `UNREADABLE ladder` / usage | the ladder cannot be read or parsed |
   | 3 | `NO PIN for <rung>` | no entry for the rung carries a sha256 |
   | 4 | `AMBIGUOUS PIN for <rung>` | two distinct sha256 values are attributed to the rung (the ladder carries the pin on its `known_red` and `inventory` rows; they must agree) |
   | 5 | `REFUSED yaml` | PyYAML is not importable |
   | 6 | `BAD PIN for <rung>` | a sha256 entry is not 64 lowercase hex |

   The recipe passes the result to CodeBuild as the `GGUF_SHA256` environment variable together
   with `BUCKET`; the buildspec compares the downloaded file against it and exits 9 on a
   mismatch, uploading nothing. The object carries `--metadata sha256=...` so the deploy ladder
   re-checks it with `head-object` and the entrypoint re-hashes the pulled file. Hashes are
   only ever logged as the word `set`, never as a value.

## Re-pin

To move to a different `apr` release or a different ladder rung:

1. Edit `pin.env` (the release keys, or `GGUF_RUNG`). If the GGUF changes, the ladder must
   already carry the new rung and `ladder_pin.py` must print its hash (exit 0).
2. `just llm-endpoint-image` rebuilds and prints `IMAGE OK <image>`. Refusals: `sha-release`,
   `sha-pin`, `lwa-digest`, `identity`.
3. Local probe of the new image against the spike-029 conformance shapes.
4. `just llm-endpoint-weights codebuild` when the GGUF changed (the object in S3 is replaced and
   its metadata re-checked).
5. `just llm-endpoint-deploy`. Run it with `DRY_RUN=1` first (offline, rungs 1-7) and `PREFLIGHT=1`
   (connected, read-only, rungs 1-10) before the live run.
6. `just llm-endpoint-prove-401`, then `just llm-endpoint-url`, then probe, then
   `just llm-endpoint-measure`.

A re-pin invalidates every earlier record: a measurement is evidence for the identity that
produced it and no other.

## Recipes (the D-12 order)

| Order | Recipe | What it does |
|---|---|---|
| 1 | `just llm-endpoint-up` | the name's bucket, ECR repository, role and log group, all tagged |
| 2 | `just llm-endpoint-weights mode=copy` or `codebuild` | stage the GGUF into the bucket (CodeBuild never crosses the operator uplink) |
| 3 | `just llm-endpoint-image push=1` | build the image (shared) and push it to the name's repository |
| 4 | `just llm-endpoint-deploy` | create or update the function (`DRY_RUN=1` offline, `PREFLIGHT=1` connected read-only, neither = live) |
| 5 | `just llm-endpoint-prove-401` | invoke the deployed function without a key and prove it answers 401 |
| 6 | `just llm-endpoint-url` | create the public URL, refused without a fresh 401 proof and bounded concurrency |
| 7 | `just llm-endpoint-probe` | the conformance probe against a server, bound to the reader test (see Probe and measure) |
| 8 | `just llm-endpoint-measure` | the D-14 measurement, with an optional budget guard |
| 9 | `just llm-endpoint-teardown` | remove what the phase created, without ever deleting an ECR repository |

`just llm-endpoint-warm` (one authenticated 1-token invoke, key from `APR_PROBE_KEY`),
`just llm-endpoint-readback` (read the live function back and record it, read calls only; see
"Several sizes at once") and `just llm-endpoint-selftest` sit outside the order. Every recipe except
`probe` takes `env` (`dev` or `prod`, default `dev`) and a trailing `variant` (default empty, the
original 0.8B endpoint). Most also take `profile` (default `ze-kasher-dev`). `just` binds positionals
in order, so `variant` is always the LAST positional; write it as `variant=2b` (the recipes strip the
`variant=` prefix). A `variant=...` argument that lands in an earlier slot (the profile, the measure label,
the weights mode, the image push flag) is refused as `REFUSED args` rather than bound to the default name.
The real parameter orders (`just --show <recipe>`):

| Recipe | Positionals, in order |
|---|---|
| `up`, `deploy`, `prove-401`, `url`, `warm`, `readback`, `teardown` | `env profile variant` |
| `weights` | `mode env profile variant` |
| `image` | `push env variant` (no profile: the build is local, the push uses the ids' account) |
| `measure` | `env profile label cold long conc budget variant` |
| `probe` | `base model label server_version` (no variant: it targets whatever server you name) |

For example `just llm-endpoint-deploy dev ze-kasher-dev variant=2b`, `just llm-endpoint-image 1 dev variant=2b`,
`just llm-endpoint-weights copy dev ze-kasher-dev variant=2b`. The image and probe recipes are owned by
plans 09-04 and 09-13.

### The deploy mode contract

`llm-endpoint-deploy` is a fail-closed ladder of ten rungs, each printing `REFUSED <check>: ...`
before any AWS write. Rungs 1-7 are local (`env`, `pin`, `tarball`, `image`, `ladder`, `auth`,
`ids`); rungs 8-10 are read-only AWS calls (`account` = sts, `s3-sha` = head-object, `digest` =
ecr describe-images).

- `DRY_RUN=1` is OFFLINE static validation: rungs 1-7 only, zero AWS calls of any kind, ending
  `DRY-RUN OK ... (offline rungs 1-7; account, s3-sha and digest NOT checked; nothing deployed)`.
- `PREFLIGHT=1` is the CONNECTED read-only preflight: rungs 1-10, only the three read calls,
  ending `PREFLIGHT OK ... (rungs 1-10; read-only AWS calls only; nothing deployed)`.
- Neither flag runs rungs 1-10 and then the live create or update, then reads back memory,
  timeout, ephemeral storage and architecture from Lambda (`REFUSED readback` on any difference).
- Both flags together are `REFUSED mode`.

`RESERVED_CONCURRENCY=<n>` applies a reserved-concurrency bound after the deploy and records the
read-back value in `.ids/function.json`. `APR_API_KEY_HASH` comes from the environment and is only
ever printed as the word `set`; it reaches Lambda through a mode-0600 temporary file, never argv.

### Refusals per recipe

| Recipe | Refusals |
|---|---|
| `llm-endpoint-up` | `env`, `account`, and a tolerated-by-exact-name idempotency check (`ResourceAlreadyExistsException`) |
| `llm-endpoint-weights` | `mode`, `env`, `pin`, `ladder`, `ids`, `account`, `source-meta` (the spike source has no matching hash: use `mode=codebuild`), `codebuild`, `s3-sha` |
| `llm-endpoint-deploy` | `mode`, `env`, `pin`, `tarball`, `image`, `ladder`, `auth`, `ids`, `account`, `s3-sha`, `digest`, `readback` |
| `llm-endpoint-prove-401` | `env`, `function`, `event`; exits 3 with `401 NOT PROVEN` on any status but 401 |
| `llm-endpoint-url` | `proof` (no record, not 401, another image digest, older than 24 h), `account`, `concurrency` (not 1-5 unless `ACCEPT_UNRESERVED_URL=1`, recorded as `risk_accepted`); removes the URL and both statements and exits 1 with `URL REMOVED` when the no-key re-check is not 401 |
| `llm-endpoint-warm` | `probe-key`, `function` |
| `llm-endpoint-probe` | `key`, `label`, `pin`, `profile`, `unreleased`, `probe`, `reader` |
| `llm-endpoint-measure` | `env`, `key`, `function`, `pin`, `label`, `conc`, `budget`; exits 4 with `MEASURE STOPPED (budget)` |
| `llm-endpoint-teardown` | `ids`, `pin`, `account`, `contain`, `policy` (a URL statement survived `remove-permission`) |

### Names: environment and variant

Nothing is shared across names, so tearing one down cannot touch another. With `S = <env>-<variant>`
when a variant is given and `S = <env>` otherwise:

| Resource | Name |
|---|---|
| function, IAM role, ECR repository | `aprender-llm-endpoint-S` |
| log group | `/aws/lambda/aprender-llm-endpoint-S` |
| weights bucket | `aprender-llm-endpoint-weights-<account>-S` |
| CodeBuild project (only for `weights codebuild`) | `aprender-llm-endpoint-weights-S` |
| CodeBuild role (only for `weights codebuild`) | `aprender-llm-endpoint-S-codebuild` |
| URL statements on the function | `llm-endpoint-url-public`, `llm-endpoint-invoke-via-url` |

Every tagged resource also carries `variant=<variant>` (empty for the default).

The variant rule: 1 to 14 characters of `[a-z0-9-]`, no leading or trailing hyphen, regex
`^[a-z0-9]([a-z0-9-]{0,12}[a-z0-9])?$`. The binding limit is the S3 bucket name (63 characters): the
30-character stem `aprender-llm-endpoint-weights-`, the 12-digit account and a hyphen leave 20 for `S`,
and `<variant>-prod` needs 5 of them. A variant outside the rule is refused as `REFUSED variant` before
any file is read or AWS call is made. One implementation of the rule exists, `lib/names.sh`, with its own
case table (`bash deploy/llm-endpoint/lib/names.sh --self-test`, re-run by the recipes' self-test).

Local records are per name:

| Record | Default variant | With a variant |
|---|---|---|
| ids directory (`aws.json`, `function.json`, `image-*.json`, `weights.json`, `readback-*.json`) | `deploy/llm-endpoint/.ids/` | `deploy/llm-endpoint/.ids/<env>-<variant>/` |
| pin file | `deploy/llm-endpoint/pin.env` | `deploy/llm-endpoint/pin-<variant>.env` |

`LLM_ENDPOINT_IDS_DIR` and `LLM_ENDPOINT_PIN_FILE` still override both. `llm-endpoint-up` records the
names in the gitignored `aws.json` (with `name`, `variant` and `env`); the later recipes read them from
there, and every reader refuses with `REFUSED ids` when a record names another function than the one
its own `env` and `variant` derive. A record with no `name` key (written before variants) serves only
the default variant, in the env it records.

Things to know:

- `just llm-endpoint-image 1` with the default `env=dev` now refuses an `aws.json` recorded for prod.
  Pass `prod` explicitly.
- The CodeBuild role is now `aprender-llm-endpoint-S-codebuild` (it was `aprender-llm-endpoint-codebuild-<env>`).
  The rename keeps IAM's single role namespace unambiguous: with the old shape `variant=codebuild-2b`
  would collide with another name's function role. Teardown requires the CodeBuild names recorded in
  `weights.json` to equal the ones the variant derives (empty for `weights copy`); the old names are not
  accepted. A variant may not be named `codebuild` or end in `-codebuild`, because its function role would
  then equal another name's CodeBuild role.
- Dev and prod share `deploy/llm-endpoint/.ids/` for the empty variant (unchanged). That is a known
  limitation: the `env` check on the record is what stops one from acting on the other. A variant gets a
  directory per env.
- The image build is shared (one local image per pin tag) and reads `pin.env`; deploy and weights read
  `pin-<variant>.env`. A variant pin file must therefore keep `APR_TAG`, `APR_ASSET`,
  `APR_TARBALL_SHA256`, `APR_PIN_PROFILE`, `LWA_*` and `IMAGE_NAME` identical to `pin.env`; only
  `GGUF_RUNG` and `GGUF_OBJECT` differ. Self-test row V18 enforces this for `pin-2b.env`. A re-pin edits
  `pin.env` and every `pin-<variant>.env` together.

### Teardown

`llm-endpoint-teardown` derives its name from `env` and `variant` and refuses (`REFUSED ids`) any record
written for another name, before the first AWS call. It also refuses a `weights.json` that names a
CodeBuild project or role not belonging to its own name, except the pre-rename names of the default
variant, which it still deletes. It contains first (reserved concurrency 0, read back), deletes the URL
config, runs `remove-permission` for both statement ids and verifies `get-policy` lists neither,
then deletes the function. Deleting a URL config does not delete the policy, which is why the
statements are removed explicitly. It deletes only the ECR image tag this deployment recorded,
and only while that tag still resolves to the recorded digest (`SKIPPED image` otherwise;
`KEEP_IMAGE=1` keeps it). It never executes `aws ecr delete-repository`: it prints how many images
the repository still holds and the command under `NOT RUN` for the human to decide. The bucket
goes only when its sole object is the GGUF (`KEEP_BUCKET=1` keeps it), then the log group, the
CodeBuild project and role if `weights codebuild` made them, and the function role. The spike-031
leftovers are printed and are removed only with `TEARDOWN_SPIKE_031=1`, which still never runs
`delete-repository`.

### Several sizes at once

Each variant is a separate function, repository, bucket and ids directory, so a 2B endpoint runs beside
the 0.8B one. The 2B sequence, with `pin-2b.env` already committed (`GGUF_RUNG=qwen35-2b-q4km`):

```bash
just llm-endpoint-up dev ze-kasher-dev variant=2b
just llm-endpoint-image 1 dev variant=2b          # pushes the shared local image to the dev-2b repository
just llm-endpoint-weights copy dev ze-kasher-dev variant=2b
RESERVED_CONCURRENCY=1 just llm-endpoint-deploy dev ze-kasher-dev variant=2b
just llm-endpoint-readback dev ze-kasher-dev variant=2b
just llm-endpoint-measure dev ze-kasher-dev smoke 1 0 1 3 variant=2b
```

`weights copy` needs the spike bucket's 2B object to carry the ladder hash as `Metadata.sha256`.
`readback` writes `readback-<UTC>.json` into the variant's ids directory (the environment as sorted key
names, never values; the URL only as `present` or `absent`, never its address). To show two endpoints are independent, read both back and
run `python3 -I deploy/llm-endpoint/readback_equal.py A.json B.json`, which names the first differing field.

### Cost amplification

A public `AuthType NONE` URL lets anyone who knows its address buy a 10,240 MB cold start that is
answered 401 after about 40 s, roughly $0.005 each. The bound is the combination of three
controls: the enforced reserved concurrency (`llm-endpoint-url` refuses unless it is 1-5), the
401 gate (no URL before `prove-401` has passed for the deployed digest) and keeping the URL out
of git (`.ids/url.json` is gitignored and only its host is ever printed). Recommended for the
operator and not created here: a CloudWatch invocation alarm with a notification target of your
own choosing.

### The URL is a secret-equivalent address

pmcp.run's durable-agent-lambda logs the full resolved endpoint on every call
(`llm/service.rs`), and the same URL sits in the LLMProvider row's `endpointTemplate`. Treat the
URL as an address whose only protection is the server's bearer gate plus the concurrency bound;
plan 09-08 records the disposition (T-09-50).

### The recipes' own test

`just llm-endpoint-selftest` puts an `aws` shim first on `PATH` and never reaches AWS (nor docker). The
offline cases run against a shim that logs and refuses, so the log stays empty (`AWS CALLS: 0`);
the PREFLIGHT cases run against a shim that answers only the three read calls (`FIXTURE READS: n`);
a negative control proves the call counter can be non-zero; the lifecycle recipes run against a
permissive shim that logs every argv, so the tests assert which AWS calls were and were not made
(both `remove-permission` ids, one `batch-delete-image`, no `delete-repository`, no
`create-function-url-config` without a proof). `SELFTEST_KEEP=1` keeps the temporary directory.
Its last row, `bench-selftest`, runs `probe/bench.py --selftest`, which has its own aws shim.

The variant rows (numbered rows 1-30 are the original ones):

| Row | What it pins |
|---|---|
| V1, V2 | the `names.sh` case table passes; `bashrs lint` of `names.sh` is clean |
| V3 | `up variant=2b`: dev-2b names in `aws.json` and in the calls, tagged `variant=2b`, no default-name call |
| V4 | `deploy variant=2b` DRY_RUN with the 2b ids and a 2b pin: `DRY-RUN OK`, zero AWS calls |
| V5, V5b | a legacy record cannot serve `2b`, and a 2b record cannot serve the default: `REFUSED ids`, zero calls |
| V6 | an uppercase variant is `REFUSED variant` before anything is read |
| V7 and V7b-e | the default still accepts a record naming `aprender-llm-endpoint-dev`; `up` refuses another name's record before creating anything and accepts its own and legacy ones |
| V8 | `image 1 dev variant=2b` pushes to the dev-2b repository and records in the 2b ids; the default record is untouched |
| V9 | `teardown variant=2b`: only dev-2b names reach the shim |
| V10, V11 | teardown refuses the legacy record as `2b` and a 2b record as the default, nothing reaching the shim, not even sts |
| V12 | `weights codebuild variant=2b` names the role `...-dev-2b-codebuild` and the project, tags them, records both |
| V13 | no recipe of the nine holds an endpoint-name literal (with a must-match control) |
| V14 | teardown `2b` over a `weights.json` naming the default's CodeBuild names: `REFUSED ids`, no delete call |
| V15 | teardown of the default over a `weights.json` naming `aprender-llm-endpoint-dev-2b` (another name's function role): `REFUSED ids`, no delete call |
| V16 | teardown `2b` with its own derived CodeBuild names deletes those |
| V17 | `readback variant=2b`: one record, `function_url` absent, no environment value anywhere, read calls only |
| V19 | `readback` with a URL config records `function_url` as `present`; the host is in neither the record nor stdout |
| V20-V22, V24-V25 | `variant=` in the profile (teardown, up), label, cold or long (measure) slot is `REFUSED args`, nothing reaching the shim |
| V23 | teardown `2b` over an `aws.json` whose bucket, `ecr_repo_name`, `log_group` or `role_arn` was altered: `REFUSED ids`, no delete call |
| V18 | `pin-2b.env` equals `pin.env` but for the two GGUF keys, selects rung `qwen35-2b-q4km`, and `deploy variant=2b` with no pin override derives and reads it (`DRY-RUN OK` with the 2b GGUF hash `aaf42c8b...9223`) |

## Probe and measure

Two recipes, one for each half of the evidence. Neither is run by the self-test against a real server
or account.

### `just llm-endpoint-probe base model label server_version`

Runs `probe/probe_openai.py` and then the Rust reader test
(`api::llm_endpoint_conformance_tests::conformance_record_matches_contract`), so a record the
contract rejects cannot sit in `probe/results/` unnoticed (`REFUSED reader`, exit 1; the record
stays for diagnosis). The key is read by the probe from `APR_PROBE_KEY` and is never an argument;
without it the recipe is `REFUSED key`.

Accepted labels: `release-local`, `interim`, `deployed-https`. Anything else is `REFUSED label`.
`baseline` is the committed spike-029 transcription and `deployed` is written only by plan 09-07's
`bench.py --conformance` through `lambda:Invoke`, so this HTTP wrapper can write neither.

| Label | File written (in `probe/results/`) | `--pin` |
|---|---|---|
| `release-local` | `conformance-<APR_TAG>-release-local.json` | `<APR_TAG>` |
| `deployed-https` | `conformance-<APR_TAG>-deployed-https.json` | `<APR_TAG>` |
| `interim` | `conformance-interim-upstream-main-plus-fixes.json` | `upstream-main+PRs` |

The profile is `APR_PIN_PROFILE` from the environment, else the one in `pin.env`.
`APR_UNRELEASED_FIXES` is a semicolon-separated list of `<probe name>=<reference>`, forwarded as
repeated `--unreleased` flags so a fix the pinned release lacks is recorded as `KNOWN LIMITATION`
with an `unreleased_at_pin` entry. It is `REFUSED unreleased` outside the `fixed` profile, for any
probe except `tool result -> final answer` and `max_completion_tokens honoured` (in particular
`no Authorization header`, the D-04 gate), and for an entry without `=`.
`LLM_ENDPOINT_PROBE_DRY=1` prints `WOULD WRITE <file> pin=<pin> profile=<profile> unreleased=<n>` and
exits 0 before contacting any server; that is how the naming and unreleased rules are tested offline.
Test seams: `LLM_ENDPOINT_PIN_FILE`, `LLM_ENDPOINT_RESULTS_DIR`.

### `just llm-endpoint-measure env profile label cold long conc budget`

Runs `probe/bench.py` against the deployed function through `aws lambda invoke --log-type Tail` (no
boto3). Defaults: `label=deployed cold=1 long=1 conc=2 budget=""`. It refuses without `APR_PROBE_KEY`
(`REFUSED key`) and without `.ids/function.json` (`REFUSED function`). The output is
`.ids/bench-<label>-<utc>.json` (gitignored) and a Markdown table; the recipe prints
`MEASURE OK <file>`. Plan 09-07 composes the evidence record from that file.

Shapes, in order: `cold_short_1tok` (after a `COLD_NONCE` bump), `short_1tok`, `short_64tok`,
`tools2_1tok`, `tools2_turn1`, `turn2_extend_1tok`, `turn2_extend_64tok`, `tools10_1tok`, `conc<N>`,
and `no_auth_401` (the statusCode is recorded, never asserted here). Every prompt carries a nonce.

Record keys: `label`, `function`, `image_digest`, `apr_tag`, `gguf_sha256`, `apr_version_line`,
`model`, `memory_mb`, `generated_at`, `cpu_part`, `price_usd_per_gb_s`, `budget_usd`, `spent_usd`,
`budget_stop`, `records[]`, `derived{}`. Each record carries `shape`, `status`, `client_wall_s`,
`duration_ms`, `billed_ms`, `init_ms` (cold only), `max_mem_mb`, `cost_usd`, `cpu_part` and
`cpu_part_source`. Every `$` figure is `memory/1024 * billed_ms/1000 * 0.0000133334`, computed from the
billed duration; `client_wall_s` is reported beside it and the two disagree (the spike saw up to 46 s).
`cpu_part` comes from the `[entry] host:` line of the record's own log tail, else from the cold
record, else from the newest such line in the log group; a record that initialised a new environment
without carrying its own line says so in `cpu_part_source`.

**The budget guard.** `budget` (USD) makes `bench.py` check, BEFORE each invocation, that the measured
spend so far plus a conservative per-shape ceiling stays within the budget. If not, no further sample
runs, the partial record is written with a `budget_stop` object (`before`, `spent_usd`, `est_usd`,
`budget_usd`; `null` otherwise) and the recipe prints `MEASURE STOPPED (budget)` and exits 4. The
ceilings (`EST_USD` in `bench.py`) are guard estimates derived from the spike-031 prices and are never
reported as measurements; `spent_usd` sums measured costs only. A failed invocation has no billed
duration and adds nothing to `spent_usd`, so a budget bounds samples that completed, not ones that
errored. Offline proofs: `python3 -I probe/bench.py --selftest` and
`--selftest-budget-only --budget-usd <x>` (exits 4 when `x` is smaller than the first estimate).

## Rules

- **No public surface before a 401 is proven (D-12).** The GGUF server has no auth of its own
  unless the entrypoint enforces `APR_API_KEY_HASH`; a Function URL with `AuthType: NONE` before
  `prove-401` has passed on the deployed function is forbidden. The recipes encode the order:
  `url` refuses without a fresh proof.
- **The live deploy is a human go/no-go (D-15).** Account and profile, region, auth posture, cost
  ceiling and the pin are confirmed by the operator before any AWS resource is created. Nothing
  in this plan's files creates one.
- **Spike numbers are context only (D-14).** Report wall time and billed duration both; they
  disagreed by up to 46 s through the adapter.
- **Never bake weights into the image.** Spikes 021 and 026 measured baked weights 7-18x worse.
- **Never read `$?` through a pipe** in a recipe or a verify; capture the status first.

## Identity lines every record carries

A record that omits any of these is not evidence:

- `apr --version`, taken from the `[entry]` line of the function's log (not from the host)
- the image digest the function actually resolved
- the GGUF sha256 (as the word `set` in logs; the value only in the record's identity field)
- `cpu_part`: Graviton2 versus Graviton3 handed identical functions a 2-3x speed difference, so
  two latencies without their `cpu_part` are not comparable

## Refusals

| Layer | Refusal | Meaning |
|---|---|---|
| image recipe | `sha-release` | the tarball hash is not the release's own sha256 file |
| image recipe | `sha-pin` | the tarball hash is not the `pin.env` pin |
| image recipe | `lwa-digest` | the Lambda Web Adapter digest in the Dockerfile differs from `pin.env` |
| image recipe | `identity` | the built image's own `apr --version` line does not contain the pinned tag |
| entrypoint | `auth` | `APR_API_KEY_HASH` is missing |
| entrypoint | `model-pin` | `MODEL_SHA256` is unset or not 64 hex |
| entrypoint | `model-sha` | the pulled GGUF does not hash to `MODEL_SHA256` |
| weights job | exit 9 | the downloaded GGUF does not hash to `GGUF_SHA256` |
| deploy ladder | `mode`, `env`, `pin`, `tarball`, `image`, `ladder`, `auth`, `ids`, `account`, `s3-sha`, `digest`, `readback` | see the mode contract above |
| url recipe | `proof`, `concurrency` | no public surface without a fresh 401 for the deployed digest and bounded concurrency |

## Constraints

- **One conversation per function.** `apr serve` keeps one session and serialises behind one
  Mutex: a foreign request resets the session, so a second concurrent conversation needs its own
  function environment (its own cold start and its own session state). Queue per conversation in
  the front door.
- **Buffered responses are limited to 6 MB** and a single invocation to **900 s**. Raise the
  client SDK's 60 s timeout and the gateway cap for this provider.
- **`cpu_part` lottery**: an identical function may land on Graviton2 or Graviton3, a 2-3x
  difference. Record it every time; never compare across it.
- **No Function URL streams inside a VPC**; this function uses default networking.

## The model, honestly (D-01)

The endpoint serves Qwen3.5-0.8B Q4_K_M, the smallest rung. **0.8B is CRUX-uncertified**
(known RED, ticket #4030; 2B is the smallest certified model) and loads with that warning. It
emitted correct tool calls in every spike-029 run, which is evidence about those runs and not a
certification. 2B and 4B are a later phase.

## Tests

`ladder_pin.py` is proven by controls that run against mutated copies of the ladder, so a pass
means the reader reads the file and not a constant:

1. **Altered pin**: every occurrence of the pin replaced; the reader prints the altered value.
2. **Unknown rung**: exit 3 with `NO PIN`.
3. **One-of-two alteration**: only one of the ladder's two occurrences of the pin replaced
   (the `known_red` row and the `inventory` row); exit 4 with `AMBIGUOUS PIN`. Replacing every
   occurrence together can never create two distinct values, so only this control reaches the
   guard. The control asserts its own premise (the pin appears on exactly two rows) so a changed
   ladder fails loudly instead of passing vacuously.
4. **No PyYAML**: `python3 -I -S` excludes site-packages; exit 5 with `REFUSED yaml`, no traceback.

The policy files are checked by loading each as JSON and classifying every statement (object
actions only on `__BUCKET__/*`, `s3:ListBucket` only on the bucket ARN, no statement mixing the
two, no wildcard resource outside the logs statement). The buildspec is checked to be a single
`commands` block that fetches only the 0.8B file with `GGUF_SHA256` from the environment and no
typed hash. Publishing safety: `bash scripts/check_include_files.sh` after adding this directory.
