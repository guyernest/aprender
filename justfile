# Deployment recipes for the SetFit MCP servers.
#
# The repo's quality gates live in the Makefile (tier1..tier4, coverage, contract
# audits) and stay there — this file is the deployment surface, which the
# Makefile never covered.
#
#   just --list                       # what is here
#   just build-trainer-asset          # the worker Lambda package
#   just synth-training dev           # validate the IaC, create nothing
#   just deploy-training dev          # create/update the AWS resources
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
        -p aprender-mcp-setfit-train-lambda --bin aprender-setfit-trainer
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
    DIR="crates/aprender-mcp-setfit-train-lambda/.pmcp"
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
        echo "       cargo pmcp deploy --manifest-path crates/aprender-mcp-setfit-train-lambda --target <t>" >&2
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
# Two things this exists to stop you forgetting, both of which cost a full
# failed build to learn:
#
# 1. `--manifest-path`. The workspace has TWO `bootstrap` binaries and the repo
#    ROOT holds the PREDICT server's .pmcp/deploy.toml, so a bare
#    `cargo pmcp deploy` from the repo root resolves project
#    `aprender-setfit-predict` and builds aprender-mcp-setfit-lambda — the wrong
#    server, silently, until you read which crate it compiled. Verified with
#    `cargo pmcp deploy outputs`: with the flag it resolves aprender-setfit-train,
#    without it aprender-setfit-predict.
#
# 2. `ulimit -n`. Linking the aarch64 bootstrap opens ~245 object files through
#    cargo-zigbuild's wrapper; under macOS's default soft limit the link dies
#    with `ProcessFdQuotaExceeded`, which reads like a toolchain fault rather
#    than a shell setting. 65536 is ample and the hard limit is unlimited.
#
# After this, `just pmcp-train-grant <env>` — the function has no access to the
# table or the worker until it runs.
pmcp-train-deploy target="":
    #!/usr/bin/env bash
    set -euo pipefail
    ulimit -n 65536 || echo "WARNING: could not raise the fd limit; a link may fail with ProcessFdQuotaExceeded" >&2
    CONFIG="crates/aprender-mcp-setfit-train-lambda/.pmcp/deploy.toml"
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
    cargo pmcp deploy --manifest-path crates/aprender-mcp-setfit-train-lambda \
        {{ if target == "" { "" } else { "--target " + target } }} --no-color
