# Deployment recipes for the SetFit MCP servers.
#
# The repo's quality gates live in the Makefile (tier1..tier4, coverage, contract
# audits) and stay there — this file is the deployment surface, which the
# Makefile never covered.
#
#   just --list                       # what is here
#
# Two deployments, two tools, easy to conflate — the names are by WHAT ships:
#   just build-trainer-asset          # the worker Lambda package
#   just synth-training dev           # validate the IaC, create nothing
#   just deploy-training dev          # CDK: table, bucket, WORKER (ours)
#   just pmcp-train-config dev        # write the request function's config from the stack
#   just pmcp-train-deploy            # cargo-pmcp: the REQUEST FUNCTION (pmcp.run)
#   just pmcp-train-grant dev         # attach the request function's IAM policy
#
# Arguments are POSITIONAL (`just synth-training dev`). `env=dev` is accepted
# too, because just passes it positionally rather than as an override and the
# resulting `--context env=env=dev` is a confusing way to learn that.

set shell := ["bash", "-uc"]

# Where the pinned encoder checkout lives. The SAME variable the core
# conformance suite, the apr-cli lifecycle suite and the train evidence suite
# read — a bespoke name here would be a sixth notion of "where the encoder is".
minilm_dir := env_var_or_default("APRENDER_MINILM_DIR", env_var("HOME") + "/.cache/aprender/minilm-l6-v2-1110a243")
target := "aarch64-unknown-linux-gnu"
asset := "deploy-extensions/assets/trainer"

# Chronos-Bolt weights (Phase 6 / D-18). `chronos_dir` is REPO-RELATIVE on
# purpose: `git check-ignore` and `git status -- models/` need the relative
# form, and overriding it (`just chronos_dir=/tmp/x fetch-chronos-tiny`) is how
# the tamper control runs against a copy. `chronos_abs` is the absolute form for
# the env-var hints — `justfile_directory()` is fixed at the workspace root,
# while `$PWD` follows any `cd` inside a recipe body.
chronos_rev := "a0e552de83495b5c28c14c71c374f3e33280b340"
chronos_dir := "models/chronos-bolt-tiny"
chronos_abs := justfile_directory() / chronos_dir

_default:
    @just --list

# Cross-compile the `apr` the worker spawns, for Lambda's arm64 runtime.
#
# `--no-default-features --features setfit` is load-bearing, not tidiness: the
# feature is dependency-closed, so this drops the GPU and inference stacks. With
# the defaults on, the build peaks past Docker's memory ceiling and dies with
# SIGKILL — that is what `cross` did before, and why this uses cargo-zigbuild
# (no Docker, host memory) instead.
build-apr-arm64:
    @command -v cargo-zigbuild >/dev/null || cargo install cargo-zigbuild
    cargo zigbuild --release --target {{target}} \
        --bin apr --no-default-features --features setfit
    @file target/{{target}}/release/apr | grep -q 'ARM aarch64' \
        || { echo "ERROR: not an aarch64 binary — check the target"; exit 1; }
    @ls -lh target/{{target}}/release/apr | awk '{print "  apr (arm64): " $5}'

# Cross-compile the training worker — the Lambda that actually runs `apr`.
#
# Same toolchain as `build-apr-arm64` and for the same reason: no Docker, so no
# memory ceiling to be SIGKILLed against. This binary is small (it supervises a
# child and talks to DynamoDB and S3); the weight in the package is `apr` and
# the encoder, not this.
build-trainer-arm64:
    @command -v cargo-zigbuild >/dev/null || cargo install cargo-zigbuild
    cargo zigbuild --release --target {{target}} \
        -p aprender-setfit-train-lambda --bin aprender-setfit-trainer
    @file target/{{target}}/release/aprender-setfit-trainer | grep -q 'ARM aarch64' \
        || { echo "ERROR: not an aarch64 binary — check the target"; exit 1; }
    @ls -lh target/{{target}}/release/aprender-setfit-trainer \
        | awk '{print "  trainer (arm64): " $5}'

