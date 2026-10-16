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

## Recipes, in the D-12 order

| Order | Recipe | What it does |
|---|---|---|
| 1 | `just llm-endpoint-up` | the environment's bucket, ECR repository, roles and log group, all tagged |
| 2 | `just llm-endpoint-weights mode=copy` or `codebuild` | stage the GGUF into the bucket (CodeBuild never crosses the operator uplink) |
| 3 | `just llm-endpoint-image push=1` | build and push the image to the environment's repository |
| 4 | `just llm-endpoint-deploy` | create or update the function (`DRY_RUN=1` offline, `PREFLIGHT=1` connected read-only, neither = live) |
| 5 | `just llm-endpoint-prove-401` | invoke the deployed function without a key and prove it answers 401 |
| 6 | `just llm-endpoint-url` | create the public URL, refused without a fresh 401 proof |
| 7 | probe | the conformance probe against the deployed function |
| 8 | `just llm-endpoint-measure` | the D-14 measurement |
| 9 | `just llm-endpoint-teardown` | remove what the phase created |

`just llm-endpoint-warm` and `just llm-endpoint-selftest` sit outside the order. The deploy
ladder's refusals are `mode`, `env`, `pin`, `tarball`, `image`, `ladder`, `auth`, `ids`,
`account`, `s3-sha`, `digest` and `readback`; the URL recipe's are `proof` and `concurrency`.
Plan 09-06 owns these recipes and documents the environment-specific resource names (the
repository is per environment, never shared) in this file.

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
| deploy ladder | see above | plan 09-06 |

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