# Assemble the worker's Lambda package: bootstrap + apr + dataset + encoder.
#
# The encoder ships as a SUBSET. The checkout carries both `model.safetensors`
# and `full_model.apr` at ~87 MB each, and the importer reads only the latter
# (`WEIGHT_FILE_CANDIDATES`), so copying the directory wholesale would put 87 MB
# of unread bytes into a package with a 250 MB ceiling.
build-trainer-asset: build-apr-arm64 build-trainer-arm64
    #!/usr/bin/env bash
    set -euo pipefail
    test -d "{{minilm_dir}}" || {
        echo "ERROR: no encoder checkout at {{minilm_dir}}"
        echo "       set APRENDER_MINILM_DIR to the pinned all-MiniLM-L6-v2 directory"
        exit 1
    }
    rm -rf "{{asset}}"
    mkdir -p "{{asset}}/assets/data" "{{asset}}/assets/encoder/1_Pooling"
    cp target/{{target}}/release/apr "{{asset}}/assets/apr"
    chmod +x "{{asset}}/assets/apr"
    cp -R data/tweet-eval-stance/. "{{asset}}/assets/data/"
    for f in full_model.apr tokenizer.json config.json modules.json full_manifest.json; do
        cp "{{minilm_dir}}/$f" "{{asset}}/assets/encoder/$f"
    done
    cp "{{minilm_dir}}/1_Pooling/config.json" "{{asset}}/assets/encoder/1_Pooling/config.json"
    # Lambda's Custom Runtime API requires the handler binary to be named
    # `bootstrap`; the workspace keeps a descriptive name so cargo-pmcp does not
    # mistake the worker for a second deployable. The rename happens here, in
    # the one place that knows it is building a Lambda package.
    cp "target/{{target}}/release/aprender-setfit-trainer" "{{asset}}/bootstrap"
    chmod +x "{{asset}}/bootstrap"
    # Both binaries are dynamically linked, so the runtime's glibc has to be new
    # enough. `provided.al2023` ships 2.34, and a toolchain bump that raises the
    # floor past it fails at INVOCATION with `GLIBC_2.xx not found` — after a
    # successful build, a successful deploy, and a client waiting on a task.
    # Measured 2026-09-03: both need at most 2.30.
    python3 - "{{asset}}/assets/apr" "{{asset}}/bootstrap" <<'PY'
    import re, sys
    CEILING = 34  # provided.al2023
    worst = 0
    for path in sys.argv[1:]:
        need = [int(v) for v in re.findall(rb'GLIBC_2\.(\d+)', open(path, 'rb').read())]
        top = max(need, default=0)
        worst = max(worst, top)
        print(f"  glibc floor: {path.split('/')[-1]} needs <= 2.{top}")
        if top > CEILING:
            sys.exit(
                f"ERROR: {path} needs GLIBC_2.{top}, above provided.al2023's 2.{CEILING}.\n"
                f"       Pin the target instead: cargo zigbuild --target "
                f"aarch64-unknown-linux-gnu.2.{CEILING}"
            )
    PY
    du -sh "{{asset}}" | awk '{print "  worker package: " $1 " (Lambda zip limit 250 MB)"}'

# Validate the IaC. Creates nothing, contacts no account.
synth-training env="dev" memory_mb="":
    @cd deploy-extensions && npx cdk synth --context env={{ trim_start_match(env, "env=") }} \
        {{ if memory_mb == "" { "" } else { "--context trainerMemoryMb=" + memory_mb } }} --quiet
    @echo "  synth OK for env={{ trim_start_match(env, "env=") }}"

# Show what a deploy WOULD change, against the real account.
diff-training env="dev" memory_mb="" profile="ze-kasher-dev":
    cd deploy-extensions && npx cdk diff --context env={{ trim_start_match(env, "env=") }} \
        {{ if memory_mb == "" { "" } else { "--context trainerMemoryMb=" + memory_mb } }} \
        --profile {{profile}}

# Create/update the training infrastructure. Real resources, real money.
#
# The optional second argument overrides the worker's memory, for an account
# whose Lambda ceiling is below the measured 4 GB envelope (a fresh account is
# capped at 3008 MB, and that ceiling is an AWS Support case, not a Service
# Quota). `just deploy-training dev 3008` deploys under it; synth then WARNS
# that training is expected to OOM, which is the point of measuring.
#
# `--require-approval never` because `diff-training` IS the review gate: it
# prints the IAM changes in full and is the documented step before this one.
# Keeping the interactive prompt here would only mean the recipe cannot run
# unattended, while the review still happened in a different command.
deploy-training env="dev" memory_mb="" profile="ze-kasher-dev":
    cd deploy-extensions && npx cdk deploy --context env={{ trim_start_match(env, "env=") }} \
        {{ if memory_mb == "" { "" } else { "--context trainerMemoryMb=" + memory_mb } }} \
        --profile {{profile}} --require-approval never

# Tear it down. dev destroys data by design; prod RETAINs the table and bucket.
destroy-training env="dev" profile="ze-kasher-dev":
    cd deploy-extensions && npx cdk destroy --context env={{ trim_start_match(env, "env=") }} --profile {{profile}}

# Point the request function at the resources the CDK stack created.
#
# The three names live in SSM because the request function is deployed by
# cargo-pmcp from a DIFFERENT app and cannot take a cross-stack reference. Two
# of the three are predictable by convention; the artifact bucket is not — it
# carries the account id for global uniqueness — so this reads all three from
# the stack rather than teaching anyone to write two down and look one up.
#
# Writes `.pmcp/deploy.toml`, which is GITIGNORED, from the tracked
# `.pmcp/deploy.toml.template`. Generated rather than edited in place because
# the result embeds the AWS account id, and this tree is destined for a public
# upstream repo. Regenerating is cheap; un-committing an account id is not.
pmcp-train-config env="dev" profile="ze-kasher-dev":
    #!/usr/bin/env bash
    set -euo pipefail
    ENV="{{ trim_start_match(env, "env=") }}"
    # The SAME guard bin/app.ts applies, because this recipe reaches the account
    # WITHOUT going through the CDK app and so inherits none of its validation.
    # Unguarded, a typo becomes an SSM path and comes back as
    # "Parameter name: can't be prefixed with ssm" — a message about a rule the
    # caller did not break, naming nothing they typed.
    case "$ENV" in
        dev|prod) ;;
        *) echo "ERROR: '$ENV' is not a known environment (expected dev or prod)" >&2
           exit 2 ;;
    esac
    DIR="crates/.pmcp"
    get() {
        aws ssm get-parameter --profile "{{profile}}" \
            --name "/aprender/setfit-train/${ENV}/$1" \
            --query Parameter.Value --output text
    }
    TABLE="$(get tasks-table)"
    BUCKET="$(get artifact-bucket)"
    TRAINER="$(get trainer-function-name)"
    python3 - "$DIR/deploy.toml.template" "$DIR/deploy.toml" "$TABLE" "$BUCKET" "$TRAINER" <<'PY'
    import sys
    template, out, table, bucket, trainer = sys.argv[1:6]
    text = open(template).read()
    values = {
        "APRENDER_SETFIT_TASKS_TABLE": table,
        "APRENDER_SETFIT_ARTIFACT_BUCKET": bucket,
        "APRENDER_SETFIT_TRAINER_FUNCTION": trainer,
    }
    for key, value in values.items():
        marker = f'{key} = "UNSET-run-just-pmcp-train-config"'
        if marker not in text:
            sys.exit(f"{template} has no placeholder for {key}; restore it before rerunning")
        text = text.replace(marker, f'{key} = "{value}"')
    open(out, "w").write(text)
    for key, value in values.items():
        print(f"  {key:34s} {value}")
    PY
    echo "  wrote $DIR/deploy.toml (gitignored) for env=$ENV"

# Grant the deployed request function access to the table and the worker.
#
# Runs AFTER `cargo pmcp deploy`, not before: pmcp.run creates the function's
# execution role, so there is nothing to attach to until it has. Between the
# deploy and this, the server is live and every `train` call compensates to
# `failed` with an AccessDenied — a clear error rather than a hang, but not a
# working server.
#
# The role name carries a random suffix (pmcp-<hash>-<server>-ExecutionRole-<id>),
# so it is DISCOVERED from the function rather than written down, and the policy
# document is read from the stack output so there is one source of truth for it.
#
# Idempotent — `put-role-policy` replaces by name. Worth re-running after any
# pmcp.run redeploy: this is an out-of-band change to a role that a
# platform-owned CloudFormation stack manages, and a stack update may drop it.
pmcp-train-grant env="dev" profile="ze-kasher-dev" server="aprender-setfit-train":
    #!/usr/bin/env bash
    set -euo pipefail
    ENV="{{ trim_start_match(env, "env=") }}"
    case "$ENV" in
        dev|prod) ;;
        *) echo "ERROR: '$ENV' is not a known environment (expected dev or prod)" >&2
           exit 2 ;;
    esac
    ROLE_ARN="$(aws lambda get-function --profile "{{profile}}" \
        --function-name "{{server}}" --query Configuration.Role --output text 2>/dev/null)" || {
        echo "ERROR: no Lambda named '{{server}}' — deploy the request function first:" >&2
        echo "       just pmcp-train-deploy" >&2
        exit 1
    }
    ROLE="${ROLE_ARN##*/}"
    POLICY="$(aws cloudformation describe-stacks --profile "{{profile}}" \
        --stack-name "aprender-setfit-training-${ENV}" \
        --query "Stacks[0].Outputs[?OutputKey=='RequestLambdaPolicy'].OutputValue" \
        --output text)"
    test -n "$POLICY" || { echo "ERROR: stack aprender-setfit-training-${ENV} has no RequestLambdaPolicy output" >&2; exit 1; }
    aws iam put-role-policy --profile "{{profile}}" \
        --role-name "$ROLE" \
        --policy-name "aprender-setfit-train-${ENV}" \
        --policy-document "$POLICY"
    echo "  granted on role: $ROLE"
    echo "  policy:          aprender-setfit-train-${ENV}"

# Deploy the TRAINING request function to pmcp.run.
#
# Two things this exists to stop you forgetting, both of which cost a deploy to
# learn — one of them a deploy that SUCCEEDED and served the wrong server:
#
# 1. `--manifest-path crates`. cargo-pmcp picks the package to build in
#    `find_lambda_package_dir`: first `<deploy-root>/{server_name}-lambda`, then
#    the FIRST `*-lambda` workspace package with a `bootstrap` binary. Two
#    packages match that fallback here and the predict one sorts first, so
#    anything but the exact deploy root builds aprender-mcp-setfit-lambda and
#    ships it under this server's name. It does not warn: the endpoint comes up
#    healthy and every MCP call answers with the predict binary's
#    "no embedded model in this build".
#
# 2. `ulimit -n`. Linking the aarch64 bootstrap opens ~245 object files through
#    cargo-zigbuild's wrapper; under macOS's default soft limit the link dies
#    with `ProcessFdQuotaExceeded`, which reads like a toolchain fault rather
#    than a shell setting. 65536 is ample and the hard limit is unlimited.
#
# The tell that it is building the right thing: `aprender-setfit-train-lambda`
# in the compile log. `aprender-mcp-setfit-lambda` means it is not.
#
# After this, `just pmcp-train-grant <env>` — the function has no access to the
# table or the worker until it runs.
pmcp-train-deploy target="":
    #!/usr/bin/env bash
    set -euo pipefail
    ulimit -n 65536 || echo "WARNING: could not raise the fd limit; a link may fail with ProcessFdQuotaExceeded" >&2
    CONFIG="crates/.pmcp/deploy.toml"
    test -f "$CONFIG" || {
        echo "ERROR: $CONFIG does not exist — generate it from the stack first:" >&2
        echo "       just pmcp-train-config dev" >&2
        exit 1
    }
    grep -q 'UNSET-run-just-pmcp-train-config' "$CONFIG" && {
        echo "ERROR: $CONFIG still holds UNSET placeholders; regenerate it:" >&2
        echo "       just pmcp-train-config dev" >&2
        exit 1
    }
    cargo pmcp deploy --manifest-path crates \
        {{ if target == "" { "" } else { "--target " + target } }} --no-color

# Pack an attested benchmark directory for `dataset_upload_url`.
#
# The archive is FLAT — `tar -C <dir> .` — so `selection-manifest.json` sits at
# its root. The worker also accepts the one-directory-down layout `tar` makes
# when run beside the directory, so this is convenience, not a requirement.
#
#   just dataset-pack                                     # the packaged benchmark
#   just dataset-pack path/to/my-attested-dir out.tar.gz
#
# Then, from an MCP client:
#   1. call dataset_upload_url            -> upload_url, dataset_uri
#   2. curl -X PUT --upload-file <out> "<upload_url>"
#   3. call train with {config, dataset_uri}
dataset-pack dir="data/tweet-eval-stance" out="/tmp/setfit-dataset.tar.gz":
    #!/usr/bin/env bash
    set -euo pipefail
    test -f "{{dir}}/selection-manifest.json" || {
        echo "ERROR: {{dir}} has no selection-manifest.json — run \`apr data select\` on it first" >&2
        exit 1
    }
    tar -czf "{{out}}" -C "{{dir}}" .
    ls -lh "{{out}}" | awk '{print "  packed: " $9 " (" $5 ")"}'
    tar -tzf "{{out}}" | sed 's|^\./||' | grep -v '^$' | sort | sed 's/^/    /'

# Fetch amazon/chronos-bolt-tiny at the PINNED revision, verify it, derive f16.
#
# NEVER run from CI's default gate — this reaches the network on a cold box and
# writes ~50 MB into `/models/`, which is root-anchored gitignored (CB-510), so
# no weight ever becomes committable. Weights are Apache-2.0 (amazon/chronos-bolt-tiny).
#
# VERIFY-ALWAYS, not fetch-if-missing (REVIEW-06-03). The download is conditional
# on the file being ABSENT; the hashing is not. A file that is already present —
# cached, mounted, restored by a CI cache action, or edited — is re-hashed on every
# run and the recipe exits non-zero naming it. A pre-existing weight file can
# therefore never be used unverified, which is the whole point: the caller
# (`just chronos-gate`, plan 06-08) invokes this unconditionally.
#
# WHAT EACH PIN PROVES. The f32 `model.safetensors` and `config.json` sha256s are
# the UPSTREAM pins — they are what the Hub served at revision {{chronos_rev}}.
# The f16 sha is a LOCAL-INTEGRITY pin: that file is re-derived here from the
# already-verified f32, never downloaded, so it proves the derivation was not
# tampered with, NOT provenance.
#
# The enforced f16 value (f5dc2ef5…) is what safetensors 0.8.x reproduces on this
# toolchain. Spike 007 recorded f9a033b42bc516e17ae5756317cb946121afdb59c94b4acfcd30fef93317cd4c
# for the same weights; the two files differ in exactly 6 bytes — the ORDER of the
# two keys inside the `__metadata__` JSON object — and are otherwise byte-identical
# (same 11 640-byte header length, same tensor entries in the same order, and a
# byte-identical 17 305 344-byte body). Newer safetensors emits `format` before
# `converted` regardless of the dict order passed in. RESEARCH A3 flagged exactly
# this risk and called the f16 sha advisory; it is pinned here anyway so the check
# is real, and re-pinned to the value this toolchain actually produces.
#
#   just fetch-chronos-tiny
#   CHRONOS_MODEL_DIR=<abs>/f16 cargo test -p aprender-forecast --lib   # arms the gated tests
#
# Fetch + verify the pinned Chronos-Bolt-tiny weights into /models/ (not for CI's gate).
fetch-chronos-tiny:
    #!/usr/bin/env bash
    set -euo pipefail
    uv run --quiet --python 3.12 --with huggingface_hub --with safetensors --with numpy \
        python - "{{chronos_rev}}" "{{chronos_dir}}" <<'PY'
    import hashlib, os, shutil, sys
    from huggingface_hub import hf_hub_download
    import numpy as np
    from safetensors.numpy import load_file, save_file

    REPO = "amazon/chronos-bolt-tiny"
    rev, root = sys.argv[1], sys.argv[2]

    # UPSTREAM pins: what the Hub served at `rev`.
    F32_PINS = {
        "model.safetensors": "75068728d376d2bec670379eeef4bfb4d24c0cfe24d957451f8d19b447030a32",
        "config.json": "278f0086733031635fb1c861cb01c1bad6477420c7fcb19381a2993e335785e0",
    }
    # LOCAL-INTEGRITY pin: re-derived here, never downloaded. See the header comment
    # for why this differs from spike 007's advisory value in 6 metadata bytes.
    F16_PIN = "f5dc2ef53533c8896bcb120a754c52c39d8917c15750a9e845192014dfa74a67"
    F16_ADVISORY = "f9a033b42bc516e17ae5756317cb946121afdb59c94b4acfcd30fef93317cd4c"
    F16_META = {"format": "pt", "converted": "f32->f16 by spike 007 tools/to_f16.py"}

    def sha256(path):
        h = hashlib.sha256()
        with open(path, "rb") as fh:
            for chunk in iter(lambda: fh.read(1 << 20), b""):
                h.update(chunk)
        return h.hexdigest()

    f32 = os.path.join(root, "f32")
    f16 = os.path.join(root, "f16")
    os.makedirs(f32, exist_ok=True)

    # (1) Fetch ONLY what is absent. A present file is never overwritten: a bad hash
    # on a present file is a supply-chain event, not a cache miss, and re-fetching
    # over it would erase the evidence.
    for name in F32_PINS:
        if not os.path.isfile(os.path.join(f32, name)):
            print(f"  download {name} from {REPO} @ {rev}")
            hf_hub_download(REPO, name, revision=rev, local_dir=f32)

    # (2) Verify ALWAYS — download or not.
    bad = []
    for name, want in sorted(F32_PINS.items()):
        p = os.path.join(f32, name)
        if not os.path.isfile(p):
            sys.exit(f"FAIL: {p} is still missing after fetch")
        got = sha256(p)
        print(f"  f32/{name:<18} {got}  pin {want}")
        if got != want:
            bad.append(f"{p}\n      computed sha256 {got}\n      pinned   sha256 {want}")
    if bad:
        sys.exit("FAIL: sha256 mismatch — a supply-chain event, not a cache miss:\n    "
                 + "\n    ".join(bad))

    # (3) f16 is DERIVED from the already-verified f32. Absent or mismatching -> drop
    # the whole directory and re-derive deterministically, then re-hash.
    p16 = os.path.join(f16, "model.safetensors")
    got16 = sha256(p16) if os.path.isfile(p16) else None
    if got16 != F16_PIN or not os.path.isfile(os.path.join(f16, "config.json")):
        shutil.rmtree(f16, ignore_errors=True)
        os.makedirs(f16, exist_ok=True)
        tensors = load_file(os.path.join(f32, "model.safetensors"))
        save_file({k: v.astype(np.float16) for k, v in tensors.items()}, p16, metadata=F16_META)
        shutil.copy(os.path.join(f32, "config.json"), os.path.join(f16, "config.json"))
        got16 = sha256(p16)
    print(f"  f16/{'model.safetensors':<18} {got16}  pin {F16_PIN}")
    print(f"      (spike 007 recorded {F16_ADVISORY} — same tensors, __metadata__ key order differs)")
    if got16 != F16_PIN:
        sys.exit(f"FAIL: sha256 mismatch for {p16} after a clean re-derivation:\n"
                 f"      computed sha256 {got16}\n      pinned   sha256 {F16_PIN}\n"
                 "      The f32 source verified, so this is a local derivation change — most\n"
                 "      likely a safetensors/numpy version that orders the header differently.\n"
                 "      Re-measure and re-pin F16_PIN in the justfile; do not loosen the check.")
    PY
    echo "  CHRONOS_MODEL_DIR={{chronos_abs}}/f16   CHRONOS_EMBED_DIR={{chronos_abs}}/f16"
