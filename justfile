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
# The declared Laya base snapshot (laya-finetune-gate-v1 `base`, revision 55cf4c4e…). The SAME variable
# the env-gated aprender-decide integration targets read, so one override arms both (plan 08-12).
laya_model_dir := env_var_or_default("LAYA_MODEL_DIR", env_var("HOME") + "/.cache/huggingface/hub/models--convaiinnovations--laya/snapshots/55cf4c4ebb4ebe31b2550e8bdf3bd21b99753851")
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

# Deploy the Chronos-Bolt zero-shot forecasting MCP server to pmcp.run
pmcp-chronos-deploy target="":
    cargo pmcp deploy --manifest-path crates/aprender-mcp-chronos-lambda \
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
#   CHRONOS_MODEL_DIR=<abs>/f32 cargo test -p aprender-forecast --lib   # arms the gated tests
#
#   f32, NOT f16. `armed_dir()` (chronos.rs) returns CHRONOS_MODEL_DIR verbatim and the
#   bolt::parity ladders load from it to compare against the PYTHON oracle on an ABSOLUTE
#   bar; f16 weights there miss `input_embeds` by ~4e-4 against a 2e-5 bar — quantization
#   error, not a regression, and not a reason to loosen an f32 bar SC4 names.
#   The f16 path is already covered by its own test on its own relative bar
#   (chronos::tests::f16_weights_within_two_percent_of_std, equations.f16_rel_std = 2 % of
#   series std); it finds the f16 directory itself via `.parent().join("f16")`, so pointing
#   this variable at f32 arms BOTH.
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
    echo "  CHRONOS_MODEL_DIR={{chronos_abs}}/f32   # f32: the ladders compare to the Python oracle; the f16 test finds ../f16 itself"

# ── Phase 6 host-gated evidence recipes ──────────────────────────────────────
#
# The timing, size and concurrency bars SC1/SC4/SC5 name hold on an AARCH64
# RELEASE build with the spike-008 NEON microkernel behind `trueno::gemm_blis`.
# CI is `[self-hosted, X64, Linux, clean-room]` and builds debug for the lib
# leg, so it asserts PARITY and REFUSALS instead and never these numbers
# (06-RESEARCH Open Question 5). Every recipe below therefore states the bar it
# enforces, writes its raw output to `target/p06-*.log`, and the measured values
# are recorded with host, profile and commit in
# `.planning/phases/06-native-time-series-forecasting-stack/06-EVIDENCE.md`.
#
# SHELL DISCIPLINE (CLAUDE.md "Verification Discipline" #1). Every recipe is a
# `#!/usr/bin/env bash` body with `set -euo pipefail`, and every exit status is
# captured with `rc=$?` on ITS OWN LINE, before any `grep`/`tail`/`awk` touches
# the log. A pipeline's `$?` is the LAST command's status, so capturing it after
# a pipe into grep reports GREP — which is how a gate ends up unable to fail.
# `set +e` brackets each measured command so `set -e` cannot abort before the
# capture. (The guard for this rule greps the justfile itself, so this comment
# deliberately describes the anti-pattern rather than spelling it.)
#
#   just chronos-gate          # D-18 clause 2, local form: weights + armed tests
#   just chronos-embed-build   # SC4: embedded release binary < 30 MB
#   just chronos-bench         # SC4: tiny-f16 forward at 2048 context < 100 ms
#   just chronos-coldstart 5   # SC4: exec -> first forecast < 150 ms (median)
#   just forecast-bench        # SC1: 3 000-point Prophet round trip < 2 s
#   just forecast-pool-ratio   # SC5: best-of-3 sequential/concurrent >= 2.0
#   just mase-rolling-origin   # D-16: the rolling-origin accuracy table

# SC4 binary-size bar: the embedded tiny-f16 release binary must stay under 30 MB.
#
# `contracts/chronos-bolt-parity-v1.yaml` (line ~220) is explicit that SC4's
# "< 30 MB" is a BINARY-SIZE bar, not a resident-memory one, which is why this
# measures the linked artifact and not RSS. `wc -c` rather than `stat`: `stat`
# takes `-c%s` on GNU and `-f%z` on BSD, and this box is BSD.
# Build the embedded tiny-f16 release binary and enforce the < 30 MB SC4 bar.
chronos-embed-build:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target
    LOG=target/p06-chronos-embed-build.log
    set +e
    CHRONOS_EMBED_DIR={{chronos_abs}}/f16 CARGO_INCREMENTAL=0 \
        cargo build --release -p aprender-mcp-chronos > "$LOG" 2>&1
    rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then
        tail -30 "$LOG"
        echo "FAIL: embedded release build exited $rc - full log $LOG" >&2
        exit "$rc"
    fi
    # `$(( ... ))` both strips BSD wc's leading padding and proves the value is
    # an integer; `${bytes//[[:space:]]/}` did the same but bashrs mis-parses the
    # character class as an unterminated `[` test.
    bytes=$(( $(wc -c < target/release/aprender-mcp-chronos) ))
    echo "  binary: target/release/aprender-mcp-chronos ($bytes bytes, CHRONOS_EMBED_DIR={{chronos_abs}}/f16)"
    if [ "$bytes" -ge 30000000 ]; then
        echo "FAIL: $bytes bytes is at or above the 30000000-byte SC4 bar" >&2
        exit 1
    fi
    echo "  SIZE OK: $bytes < 30000000 bytes (SC4)"

# The phase's embedded-weights gate (D-18 clause 2, local form).
#
# REVIEW-06-03, verified MEDIUM-HIGH. `just fetch-chronos-tiny` runs
# UNCONDITIONALLY and is never guarded on the weights being absent. That recipe
# is verify-always: present files are re-hashed against their pins on every run
# and it exits non-zero naming any mismatch. Gating the call on file absence is
# precisely what let a cached, pre-mounted or tampered weights directory reach
# the parity tests with its sha256 never checked — and the CI leg this phase
# proposes would mount weights across a trust boundary. Its output is echoed
# into this gate's own stdout so the pins it verified are part of the evidence.
#
# The two positional filters on the aprender-forecast command are deliberate:
# modern libtest unions positional filters (verified on cargo 1.98.0:
# `--lib -- alpha:: beta::` reported `2 passed; 2 filtered out`). Non-vacuity
# does not rest on that anyway — the gate requires `0 ignored` AND at least one
# passing test in BOTH summaries, so a filter that matched nothing would fail.
# THE embedded-weights gate: verify the pinned weights, then run both armed suites.
# MANUAL BY DECISION, NOT BY OVERSIGHT (UAT item 3, D-ITEM-06-03, decided 2026-09-07).
# This gate runs NOWHERE automatically: .github/workflows/ci.yml contains zero `chronos`
# and zero `forecast` matches, and that was ACCEPTED rather than fixed. Every SC4 parity
# claim therefore rests on someone running THIS recipe. A green recorded in a SUMMARY is
# evidence that it passed once, on the machine that ran it — not that it is enforced.
chronos-gate:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target
    LOG0=target/p06-chronos-gate-weights.log
    LOG1=target/p06-chronos-gate-forecast.log
    LOG2=target/p06-chronos-gate-server.log
    set +e
    just fetch-chronos-tiny > "$LOG0" 2>&1
    rc=$?
    set -e
    rc0="$rc"
    cat "$LOG0"
    if [ "$rc0" -ne 0 ]; then
        echo "FAIL: weight verification exited $rc0 - no test was run" >&2
        exit "$rc0"
    fi
    set +e
    CHRONOS_MODEL_DIR={{chronos_abs}}/f32 CARGO_INCREMENTAL=0 \
        cargo test -p aprender-forecast --lib -- bolt::parity chronos::parity > "$LOG1" 2>&1
    rc=$?
    set -e
    rc1="$rc"
    set +e
    CHRONOS_EMBED_DIR={{chronos_abs}}/f16 CHRONOS_MODEL_DIR={{chronos_abs}}/f32 CARGO_INCREMENTAL=0 \
        cargo test -p aprender-mcp-chronos --lib > "$LOG2" 2>&1
    rc=$?
    set -e
    rc2="$rc"
    fail=0
    if [ "$rc1" -ne 0 ]; then
        tail -30 "$LOG1"
        echo "FAIL: aprender-forecast parity tests exited $rc1 - log $LOG1" >&2
        fail=1
    fi
    if [ "$rc2" -ne 0 ]; then
        tail -30 "$LOG2"
        echo "FAIL: aprender-mcp-chronos tests exited $rc2 - log $LOG2" >&2
        fail=1
    fi
    # An ARMED suite that reports `1 ignored` is a weights test that skipped
    # itself, which is exactly the failure this gate exists to catch.
    check_summary() {
        label=$1
        log=$2
        summary=$(grep -E '^test result:' "$log" | tail -1)
        if [ -z "$summary" ]; then
            echo "FAIL: $label produced no 'test result:' summary - log $log" >&2
            return 1
        fi
        echo "  $label: $summary"
        case "$summary" in
            *"0 ignored"*) ;;
            *)
                echo "FAIL: $label did not report 0 ignored - the weights tests were not armed" >&2
                return 1
                ;;
        esac
        passed=$(printf '%s\n' "$summary" | sed -n 's/^test result: ok\. \([0-9][0-9]*\) passed.*/\1/p')
        if [ -z "$passed" ] || [ "$passed" -lt 1 ]; then
            echo "FAIL: $label reported fewer than 1 passing test - a vacuous green" >&2
            return 1
        fi
        return 0
    }
    check_summary "aprender-forecast (bolt::parity + chronos::parity)" "$LOG1" || fail=1
    check_summary "aprender-mcp-chronos (--lib)" "$LOG2" || fail=1
    if [ "$fail" -ne 0 ]; then
        exit 1
    fi
    echo "CHRONOS GATE: PASS"

# SC4 latency bar: the tiny-f16 forward at 2 048 context must stay under 100 ms.
#
# Reads the D-14 PRODUCTION row (`fast + attn_gemm + dot8`) of the f16 section,
# because that is the routing the server actually takes; the other rows in the
# table are the variants it is measured against, not what ships.
# Kernel/latency tables, and the < 100 ms SC4 bar on the tiny-f16 2 048-context forward.
chronos-bench: chronos-embed-build
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target
    LOG=target/p06-chronos-bench.log
    set +e
    target/release/aprender-mcp-chronos --bench {{chronos_abs}}/f32 {{chronos_abs}}/f16 > "$LOG" 2>&1
    rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then
        tail -20 "$LOG"
        echo "FAIL: --bench exited $rc - log $LOG" >&2
        exit "$rc"
    fi
    cat "$LOG"
    ms=$(awk -F'|' '/^### /{f16 = ($0 ~ /weights F16/); next} f16 && /D-14 production/ {gsub(/[^0-9.]/, "", $3); print $3; exit}' "$LOG")
    if [ -z "$ms" ]; then
        echo "FAIL: no f16 D-14-production row in $LOG - the bar was never measured" >&2
        exit 1
    fi
    echo "  tiny-f16 forward at 2048 context (D-14 production routing): $ms ms"
    # IN-01: the bar goes through the shared shape-checking validator, never
    # through `awk -v v="$ms" '{ exit (v + 0 < 100) }'` — that coerced a
    # non-numeric token to 0, and 0 is under a 100 ms bar. Note the UNITS here
    # are milliseconds, not seconds: this site and the 2 s SC1 sites share one
    # validator and NOT one bar.
    if ! bash scripts/assert_measurement_under.sh under "$ms" 100 "tiny-f16 forward (SC4)"; then
        echo "FAIL: $ms ms is at or above the 100 ms SC4 bar" >&2
        exit 1
    fi
    echo "  FORWARD OK: $ms ms < 100 ms (SC4)"

# SC4 cold-start bar: median exec -> first forecast reply under 150 ms.
#
# `--coldstart` spawns THIS binary as a stdio MCP server, so the embedded build
# is what is timed: process exec + weight decode + initialize + one forecast.
# Time exec -> first forecast over stdio N times; enforce the < 150 ms SC4 median bar.
chronos-coldstart N="3": chronos-embed-build
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target
    LOG=target/p06-chronos-coldstart.log
    set +e
    target/release/aprender-mcp-chronos --coldstart {{N}} > "$LOG" 2>&1
    rc=$?
    set -e
    cat "$LOG"
    if [ "$rc" -ne 0 ]; then
        echo "FAIL: --coldstart exited $rc - log $LOG" >&2
        exit "$rc"
    fi
    med=$(sed -n 's/^median: initialize [0-9][0-9]* ms, forecast \([0-9][0-9]*\) ms.*/\1/p' "$LOG")
    if [ -z "$med" ]; then
        echo "FAIL: no median line in $LOG - the bar was never measured" >&2
        exit 1
    fi
    echo "  median: exec to first forecast reply $med ms over {{N}} runs"
    if [ "$med" -ge 150 ]; then
        echo "FAIL: $med ms is at or above the 150 ms SC4 bar" >&2
        exit 1
    fi
    echo "  COLD START OK: $med ms < 150 ms (SC4)"

# SC1 bar: a 3 000-point daily Prophet fit + 365-step predict under 2 s total.
forecast-bench:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target
    LOG=target/p06-forecast-bench.log
    set +e
    CARGO_INCREMENTAL=0 cargo run --release -p aprender-mcp-forecast -- --bench 1000 3000 > "$LOG" 2>&1
    rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then
        tail -20 "$LOG"
        echo "FAIL: --bench exited $rc - log $LOG" >&2
        exit "$rc"
    fi
    grep -E '^\| ' "$LOG" || true
    total=$(awk -F'|' '$2 + 0 == 3000 && $3 ~ /prophet/ {gsub(/[^0-9.]/, "", $6); print $6; exit}' "$LOG")
    if [ -z "$total" ]; then
        echo "FAIL: no 3000-point prophet row in $LOG - the bar was never measured" >&2
        exit 1
    fi
    echo "  3000-point prophet fit + 365-step predict: $total s total"
    # IN-01: shared validator, not the inline awk coercion. See
    # scripts/assert_measurement_under.sh and its 23-row case table.
    if ! bash scripts/assert_measurement_under.sh under "$total" 2.0 "ROUND TRIP (SC1)"; then
        echo "FAIL: $total s is at or above the 2.0 s SC1 bar" >&2
        exit 1
    fi
    echo "  ROUND TRIP OK: $total s < 2.0 s (SC1)"

# SC5 bar: sequential wall / concurrent wall >= 2.0, BEST OF THREE.
#
# REVIEW-06-04, both reviewers independently. THIS RECIPE owns the ratio bar —
# `pool_equality` asserts only bit-identical responses under load and PRINTS the
# ratio on one machine-parsable line. A wall-clock ratio inside libtest moves
# with CPU throttling and background load independently of the router
# serialisation the pool removes, so a hard `assert!(speedup >= 2.0)` there
# fails for reasons the pool does not control, and a suite that cries wolf gets
# its real failures ignored.
#
# THE RETRIES ARE FOR THROTTLING, NOT FOR ASSERTIONS. A non-zero cargo exit is a
# CORRECTNESS failure — a response differed under load — and fails the whole
# recipe on the spot. Only the ratio, a wall-clock measurement, is taken
# best-of-3; three low ratios are reported as a real SC5 failure with all three
# numbers.
# Run pool_equality up to 3x on release and enforce the >= 2.0 SC5 speed-up, best of three.
forecast-pool-ratio:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target
    best=0
    best_line=""
    ratios=""
    for i in 1 2 3; do
        LOG="target/p06-forecast-pool-ratio-$i.log"
        set +e
        FORECAST_POOL_SERIES=peyton CARGO_INCREMENTAL=0 \
            cargo test --release -p aprender-mcp-forecast --lib pool_equality -- --nocapture > "$LOG" 2>&1
        rc=$?
        set -e
        if [ "$rc" -ne 0 ]; then
            tail -40 "$LOG"
            echo "FAIL: attempt $i exited $rc. A pool_equality failure is a CORRECTNESS failure" >&2
            echo "      (a response was not bit-identical under load) and is NEVER retried away." >&2
            exit "$rc"
        fi
        line=$(grep -m1 '^POOL SPEEDUP: ' "$LOG" || true)
        if [ -z "$line" ]; then
            echo "FAIL: attempt $i printed no 'POOL SPEEDUP: ' line - log $LOG" >&2
            exit 1
        fi
        ratio=$(printf '%s\n' "$line" | sed -n 's/^POOL SPEEDUP: \([0-9][0-9.]*\)x .*/\1/p')
        if [ -z "$ratio" ]; then
            echo "FAIL: attempt $i printed an unparseable ratio: $line" >&2
            exit 1
        fi
        echo "  attempt $i: ${ratio}x   ($LOG)"
        ratios="$ratios $ratio"
        if awk -v a="$ratio" -v b="$best" 'BEGIN { exit (a + 0 > b + 0) ? 0 : 1 }'; then
            best="$ratio"
            best_line="$line"
        fi
    done
    echo "  ratios:$ratios   best: ${best}x"
    echo "$best_line"
    # IN-01, and note the DIRECTION: this bar is `best >= 2.0`, not `< 2.0`.
    # Passing `under` here would invert the gate, which is exactly why the shared
    # validator makes the mode a required argument and refuses an unknown one
    # rather than defaulting to a direction.
    if ! bash scripts/assert_measurement_under.sh atleast "$best" 2.0 "POOL SPEEDUP (SC5)"; then
        echo "FAIL: the best of three ratios ($ratios) is below the 2.0 SC5 bar" >&2
        exit 1
    fi
    echo "  POOL SPEEDUP OK: best ${best}x >= 2.0 (SC5)"

# D-16: the rolling-origin MASE/coverage/WQL3 table. Informational, not a bar —
# it ships as a compiled EXAMPLE, never as a per-commit test.
# The D-16 rolling-origin accuracy table (Prophet / NP-lite / Chronos vs naive baselines).
mase-rolling-origin:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target
    LOG=target/p06-mase-rolling-origin.log
    set +e
    CHRONOS_MODEL_DIR={{chronos_abs}}/f32 CARGO_INCREMENTAL=0 \
        cargo run --release -p aprender-forecast --example mase_rolling_origin > "$LOG" 2>&1
    rc=$?
    set -e
    cat "$LOG"
    if [ "$rc" -ne 0 ]; then
        echo "FAIL: the mase_rolling_origin example exited $rc - log $LOG" >&2
        exit "$rc"
    fi

# The holiday design-build wall, MEASURED and ASSERTED against SC1's 2 s bar (06-12).
#
# WHAT THIS BAR CLAIMS, AND WHAT IT DOES NOT. It asserts that the WORST holiday-carrying
# request the door still accepts — the at-the-bound default geometry below — walls under
# 2 s on this release host. It is NOT a general SC1 guarantee for every accepted holiday
# request: 06-11 measured that NO payload statistic bounds the wall, because the L-BFGS
# iteration count is data-dependent (a 4 700-point / 5-column request is 25 000 cells,
# half the bound, and reproducibly walls at ~4.2 s). `MAX_HOLIDAY_DESIGN_COST` caps WORK,
# not WALL; the residual wall-clock exposure is FIT_BUDGET_SECS. Whether SC1's 2 s bar
# should apply to holiday-carrying requests at all is an OPEN human decision
# (06-11-SUMMARY coverage D7, WINDOWS.md entry 7) and this recipe does not close it.
#
# `forecast-bench` measures the NO-HOLIDAY SC1 shape (0.214 s). This one measures the
# holiday-carrying shape, which shares its point count and missed the bar by 8x:
# 3000 x 181 x 84 walled at 16.081 s inside every bound the door checked.
#
# THE DEFAULTS ARE THE SLOWEST OF THE THREE COMPOSITIONS ACTUALLY MEASURED at the
# design-cost bound — not the worst shape the door accepts. 800 points + a 200-step
# horizon x 50 holiday columns = 50 000 design feature cells, exactly
# `constants.fit_max_holiday_design_cost`, and 1.692 s on the aarch64 release host
# against 1.174 s for the many-rows composition (9 500 x 5) and 0.106 s for the
# many-columns one (50 x 500). Three points, and the defaults are the slowest of them.
#
# That is a DIFFERENT and weaker claim than the superlative this block used to make,
# and the paragraph above is why it had to change (06-REVIEW.md WR-04): the very next
# paragraph records an ACCEPTED 4 700-point / 5-column request walling at ~4.2 s, which
# is 2.5x slower than these "worst" defaults. Both sentences cannot be true. Since
# 06-11 measured that no payload statistic bounds the wall, no set of defaults can be
# the worst accepted shape, and a comment asserting one is the sentence a future reader
# would quote as coverage evidence.
#
# THE SURFACE THIS ONE RECIPE CANNOT COVER IS COVERED BY `just forecast-sc1-sweep`,
# which sweeps freq x growth x holiday shape rather than one hard-coded geometry. This
# recipe remains the single-composition entry point onto the same builder.
# The verifier's original 3000/181/84 is now REFUSED at the door and can only be run
# against a build without the bound.
#
# The ignored test PRINTS one machine-parsable measurement line and asserts NO wall
# (REVIEW-06-04: a wall-clock assertion inside libtest moves with CPU throttling).
# This recipe only re-prints it.
#
# Release-only ON PURPOSE: this crate carries `[profile.dev.package.aprender-forecast]
# opt-level = 3`, which makes a dev-profile number look plausible and still not be the
# SC1 bar — hence `profile=` on the printed line (CLAUDE.md rule 2).
forecast-holiday-bench points="800" columns="50" dates="84" horizon="200":
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target
    LOG=target/p06-forecast-holiday-bench.log
    set +e
    HOLIDAY_BENCH_POINTS={{points}} HOLIDAY_BENCH_COLUMNS={{columns}} \
    HOLIDAY_BENCH_DATES={{dates}} HOLIDAY_BENCH_HORIZON={{horizon}} \
    CARGO_INCREMENTAL=0 cargo test --release -p aprender-forecast --lib \
        prophet::design_cost::holiday_design_wall -- --ignored --nocapture > "$LOG" 2>&1
    rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then
        tail -30 "$LOG"
        echo "FAIL: the holiday design bench exited $rc - log $LOG" >&2
        exit "$rc"
    fi
    line=$(grep -m1 '^HOLIDAY DESIGN WALL: ' "$LOG" || true)
    if [ -z "$line" ]; then
        tail -30 "$LOG"
        echo "FAIL: no 'HOLIDAY DESIGN WALL:' line in $LOG - the wall was never measured" >&2
        exit 1
    fi
    echo "$line"
    # Parse total_s BY TOKEN, never by column position: the printed field order must
    # not become load-bearing, or a reordering of the measurement line silently moves
    # what this bar reads.
    total=$(printf '%s\n' "$line" \
        | awk '{ for (i = 1; i <= NF; i++) if ($i ~ /^total_s=/) { sub(/^total_s=/, "", $i); print $i; exit } }')
    if [ -z "$total" ]; then
        echo "FAIL: the HOLIDAY DESIGN WALL line carries no total_s= token - the wall" >&2
        echo "      was never measured. line: $line" >&2
        exit 1
    fi
    # CLAUDE.md rule 2 - prove the mechanism engaged, never label a run by intent. This
    # crate carries [profile.dev.package.aprender-forecast] opt-level = 3, so a debug
    # wall looks plausible and still is not the SC1 bar.
    case "$line" in
        *profile=release*) ;;
        *)
            echo "FAIL: the wall was not measured on a release build (profile= is not" >&2
            echo "      release), so it is not the SC1 bar. line: $line" >&2
            exit 1
            ;;
    esac
    # IN-01, the instance the review named (this was justfile:844). The inline
    # `awk -v v="$total" '{ exit (v + 0 < 2.0) }'` read a non-numeric total_s as
    # 0 and printed OK, so a renamed field or a truncated log made this gate
    # green without measuring anything.
    if ! bash scripts/assert_measurement_under.sh under "$total" 2.0 "HOLIDAY DESIGN (SC1)"; then
        echo "FAIL: $total s is at or above the 2.0 s SC1 bar, measured on the" >&2
        echo "      at-the-bound geometry points={{points}} columns={{columns}}" >&2
        echo "      dates={{dates}} horizon={{horizon}}. line: $line" >&2
        exit 1
    fi
    echo "  HOLIDAY DESIGN OK: $total s < 2.0 s (SC1)"

# THE REGRESSOR COST GATE (cost axis C-17, plan 06.1-03).
#
# `forecast-holiday-bench` walls the HOLIDAY design axis and `forecast-sc1-sweep`
# sweeps freq x growth x holiday shape. Neither carries a single regressor, so the
# axis the external-regressor surface actually costs on — `(len(ds) + horizon) *
# n_regressors` for the design cells, plus `len(ds) * K^2 + K^3` for the
# identifiability Gram and its factorisation — was covered by NOTHING.
#
# This recipe is the DERIVATION harness for `fit_max_regressor_design_cost` and
# `fit_max_regressors`, and it is also the gate that keeps them honest. It runs
# five compositions of the SAME product that differ in every factor: many rows /
# few regressors, balanced, few rows / many regressors, the MAXIMUM-WIDTH case
# where the diagnostic's `N * K^2` term peaks, and a COMBINED case carrying
# holidays at their own at-the-bound column count beside regressors at the count
# ceiling — because a caller can send both and the two design-cost ceilings are
# different constants against the SAME 2 s bar.
#
# One failing input is an anecdote (CLAUDE.md rule 6). A ceiling derived on one
# geometry is a statement about that geometry, not about the axis, which is why
# `sc1_wall::regressor_geometry::the_compositions_differ_in_every_factor` runs in
# the always-on suite and fails a builder that collapses the five into one shape.
#
# The bar is asserted TWICE and neither is redundant: once inside the harness,
# where the message names the failing composition, and once here over the
# re-parsed log, which is what catches a harness that silently stopped emitting
# lines. `REGRESSOR BENCH OK` reports the count it checked, so a run that checked
# zero lines cannot report success.
#
# Release-only ON PURPOSE: this crate carries `[profile.dev.package.aprender-forecast]
# opt-level = 3`, which covers the crate and NOT its dependencies, so a dev-profile
# number looks plausible and still is not the SC1 bar (CLAUDE.md rule 2) — hence the
# `profile=` token on every line and the hard guard on it below.
#
# The two arguments exist so the LADDER can be run: a candidate ceiling is measured
# by pointing the compositions at it. They default to EMPTY, not to a number, and
# an empty argument means "use the shipped constant" — the harness reads
# `crate::types::MAX_REGRESSOR_DESIGN_COST` / `MAX_REGRESSORS` when the env var is
# absent. A numeric default here would be a THIRD copy of a bound that already
# lives in the contract and its Rust mirror, and it would go stale the first time
# the ceiling moved — which it did, from the starting candidate down to the
# measured value, inside this very plan. A bare invocation therefore always
# re-certifies what is actually enforced.
# Wall the five regressor compositions on release and enforce the 2 s SC1 bar.
forecast-regressor-bench cost="" max_regressors="":
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target
    # The validator's OWN guard runs first, on every gate invocation — a bar whose
    # parser was never exercised is the IN-01 defect waiting to come back.
    bash scripts/check_assert_measurement_under_cases.sh
    LOG=target/p06.1-03-forecast-regressor-bench.log
    # An UNSET variable means "the shipped constant"; exporting an empty one would
    # be parsed as 0 by a less careful reader, so the variables are only exported
    # when they carry a value.
    if [ -n "{{cost}}" ]; then export REGRESSOR_BENCH_COST="{{cost}}"; fi
    if [ -n "{{max_regressors}}" ]; then export REGRESSOR_BENCH_MAX_REGRESSORS="{{max_regressors}}"; fi
    set +e
    CARGO_INCREMENTAL=0 cargo test --release -p aprender-forecast --lib \
        sc1_wall::regressor_design_wall -- --ignored --nocapture > "$LOG" 2>&1
    rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then
        grep -E '^REGRESSOR (WALL|SWEEP)' "$LOG" || true
        tail -30 "$LOG"
        echo "FAIL: the regressor bench exited $rc - log $LOG" >&2
        exit "$rc"
    fi
    if ! grep -q '^REGRESSOR WALL: ' "$LOG"; then
        tail -30 "$LOG"
        echo "FAIL: no 'REGRESSOR WALL:' line in $LOG - nothing was measured. A gate" >&2
        echo "      that checked zero compositions must never report success." >&2
        exit 1
    fi
    checked=0
    while IFS= read -r line; do
        echo "$line"
        # CLAUDE.md rule 2 - prove the mechanism engaged, never label a run by
        # intent. A debug wall on this crate looks plausible and is not the bar.
        case "$line" in
            *profile=release*) ;;
            *)
                echo "FAIL: a composition was not measured on a release build" >&2
                echo "      (profile= is not release), so it is not the SC1 bar." >&2
                echo "      line: $line" >&2
                exit 1
                ;;
        esac
        # Parse total_s BY TOKEN, never by column position: the printed field
        # order must not become load-bearing.
        total=$(printf '%s\n' "$line" \
            | awk '{ for (i = 1; i <= NF; i++) if ($i ~ /^total_s=/) { sub(/^total_s=/, "", $i); print $i; exit } }')
        if [ -z "$total" ]; then
            echo "FAIL: a REGRESSOR WALL line carries no total_s= token - that" >&2
            echo "      composition was never measured. line: $line" >&2
            exit 1
        fi
        label=$(printf '%s\n' "$line" | sed -n 's/^REGRESSOR WALL: composition=\([^ ]*\) .*/\1/p')
        bash scripts/assert_measurement_under.sh under "$total" 2.0 "REGRESSOR $label"
        checked=$((checked + 1))
    done < <(grep '^REGRESSOR WALL: ' "$LOG")
    if [ "$checked" -eq 0 ]; then
        echo "FAIL: the loop checked zero compositions." >&2
        exit 1
    fi
    if [ "$checked" -ne 5 ]; then
        echo "FAIL: the sweep is FIVE compositions and this run checked $checked." >&2
        echo "      A silently shrunk matrix is the WR-04 defect, not a faster gate." >&2
        exit 1
    fi
    # The SWEEP line reports the values actually USED, resolved by the harness, so
    # the gate's own summary cannot claim a geometry it did not measure.
    grep -m1 '^REGRESSOR SWEEP: ' "$LOG"
    echo "  REGRESSOR BENCH OK: $checked compositions, every one under the 2.0 s SC1 bar"

# THE SC1 GATE, SWEPT (06-REVIEW.md WR-04).
#
# `forecast-bench` walls the no-holiday shape, `forecast-holiday-bench` walls the
# holiday axis, and `logistic_band_wall` walled the logistic band. Three benches,
# three hard-coded geometries, and the axis CR-01 actually lived on — `freq` —
# covered by NONE of them: at 33 points and horizon 3650, `"D"` measures 0.231 s
# and `"MS"` measures 2.334 s. Nothing in the phase could have caught that.
#
# THIS recipe is the gate. It runs `sc1_wall::sc1_wall_sweep` over the full cross
# product freq {D,W,MS} x growth {linear,logistic,flat} x holiday {none,
# at-the-design-cost-bound} at the tightest legal history span, plus the
# NeuralProphet row widened to its own at-the-bound geometry, on a RELEASE build,
# and asserts SC1's 2 s bar over every printed line.
#
# The bar is asserted TWICE and neither is redundant: once inside the harness,
# where the message names the failing composition, and once here over the
# re-parsed log, which is what catches a harness that silently stopped emitting
# lines. `SC1 SWEEP OK` reports the count it checked, so a run that checked zero
# lines cannot report success.
#
# OBSERVED FAILING on the defect it exists for (CLAUDE.md rule 5): with
# `MAX_LOGISTIC_CHANGEPOINT_LAMBDA` and its contract mirror raised past the
# structural maximum, `freq=MS growth=logistic` walls at 2.443 s and this gate
# goes red naming it, while `freq=D` passes at 0.219 s.
#
# Release-only ON PURPOSE: this crate carries `[profile.dev.package.aprender-forecast]
# opt-level = 3`, which covers the crate and not its dependencies, so a dev-profile
# number looks plausible and still is not the SC1 bar (CLAUDE.md rule 2) — hence the
# `profile=` token on every line and the hard guard on it below.
# Sweep freq x growth x holiday shape on release and enforce the 2 s SC1 bar.
forecast-sc1-sweep points="33" horizon="3650" np_points="2000" np_lags="41" np_horizon="365":
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target
    # The validator's OWN guard runs first, on every gate invocation — not only
    # when somebody remembers. A bar whose parser was never exercised is the
    # IN-01 defect waiting to come back.
    bash scripts/check_assert_measurement_under_cases.sh
    LOG=target/p06-forecast-sc1-sweep.log
    set +e
    SC1_SWEEP_POINTS={{points}} SC1_SWEEP_HORIZON={{horizon}} \
    SC1_SWEEP_NP_POINTS={{np_points}} SC1_SWEEP_NP_LAGS={{np_lags}} \
    SC1_SWEEP_NP_HORIZON={{np_horizon}} \
    CARGO_INCREMENTAL=0 cargo test --release -p aprender-forecast --lib \
        sc1_wall:: -- --nocapture > "$LOG" 2>&1
    rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then
        grep -E '^SC1 (WALL|SWEEP)' "$LOG" || true
        tail -30 "$LOG"
        echo "FAIL: the SC1 sweep exited $rc - log $LOG" >&2
        exit "$rc"
    fi
    if ! grep -q '^SC1 WALL: ' "$LOG"; then
        tail -30 "$LOG"
        echo "FAIL: no 'SC1 WALL:' line in $LOG - nothing was measured. A gate that" >&2
        echo "      checked zero compositions must never report success." >&2
        exit 1
    fi
    checked=0
    while IFS= read -r line; do
        echo "$line"
        # CLAUDE.md rule 2 - prove the mechanism engaged, never label a run by
        # intent. A debug wall on this crate looks plausible and is not the bar.
        case "$line" in
            *profile=release*) ;;
            *)
                echo "FAIL: a composition was not measured on a release build" >&2
                echo "      (profile= is not release), so it is not the SC1 bar." >&2
                echo "      line: $line" >&2
                exit 1
                ;;
        esac
        # Parse total_s BY TOKEN, never by column position: the printed field
        # order must not become load-bearing.
        total=$(printf '%s\n' "$line" \
            | awk '{ for (i = 1; i <= NF; i++) if ($i ~ /^total_s=/) { sub(/^total_s=/, "", $i); print $i; exit } }')
        if [ -z "$total" ]; then
            echo "FAIL: an SC1 WALL line carries no total_s= token - that" >&2
            echo "      composition was never measured. line: $line" >&2
            exit 1
        fi
        label=$(printf '%s\n' "$line" | sed -n 's/^SC1 WALL: \(.*\) points=.*/\1/p')
        bash scripts/assert_measurement_under.sh under "$total" 2.0 "SC1 $label"
        checked=$((checked + 1))
    done < <(grep '^SC1 WALL: ' "$LOG")
    if [ "$checked" -eq 0 ]; then
        echo "FAIL: the loop checked zero compositions." >&2
        exit 1
    fi
    echo "  SC1 SWEEP OK: $checked compositions, every one under the 2.0 s SC1 bar"

# THE C-08 EVENT-COLUMN CALIBRATION (SC4, D-32, D-34).
#
# `np::train_cost` had NO event term: at `fit_max_holiday_columns` (1 000 — the
# ceiling the event surface inherits by reusing `HolidayArg`) a request bought
# 7.6x the work it was priced at, measured by spike 013. This recipe is the
# DERIVATION harness for `fit_np_event_cost_per_column`, and it is what a later
# re-calibration (Phase 7, or the D-32 on-target measurement) re-runs.
#
# WHAT IT MEASURES, and what it deliberately does not. The per-column SLOPE of
# microseconds-per-step against the event-column count — NOT the 47.924 s
# structural maximum. D-32 is explicit that the slope is cheap enough to live in
# CI while the structural maximum is not.
#
# THE COEFFICIENT IS A RATIO, which is why the recipe needs no batch size, step
# count or sample count. The sweep fits `us/step ~= a + b*E` at ONE geometry; the
# coefficient is `b * (n_lags_cal + 1) / a`, in the proxy's own width units. The
# batch size divides `a` and `b` identically and cancels, so an absolute
# microsecond figure would be the one number the door could not convert at check
# time. `n_lags_cal` is printed on the FIT line so the reduction to `b/a` (valid
# only when the calibration is lag-free) is checkable rather than assumed.
#
# Release-only ON PURPOSE, and the guard is hard: this crate carries
# `[profile.dev.package.aprender-forecast] opt-level = 3`, which covers the crate
# and NOT its dependencies, so a dev-profile number looks plausible and is not the
# measurement (CLAUDE.md rule 2). Every line carries `profile=`, derived from
# `cfg!(debug_assertions)` rather than from intent, and this recipe refuses any
# line that does not say `release`.
#
# Every line also carries `commit=` and `arch=`, so a pasted sweep identifies the
# tree and the architecture it came from. Never label a run by intent.
#
# `rc` is captured BEFORE any pipe. Reading a status through a pipe gives the LAST
# command's status, and that exact defect shipped twice in this repo and made
# three green runs prove nothing (#2336, #2360).
#
# ON A BARE x86_64 HOST this needs nothing but a Rust toolchain and a checkout:
# no fixtures, no model weights, no network, no Python. `just` itself is the only
# non-cargo dependency, and the two cargo commands below are the whole recipe.
# Sweep the C-08 event-column axis on release and fit the per-column slope.
forecast-np-event-calibration points="" lags="" epochs="":
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target
    LOG=target/p06.1-05-forecast-np-event-calibration.log
    # An UNSET variable means "the harness default"; exporting an empty one would
    # be parsed as 0 by a less careful reader, so they are exported only when set.
    if [ -n "{{points}}" ]; then export NP_EVENT_CAL_POINTS="{{points}}"; fi
    if [ -n "{{lags}}" ]; then export NP_EVENT_CAL_LAGS="{{lags}}"; fi
    if [ -n "{{epochs}}" ]; then export NP_EVENT_CAL_EPOCHS="{{epochs}}"; fi
    # The commit the numbers were produced at, carried onto every printed line.
    NP_EVENT_CAL_COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
    export NP_EVENT_CAL_COMMIT
    set +e
    CARGO_INCREMENTAL=0 cargo test --release -p aprender-forecast --lib \
        sc1_wall::np_event_calibration -- --ignored --nocapture > "$LOG" 2>&1
    rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then
        grep -E '^NP EVENT CAL' "$LOG" || true
        tail -30 "$LOG"
        echo "FAIL: the event calibration exited $rc - log $LOG" >&2
        exit "$rc"
    fi
    if ! grep -q '^NP EVENT CAL: ' "$LOG"; then
        tail -30 "$LOG"
        echo "FAIL: no 'NP EVENT CAL:' line in $LOG - nothing was measured. A sweep" >&2
        echo "      that measured zero points must never report success." >&2
        exit 1
    fi
    checked=0
    while IFS= read -r line; do
        echo "$line"
        # CLAUDE.md rule 2 - prove the mechanism engaged, never label a run by
        # intent. A debug number on this crate looks plausible and is not the
        # measurement the coefficient is derived from.
        case "$line" in
            *profile=release*) ;;
            *)
                echo "FAIL: a calibration point was not measured on a release build" >&2
                echo "      (profile= is not release), so it is not the calibration." >&2
                echo "      line: $line" >&2
                exit 1
                ;;
        esac
        # Parse BY TOKEN, never by column position: the printed field order must
        # not become load-bearing.
        us=$(printf '%s\n' "$line" \
            | awk '{ for (i = 1; i <= NF; i++) if ($i ~ /^us_per_step=/) { sub(/^us_per_step=/, "", $i); print $i; exit } }')
        if [ -z "$us" ]; then
            echo "FAIL: an NP EVENT CAL line carries no us_per_step= token - that" >&2
            echo "      point was never measured. line: $line" >&2
            exit 1
        fi
        checked=$((checked + 1))
    done < <(grep '^NP EVENT CAL: ' "$LOG")
    if [ "$checked" -lt 6 ]; then
        echo "FAIL: the sweep is at least SIX event-column counts and this run" >&2
        echo "      checked $checked. A slope fitted through fewer points is not a" >&2
        echo "      measurement of a shape." >&2
        exit 1
    fi
    if ! grep -q '^NP EVENT CAL FIT: ' "$LOG"; then
        echo "FAIL: the sweep printed no FIT line, so no slope was fitted." >&2
        exit 1
    fi
    grep -m1 '^NP EVENT CAL FIT: ' "$LOG"
    echo "  NP EVENT CALIBRATION OK: $checked points swept on release, slope fitted"

# Sweep the C-08 NUMERIC-REGRESSOR column axis on release and fit the per-column
# slope. A SIBLING of forecast-np-event-calibration on the same shape, not a
# reuse of it: the two coefficients are separately measured, and one recipe
# producing both would make a re-measure of either move the other.
#
# `rc` is captured BEFORE any pipe. Reading `$?` through a pipe gives the LAST
# command's status, and that exact defect shipped twice in this repo and made
# three green runs prove nothing (#2336, #2360).
forecast-np-regressor-calibration points="" lags="" epochs="":
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p target
    LOG=target/p06.1-07-forecast-np-regressor-calibration.log
    # An UNSET variable means "the harness default"; exporting an empty one would
    # be parsed as 0 by a less careful reader, so they are exported only when set.
    if [ -n "{{points}}" ]; then export NP_REG_CAL_POINTS="{{points}}"; fi
    if [ -n "{{lags}}" ]; then export NP_REG_CAL_LAGS="{{lags}}"; fi
    if [ -n "{{epochs}}" ]; then export NP_REG_CAL_EPOCHS="{{epochs}}"; fi
    # The commit the numbers were produced at, carried onto every printed line.
    NP_REG_CAL_COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
    export NP_REG_CAL_COMMIT
    set +e
    CARGO_INCREMENTAL=0 cargo test --release -p aprender-forecast --lib \
        sc1_wall::np_regressor_calibration -- --ignored --nocapture > "$LOG" 2>&1
    rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then
        grep -E '^NP REG CAL' "$LOG" || true
        tail -30 "$LOG"
        echo "FAIL: the regressor calibration exited $rc - log $LOG" >&2
        exit "$rc"
    fi
    if ! grep -q '^NP REG CAL: ' "$LOG"; then
        tail -30 "$LOG"
        echo "FAIL: no 'NP REG CAL:' line in $LOG - nothing was measured. A sweep" >&2
        echo "      that measured zero points must never report success." >&2
        exit 1
    fi
    checked=0
    while IFS= read -r line; do
        echo "$line"
        # CLAUDE.md rule 2 - prove the mechanism engaged, never label a run by
        # intent. A debug number on this crate looks plausible and is not the
        # measurement the coefficient is derived from.
        case "$line" in
            *profile=release*) ;;
            *)
                echo "FAIL: a calibration point was not measured on a release build" >&2
                echo "      (profile= is not release), so it is not the calibration." >&2
                echo "      line: $line" >&2
                exit 1
                ;;
        esac
        # Parse BY TOKEN, never by column position: the printed field order must
        # not become load-bearing.
        us=$(printf '%s\n' "$line" \
            | awk '{ for (i = 1; i <= NF; i++) if ($i ~ /^us_per_step=/) { sub(/^us_per_step=/, "", $i); print $i; exit } }')
        if [ -z "$us" ]; then
            echo "FAIL: an NP REG CAL line carries no us_per_step= token - that" >&2
            echo "      point was never measured. line: $line" >&2
            exit 1
        fi
        checked=$((checked + 1))
    done < <(grep '^NP REG CAL: ' "$LOG")
    if [ "$checked" -lt 6 ]; then
        echo "FAIL: the sweep is at least SIX regressor-column counts and this run" >&2
        echo "      checked $checked. A slope fitted through fewer points is not a" >&2
        echo "      measurement of a shape." >&2
        exit 1
    fi
    if ! grep -q '^NP REG CAL FIT: ' "$LOG"; then
        echo "FAIL: the sweep printed no FIT line, so no slope was fitted." >&2
        exit 1
    fi
    grep -m1 '^NP REG CAL FIT: ' "$LOG"
    echo "  NP REGRESSOR CALIBRATION OK: $checked points swept on release, slope fitted"

# ---------------------------------------------------------------------------
# Phase 8: the Laya back office (scripts/laya_train, a pinned uv project, D-02).
#
# Laptop-only: CI never installs this project's torch stack. The Rust side
# re-derives what CI must check (plan 08-09). At plan 08-12's CI checkpoint the
# user kept the torch-free Python self-tests and the four env-gated parity
# targets OUT of CI; `just laya-verify-suite` runs all of them locally.
# `--frozen` everywhere: the committed uv.lock (human-verified pins, plan 08-02
# Task 1) is what runs, never a fresh resolution.
# ---------------------------------------------------------------------------

# Regenerate the two tiny synthetic CI fixtures from Laya's / transformers' own code (byte-identical on re-run).
laya-fixtures:
    #!/usr/bin/env bash
    set -euo pipefail
    uv run --project scripts/laya_train --frozen python scripts/laya_train/metrics.py --selftest
    uv run --project scripts/laya_train --frozen python scripts/laya_train/fixtures.py

# TweetEval stance demo data (D-19 as amended by A2, laya-finetune-gate-v1 1.4.0). cell s64 (default, the
# contract's demo_s64) -> data/decide/tweet-stance-64: the 192 s64-seed13 shots, eval.jsonl built BY RULE
# (eval_set.demo_rule, asserted 459 rows [111, 291, 57]) and shift.jsonl (the 280 test rows, the shift
# probe). cell s16 -> data/decide/tweet-stance-16, the 1.2.0 demo data, byte-identical. Every shot verified
# against the manifest's exact_hash; an existing dir is never overwritten with different bytes.
# Output is under the root-anchored, gitignored /data/ — tweet text is never committed.
laya-prepare-stance cell="s64":
    #!/usr/bin/env bash
    set -euo pipefail
    need="data/tweet-eval-stance/train.jsonl data/tweet-eval-stance/test.jsonl"
    if [ "{{cell}}" = "s64" ]; then need="$need data/tweet-eval-stance/validation.jsonl"; fi
    for f in $need; do
        test -f "$f" || {
            echo "ERROR: $f is missing — fetch the local dataset first:" >&2
            echo "       apr data tweet-eval-stance --output data/tweet-eval-stance" >&2
            exit 1
        }
    done
    uv run --project scripts/laya_train --frozen python scripts/laya_train/prepare_stance.py --cell "{{cell}}"

# Fine-tune Laya on <data> (task.json + train.jsonl + a REQUIRED eval.jsonl + an OPTIONAL shift.jsonl),
# calibrate and gate into the run dir <out>. Exit 0 = GATE PASS, 3 = GATE FAIL, 2 = input refused.
# Production (laya-finetune-gate-v1 1.4.0) trains the three gate seeds 13/17/23 and ships the MEDIAN-ECE
# seed (A3; --seeds other than 3 is refused), writes the float64 re-score noise record rescore-noise.json
# (A1), and scores an optional shift.jsonl as a REPORTED probe after the gate is decided (A2, never a
# gate clause). Extra args pass through (--epochs E above 16 shots/class, --stopping
# early_stopping|fixed_epochs, --device mps|cuda|cpu). The default stopping rule is the contract's
# (early_stopping, 1.1.0).
laya-train data out *args:
    #!/usr/bin/env bash
    set -euo pipefail
    test -f "{{data}}/task.json" || { echo "ERROR: {{data}}/task.json does not exist" >&2; exit 2; }
    uv run --project scripts/laya_train --frozen python scripts/laya_train/train.py \
        --data "{{data}}" --out "{{out}}" {{args}}

# The tracer's thin slice: a real train -> F16 save -> complete dir -> reload -> calibrate -> gate on
# the committed tiny checkpoint, on CPU in seconds (synthetic-fixture variant), once per declared
# stopping rule (early_stopping, fixed_epochs). Prints LIFECYCLE OK.
laya-train-lifecycle:
    #!/usr/bin/env bash
    set -euo pipefail
    uv run --project scripts/laya_train --frozen python scripts/laya_train/lifecycle.py

# Every Python-side training claim of laya-finetune-gate-v1, locally (CI never installs the torch stack):
# the torch-free self-tests (metrics, data refusals + split + the held-out rule, gate decision + the two
# fail-closed demo vectors + T clamp + early stopping + the median rule), then the torch lifecycle (both
# stopping rules, the legacy single seed, the three-seed median run, the noise record, the shift probe). Each
# step's status is checked directly; the recipe stops at the first failure. Prints LAYA TRAIN SELFTEST OK.
laya-train-selftest:
    #!/usr/bin/env bash
    set -euo pipefail
    for m in metrics data gate; do
        uv run --project scripts/laya_train --frozen python "scripts/laya_train/$m.py" --selftest
    done
    uv run --project scripts/laya_train --frozen python scripts/laya_train/lifecycle.py
    echo "LAYA TRAIN SELFTEST OK"

# The Python-parity / real-weights surface of Phase 8 in ONE local command. LOCAL ONLY, by the user's
# decision at plan 08-12's CI checkpoint (2026-09-27): these legs need the 0.84 GB base snapshot, the
# gitignored run dirs and the torch stack, none of which CI has, so in CI they could only SKIP. CI runs
# the pure-Rust targets instead (`-p aprender-decide --test ui`, `-p aprender-mcp-decide --test e2e_stdio`).
# Steps, stopping at the first failure:
#   1. the torch-free Python self-tests: metrics.py, data.py, gate.py --selftest (numpy + pyyaml);
#   2. laya_parity (LAYA_MODEL_DIR), fail_closed_vectors (+ LAYA_FAIL_CLOSED_VECTORS=1) and demo_run
#      (+ LAYA_DEMO_RUN=1), ARMED against <model>;
#   3. the torch lifecycle kept to a temp dir (LAYA_LIFECYCLE_KEEP), then python_records against it
#      (LAYA_PY_RUN_DIR + LAYA_PY_DATA_DIR).
# laya_parity runs with LAYA_LADDER_BIN (default: the MAIN checkout's gitignored spike-025 ladder
# dump), so its ladder rung runs too. Each leg's verdict is `_laya-leg-verdict` (plan 08-22): a SKIP
# ANYWHERE on a line fails it, and so does a missing POSITIVE evidence line (a leg must say what it
# measured). A missing <model> dir or ladder dump is a refusal (exit 2), not a SKIP. <model> defaults
# to `laya_model_dir` above. Prints LAYA VERIFY SUITE OK on success.
# Phase 8 Python-parity + real-weights suite, LOCAL ONLY (not in CI): Python self-tests + 4 armed targets
laya-verify-suite model=laya_model_dir:
    #!/usr/bin/env bash
    set -euo pipefail
    model="{{model}}"
    test -f "$model/model.safetensors" || { echo "ERROR: $model/model.safetensors is missing (the declared Laya base snapshot)" >&2; exit 2; }
    # The ladder dump is gitignored and lives only in the MAIN checkout (a worktree has none).
    MAIN="$(cd "$(git rev-parse --git-common-dir)/.." && pwd -P)"
    ladder="${LAYA_LADDER_BIN:-$MAIN/.planning/spikes/025-laya-rust-forward-parity/fixtures/laya-en_ladder.bin}"
    test -f "$ladder" || { echo "ERROR: the ladder dump $ladder is missing (spike 025 tools/oracle.py regenerates it); the ladder rung would not run" >&2; exit 2; }
    for m in metrics data gate; do
        uv run --project scripts/laya_train --frozen python "scripts/laya_train/$m.py" --selftest
    done
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT
    leg() {
        name="$1"; shift
        log="$work/$name.log"
        if ! env "$@" cargo test -p aprender-decide --release --test "$name" -- --nocapture > "$log" 2>&1; then
            cat "$log"; echo "FAIL: $name" >&2; return 1
        fi
        grep -Ev '^[[:space:]]*(Compiling|Finished|Running|Blocking|Doc-tests|Downloaded|Locking|Updating)( |$)|^$' "$log" || true
        just _laya-leg-verdict "$name" "$log" || return 1
    }
    leg laya_parity LAYA_MODEL_DIR="$model" LAYA_LADDER_BIN="$ladder"
    leg fail_closed_vectors LAYA_MODEL_DIR="$model" LAYA_FAIL_CLOSED_VECTORS=1
    leg demo_run LAYA_MODEL_DIR="$model" LAYA_DEMO_RUN=1
    LAYA_LIFECYCLE_KEEP="$work/keep" uv run --project scripts/laya_train --frozen python scripts/laya_train/lifecycle.py
    leg python_records LAYA_PY_RUN_DIR="$work/keep/run" LAYA_PY_DATA_DIR="$work/keep/data"
    echo "LAYA VERIFY SUITE OK"

# The verdict of ONE laya-verify-suite leg over its log (plan 08-22; V11-a, AL7, D3-1). Pure: no cargo.
# FAILS when any line carries the token SKIP ANYWHERE (libtest prefixes `test <name> ... ` onto the
# first output line, and laya_parity prints `SKIP ladder rung: ...`, so a column-0 `^SKIP:` grep saw
# neither), when the leg's POSITIVE evidence is absent, or when <name> is not a known leg. Prints
# `LEG OK: <name>` otherwise. Row `leg-verdict` of scripts/laya_gates.tsv feeds it canned logs.
[positional-arguments]
_laya-leg-verdict name log:
    #!/usr/bin/env bash
    set -euo pipefail
    NAME="$1"; LOG="$2"
    # Required positive evidence per leg: every ERE must match some line (anywhere on it).
    case "$NAME" in
        laya_parity) NEED=('MEASURED ids [0-9]+/[0-9]+' 'MEASURED probs max_abs [0-9]' 'MEASURED ladder [0-9]+ blocks within bars') ;;
        fail_closed_vectors) NEED=('FAIL-CLOSED VECTORS REFUSED 2/2 ') ;;
        demo_run) NEED=('DEMO OUTCOME [a-z_]+ decided on the exact bytes') ;;
        python_records) NEED=('NOISE which=[^ ]+ rust=' 'MEDIAN rust=[0-9]+ python=[0-9]+') ;;
        *) echo "FAIL: unknown leg '$NAME' (known: laya_parity fail_closed_vectors demo_run python_records)" >&2; exit 1 ;;
    esac
    [ -f "$LOG" ] || { echo "FAIL: $NAME: log $LOG does not exist" >&2; exit 1; }
    if grep -n 'SKIP' "$LOG" > /dev/null; then
        echo "FAIL: $NAME is armed but printed SKIP (measured nothing): $(grep -m 1 'SKIP' "$LOG" | cut -c 1-200)" >&2
        exit 1
    fi
    for re in "${NEED[@]}"; do
        grep -Eq "$re" "$LOG" || { echo "FAIL: $NAME printed no positive evidence matching '$re'" >&2; exit 1; }
    done
    echo "LEG OK: $NAME"

# Pack a Laya run dir FOR SERVING (plan 08-09, D-07 fail-closed in Rust): production variant, the
# contract's base, input hashes, split, a Rust re-score of every eval row from the packed bytes and
# from <base>, and the gate RECOMPUTED from those verified probabilities -- all before anything is
# written. Exit 0 = PACKED, 3 = gate failed, 2 = any other refusal; a refusal writes nothing. The
# policy is read from the contracts; no argument or env var overrides it.
laya-pack run data base out:
    #!/usr/bin/env bash
    set -euo pipefail
    exec cargo run --release -p aprender-decide --example pack_laya -- \
        pack --run "{{run}}" --data "{{data}}" --base "{{base}}" --out "{{out}}"

# Deployment eligibility of the EXACT file <apr> (decide-apr-v1 deploy_eligibility; plan 08-09): the full
# load ladder, the manifest bound to <run>/<data>, then every `laya-pack` check on those bytes. Prints
# one JSON line with deploy_eligible only on accept; exit 3 = gate failed, 2 = any other refusal. This
# is the ONLY eligibility check the deploy recipes use; the policy is read from the contracts.
laya-verify apr run data base:
    #!/usr/bin/env bash
    set -euo pipefail
    exec cargo run --release -p aprender-decide --example pack_laya -- \
        verify "{{apr}}" --run "{{run}}" --data "{{data}}" --base "{{base}}"

# Identity of a decide .apr (load rungs 1-4, so every field is bound to its blobs): sha256, recipe_id, base, variant, labels
# and the embedded gate summary. Makes NO eligibility claim -- that is `just laya-verify`.
laya-inspect file:
    #!/usr/bin/env bash
    set -euo pipefail
    exec cargo run --release -p aprender-decide --example pack_laya -- inspect "{{file}}"

# Write a synthetic-fixture test artifact (the ONLY variant it writes; every verify refuses it).
# Any other variant is refused with exit 2 and nothing written.
laya-pack-fixture run data out:
    #!/usr/bin/env bash
    set -euo pipefail
    exec cargo run --release -p aprender-decide --example pack_laya -- \
        pack-fixture --run "{{run}}" --data "{{data}}" --out "{{out}}"

# ---------------------------------------------------------------------------
# Laya decide server: fail-closed deploy to pmcp.run (plan 08-10, D-07, D-11, D-18)
#
# DEPLOY ROOT (user decision 2026-09-26, shared-crates-root): `cargo pmcp deploy
# --manifest-path crates` with server name `aprender-mcp-decide`. cargo-pmcp's
# `find_lambda_package_dir` returns `<root>/<server>-lambda` when it exists, BEFORE its
# workspace-wide search, so this root resolves to crates/aprender-mcp-decide-lambda by
# construction. The per-crate root (`--manifest-path crates/aprender-mcp-decide-lambda`)
# misses that branch and falls through to the FIRST `*-lambda` package with a `bootstrap`
# bin, which is aprender-mcp-chronos-lambda: the Chronos binary would ship under the decide
# name, healthy-looking (RESEARCH Pitfall 1). `just laya-resolver-proof` EXECUTES that
# resolver on this workspace; `laya-deploy` refuses without its proof.
#
# Limits of this choice: the server name is forced to the package stem, so ONE decide model
# per workspace; and `crates/.pmcp/` + `crates/deploy/` belong to the setfit training server,
# so `laya-deploy` swaps them out and restores them byte-identically on every exit path
# (`_laya-crates-root-swap`, proven by `laya-deploy-selftest`). The durable fix is upstream:
# in cargo-pmcp, return the project root when it is itself a `*-lambda` package with a
# `bootstrap` bin (recommended future SDK work, not done here).
#
# Nothing here writes to AWS unless `just laya-verify` accepted the exact file first.
# ---------------------------------------------------------------------------

# Execute cargo-pmcp's own `find_lambda_package_dir` on THIS workspace (root `crates`, server
# `aprender-mcp-decide`) from a `git archive` of <sdk> at <commit> unpacked under <work> (a
# scratch dir; the SDK checkout is only read). The source version must equal the installed
# `cargo pmcp --version`. Writes models/decide/resolver-proof.txt, which `laya-deploy` requires.
laya-resolver-proof sdk commit work:
    #!/usr/bin/env bash
    set -euo pipefail
    SDK="{{sdk}}"
    WORK="{{work}}"
    REPO="$(pwd -P)"
    PROOF="models/decide/resolver-proof.txt"
    EXPECT="crates/aprender-mcp-decide-lambda"
    INJECT="scripts/laya_deploy/cargo_pmcp_resolver_proof.rs"
    TEST="deployment::builder::aprender_resolver_proof::aprender_decide_resolves_from_shared_crates_root"
    git -C "$SDK" rev-parse --git-dir >/dev/null 2>&1 || { echo "ERROR: $SDK is not a git checkout" >&2; exit 2; }
    FULL="$(git -C "$SDK" rev-parse --verify "{{commit}}^{commit}")"
    SRC_VER="$(git -C "$SDK" show "$FULL:cargo-pmcp/Cargo.toml" \
        | python3 -c 'import sys, tomllib; print(tomllib.loads(sys.stdin.read())["package"]["version"])')"
    INST_VER="$(cargo pmcp --version | awk '{print $2}')"
    if [ "$SRC_VER" != "$INST_VER" ]; then
        echo "REFUSED version: cargo-pmcp at $FULL is $SRC_VER but the installed tool is $INST_VER;" >&2
        echo "        prove the resolver of the version that will deploy" >&2
        exit 2
    fi
    BUILDER_LAST="$(git -C "$SDK" log -1 --format=%H "$FULL" -- cargo-pmcp/src/deployment/builder.rs)"
    DEST="$WORK/rust-mcp-sdk-${FULL:0:12}"
    rm -rf "$DEST"
    mkdir -p "$DEST"
    git -C "$SDK" archive --format=tar "$FULL" | tar -x -C "$DEST"
    cat "$INJECT" >> "$DEST/cargo-pmcp/src/deployment/builder.rs"
    # The SDK gitignores Cargo.lock, so the archive has none: seed it with the checkout's
    # (a read-only copy) and record the cargo_metadata version the resolver ran with.
    [ -f "$SDK/Cargo.lock" ] && cp "$SDK/Cargo.lock" "$DEST/Cargo.lock"
    LOG="$WORK/resolver-proof-test.log"
    set +e
    (cd "$DEST" && APRENDER_WORKSPACE="$REPO" CARGO_TARGET_DIR="$WORK/target" \
        cargo test -p cargo-pmcp --bin cargo-pmcp aprender_resolver_proof -- --nocapture) \
        > "$LOG" 2>&1
    rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then
        tail -30 "$LOG" >&2
        echo "ERROR: the resolver test exited $rc (log: $LOG)" >&2
        exit 1
    fi
    grep -q 'test result: ok. 1 passed' "$LOG" || { echo "ERROR: the filter did not run exactly one test (log: $LOG)" >&2; exit 1; }
    PARSED="$(just _laya-resolver-parse "$LOG")" || { echo "ERROR: the resolver test printed no RESOLVED line (log: $LOG)" >&2; exit 1; }
    RESOLVED="$(printf '%s\n' "$PARSED" | sed -n 's/^RESOLVED=//p')"
    CONTROL="$(printf '%s\n' "$PARSED" | sed -n 's/^CONTROL=//p')"
    test "$RESOLVED" = "$EXPECT" || { echo "ERROR: the resolver returned '$RESOLVED', not $EXPECT" >&2; exit 1; }
    mkdir -p models/decide
    {
        echo "$RESOLVED"
        echo "test=$TEST"
        echo "command=cargo test -p cargo-pmcp --bin cargo-pmcp aprender_resolver_proof -- --nocapture"
        echo "cargo_metadata_crate=$(python3 -c 'import sys, tomllib; print(next(p["version"] for p in tomllib.load(open(sys.argv[1], "rb"))["package"] if p["name"] == "cargo_metadata"))' "$DEST/Cargo.lock")"
        echo "cargo_pmcp_version=$INST_VER"
        echo "sdk_commit=$FULL"
        echo "builder_rs_last_commit=$BUILDER_LAST"
        echo "deploy_root=crates"
        echo "server=aprender-mcp-decide"
        echo "control_per_crate_root=$CONTROL"
        echo "proven_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } > "$PROOF"
    echo "RESOLVER PROOF: $RESOLVED (control: root $EXPECT -> $CONTROL)"
    echo "  cargo-pmcp $INST_VER, source $FULL (builder.rs last changed in $BUILDER_LAST)"
    echo "  wrote $PROOF (gitignored)"

# The RESOLVED / CONTROL values of a resolver-proof test log (plan 08-22, D3-1). The tokens are read
# ANYWHERE on a line: under --nocapture libtest writes `test <name> ... ` onto the first output line,
# so a column-0 anchor can miss them. Prints `RESOLVED=<dir>` and `CONTROL=<dir>` (the first of each);
# no RESOLVED line exits 1. Pure: no cargo. Row `resolver-proof-sed` of scripts/laya_gates.tsv.
[positional-arguments]
_laya-resolver-parse log:
    #!/usr/bin/env bash
    set -euo pipefail
    LOG="$1"
    [ -f "$LOG" ] || { echo "REFUSED resolver-parse: $LOG does not exist" >&2; exit 1; }
    first() { awk -v pat="$1" 'match($0, pat) { print substr($0, RSTART + RLENGTH); exit }' "$LOG"; }
    RESOLVED="$(first 'RESOLVED root=crates server=aprender-mcp-decide -> ')"
    CONTROL="$(first 'CONTROL root=[^ ]* server=aprender-mcp-decide -> ')"
    [ -n "$RESOLVED" ] || { echo "REFUSED resolver-parse: no RESOLVED line in $LOG" >&2; exit 1; }
    printf 'RESOLVED=%s\nCONTROL=%s\n' "$RESOLVED" "$CONTROL"

# Write the gitignored crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml from its tracked
# template for <apr>: server name, s3://<bucket>/decide/<server>/<sha256>.apr, the sha256 pin,
# and [auth] enabled from <auth> (on|off, the plan 08-11 decision). LOCAL ONLY: eligibility is
# enforced by the recipes that write to AWS. DRY_RUN=1 uses the literal bucket dry-run-bucket.
laya-deploy-config apr auth server="aprender-mcp-decide" env="dev" profile="ze-kasher-dev":
    #!/usr/bin/env bash
    set -euo pipefail
    APR="{{apr}}"
    SERVER="{{server}}"
    ENV="{{ trim_start_match(env, "env=") }}"
    case "$ENV" in
        dev|prod) ;;
        *) echo "ERROR: '$ENV' is not a known environment (expected dev or prod)" >&2; exit 2 ;;
    esac
    case "{{auth}}" in
        on) AUTH=true ;;
        off) AUTH=false ;;
        *) echo "ERROR: auth must be on or off, got '{{auth}}'" >&2; exit 2 ;;
    esac
    # shared-crates-root: only this name resolves to the decide package (see the section header).
    test "$SERVER" = "aprender-mcp-decide" || { echo "ERROR: server must be aprender-mcp-decide under the shared-crates-root deploy (got '$SERVER')" >&2; exit 2; }
    test -f "$APR" || { echo "ERROR: $APR does not exist" >&2; exit 2; }
    # shasum directly (plan 08-22, V11-c): the rtk hook never rewrites a command inside a recipe, so a
    # proxy branch added nothing and made the identity pin depend on which `rtk` is on PATH.
    sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
    H="$(sha256 "$APR")"
    if [ "${DRY_RUN:-0}" = "1" ]; then
        BUCKET="dry-run-bucket"
    else
        ACCOUNT="$(aws sts get-caller-identity --profile "{{profile}}" --query Account --output text)"
        BUCKET="aprender-decide-weights-${ACCOUNT}-${ENV}"
    fi
    DIR="crates/aprender-mcp-decide-lambda/.pmcp"
    python3 - "$DIR/deploy.toml.template" "$DIR/deploy.toml" "$SERVER" \
        "s3://$BUCKET/decide/$SERVER/$H.apr" "$H" "$AUTH" <<'PY'
    import re, sys, tomllib
    template, out, server, uri, sha, auth = sys.argv[1:7]
    text = open(template).read()
    unset = '"UNSET-run-just-laya-deploy-config"'
    for key, value in (("name", server), ("APRENDER_DECIDE_S3_URI", uri), ("APRENDER_DECIDE_SHA256", sha)):
        marker = f"{key} = {unset}"
        if text.count(marker) != 1:
            sys.exit(f"{template}: expected exactly one placeholder for {key}; restore the template")
        text = text.replace(marker, f'{key} = "{value}"')
    # The stack-declared weights read: s3:GetObject on this server's prefix in the URI's bucket.
    bucket = uri[len("s3://"):].split("/", 1)[0]
    arn = f"arn:aws:s3:::{bucket}/decide/{server}/*"
    rmarker = f"resources = [{unset}]"
    if text.count(rmarker) != 1:
        sys.exit(f"{template}: expected exactly one [[iam.statements]] resources placeholder; restore the template")
    text = text.replace(rmarker, f'resources = ["{arn}"]')
    text, n = re.subn(r"(\[auth\]\nenabled = )(true|false)", r"\g<1>" + auth, text)
    if n != 1:
        sys.exit(f"{template}: no '[auth]' + 'enabled =' pair to set")
    if "UNSET-run-just-laya-deploy-config" in text:
        sys.exit("a placeholder remains after substitution; refusing to write")
    cfg = tomllib.loads(text)
    assert cfg["server"]["name"] == server
    assert cfg["environment"]["APRENDER_DECIDE_S3_URI"] == uri
    assert cfg["environment"]["APRENDER_DECIDE_SHA256"] == sha
    assert cfg["auth"]["enabled"] is (auth == "true")
    assert cfg["iam"]["statements"][0]["resources"] == [arn]
    open(out, "w").write(text)
    print(f"  server   {server}\n  s3 uri   {uri}\n  sha256   {sha}\n  auth     {auth}\n  iam      Allow s3:GetObject {arn}")
    PY
    just _laya-iam-check "$DIR/deploy.toml" || { rm -f "$DIR/deploy.toml"; echo "ERROR: the generated config failed the iam check; removed it" >&2; exit 1; }
    echo "  wrote $DIR/deploy.toml (gitignored) for env=$ENV"

# The decide config's [iam] must be EXACTLY one statement: Allow, actions ["s3:GetObject"],
# resources ["arn:aws:s3:::<bucket-of-APRENDER_DECIDE_S3_URI>/decide/<server.name>/*"], and no
# other [iam] key (tables/buckets sugar). Anything broader (s3:*, a bucket-wide `*`, a second
# statement, ListBucket) is refused (exit 1). Pure: no network. Called by laya-deploy-config and
# laya-deploy; exercised by laya-deploy-selftest's IAM table.
[positional-arguments]
_laya-iam-check cfg:
    #!/usr/bin/env bash
    set -euo pipefail
    python3 - "$1" <<'PY'
    import sys, tomllib
    try:
        c = tomllib.load(open(sys.argv[1], "rb"))
    except Exception as e:
        print(f"REFUSED iam: {sys.argv[1]} does not parse ({e})", file=sys.stderr); sys.exit(1)
    def refuse(msg):
        print(f"REFUSED iam: {msg}", file=sys.stderr); sys.exit(1)
    server = c.get("server", {}).get("name", "")
    uri = c.get("environment", {}).get("APRENDER_DECIDE_S3_URI", "")
    if not uri.startswith("s3://") or "/" not in uri[5:]:
        refuse(f"APRENDER_DECIDE_S3_URI {uri!r} is not an s3:// object URI")
    bucket = uri[5:].split("/", 1)[0]
    want = {"effect": "Allow", "actions": ["s3:GetObject"], "resources": [f"arn:aws:s3:::{bucket}/decide/{server}/*"]}
    iam = c.get("iam")
    if not isinstance(iam, dict):
        refuse("no [iam] section: the weights read would not be in the stack")
    if set(iam) != {"statements"}:
        refuse(f"[iam] carries {sorted(set(iam) - {'statements'})}; only [[iam.statements]] is allowed")
    st = iam["statements"]
    if not isinstance(st, list) or len(st) != 1:
        refuse(f"expected exactly one [[iam.statements]], found {len(st) if isinstance(st, list) else st!r}")
    if st[0] != want:
        refuse(f"the statement is {st[0]!r}, not the scoped read {want!r}")
    print(f"iam ok: Allow s3:GetObject {want['resources'][0]} and nothing else")
    PY

# Run <cmd> with the decide config installed ALONE at the shared deploy root <root>: its
# `.pmcp/{deploy,deployment}.toml`, `.pmcp/active-target` and `deploy/` (setfit-train's
# rendered stack.ts and bootstrap) are backed up, removed, and restored on EXIT, INT, TERM
# and HUP, then checked byte-identical by sha256 (exit 70 and the backup kept if not).
# `deploy/` goes too: cargo-pmcp PRESERVES an existing deploy/lib/stack.ts, so the decide
# deploy would otherwise synthesize setfit-train's stack. cargo-pmcp's decide-side
# deployment.toml and stack.ts are copied to <snap> first. A leftover backup refuses.
[positional-arguments]
_laya-crates-root-swap root cfg snap +cmd:
    #!/usr/bin/env bash
    set -euo pipefail
    ROOT="$1"; CFG="$2"; SNAP="$3"; shift 3
    STATE=(.pmcp/deploy.toml .pmcp/deployment.toml .pmcp/active-target deploy)
    test -d "$ROOT" || { echo "REFUSED swap: deploy root $ROOT is not a directory" >&2; exit 2; }
    test -f "$CFG" || { echo "REFUSED swap: decide config $CFG does not exist" >&2; exit 2; }
    BK="models/decide/swap-backup/$(printf '%s' "$ROOT" | tr '/.' '__')"
    if [ -e "$BK" ]; then
        echo "REFUSED swap: $BK exists -- an earlier swap of $ROOT did not finish restoring." >&2
        echo "        Compare it with $ROOT, restore by hand, then remove it." >&2
        exit 2
    fi
    digest() {
        local e
        for e in "${STATE[@]}"; do
            if [ -e "$ROOT/$e" ]; then
                (cd "$ROOT" && find "$e" -type f -print0 | LC_ALL=C sort -z | xargs -0 shasum -a 256)
            else
                echo "absent $e"
            fi
        done | shasum -a 256 | awk '{print $1}'
    }
    BEFORE="$(digest)"
    PMCP_EXISTED=0; [ -d "$ROOT/.pmcp" ] && PMCP_EXISTED=1
    mkdir -p "$BK"
    for e in "${STATE[@]}"; do
        if [ -e "$ROOT/$e" ]; then
            mkdir -p "$BK/$(dirname "$e")"
            cp -Rp "$ROOT/$e" "$BK/$e"
            echo "present $e"
        else
            echo "absent $e"
        fi
    done > "$BK/MANIFEST"
    restore() {
        local rc=$? state e after
        set +e
        trap - EXIT INT TERM HUP
        mkdir -p "$SNAP"
        for e in .pmcp/deployment.toml deploy/lib/stack.ts; do
            [ -f "$ROOT/$e" ] && cp -p "$ROOT/$e" "$SNAP/$(basename "$e")"
        done
        while read -r state e; do
            rm -rf "${ROOT:?}/$e"
            [ "$state" = "present" ] && cp -Rp "$BK/$e" "$ROOT/$e"
        done < "$BK/MANIFEST"
        [ "$PMCP_EXISTED" = "1" ] || rmdir "$ROOT/.pmcp" 2>/dev/null
        after="$(digest)"
        if [ "$after" != "$BEFORE" ]; then
            echo "ERROR: $ROOT was NOT restored byte-identically (state sha256 $BEFORE -> $after); backup kept at $BK" >&2
            exit 70
        fi
        rm -rf "$BK"
        echo "RESTORED $ROOT byte-identical (state sha256 $after, exit $rc)" >&2
        exit "$rc"
    }
    trap restore EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    trap 'exit 129' HUP
    for e in "${STATE[@]}"; do rm -rf "${ROOT:?}/$e"; done
    mkdir -p "$ROOT/.pmcp"
    cp "$CFG" "$ROOT/.pmcp/deploy.toml"
    echo "SWAPPED $ROOT: decide config installed alone (backup $BK)" >&2
    set +e
    "$@"
    rc=$?
    set -e
    exit "$rc"

# Cross-compile the decide bootstrap for Lambda arm64 and prove it: rebuilt now (the output is
# newer than the build's start), an aarch64 ELF, and carrying `aprender-mcp-decide-lambda` (08-07
# found a stale bootstrap from another crate at this path). A BUILD check, never resolver evidence.
laya-build-bootstrap:
    #!/usr/bin/env bash
    set -euo pipefail
    ulimit -n 65536 || echo "WARNING: could not raise the fd limit; a link may fail with ProcessFdQuotaExceeded" >&2
    command -v cargo-zigbuild >/dev/null || { echo "ERROR: cargo-zigbuild is not installed" >&2; exit 1; }
    TD="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
    OUT="$TD/{{target}}/release/bootstrap"
    STAMP="$(mktemp)"
    trap 'rm -f "$STAMP"' EXIT
    sleep 1
    # A cached build relinks nothing, so "newer than the start" needs the package to rebuild.
    touch crates/aprender-mcp-decide-lambda/src/main.rs
    set +e
    cargo zigbuild --release --target {{target}}.2.34 -p aprender-mcp-decide-lambda --bin bootstrap
    rc=$?
    set -e
    [ "$rc" -eq 0 ] || { echo "ERROR: cargo zigbuild exited $rc" >&2; exit "$rc"; }
    [ -f "$OUT" ] || { echo "ERROR: $OUT does not exist after the build" >&2; exit 1; }
    [ "$OUT" -nt "$STAMP" ] || { echo "ERROR: $OUT is older than this build -- not the binary just built" >&2; exit 1; }
    FT="$(file "$OUT")"
    case "$FT" in
        *"ARM aarch64"*) ;;
        *) echo "ERROR: not an aarch64 binary: $FT" >&2; exit 1 ;;
    esac
    NAMED="$(strings "$OUT" | grep -c 'aprender-mcp-decide-lambda' || true)"
    [ "${NAMED:-0}" -gt 0 ] || { echo "ERROR: $OUT does not name aprender-mcp-decide-lambda -- a stale bootstrap from another crate" >&2; exit 1; }
    echo "BOOTSTRAP aarch64 OK $OUT ($(wc -c < "$OUT" | tr -d ' ') bytes)"

# Deploy <apr> as the decide server, FAIL-CLOSED. Refusals, each `REFUSED <check>: ...` before
# any AWS call: (1) generated config present, (2) no placeholder left, (3) local sha256 ==
# the config's pin and content-addressed key, (4) resolver proof present, naming the decide
# package, for the installed cargo-pmcp, (5) ELIGIBILITY: `just laya-verify` accepts the exact
# file, (6) the S3 object has the local size (live only: first (6a) the config's bucket must be
# this env's, aprender-decide-weights-<account>-<env>). DRY_RUN=1 stops at (6) with DRY-RUN OK. Live:
# touch -> DEPLOYING -> cargo pmcp deploy (crates root, swapped) -> compile-log, edge-health
# serverId and live identity-probe assertions; any identity failure runs `just laya-teardown`
# (containment).
laya-deploy apr run data base server="aprender-mcp-decide" env="dev" profile="ze-kasher-dev":
    #!/usr/bin/env bash
    set -euo pipefail
    APR="{{apr}}"
    SERVER="{{server}}"
    ENV="{{ trim_start_match(env, "env=") }}"
    PROFILE="{{profile}}"
    CFG="crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml"
    PROOF="models/decide/resolver-proof.txt"
    EXPECT_PKG="crates/aprender-mcp-decide-lambda"
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    # shasum directly (plan 08-22, V11-c): the rtk hook never rewrites a command inside a recipe, so a
    # proxy branch added nothing and made the identity pin depend on which `rtk` is on PATH.
    sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
    case "$ENV" in
        dev|prod) ;;
        *) refuse env "'$ENV' is not a known environment (expected dev or prod)" ;;
    esac
    [ "$SERVER" = "aprender-mcp-decide" ] || refuse server "only aprender-mcp-decide resolves to the decide package from the crates root (got '$SERVER')"
    [ -f "$APR" ] || refuse artifact "$APR does not exist"
    # (1) the generated config
    [ -f "$CFG" ] || refuse config "$CFG does not exist -- generate it: just laya-deploy-config <apr> <on|off>"
    # (2) no placeholder left
    if grep -q 'UNSET-run-just-laya-deploy-config' "$CFG"; then
        refuse placeholder "$CFG still holds an UNSET placeholder -- regenerate it with just laya-deploy-config"
    fi
    # (3) the pin: the config names THIS file, by content
    H="$(sha256 "$APR")"
    FIELDS="$(python3 -c 'import sys, tomllib; c = tomllib.load(open(sys.argv[1], "rb")); e = c["environment"]; print(c["server"]["name"], e["APRENDER_DECIDE_S3_URI"], e["APRENDER_DECIDE_SHA256"])' "$CFG" 2>/dev/null)" \
        || refuse config "$CFG does not parse as the decide deploy config"
    read -r CFG_NAME CFG_URI CFG_SHA <<< "$FIELDS"
    [ "$CFG_NAME" = "$SERVER" ] || refuse config "the config deploys '$CFG_NAME', not $SERVER"
    [ "$CFG_SHA" = "$H" ] || refuse sha-pin "APRENDER_DECIDE_SHA256 in $CFG is $CFG_SHA but $APR hashes to $H"
    case "$CFG_URI" in
        s3://*/decide/"$SERVER"/"$H".apr) ;;
        *) refuse sha-pin "APRENDER_DECIDE_S3_URI $CFG_URI is not the content-addressed key decide/$SERVER/$H.apr" ;;
    esac
    # (3b) the stack-declared weights read: exactly s3:GetObject on decide/$SERVER/* and nothing broader
    IAMLOG="$(just _laya-iam-check "$CFG" 2>&1)" || refuse iam "$(printf '%s' "$IAMLOG" | grep -m 1 '^REFUSED' | sed 's/^REFUSED iam: //')"
    # (4) the resolver proof for the installed cargo-pmcp
    [ -f "$PROOF" ] || refuse resolver-proof "$PROOF does not exist -- run: just laya-resolver-proof <sdk> <commit> <scratch>"
    [ "$(head -n 1 "$PROOF")" = "$EXPECT_PKG" ] || refuse resolver-proof "$PROOF names '$(head -n 1 "$PROOF")', not $EXPECT_PKG"
    PROVEN_VER="$(sed -n 's/^cargo_pmcp_version=//p' "$PROOF")"
    INSTALLED_VER="$(cargo pmcp --version 2>/dev/null | awk '{print $2}' || true)"
    if [ -z "$INSTALLED_VER" ] || [ "$INSTALLED_VER" != "$PROVEN_VER" ]; then
        refuse resolver-proof "the proof is for cargo-pmcp ${PROVEN_VER:-?}, the installed tool is ${INSTALLED_VER:-absent}; re-run just laya-resolver-proof"
    fi
    # (5) ELIGIBILITY -- the Rust verifier on the exact file (decide-apr-v1 deploy_eligibility),
    # never `inspect`, and nothing below reaches AWS unless it accepted.
    mkdir -p models/decide
    VLOG="models/decide/eligibility-$SERVER.log"
    set +e
    just laya-verify "$APR" "{{run}}" "{{data}}" "{{base}}" > "$VLOG" 2>&1
    vrc=$?
    set -e
    if [ "$vrc" -ne 0 ]; then
        refuse eligibility "$(grep -m 1 '^REFUSED' "$VLOG" || echo "laya-verify exited $vrc (log: $VLOG)")" "$vrc"
    fi
    python3 - "$VLOG" "$H" <<'PY' || refuse eligibility "laya-verify exited 0 without deploy_eligible true for sha256 $H (log: $VLOG)"
    import json, sys
    lines = [l for l in open(sys.argv[1]) if l.startswith("{")]
    v = json.loads(lines[-1])
    sys.exit(0 if v.get("deploy_eligible") is True and v.get("artifact_sha256") == sys.argv[2] else 1)
    PY
    echo "  eligible: laya-verify accepted $APR (sha256 $H)"
    # (6) the uploaded object
    BK_KEY="${CFG_URI#s3://}"
    BUCKET="${BK_KEY%%/*}"
    KEY="${BK_KEY#*/}"
    SIZE="$(wc -c < "$APR" | tr -d ' ')"
    if [ "${DRY_RUN:-0}" = "1" ]; then
        echo "DRY-RUN: skipping the head-object check of s3://$BUCKET/$KEY (expected ContentLength $SIZE)"
        just laya-build-bootstrap
        echo "RESOLVER PROOF: $(head -n 1 "$PROOF")"
        echo "DRY-RUN OK $SERVER (sha256 $H; nothing deployed)"
        exit 0
    fi
    # (6a) the config's weights bucket is THIS environment's. A config generated for another env
    # would otherwise pass every check above, deploy live (replacing the running function), and
    # only then be refused by laya-grant's bucket ARN -- and contained, i.e. taken down.
    ACCOUNT="$(aws sts get-caller-identity --profile "$PROFILE" --query Account --output text)" \
        || refuse env "cannot read the AWS account for profile $PROFILE"
    WANT_BUCKET="aprender-decide-weights-${ACCOUNT}-${ENV}"
    [ "$BUCKET" = "$WANT_BUCKET" ] \
        || refuse env "the config's weights bucket is $BUCKET but env=$ENV expects $WANT_BUCKET -- regenerate it: just laya-deploy-config $APR <on|off> $SERVER $ENV $PROFILE"
    REMOTE="$(aws s3api head-object --profile "$PROFILE" --bucket "$BUCKET" --key "$KEY" \
        --query ContentLength --output text 2>/dev/null)" \
        || refuse s3-object "s3://$BUCKET/$KEY is absent -- upload it: just laya-upload $APR {{run}} {{data}} {{base}}"
    [ "$REMOTE" = "$SIZE" ] || refuse s3-object "s3://$BUCKET/$KEY is $REMOTE bytes, $APR is $SIZE"
    # Live. The touch makes the decide package ALWAYS recompile, so its absence from the log
    # means cargo-pmcp built something else (a cached build prints no Compiling line).
    ulimit -n 65536 || echo "WARNING: could not raise the fd limit; a link may fail with ProcessFdQuotaExceeded" >&2
    touch crates/aprender-mcp-decide-lambda/src/main.rs
    LOG="models/decide/deploy-$SERVER.log"
    SNAP="models/decide/deploy-$SERVER.state"
    rm -rf "$SNAP"
    contain() {
        echo "IDENTITY FAILURE: $1 -- containing (reserved concurrency 0)" >&2
        just laya-teardown "$SERVER" "$ENV" "$PROFILE" \
            || echo "ERROR: containment failed -- throttle it by hand: aws lambda put-function-concurrency --function-name $SERVER --reserved-concurrent-executions 0" >&2
        exit 1
    }
    # --no-post-deploy-test: this recipe's identity chain (edge health serverId, then a live
    # identity probe) is the verification this server is held to, not cargo-pmcp's suite.
    # The weights read is now IN THE STACK ([[iam.statements]], checked at (3b)), so the
    # function never runs without it -- the out-of-band grant it replaces arrived after
    # pmcp.run's own post-deploy invocation had already failed the load. (plan 08-17)
    # --no-oauth when the config says [auth] enabled = false: the human's auth-off decision
    # made explicit on the command line, not only in the file.
    AUTH_ON="$(python3 -c 'import sys, tomllib; print(str(tomllib.load(open(sys.argv[1], "rb"))["auth"]["enabled"]).lower())' "$CFG")"
    OAUTH_FLAG=()
    [ "$AUTH_ON" = "true" ] || OAUTH_FLAG=(--no-oauth)
    echo "DEPLOYING $SERVER (auth enabled=$AUTH_ON)"
    set +e
    just _laya-crates-root-swap crates "$CFG" "$SNAP" \
        cargo pmcp deploy --manifest-path crates --regenerate-stack --no-post-deploy-test ${OAUTH_FLAG[@]+"${OAUTH_FLAG[@]}"} --no-color > "$LOG" 2>&1
    rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then
        echo "ERROR: the deploy exited $rc (log: $LOG). If it created the function, contain it: just laya-teardown $SERVER $ENV $PROFILE" >&2
        exit "$rc"
    fi
    grep -q 'Compiling aprender-mcp-decide-lambda ' "$LOG" || contain "the compile log does not name aprender-mcp-decide-lambda"
    OTHER="$(grep -oE 'Compiling [A-Za-z0-9_-]+-lambda ' "$LOG" | grep -v 'aprender-mcp-decide-lambda' | sort -u | tr '\n' ' ' || true)"
    [ -z "$OTHER" ] || contain "the compile log also builds $OTHER"
    ENDPOINT="$(python3 -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["deployment"]["endpoint"])' "$SNAP/deployment.toml" 2>/dev/null)" \
        || contain "no endpoint in $SNAP/deployment.toml"
    # Read-only: the stack put the scoped read on the role before the function existed.
    just laya-grant "$SERVER" "$ENV" "$PROFILE" || contain "the function's role does not carry the stack-declared scoped weights read"
    # Edge health (D-ITEM-08-17-A). No GET reaches the bootstrap on pmcp.run: the edge answers
    # GET /mcp itself (405) and serves /health as platform JSON with no package field. So this
    # step proves only that the edge routes <server>: /health must name it as serverId. Package
    # identity is the probe's job below -- a POST tools/call that must return artifact_sha256
    # == H with the labels in order, which a wrong binary under this name cannot answer.
    HEALTH_URL="$(just _laya-edge-health-url "$ENDPOINT")" || contain "cannot derive the edge health URL from $ENDPOINT"
    LOGGED_HEALTH="$(sed -n 's/^ *health_endpoint: "\(.*\)"$/\1/p' "$LOG" | head -n 1)"
    [ -z "$LOGGED_HEALTH" ] || [ "$LOGGED_HEALTH" = "$HEALTH_URL" ] \
        || contain "cargo pmcp reported health_endpoint $LOGGED_HEALTH, but $ENDPOINT derives $HEALTH_URL"
    HEALTH="$(curl -fsS --max-time 30 "$HEALTH_URL")" || contain "GET $HEALTH_URL failed"
    printf '%s\n' "$HEALTH" > "models/decide/deploy-health-$SERVER.json"
    just _laya-edge-health-check "$HEALTH" "$SERVER" \
        || contain "the edge health body at $HEALTH_URL does not name $SERVER as serverId: $HEALTH"
    echo "  edge health: $HEALTH_URL names serverId $SERVER"
    # The edge settles AFTER cargo-pmcp reports success: on 2026-09-27 pmcp.run's own post-deploy
    # invocation loaded the model (24.6 s, success) and the edge still answered POST /mcp with 503
    # "Server is in error state" for under a minute, then forwarded. So THAT refusal, and only
    # that one, is retried (bounded); any other probe failure contains at once. (plan 08-17)
    PLOG="models/decide/deploy-probe-$SERVER.log"
    TRIES="${LAYA_EDGE_SETTLE_TRIES:-8}"
    t=1
    while :; do
        set +e
        cargo run --release -p aprender-mcp-decide-lambda --example probe -- \
            --url "$ENDPOINT" --apr "$APR" --expect-sha256 "$H" > "$PLOG" 2>&1
        prc=$?
        set -e
        [ "$prc" -eq 0 ] && break
        if grep -q 'Server is in error state' "$PLOG" && [ "$t" -lt "$TRIES" ]; then
            echo "  edge not settled (503 Server is in error state), attempt $t/$TRIES; retrying in 15 s" >&2
            t=$((t + 1))
            sleep 15
            continue
        fi
        contain "the identity probe failed after $t attempt(s) (log: $PLOG)"
    done
    echo "DEPLOYED $SERVER at $ENDPOINT: compile log, edge health serverId and live identity (sha256 $H) name the decide server"
    echo "  next: just laya-deploy-verify $APR $SERVER $PROFILE"

# The pmcp.run edge health URL for an MCP endpoint: https://<host>/mcp -> https://<host>/health.
# Anything else is refused (exit 2), so a changed endpoint shape fails closed instead of probing
# an unrelated URL. Pure: no network. Exercised by laya-deploy-selftest's EDGE HEALTH table.
[positional-arguments]
_laya-edge-health-url endpoint:
    #!/usr/bin/env bash
    set -euo pipefail
    python3 - "$1" <<'PY'
    import re, sys
    m = re.fullmatch(r"(https://[A-Za-z0-9.-]+)/mcp", sys.argv[1])
    if not m:
        print(f"REFUSED edge-health-url: {sys.argv[1]!r} is not https://<host>/mcp", file=sys.stderr)
        sys.exit(2)
    print(m.group(1) + "/health")
    PY

# Accept an edge /health body only when it is a JSON object whose serverId is exactly <server>.
# No package field is asserted: the edge body has none (D-ITEM-08-17-A); package identity is the
# live identity probe's. Pure: no network. Exercised by laya-deploy-selftest's EDGE HEALTH table.
[positional-arguments]
_laya-edge-health-check body server:
    #!/usr/bin/env bash
    set -euo pipefail
    python3 - "$1" "$2" <<'PY'
    import json, sys
    body, server = sys.argv[1], sys.argv[2]
    try:
        b = json.loads(body)
    except ValueError:
        print("REFUSED edge-health: the body is not JSON", file=sys.stderr)
        sys.exit(1)
    got = b.get("serverId") if isinstance(b, dict) else None
    if got != server:
        print(f"REFUSED edge-health: serverId is {got!r}, not {server!r}", file=sys.stderr)
        sys.exit(1)
    print(f"edge-health ok: serverId {server}")
    PY

# Prove every deploy refusal OFFLINE on the synthetic tiny artifact (the only one 08-09 writes),
# DRY_RUN=1 throughout, with `aws` shadowed by a recorder that must stay empty. Cases: placeholder,
# sha-pin, resolver-proof, deploy-eligibility, upload-eligibility (the last two from laya-verify:
# SyntheticNotDeployable); the crates-root swap restored byte-identical on success, forced
# failure, SIGTERM and an absent root; the edge-health step's URL and body tables and its wiring
# into laya-deploy (D-ITEM-08-17-A); resolver proof; bootstrap build. The POSITIVE dry run (a full
# `laya-deploy` DRY_RUN on an artifact `laya-verify` must accept; loads the multi-GB base, run it under
# the host's real-weights lock) is ARMED BY DEFAULT when the deployed artifact
# models/decide/laya-stance-64.apr, its run dir, data/decide/tweet-stance-64 and the pinned base
# snapshot all exist in the MAIN checkout (plan 08-22, AL7). LAYA_ELIGIBLE_APR/_RUN/_DATA/_BASE (all
# four) override the artifact; LAYA_DEPLOY_SELFTEST_POSITIVE=0 disarms it. A skipped positive run is
# never reported as a bare OK: the last line is then `DEPLOY SELFTEST OK (positive dry run SKIPPED: <why>)`.
laya-deploy-selftest:
    #!/usr/bin/env bash
    set -euo pipefail
    export DRY_RUN=1
    T="crates/aprender-decide/tests/fixtures/laya_tiny"
    ST="models/decide/selftest"
    A="$ST/laya_tiny.apr"
    CASES="$ST/cases"
    CFG="crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml"
    PROOF="models/decide/resolver-proof.txt"
    GOLDEN="37d65159b2be0fa091aa840cd56c1a84b73c0bcd9e2df5906d1f8218f5448561"
    FAILS=0
    fail() { echo "FAIL $1" >&2; FAILS=$((FAILS + 1)); }
    sha_or_absent() { if [ -f "$1" ]; then shasum -a 256 "$1" | awk '{print $1}'; else echo absent; fi; }
    rm -rf "$CASES" "$ST/snap" "$ST/root-absent"
    mkdir -p "$CASES" "$ST/shim"
    # Keep a pre-existing generated decide config and the resolver proof; restore both on EXIT.
    BK="$(mktemp -d "$ST/bk.XXXXXX")"
    [ -f "$CFG" ] && cp -p "$CFG" "$BK/decide-deploy.toml"
    cp -p "$PROOF" "$BK/resolver-proof.txt" 2>/dev/null || true
    cleanup() {
        if [ -f "$BK/decide-deploy.toml" ]; then cp -p "$BK/decide-deploy.toml" "$CFG"; else rm -f "$CFG"; fi
        [ -f "$BK/resolver-proof.txt" ] && cp -p "$BK/resolver-proof.txt" "$PROOF"
        rm -rf "$BK"
    }
    trap cleanup EXIT
    # The shared deploy root's state (setfit-train's), before anything runs.
    ROOT_TOML_BEFORE="$(sha_or_absent crates/.pmcp/deploy.toml)"
    root_digest() {
        local e
        for e in .pmcp/deploy.toml .pmcp/deployment.toml .pmcp/active-target deploy; do
            if [ -e "crates/$e" ]; then
                (cd crates && find "$e" -type f -print0 | LC_ALL=C sort -z | xargs -0 shasum -a 256)
            else
                echo "absent $e"
            fi
        done | shasum -a 256 | awk '{print $1}'
    }
    ROOT_BEFORE="$(root_digest)"
    # The aws recorder: first on PATH, appends its argv, never reaches AWS.
    REC="$(pwd -P)/$ST/aws-calls.log"
    : > "$REC"
    printf '#!/usr/bin/env bash\nprintf "%%s\\n" "$*" >> "%s"\necho "aws recorder: the selftest never reaches AWS" >&2\nexit 97\n' "$REC" > "$ST/shim/aws"
    chmod +x "$ST/shim/aws"
    export PATH="$(pwd -P)/$ST/shim:$PATH"
    [ "$(command -v aws)" = "$(pwd -P)/$ST/shim/aws" ] || { echo "ERROR: the aws recorder is not first on PATH" >&2; exit 1; }
    # Positive control: a zero count means nothing only if the recorder demonstrably records.
    set +e; aws recorder-control > /dev/null 2>&1; crc=$?; set -e
    [ "$crc" -eq 97 ] && [ "$(wc -l < "$REC" | tr -d ' ')" -eq 1 ] || { echo "ERROR: the aws recorder did not record its control call (exit $crc)" >&2; exit 1; }
    : > "$REC"
    echo "AWS RECORDER: control call recorded and cleared"
    # Set-up: the synthetic artifact and a consistent config for it.
    just laya-pack-fixture "$T" "$T/data" "$A" > "$CASES/0-fixture.log" 2>&1 || { tail -5 "$CASES/0-fixture.log" >&2; exit 1; }
    grep -q "sha256=$GOLDEN" "$CASES/0-fixture.log" || { echo "ERROR: laya-pack-fixture did not reproduce the golden $GOLDEN" >&2; exit 1; }
    regen() { just laya-deploy-config "$A" off > "$CASES/config-$1.log" 2>&1 || { tail -5 "$CASES/config-$1.log" >&2; exit 1; }; }
    regen setup
    [ -f "$PROOF" ] || { echo "ERROR: $PROOF is missing -- run just laya-resolver-proof first" >&2; exit 1; }
    # One refusal case: non-zero AND the expected reason, or it is a FAIL.
    expect_refused() {
        local n="$1" name="$2" pat="$3" log rc reason
        shift 3
        log="$CASES/$n-$name.log"
        set +e
        "$@" > "$log" 2>&1
        rc=$?
        set -e
        if [ "$rc" -ne 0 ] && grep -Eq "$pat" "$log"; then
            reason="$(grep -Eo "$pat.*" "$log" | head -n 1 | cut -c 1-300)"
            echo "CASE $n $name: REFUSED as expected ($reason)"
        else
            fail "CASE $n $name: rc=$rc, expected a refusal matching '$pat' (log: $log)"
            tail -5 "$log" >&2
        fi
    }
    deploy_tiny() { just laya-deploy "$A" "$T" "$T/data" "$T/checkpoint"; }
    # Each mutation leaves VALID TOML, so the refusal is the check under test, not a parse error.
    mutate() { python3 -c 'import re, sys; p, key, value = sys.argv[1:4]; t = open(p).read(); t, n = re.subn(r"^(" + key + r" = )\"[^\"]*\"$", lambda m: m.group(1) + chr(34) + value + chr(34), t, count=1, flags=re.M); assert n == 1, key; open(p, "w").write(t)' "$CFG" "$1" "$2"; }
    mutate APRENDER_DECIDE_S3_URI UNSET-run-just-laya-deploy-config
    expect_refused 1 placeholder 'REFUSED placeholder:' deploy_tiny
    regen 2
    mutate APRENDER_DECIDE_SHA256 0000000000000000000000000000000000000000000000000000000000000000
    expect_refused 2 sha-pin 'REFUSED sha-pin:' deploy_tiny
    regen 3
    mv "$PROOF" "$BK/proof.moved"
    expect_refused 3 resolver-proof 'REFUSED resolver-proof:' deploy_tiny
    mv "$BK/proof.moved" "$PROOF"
    expect_refused 4 deploy-eligibility 'REFUSED eligibility: .*SyntheticNotDeployable' deploy_tiny
    expect_refused 5 upload-eligibility 'REFUSED eligibility: .*SyntheticNotDeployable' \
        just laya-upload "$A" "$T" "$T/data" "$T/checkpoint"
    # The shared-root swap: the setfit-train state comes back byte-identical on every path.
    SNAP="$ST/snap"
    swap_case() {
        local n="$1" name="$2" want="$3" rc
        shift 3
        set +e
        just _laya-crates-root-swap "$@" > "$CASES/swap-$n-$name.log" 2>&1
        rc=$?
        set -e
        local now; now="$(root_digest)"
        if [ "$rc" -eq "$want" ] && [ "$now" = "$ROOT_BEFORE" ] && grep -q '^RESTORED ' "$CASES/swap-$n-$name.log" \
            && [ ! -e models/decide/swap-backup/crates ]; then
            echo "SWAP $n $name: exit $rc, crates root restored byte-identical (state sha256 $now)"
        else
            fail "SWAP $n $name: exit $rc (want $want), state $now vs $ROOT_BEFORE (log: $CASES/swap-$n-$name.log)"
        fi
    }
    # 1: the decide config is in place ALONE while the command runs (deploy/ cleared), then restored.
    swap_case 1 success 0 crates "$CFG" "$SNAP" \
        sh -c 'cmp -s crates/.pmcp/deploy.toml "$1" && test ! -e crates/deploy && test ! -e crates/.pmcp/deployment.toml' _ "$CFG"
    swap_case 2 forced-failure 1 crates "$CFG" "$SNAP" false
    swap_case 3 sigterm 143 crates "$CFG" "$SNAP" sh -c 'kill -TERM "$PPID"'
    # 4: a root with no prior state gets none back.
    mkdir -p "$ST/root-absent"
    set +e
    just _laya-crates-root-swap "$ST/root-absent" "$CFG" "$SNAP" test -f "$ST/root-absent/.pmcp/deploy.toml" > "$CASES/swap-4-absent-root.log" 2>&1
    rc=$?
    set -e
    if [ "$rc" -eq 0 ] && [ -z "$(ls -A "$ST/root-absent")" ]; then
        echo "SWAP 4 absent-root: exit 0, the decide config was installed and nothing is left behind"
    else
        fail "SWAP 4 absent-root: exit $rc, left: $(ls -A "$ST/root-absent" | tr '\n' ' ')"
    fi
    # EDGE HEALTH (D-ITEM-08-17-A): the live health step, as a must-accept / must-refuse table
    # over the two pure helpers laya-deploy calls. No network: the bodies are the ones measured
    # on pmcp.run on 2026-09-27 (decide, chronos, the edge's 405 error) plus the bootstrap's own
    # body, which carries package/server but no serverId and so must NOT pass.
    EP="https://aprender-mcp-decide.us-east.true-mcp.com/mcp"
    url_case() {
        local want="$1" in="$2" expect="${3:-}" got rc
        set +e; got="$(just _laya-edge-health-url "$in" 2> /dev/null)"; rc=$?; set -e
        if [ "$want" = accept ] && [ "$rc" -eq 0 ] && [ "$got" = "$expect" ]; then
            echo "EDGE HEALTH url accept: $in -> $got"
        elif [ "$want" = refuse ] && [ "$rc" -ne 0 ] && [ -z "$got" ]; then
            echo "EDGE HEALTH url refuse: '$in' (exit $rc)"
        else
            fail "EDGE HEALTH url $want '$in': exit $rc, got '$got'"
        fi
    }
    url_case accept "$EP" "https://aprender-mcp-decide.us-east.true-mcp.com/health"
    url_case refuse "https://aprender-mcp-decide.us-east.true-mcp.com/mcp/"
    url_case refuse "https://aprender-mcp-decide.us-east.true-mcp.com/health"
    url_case refuse "http://aprender-mcp-decide.us-east.true-mcp.com/mcp"
    url_case refuse "https://aprender-mcp-decide.us-east.true-mcp.com/x/mcp"
    url_case refuse ""
    body_case() {
        local want="$1" name="$2" body="$3" rc
        set +e; just _laya-edge-health-check "$body" aprender-mcp-decide > /dev/null 2>&1; rc=$?; set -e
        if { [ "$want" = accept ] && [ "$rc" -eq 0 ]; } || { [ "$want" = refuse ] && [ "$rc" -ne 0 ]; }; then
            echo "EDGE HEALTH body $want: $name (exit $rc)"
        else
            fail "EDGE HEALTH body $want $name: exit $rc"
        fi
    }
    body_case accept decide-edge '{"status":"healthy","serverId":"aprender-mcp-decide","serverName":"aprender-mcp-decide","hasDeployment":true}'
    body_case refuse chronos-edge '{"status":"healthy","serverId":"chronos-forecaster","serverName":"chronos-forecaster","hasDeployment":true}'
    body_case refuse bootstrap-body '{"package":"aprender-mcp-decide-lambda","server":"aprender-mcp-decide"}'
    body_case refuse edge-405 '{"jsonrpc":"2.0","error":{"code":-32600,"message":"SSE streams are not offered at this endpoint. Use POST /mcp."},"id":null}'
    body_case refuse prefix-server '{"serverId":"aprender-mcp-decide-2"}'
    body_case refuse not-an-object '["aprender-mcp-decide"]'
    body_case refuse not-json '<html>503</html>'
    body_case refuse empty ''
    # Wiring: laya-deploy calls both helpers and never GETs the /mcp endpoint itself.
    DEPLOY_BODY="$(just --show laya-deploy)"
    if printf '%s' "$DEPLOY_BODY" | grep -q 'just _laya-edge-health-url "\$ENDPOINT"' \
        && printf '%s' "$DEPLOY_BODY" | grep -q 'just _laya-edge-health-check "\$HEALTH" "\$SERVER"' \
        && ! printf '%s' "$DEPLOY_BODY" | grep -q 'curl [^|]*"\$ENDPOINT"'; then
        echo "EDGE HEALTH wiring: laya-deploy checks \$HEALTH_URL's serverId and never GETs \$ENDPOINT"
    else
        fail "EDGE HEALTH wiring: laya-deploy does not call both helpers, or still GETs \$ENDPOINT"
    fi
    # IAM (08-17 option 1): the generated config carries the scoped weights read IN THE STACK and
    # nothing broader. must-accept / must-refuse table over _laya-iam-check, then laya-deploy's own
    # refusal of a broadened config. Each variant is the regenerated config with ONE edit.
    regen iam
    iam_variant() {
        python3 - "$CFG" "$CASES/iam-$1.toml" "$1" <<'PY'
    import re, sys
    src, out, kind = sys.argv[1:4]
    t = open(src).read()
    def sub(pat, rep):
        global t
        t, n = re.subn(pat, rep, t, count=1, flags=re.M)
        assert n == 1, (kind, pat)
    if kind == "s3-star":
        sub(r'^actions = \["s3:GetObject"\]$', 'actions = ["s3:*"]')
    elif kind == "list-too":
        sub(r'^actions = \["s3:GetObject"\]$', 'actions = ["s3:GetObject", "s3:ListBucket"]')
    elif kind == "bucket-wide":
        sub(r'^resources = \["arn:aws:s3:::([^/"]+)/decide/[^"]*"\]$', r'resources = ["arn:aws:s3:::\1/*"]')
    elif kind == "star-resource":
        sub(r'^resources = \["arn:aws:s3:::[^"]*"\]$', 'resources = ["*"]')
    elif kind == "other-server":
        sub(r'^resources = \["arn:aws:s3:::([^/"]+)/decide/[^"]*"\]$', r'resources = ["arn:aws:s3:::\1/decide/*"]')
    elif kind == "two-statements":
        t += '\n[[iam.statements]]\neffect = "Allow"\nactions = ["s3:GetObject"]\nresources = ["arn:aws:s3:::other/x"]\n'
    elif kind == "bucket-sugar":
        t += '\n[[iam.buckets]]\nname = "other"\npermissions = ["read"]\n'
    elif kind == "no-iam":
        t = t[: t.index("[[iam.statements]]")]
    elif kind != "as-generated":
        sys.exit(f"unknown variant {kind}")
    open(out, "w").write(t)
    PY
    }
    iam_case() {
        local want="$1" kind="$2" rc
        iam_variant "$kind"
        set +e; just _laya-iam-check "$CASES/iam-$kind.toml" > "$CASES/iam-$kind.log" 2>&1; rc=$?; set -e
        if { [ "$want" = accept ] && [ "$rc" -eq 0 ]; } || { [ "$want" = refuse ] && [ "$rc" -ne 0 ] && grep -q '^REFUSED iam:' "$CASES/iam-$kind.log"; }; then
            echo "IAM config $want: $kind ($(head -n 1 "$CASES/iam-$kind.log" | cut -c 1-160))"
        else
            fail "IAM config $want $kind: exit $rc (log: $CASES/iam-$kind.log)"
        fi
    }
    iam_case accept as-generated
    if python3 -c 'import sys, tomllib; c = tomllib.load(open(sys.argv[1], "rb")); s = c["iam"]["statements"]; sys.exit(0 if s == [{"effect": "Allow", "actions": ["s3:GetObject"], "resources": ["arn:aws:s3:::dry-run-bucket/decide/aprender-mcp-decide/*"]}] and set(c["iam"]) == {"statements"} else 1)' "$CFG"; then
        echo "IAM config literal: exactly [Allow s3:GetObject arn:aws:s3:::dry-run-bucket/decide/aprender-mcp-decide/*]"
    else
        fail "IAM config literal: the generated [iam] is not exactly the scoped read"
    fi
    for k in s3-star list-too bucket-wide star-resource other-server two-statements bucket-sugar no-iam; do iam_case refuse "$k"; done
    cp "$CASES/iam-s3-star.toml" "$CFG"
    expect_refused 6 iam-broadened 'REFUSED iam:' deploy_tiny
    regen 7
    # GRANT: the read-only role check, over policy documents shaped like `aws iam get-role-policy`.
    GB="dry-run-bucket"
    SCOPED='{"Effect":"Allow","Action":"s3:GetObject","Resource":"arn:aws:s3:::dry-run-bucket/decide/aprender-mcp-decide/*"}'
    XRAY='{"Effect":"Allow","Action":["xray:PutTraceSegments","xray:PutTelemetryRecords"],"Resource":"*"}'
    grant_case() {
        local want="$1" name="$2" docs="$3" rc
        set +e; just _laya-grant-check "$docs" "$GB" aprender-mcp-decide aprender-decide-weights-dev > "$CASES/grant-$name.log" 2>&1; rc=$?; set -e
        if { [ "$want" = accept ] && [ "$rc" -eq 0 ]; } || { [ "$want" = refuse ] && [ "$rc" -ne 0 ] && grep -q '^REFUSED grant:' "$CASES/grant-$name.log"; }; then
            echo "GRANT $want: $name"
        else
            fail "GRANT $want $name: exit $rc (log: $CASES/grant-$name.log)"
        fi
    }
    grant_case accept stack-declared "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$XRAY,$SCOPED]}}]"
    grant_case refuse absent "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$XRAY]}}]"
    grant_case refuse no-policies '[]'
    grant_case refuse s3-star "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED,{\"Effect\":\"Allow\",\"Action\":\"s3:*\",\"Resource\":\"arn:aws:s3:::other/*\"}]}}]"
    grant_case refuse bucket-wide "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED,{\"Effect\":\"Allow\",\"Action\":\"s3:GetObject\",\"Resource\":\"arn:aws:s3:::dry-run-bucket/*\"}]}}]"
    grant_case refuse legacy-still-on "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED]}},{\"name\":\"aprender-decide-weights-dev\",\"document\":{\"Statement\":[$SCOPED]}}]"
    grant_case refuse not-json 'nope'
    # WR-07 (plan 08-22): attached managed policies, wildcard actions/resources and NotAction are broad.
    LOGS='{"Effect":"Allow","Action":["logs:CreateLogStream","logs:PutLogEvents"],"Resource":"*"}'
    grant_case accept attached-logs-only "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$XRAY,$SCOPED]}},{\"name\":\"arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole\",\"kind\":\"attached\",\"document\":{\"Statement\":[$LOGS]}}]"
    grant_case refuse attached-s3-any "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED]}},{\"name\":\"arn:aws:iam::aws:policy/AmazonS3ReadOnlyAccess\",\"kind\":\"attached\",\"document\":{\"Statement\":[{\"Effect\":\"Allow\",\"Action\":\"s3:GetObject\",\"Resource\":\"arn:aws:s3:::*\"}]}}]"
    grant_case refuse get-star "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED,{\"Effect\":\"Allow\",\"Action\":\"s3:Get*\",\"Resource\":\"arn:aws:s3:::dry-run-bucket/decide/aprender-mcp-decide/*\"}]}}]"
    grant_case refuse decide-prefix-star "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED,{\"Effect\":\"Allow\",\"Action\":\"s3:GetObject\",\"Resource\":\"arn:aws:s3:::dry-run-bucket/decide/*\"}]}}]"
    grant_case refuse not-action "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED,{\"Effect\":\"Allow\",\"NotAction\":\"iam:*\",\"Resource\":\"*\"}]}}]"
    # Wiring: laya-grant only READS IAM, and laya-deploy runs the config check and the role check.
    GRANT_BODY="$(just --show laya-grant)"
    if printf '%s' "$GRANT_BODY" | grep -q 'just _laya-grant-check' \
        && ! printf '%s' "$GRANT_BODY" | grep -Eq 'aws iam (put|delete|attach|detach|create)'; then
        echo "GRANT wiring: laya-grant reads the role's policies and writes nothing"
    else
        fail "GRANT wiring: laya-grant does not call _laya-grant-check, or still writes IAM"
    fi
    if printf '%s' "$DEPLOY_BODY" | grep -q 'just _laya-iam-check "\$CFG"' \
        && printf '%s' "$DEPLOY_BODY" | grep -q 'just laya-grant "\$SERVER" "\$ENV" "\$PROFILE" || contain'; then
        echo "IAM wiring: laya-deploy checks the config's [iam] and contains when the role lacks the stack grant"
    else
        fail "IAM wiring: laya-deploy does not run _laya-iam-check and the contained laya-grant check"
    fi
    if printf '%s' "$DEPLOY_BODY" | grep -qF "grep -q 'Server is in error state'" \
        && printf '%s' "$DEPLOY_BODY" | grep -qF '[ "$t" -lt "$TRIES" ]' \
        && printf '%s' "$DEPLOY_BODY" | grep -qF 'contain "the identity probe failed after'; then
        echo "EDGE SETTLE wiring: only the edge's 503 error-state refusal is retried (bounded); any other probe failure contains"
    else
        fail "EDGE SETTLE wiring: laya-deploy's identity probe retry is missing or unbounded"
    fi
    # Positive dry run (AL7): armed by default on the deployed artifact in the MAIN checkout, by an
    # explicit LAYA_ELIGIBLE_* quadruple otherwise; laya-verify itself must accept the artifact.
    POS_SKIPPED=""
    P_APR=""; P_RUN=""; P_DATA=""; P_BASE=""
    if [ -n "${LAYA_ELIGIBLE_APR:-}${LAYA_ELIGIBLE_RUN:-}${LAYA_ELIGIBLE_DATA:-}${LAYA_ELIGIBLE_BASE:-}" ]; then
        if [ -n "${LAYA_ELIGIBLE_APR:-}" ] && [ -n "${LAYA_ELIGIBLE_RUN:-}" ] && [ -n "${LAYA_ELIGIBLE_DATA:-}" ] && [ -n "${LAYA_ELIGIBLE_BASE:-}" ]; then
            P_APR="$LAYA_ELIGIBLE_APR"; P_RUN="$LAYA_ELIGIBLE_RUN"; P_DATA="$LAYA_ELIGIBLE_DATA"; P_BASE="$LAYA_ELIGIBLE_BASE"
        else
            fail "positive dry run: set ALL of LAYA_ELIGIBLE_APR/_RUN/_DATA/_BASE, or none"
        fi
    elif [ "${LAYA_DEPLOY_SELFTEST_POSITIVE:-1}" = "0" ]; then
        POS_SKIPPED="disarmed by LAYA_DEPLOY_SELFTEST_POSITIVE=0"
    else
        MAIN="$(cd "$(git rev-parse --git-common-dir)/.." && pwd -P)"
        P_APR="$MAIN/models/decide/laya-stance-64.apr"; P_RUN="$MAIN/models/decide/laya-stance-64"
        P_DATA="$MAIN/data/decide/tweet-stance-64"; P_BASE="{{laya_model_dir}}"
        for need in "$P_APR" "$P_RUN/gate-report.json" "$P_DATA/eval.jsonl" "$P_BASE/model.safetensors"; do
            if [ ! -e "$need" ]; then POS_SKIPPED="the deployed artifact's inputs are absent ($need)"; P_APR=""; break; fi
        done
    fi
    if [ -n "$P_APR" ]; then
        echo "POSITIVE dry run armed: $P_APR"
        just laya-deploy-config "$P_APR" off > "$CASES/positive-config.log" 2>&1 || fail "positive: laya-deploy-config"
        set +e
        just laya-deploy "$P_APR" "$P_RUN" "$P_DATA" "$P_BASE" > "$CASES/positive.log" 2>&1
        rc=$?
        set -e
        if [ "$rc" -eq 0 ] && grep -q '^DRY-RUN OK' "$CASES/positive.log"; then
            grep '^DRY-RUN OK' "$CASES/positive.log"
        else
            fail "positive dry run: exit $rc without DRY-RUN OK (log: $CASES/positive.log)"
        fi
    elif [ -n "$POS_SKIPPED" ]; then
        echo "SKIP positive dry run: $POS_SKIPPED"
    fi
    # Independent checks: the resolver proof on file, and the bootstrap build.
    RP="$(head -n 1 "$PROOF")"
    echo "RESOLVER PROOF: $RP ($(sed -n 's/^cargo_pmcp_version=/cargo-pmcp /p' "$PROOF"), sdk $(sed -n 's/^sdk_commit=//p' "$PROOF" | cut -c 1-12))"
    [ "$RP" = "crates/aprender-mcp-decide-lambda" ] || fail "resolver proof names $RP"
    if just laya-build-bootstrap > "$CASES/bootstrap.log" 2>&1 && grep -q '^BOOTSTRAP aarch64 OK' "$CASES/bootstrap.log"; then
        grep '^BOOTSTRAP aarch64 OK' "$CASES/bootstrap.log"
    else
        fail "laya-build-bootstrap (log: $CASES/bootstrap.log)"
    fi
    ROOT_TOML_AFTER="$(sha_or_absent crates/.pmcp/deploy.toml)"
    ROOT_AFTER="$(root_digest)"
    echo "CRATES ROOT: crates/.pmcp/deploy.toml sha256 before=$ROOT_TOML_BEFORE after=$ROOT_TOML_AFTER; state sha256 before=$ROOT_BEFORE after=$ROOT_AFTER"
    [ "$ROOT_TOML_BEFORE" = "$ROOT_TOML_AFTER" ] && [ "$ROOT_BEFORE" = "$ROOT_AFTER" ] || fail "the shared crates root changed"
    CALLS="$(wc -l < "$REC" | tr -d ' ')"
    MARKERS="$(cat "$CASES"/*.log | grep -c '^DEPLOYING' || true)"
    echo "AWS CALLS: $CALLS"
    echo "DEPLOY MARKERS: $MARKERS"
    if [ "$FAILS" -eq 0 ] && [ "$CALLS" -eq 0 ] && [ "$MARKERS" -eq 0 ]; then
        # Never the bare OK after a skipped positive run (AL7): the skip is part of the verdict.
        if [ -n "$POS_SKIPPED" ]; then
            echo "DEPLOY SELFTEST OK (positive dry run SKIPPED: $POS_SKIPPED)"
        else
            echo "DEPLOY SELFTEST OK"
        fi
    else
        echo "DEPLOY SELFTEST FAILED ($FAILS failed checks)" >&2
        exit 1
    fi

# Create (if absent) the decide weights bucket aprender-decide-weights-<account>-<env> in us-east-1
# and (re)assert its posture on every run: all public access blocked, SSE-S3 default encryption,
# tags project=aprender component=decide-weights env=<env>. No lifecycle expiry: objects are
# content-addressed (decide/<server>/<sha256>.apr) and immutable; a new model is a new key.
laya-weights-bucket env="dev" profile="ze-kasher-dev":
    #!/usr/bin/env bash
    set -euo pipefail
    ENV="{{ trim_start_match(env, "env=") }}"
    case "$ENV" in
        dev|prod) ;;
        *) echo "ERROR: '$ENV' is not a known environment (expected dev or prod)" >&2; exit 2 ;;
    esac
    P="{{profile}}"
    ACCOUNT="$(aws sts get-caller-identity --profile "$P" --query Account --output text)"
    BUCKET="aprender-decide-weights-${ACCOUNT}-${ENV}"
    if aws s3api head-bucket --profile "$P" --bucket "$BUCKET" 2>/dev/null; then
        echo "  exists:  $BUCKET"
    else
        # us-east-1 takes no LocationConstraint.
        aws s3api create-bucket --profile "$P" --region us-east-1 --bucket "$BUCKET" > /dev/null
        echo "  created: $BUCKET"
    fi
    aws s3api put-public-access-block --profile "$P" --bucket "$BUCKET" \
        --public-access-block-configuration BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true
    aws s3api put-bucket-encryption --profile "$P" --bucket "$BUCKET" \
        --server-side-encryption-configuration '{"Rules":[{"ApplyServerSideEncryptionByDefault":{"SSEAlgorithm":"AES256"}}]}'
    aws s3api put-bucket-tagging --profile "$P" --bucket "$BUCKET" \
        --tagging "TagSet=[{Key=project,Value=aprender},{Key=component,Value=decide-weights},{Key=env,Value=$ENV}]"
    echo "  private, SSE-S3, tagged: s3://$BUCKET (region us-east-1)"

# Upload <apr> content-addressed to s3://aprender-decide-weights-<account>-<env>/decide/<server>/<sha256>.apr.
# `just laya-verify` on the exact file runs FIRST and must accept it before any AWS call (sts
# included): nothing leaves this machine that the Rust verifier did not accept. The key's hash is
# verify's artifact_sha256; the upload is checked by head-object size and is idempotent.
laya-upload apr run data base server="aprender-mcp-decide" env="dev" profile="ze-kasher-dev":
    #!/usr/bin/env bash
    set -euo pipefail
    APR="{{apr}}"
    SERVER="{{server}}"
    ENV="{{ trim_start_match(env, "env=") }}"
    P="{{profile}}"
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    # shasum directly (plan 08-22, V11-c): the rtk hook never rewrites a command inside a recipe, so a
    # proxy branch added nothing and made the identity pin depend on which `rtk` is on PATH.
    sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
    case "$ENV" in
        dev|prod) ;;
        *) refuse env "'$ENV' is not a known environment (expected dev or prod)" ;;
    esac
    [ -f "$APR" ] || refuse artifact "$APR does not exist"
    H_LOCAL="$(sha256 "$APR")"
    # ELIGIBILITY FIRST -- the Rust verifier on the exact file (decide-apr-v1 deploy_eligibility).
    mkdir -p models/decide
    VLOG="models/decide/eligibility-upload-$SERVER.log"
    set +e
    just laya-verify "$APR" "{{run}}" "{{data}}" "{{base}}" > "$VLOG" 2>&1
    vrc=$?
    set -e
    if [ "$vrc" -ne 0 ]; then
        refuse eligibility "$(grep -m 1 '^REFUSED' "$VLOG" || echo "laya-verify exited $vrc (log: $VLOG)")" "$vrc"
    fi
    H="$(python3 - "$VLOG" <<'PY'
    import json, sys
    lines = [l for l in open(sys.argv[1]) if l.startswith("{")]
    v = json.loads(lines[-1])
    if v.get("deploy_eligible") is not True:
        sys.exit(1)
    print(v["artifact_sha256"])
    PY
    )" || refuse eligibility "laya-verify exited 0 without deploy_eligible true (log: $VLOG)"
    [ "$H" = "$H_LOCAL" ] || refuse eligibility "laya-verify accepted sha256 $H but $APR hashes to $H_LOCAL"
    KEY="decide/$SERVER/$H.apr"
    SIZE="$(wc -c < "$APR" | tr -d ' ')"
    if [ "${DRY_RUN:-0}" = "1" ]; then
        echo "DRY-RUN: would upload $APR ($SIZE bytes) to s3://aprender-decide-weights-<account>-$ENV/$KEY"
        exit 0
    fi
    ACCOUNT="$(aws sts get-caller-identity --profile "$P" --query Account --output text)"
    BUCKET="aprender-decide-weights-${ACCOUNT}-${ENV}"
    head_size() { aws s3api head-object --profile "$P" --bucket "$BUCKET" --key "$KEY" --query ContentLength --output text 2>/dev/null || true; }
    if [ "$(head_size)" = "$SIZE" ]; then
        echo "  already uploaded (content-addressed, $SIZE bytes): s3://$BUCKET/$KEY"
        exit 0
    fi
    aws s3 cp --profile "$P" --only-show-errors --sse AES256 "$APR" "s3://$BUCKET/$KEY"
    REMOTE="$(head_size)"
    [ "$REMOTE" = "$SIZE" ] || { echo "ERROR: s3://$BUCKET/$KEY is '$REMOTE' bytes after the upload, $APR is $SIZE" >&2; exit 1; }
    echo "UPLOADED s3://$BUCKET/$KEY ($SIZE bytes, sha256 $H)"

# READ-ONLY check (08-17 option 1; plan 08-22, WR-07 / V11-b) that the deployed function's role holds
# the STACK-DECLARED weights read -- s3:GetObject on decide/<server>/* of this env's weights bucket,
# which cargo-pmcp renders from the deploy config's [[iam.statements]] into the role's default
# inline policy -- and no other S3 grant. It reads every INLINE policy (list-role-policies,
# get-role-policy) and every ATTACHED managed policy (list-attached-role-policies, get-policy,
# get-policy-version) of the role and hands them to `_laya-grant-check`. It creates, attaches and
# deletes nothing, so there is nothing to re-run after a redeploy: the stack re-declares the grant.
# A failed IAM read exits 1 as an IAM READ FAILURE, never as a missing grant. The role is
# discovered from the function (pmcp.run owns it).
laya-grant server="aprender-mcp-decide" env="dev" profile="ze-kasher-dev":
    #!/usr/bin/env bash
    set -euo pipefail
    SERVER="{{server}}"
    ENV="{{ trim_start_match(env, "env=") }}"
    P="{{profile}}"
    case "$ENV" in
        dev|prod) ;;
        *) echo "ERROR: '$ENV' is not a known environment (expected dev or prod)" >&2; exit 2 ;;
    esac
    read_failed() { echo "ERROR: IAM read failed: $1 -- the role's policies are UNKNOWN, not missing; nothing was concluded" >&2; exit 1; }
    ROLE_ARN="$(aws lambda get-function --profile "$P" --function-name "$SERVER" \
        --query Configuration.Role --output text 2>/dev/null)" || {
        echo "ERROR: no Lambda named '$SERVER' (or it cannot be read) -- deploy it first: just laya-deploy ..." >&2
        exit 1
    }
    ROLE="${ROLE_ARN##*/}"
    ACCOUNT="$(aws sts get-caller-identity --profile "$P" --query Account --output text)" \
        || { echo "ERROR: cannot read the AWS account for profile $P" >&2; exit 1; }
    BUCKET="aprender-decide-weights-${ACCOUNT}-${ENV}"
    W="$(mktemp -d)"
    trap 'rm -rf "$W"' EXIT
    # Every read's status is checked on its own: a failed listing must not read as an empty one.
    NAMES="$(aws iam list-role-policies --profile "$P" --role-name "$ROLE" --query 'PolicyNames' --output json)" \
        || read_failed "listing the inline policies of $ROLE (aws iam list-role-policies)"
    ARNS="$(aws iam list-attached-role-policies --profile "$P" --role-name "$ROLE" --query 'AttachedPolicies[].PolicyArn' --output json)" \
        || read_failed "listing the attached managed policies of $ROLE (aws iam list-attached-role-policies)"
    lines() { python3 -c 'import json, sys; v = json.loads(sys.argv[1]); assert isinstance(v, list) and all(isinstance(x, str) for x in v); print("\n".join(v))' "$1"; }
    lines "$NAMES" > "$W/inline.txt" || read_failed "list-role-policies on $ROLE did not return a JSON list of names"
    lines "$ARNS" > "$W/attached.txt" || read_failed "list-attached-role-policies on $ROLE did not return a JSON list of ARNs"
    i=0
    while IFS= read -r NAME; do
        [ -n "$NAME" ] || continue
        i=$((i + 1))
        printf '%s' "$NAME" > "$W/inline-$i.name"
        aws iam get-role-policy --profile "$P" --role-name "$ROLE" --policy-name "$NAME" --query PolicyDocument --output json > "$W/inline-$i.json" \
            || read_failed "reading inline policy $NAME of $ROLE (aws iam get-role-policy)"
    done < "$W/inline.txt"
    j=0
    while IFS= read -r ARN; do
        [ -n "$ARN" ] || continue
        j=$((j + 1))
        printf '%s' "$ARN" > "$W/attached-$j.name"
        VER="$(aws iam get-policy --profile "$P" --policy-arn "$ARN" --query Policy.DefaultVersionId --output text)" \
            || read_failed "reading attached policy $ARN (aws iam get-policy)"
        aws iam get-policy-version --profile "$P" --policy-arn "$ARN" --version-id "$VER" --query PolicyVersion.Document --output json > "$W/attached-$j.json" \
            || read_failed "reading version $VER of attached policy $ARN (aws iam get-policy-version)"
    done < "$W/attached.txt"
    DOCS="$(python3 - "$W" "$i" "$j" <<'PY'
    import json, sys
    d, ni, nj = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    out = []
    for kind, n in (("inline", ni), ("attached", nj)):
        for k in range(1, n + 1):
            out.append({"name": open(f"{d}/{kind}-{k}.name").read(), "kind": kind,
                        "document": json.load(open(f"{d}/{kind}-{k}.json"))})
    print(json.dumps(out))
    PY
    )" || read_failed "a policy document of $ROLE is not JSON"
    if ! just _laya-grant-check "$DOCS" "$BUCKET" "$SERVER" "aprender-decide-weights-${ENV}" > "$W/check.log" 2>&1; then
        sed "s/$BUCKET/<weights-bucket>/g" "$W/check.log" >&2
        exit 1
    fi
    sed "s/$BUCKET/<weights-bucket>/g" "$W/check.log"
    echo "  role: $ROLE ($i inline, $j attached)"

# Decide whether a role's policies (<docs>: a JSON list of {name, kind: inline|attached, document};
# a missing kind is inline) carry the stack-declared weights read and NO OTHER S3 grant (plan 08-22,
# WR-07). Accepts only when some INLINE Allow statement has Action exactly s3:GetObject and Resource
# exactly arn:aws:s3:::<bucket>/decide/<server>/*, and nothing else in the list reaches S3. Refuses
# (exit 1, naming the policy and statement) on: any Allow with NotAction or NotResource; any S3 action
# carrying `*` or `?` (s3:Get*, s3:*, *); any S3 resource carrying `*` or `?` other than exactly that
# ARN; any other S3 action or resource; ANY attached managed policy granting an S3 action; and the
# legacy out-of-band policy <legacy> (08-17 option 1 retired it). It checks the documents it is given:
# permission boundaries, SCPs and bucket policies are not inspected, and the success line says so.
# Pure: no network. Exercised by laya-deploy-selftest's GRANT table and row grant-check.
[positional-arguments]
_laya-grant-check docs bucket server legacy:
    #!/usr/bin/env bash
    set -euo pipefail
    python3 - "$1" "$2" "$3" "$4" <<'PY'
    import fnmatch, json, sys
    docs, bucket, server, legacy = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
    def refuse(msg):
        print(f"REFUSED grant: {msg}", file=sys.stderr); sys.exit(1)
    try:
        pols = json.loads(docs)
    except ValueError:
        refuse("the policy list is not JSON")
    if not isinstance(pols, list):
        refuse("the policy list is not a JSON list")
    want = f"arn:aws:s3:::{bucket}/decide/{server}/*"
    listify = lambda v: v if isinstance(v, list) else [v]
    wild = lambda s: "*" in s or "?" in s
    def reaches_s3(action):
        svc = action.split(":", 1)[0].lower()
        return action == "*" or fnmatch.fnmatchcase("s3", svc)
    found, counts = None, {"inline": 0, "attached": 0}
    for p in pols:
        if not isinstance(p, dict):
            refuse(f"entry {p!r} is not a name/kind/document object")
        name, kind = p.get("name"), p.get("kind", "inline")
        if kind not in counts:
            refuse(f"policy {name}: unknown kind {kind!r}")
        counts[kind] += 1
        if kind == "inline" and name == legacy:
            refuse(f"the legacy out-of-band policy {legacy} is still on the role; the stack is the only grant")
        doc = p.get("document")
        if not isinstance(doc, dict):
            refuse(f"{kind} policy {name}: the document is not a JSON object")
        for n, s in enumerate(listify(doc.get("Statement", [])), 1):
            if not isinstance(s, dict):
                refuse(f"{kind} policy {name} statement {n}: not an object")
            if s.get("Effect") != "Allow":
                continue
            where = f"{kind} policy {name} statement {n}"
            if "NotAction" in s or "NotResource" in s:
                refuse(f"{where} is an Allow with NotAction/NotResource: it grants everything it does not name")
            acts = [str(a) for a in listify(s.get("Action", []))]
            res = [str(r) for r in listify(s.get("Resource", []))]
            s3acts = [a for a in acts if reaches_s3(a)]
            if not s3acts:
                continue
            if kind == "attached":
                refuse(f"{where} grants {s3acts} on {res}: an attached managed policy reaches S3, and the stack grant must be the only S3 read")
            broad_acts = [a for a in s3acts if wild(a)]
            if broad_acts:
                refuse(f"{where} grants {broad_acts}: a wildcard S3 action is broader than s3:GetObject")
            broad_res = [r for r in res if r != want and wild(r)]
            if broad_res:
                refuse(f"{where} grants {s3acts} on {broad_res}: a wildcard S3 resource broader than decide/{server}/*")
            if [a.lower() for a in acts] == ["s3:getobject"] and res == [want]:
                found = name
                continue
            refuse(f"{where} grants {s3acts} on {res}: an S3 grant other than the stack-declared s3:GetObject on {want}")
    if not found:
        refuse(f"no inline policy grants exactly s3:GetObject on {want}")
    print(f"grant ok: checked {counts['inline']} inline and {counts['attached']} attached policies of the role;"
          f" {found} (stack-declared) allows s3:GetObject on {want} and no other S3 grant is in them"
          f" (permission boundary, SCPs and bucket policy not inspected)")
    PY

# Measure decide-tool-boundary-v1 accepted_region_cold LIVE (the post-spike deploy; not this phase).
# Per cold sample: bump DECIDE_COLD_BUMP in the function config (a config change retires every
# warm environment) and wait for LastUpdateStatus=Successful; send the maximal request as the
# instance's FIRST POST (`probe --cold-first --maximal <shape> --probe-id <uuid>`); require identity
# and < 30000 ms; then find the server's `decide.load performed_load=true probe_id=<uuid>` line in
# CloudWatch -- no line, not cold (fail closed). >= <samples> per shape, alternating CONCENTRATED
# and DISTRIBUTED, EVERY one < 30000 ms; then an identity probe (labels in order) and 5 warm calls.
# Evidence: models/decide/deploy-evidence-<server>.json. The env bump is out-of-band drift the next
# `laya-deploy` resets. DRY_RUN=1 sizes both shapes from <apr> offline and prints the plan.
laya-deploy-verify apr server="aprender-mcp-decide" profile="ze-kasher-dev" samples="2":
    #!/usr/bin/env bash
    set -euo pipefail
    APR="{{apr}}"
    SERVER="{{server}}"
    P="{{profile}}"
    N="{{samples}}"
    CAP_MS=30000
    [ -f "$APR" ] || { echo "ERROR: $APR does not exist" >&2; exit 2; }
    case "$N" in
        ''|*[!0-9]*) echo "ERROR: samples must be a whole number >= 2 (got '$N')" >&2; exit 2 ;;
    esac
    [ "$N" -ge 2 ] || { echo "ERROR: samples must be >= 2 per shape (accepted_region_cold)" >&2; exit 2; }
    cargo build --release -q -p aprender-mcp-decide-lambda --example probe
    TD="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
    PROBE="$TD/release/examples/probe"
    # Size both shapes from the local artifact under ClassifyLimits::CONTRACTED (no network).
    for shape in concentrated distributed; do
        "$PROBE" --plan-only --apr "$APR" --maximal "$shape" > "models/decide/plan-$shape.json" \
            || { echo "ERROR: cannot build the $shape maximal request for $APR" >&2; exit 1; }
    done
    H="$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["artifact_sha256"])' models/decide/plan-concentrated.json)"
    SHAPES=()
    for _ in $(seq 1 "$N"); do SHAPES+=(concentrated distributed); done
    STATE="models/decide/deploy-$SERVER.state/deployment.toml"
    if [ "${DRY_RUN:-0}" = "1" ]; then
        ENDPOINT="<endpoint from $STATE>"
        echo "DRY-RUN laya-deploy-verify $SERVER: artifact sha256 $H, ${#SHAPES[@]} cold samples, cap $CAP_MS ms each"
        for shape in concentrated distributed; do
            python3 -c 'import json, sys; p = json.load(open(sys.argv[1])); print("  shape %s: %s texts, %s tokens planned, max text %s bytes" % (p["shape"], p["texts"], p["tokens_planned"], p["max_text_bytes"]))' "models/decide/plan-$shape.json"
        done
        i=0
        for shape in "${SHAPES[@]}"; do
            i=$((i + 1))
            echo "  sample $i $shape:"
            echo "    1. aws lambda update-function-configuration --function-name $SERVER (+ DECIDE_COLD_BUMP=<utc timestamp>, other variables kept); aws lambda wait function-updated; LastUpdateStatus must be Successful"
            echo "    2. probe --url $ENDPOINT --apr $APR --expect-sha256 $H --cold-first --maximal $shape --probe-id <fresh uuid>  (first POST, no initialize; identity and elapsed_ms < $CAP_MS required)"
            echo "    3. aws logs filter-log-events on the function's log group over the sample window: 'decide.load performed_load=true probe_id=<uuid>' required, else the sample is NOT cold (fail closed); x-decide-load header recorded"
        done
        echo "  then: probe --url $ENDPOINT --apr $APR --expect-sha256 $H (initialize, tools/list: labels in order, identity), and 5 warm single-text calls (p50/max)"
        echo "  evidence -> models/decide/deploy-evidence-$SERVER.json (per cold sample: shape, tokens_total, elapsed_ms, load_ms, probe_id, cold_evidence cloudwatch, graviton)"
        echo "DRY-RUN: stopped before the network (no AWS call, no HTTP request)"
        exit 0
    fi
    [ -f "$STATE" ] || { echo "ERROR: $STATE does not exist -- it is written by just laya-deploy" >&2; exit 2; }
    ENDPOINT="$(python3 -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["deployment"]["endpoint"])' "$STATE")"
    LOG_GROUP="$(aws lambda get-function-configuration --profile "$P" --function-name "$SERVER" --query LoggingConfig.LogGroup --output text)"
    case "$LOG_GROUP" in ''|None) LOG_GROUP="/aws/lambda/$SERVER" ;; esac
    EVID="$(mktemp -d)"
    trap 'rm -rf "$EVID"' EXIT
    FAILS=0
    i=0
    for shape in "${SHAPES[@]}"; do
        i=$((i + 1))
        # 1. a fresh environment: a configuration change retires every warm one.
        VARS="$(aws lambda get-function-configuration --profile "$P" --function-name "$SERVER" --query Environment.Variables --output json)"
        NEWENV="$(python3 -c 'import json, sys, time; v = json.loads(sys.argv[1]) or {}; v["DECIDE_COLD_BUMP"] = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime()) + "-" + sys.argv[2]; print(json.dumps({"Variables": v}))' "$VARS" "$i")"
        aws lambda update-function-configuration --profile "$P" --function-name "$SERVER" --environment "$NEWENV" > /dev/null
        aws lambda wait function-updated --profile "$P" --function-name "$SERVER"
        ST="$(aws lambda get-function-configuration --profile "$P" --function-name "$SERVER" --query LastUpdateStatus --output text)"
        [ "$ST" = "Successful" ] || { echo "ERROR: sample $i: LastUpdateStatus is $ST" >&2; exit 1; }
        # 2. the maximal request as the instance's FIRST POST.
        PID="$(python3 -c 'import uuid; print(uuid.uuid4())')"
        T0="$(python3 -c 'import time; print(int(time.time() * 1000) - 5000)')"
        set +e
        "$PROBE" --url "$ENDPOINT" --apr "$APR" --expect-sha256 "$H" --cold-first --maximal "$shape" --probe-id "$PID" \
            > "$EVID/sample-$i.json" 2> "$EVID/sample-$i.err"
        prc=$?
        set -e
        [ "$prc" -eq 0 ] || { echo "FAIL sample $i $shape: probe exited $prc ($(tail -1 "$EVID/sample-$i.err"))" >&2; FAILS=$((FAILS + 1)); continue; }
        # 3. the server's own load line for THIS probe id, or the sample is not cold.
        LINE=""
        for _ in $(seq 1 30); do
            LINE="$(aws logs filter-log-events --profile "$P" --log-group-name "$LOG_GROUP" --start-time "$T0" \
                --filter-pattern "\"probe_id=$PID\"" --query 'events[].message' --output text 2>/dev/null \
                | tr '\t' '\n' | grep "decide.load performed_load=true probe_id=$PID" | head -n 1 || true)"
            [ -n "$LINE" ] && break
            sleep 2
        done
        printf '%s\n' "$LINE" > "$EVID/sample-$i.log"
        python3 - "$EVID/sample-$i.json" "$EVID/sample-$i.log" "$shape" "$CAP_MS" "$i" <<'PY' || FAILS=$((FAILS + 1))
    import json, re, sys
    sample, logline, shape, cap, i = sys.argv[1], open(sys.argv[2]).read().strip(), sys.argv[3], int(sys.argv[4]), sys.argv[5]
    s = json.loads([l for l in open(sample) if l.startswith("{")][-1])
    problems = []
    if not s.get("identity_matches"):
        problems.append("identity mismatch")
    if s.get("elapsed_ms", cap) >= cap:
        problems.append(f"elapsed {s.get('elapsed_ms')} ms >= {cap}")
    if "performed_load=true" not in logline:
        problems.append("no performed_load=true line in CloudWatch for this probe id: NOT a cold sample")
    status = "ok" if not problems else "FAIL " + "; ".join(problems)
    print(f"sample {i} {shape}: elapsed {s.get('elapsed_ms')} ms, tokens {s.get('tokens_total')}, header {s.get('load_header')}, {status}")
    sys.exit(0 if not problems else 1)
    PY
    done
    # Labels in order + identity, then 5 warm single-text calls.
    "$PROBE" --url "$ENDPOINT" --apr "$APR" --expect-sha256 "$H" > "$EVID/identity.json" 2> "$EVID/identity.err" \
        || { echo "FAIL identity probe ($(tail -1 "$EVID/identity.err"))" >&2; FAILS=$((FAILS + 1)); }
    for w in 1 2 3 4 5; do
        "$PROBE" --url "$ENDPOINT" --apr "$APR" --expect-sha256 "$H" --cold-first > "$EVID/warm-$w.json" 2> "$EVID/warm-$w.err" \
            || { echo "FAIL warm call $w" >&2; FAILS=$((FAILS + 1)); }
    done
    python3 - "$EVID" "$SERVER" "$H" "${#SHAPES[@]}" "models/decide/deploy-evidence-$SERVER.json" <<'PY'
    import glob, json, os, re, statistics, sys
    d, server, sha, n, out = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4]), sys.argv[5]
    def last_json(p):
        try:
            return json.loads([l for l in open(p) if l.startswith("{")][-1])
        except (OSError, IndexError, ValueError):
            return None
    samples = []
    for i in range(1, n + 1):
        s = last_json(f"{d}/sample-{i}.json")
        line = open(f"{d}/sample-{i}.log").read().strip() if os.path.exists(f"{d}/sample-{i}.log") else ""
        kv = dict(re.findall(r"(\w+)=(\S+)", line))
        samples.append({
            "shape": s and s.get("shape"), "tokens_total": s and s.get("tokens_total"),
            "elapsed_ms": s and s.get("elapsed_ms"), "load_ms": kv.get("load_ms"),
            "probe_id": s and s.get("probe_id"), "load_header": s and s.get("load_header"),
            "cold_evidence": "cloudwatch" if kv.get("performed_load") == "true" else None,
            "graviton": kv.get("graviton"), "log_line": line or None,
        })
    warm = [w["elapsed_ms"][-1] if isinstance(w.get("elapsed_ms"), list) else w.get("elapsed_ms")
            for w in (last_json(p) for p in sorted(glob.glob(f"{d}/warm-*.json"))) if w]
    ident = last_json(f"{d}/identity.json")
    ev = {"server": server, "artifact_sha256": sha, "cap_ms": 30000, "cold_samples": samples,
          "identity": ident, "warm_ms": warm,
          "warm_p50_ms": statistics.median(warm) if warm else None, "warm_max_ms": max(warm) if warm else None}
    json.dump(ev, open(out, "w"), indent=2)
    print(f"  evidence -> {out}")
    PY
    if [ "$FAILS" -eq 0 ]; then
        echo "DEPLOY VERIFY OK $SERVER: ${#SHAPES[@]} cold samples (both shapes), every one < $CAP_MS ms with CloudWatch cold evidence"
    else
        echo "DEPLOY VERIFY FAILED $SERVER: $FAILS failed checks" >&2
        exit 1
    fi

# CONTAINMENT FIRST: throttle <server> to reserved concurrency 0 (every invocation is refused at
# once, warm instances included; undo with `aws lambda delete-function-concurrency`), verify it,
# then delete the inline weights policy. The bucket stays (idle-free). Removing the deployment and
# the object is the human's call: the commands are PRINTED, never run.
laya-teardown server="aprender-mcp-decide" env="dev" profile="ze-kasher-dev":
    #!/usr/bin/env bash
    set -euo pipefail
    SERVER="{{server}}"
    ENV="{{ trim_start_match(env, "env=") }}"
    P="{{profile}}"
    case "$ENV" in
        dev|prod) ;;
        *) echo "ERROR: '$ENV' is not a known environment (expected dev or prod)" >&2; exit 2 ;;
    esac
    aws lambda put-function-concurrency --profile "$P" --function-name "$SERVER" --reserved-concurrent-executions 0 > /dev/null
    GOT="$(aws lambda get-function-concurrency --profile "$P" --function-name "$SERVER" --query ReservedConcurrentExecutions --output text)"
    [ "$GOT" = "0" ] || { echo "ERROR: reserved concurrency of $SERVER reads '$GOT', not 0 -- it may still be serving" >&2; exit 1; }
    echo "  CONTAINED: $SERVER reserved concurrency 0 (no invocation runs, warm instances included)"
    ROLE_ARN="$(aws lambda get-function --profile "$P" --function-name "$SERVER" --query Configuration.Role --output text)"
    ROLE="${ROLE_ARN##*/}"
    # Only the LEGACY out-of-band policy (pre-08-17-option-1 laya-grant wrote it) is deleted, by its
    # exact name. The stack-declared weights read (cargo-pmcp's default policy, e.g. `pmcp-declared`)
    # is stack-managed and is NEVER touched here: deleting it would be drift the next deploy
    # silently re-creates. It stays attached while contained, and grants nothing usable: with
    # reserved concurrency 0 no invocation runs. `cargo pmcp deploy destroy` (below) removes it.
    # Only NoSuchEntity means "absent" (plan 08-22, V11-b); any other failure is reported, not hidden.
    set +e
    DERR="$(aws iam delete-role-policy --profile "$P" --role-name "$ROLE" --policy-name "aprender-decide-weights-${ENV}" 2>&1 > /dev/null)"
    drc=$?
    set -e
    if [ "$drc" -eq 0 ]; then
        echo "  removed LEGACY out-of-band policy aprender-decide-weights-${ENV} from $ROLE"
    else
        case "$DERR" in
            *NoSuchEntity*) echo "  no legacy policy aprender-decide-weights-${ENV} on $ROLE (expected since 08-17 option 1)" ;;
            *)
                echo "ERROR: deleting the legacy policy aprender-decide-weights-${ENV} from $ROLE failed (exit $drc): $DERR" >&2
                echo "       containment IS in place ($SERVER at reserved concurrency 0); the legacy policy's state is unknown" >&2
                exit 1
                ;;
        esac
    fi
    echo "  stack-declared weights read left in place (stack-managed; inert at reserved concurrency 0)"
    ACCOUNT="$(aws sts get-caller-identity --profile "$P" --query Account --output text)"
    echo "  NOT RUN -- the human's call:"
    echo "    remove the deployment (the decide config must be at the shared root while it runs):"
    echo "      just _laya-crates-root-swap crates crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml models/decide/destroy-$SERVER.state cargo pmcp deploy destroy --manifest-path crates"
    echo "    remove the weights:"
    echo "      aws s3 rm --profile $P --recursive s3://aprender-decide-weights-${ACCOUNT}-${ENV}/decide/$SERVER/"
    echo "    or resume serving instead: aws lambda delete-function-concurrency --profile $P --function-name $SERVER"

# ---------------------------------------------------------------------------
# Class C, gate honesty (plan 08-22): every Phase 8 gate is a row of scripts/laya_gates.tsv, and every
# row can be shown to fail. CLAUDE.md Verification Discipline 4/5/7: a gate that cannot fail is theater,
# and guard regexes ship a case table.
# ---------------------------------------------------------------------------

# Sweep scripts/laya_gates.tsv (or $LAYA_GATES_TSV): for each row run its MUST-FAIL case (non-zero
# with the expected message) and its MUST-PASS case (exit 0 with the expected evidence). FIRST, before
# any case: the verdict classifier's case table, then the DRIFT CHECK over EVERY recipe, private ones
# included (`just --dump --dump-format json`; `just --summary` hides `_laya-*`, so the enumeration must
# contain `_laya-iam-check` or it fails). A laya/_laya recipe is a GATE when its body has `exit 1|2|3`
# (or `exit(N)`), FAIL, `exec` (its status is another program's) or a verdict token; each gate must be
# a row target or a `not-a-gate` row, and a `not-a-gate` row is refused (`DRIFT: <recipe> prints a
# verdict and cannot be not-a-gate`) when the recipe prints a verdict or execs. An unknown gate_id
# fails before any case runs. `external:<08-NN>` rows (only parser-guard, cascade-guard) name the plan
# that owns the offender and run their case when one is given. DRY_RUN=1 is exported and `aws` is a
# recorder that must stay unused: AWS CALLS: 0 (AWS-touching cases run against a PATH-shimmed fake aws).
# The default sweep runs ONE real-weights leg (the default-armed deploy selftest: laya-verify of the
# deployed artifact), so run it under the host's real-weights lock; LAYA_GATES_FULL=1 adds the FULL
# tier (the armed laya-verify-suite, a real repack, a DRY_RUN upload, the real resolver proof), and
# without it the OK line names what it did not run. LAYA_GATES_ONLY=<id,id> runs only those rows
# (the drift check still runs in full) and prints `LAYA GATES ROWS OK`, never the sweep's OK line.
laya-gates-selftest:
    #!/usr/bin/env bash
    set -euo pipefail
    TSV="${LAYA_GATES_TSV:-scripts/laya_gates.tsv}"
    ONLY="${LAYA_GATES_ONLY:-}"
    FULL="${LAYA_GATES_FULL:-0}"
    export DRY_RUN=1
    [ -f "$TSV" ] || { echo "LAYA GATES SELFTEST FAILED: the gate table $TSV does not exist" >&2; exit 1; }
    W="$(mktemp -d)"
    LOGD="$W/logs"
    mkdir -p "$LOGD"
    CFG="crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml"
    PROOF="models/decide/resolver-proof.txt"
    BK="$W/backup"
    mkdir -p "$BK"
    # Files a case may rewrite (all gitignored) come back byte-identical on every exit path.
    for f in "$CFG" "$PROOF" models/decide/plan-concentrated.json models/decide/plan-distributed.json; do
        if [ -f "$f" ]; then mkdir -p "$BK/$(dirname "$f")"; cp -p "$f" "$BK/$f"; fi
    done
    cleanup() {
        local f
        for f in "$CFG" "$PROOF" models/decide/plan-concentrated.json models/decide/plan-distributed.json; do
            if [ -f "$BK/$f" ]; then cp -p "$BK/$f" "$f"; elif [ "$f" = "$CFG" ]; then rm -f "$f"; fi
        done
        rm -rf "$W"
    }
    trap cleanup EXIT
    # 1. The verdict classifier's case table, then the drift check. Both before any case.
    just --dump --dump-format json > "$W/dump.json"
    python3 -c 'import json, sys; print("\n".join(json.load(open(sys.argv[1]))["recipes"]))' "$W/dump.json" > "$W/recipes.txt"
    python3 - "$W/dump.json" "$W/recipes.txt" "$TSV" "$W/rows.tsv" <<'PY' || { echo "LAYA GATES SELFTEST FAILED: the verdict case table or the drift check (above) failed; no case ran" >&2; exit 1; }
    import json, re, sys
    dump, names_file, tsv, rows_out = sys.argv[1:5]
    VERDICT = re.compile(r"(^|[^A-Za-z_])(OK|PASS|PASSED|FAIL|FAILED|REFUSED|ELIGIBLE)([^A-Za-z_]|$)")
    def verdict_line(line):
        if re.match(r"\s*#", line) or "usage" in line or "Usage" in line:
            return False
        return bool(VERDICT.search(line))
    bad = 0
    for want, line in [
        (True, 'echo "DEPLOY VERIFY OK"'), (True, 'echo "FAIL: pin mismatch"'), (True, 'echo "REFUSED deploy"'),
        (True, "printf 'LEG OK: %s\\n'"), (True, "echo PASS"),
        (False, 'echo "usage: just laya-inspect <apr>"'), (False, "OKAY=1"), (False, "# prints OK"),
        (False, "NOFAIL=1"), (False, 'echo "passing"'),
    ]:
        if verdict_line(line) != want:
            print(f"VERDICT CASE FAIL: {line!r} classified {not want}, expected {want}")
            bad += 1
    if bad:
        sys.exit(1)
    print("VERDICT CASE TABLE: 10 cases as expected (5 must-match, 5 must-not-match)")
    def render(line):
        return "".join(f if isinstance(f, str) else "<interpolation>" for f in line)
    recipes = json.load(open(dump))["recipes"]
    names = [n.strip() for n in open(names_file) if n.strip()]
    problems = []
    if "_laya-iam-check" not in names:
        problems.append("DRIFT: the recipe enumeration does not contain the private recipe _laya-iam-check (a must-match control): it is not the full recipe list")
    GATE = re.compile(r"\bexit [123]\b|\bexit\(\s*[123]\s*\)|FAIL")
    gates, capable = set(), {}
    for n in names:
        if not (n.startswith("laya") or n.startswith("_laya")) or n not in recipes:
            continue
        body = [render(l) for l in recipes[n]["body"]]
        ex = any(re.match(r"\s*exec\s", l) for l in body)
        vl = [l.strip() for l in body if verdict_line(l)]
        capable[n] = (ex, vl)
        if ex or vl or any(GATE.search(l) for l in body if not re.match(r"\s*#", l)):
            gates.add(n)
    lines = open(tsv).read().splitlines()
    HEADER = "gate_id\ttarget\tmust_fail\tmust_pass\tfinding"
    if not lines or lines[0] != HEADER:
        problems.append(f"TABLE: the first line of {tsv} is not the header {HEADER!r}")
    covered, exempt, ids, out = set(), {}, set(), []
    for i, raw in enumerate(lines[1:], 2):
        f = raw.split("\t")
        if len(f) != 5 or any(not x.strip() for x in f):
            problems.append(f"TABLE: line {i} does not have five non-empty tab-separated fields: {raw!r}")
            continue
        gid, target, mf, mp, finding = f
        if gid == "not-a-gate":
            if target not in recipes:
                problems.append(f"TABLE: line {i} exempts {target}, which is not a recipe")
            elif mf != "-" or mp != "-" or finding == "-":
                problems.append(f"TABLE: line {i}: a not-a-gate row is not-a-gate<TAB><recipe><TAB>-<TAB>-<TAB><reason>")
            exempt[target] = finding
            continue
        if not re.fullmatch(r"[a-z0-9]+(-[a-z0-9]+)*", gid) or gid in ids:
            problems.append(f"TABLE: line {i}: gate_id {gid!r} is malformed or repeated")
        ids.add(gid)
        if target.startswith("external:"):
            if not re.fullmatch(r"external:08-[0-9][0-9]", target):
                problems.append(f"TABLE: line {i}: {gid} names no owner plan 08-NN ({target!r})")
            if gid not in ("parser-guard", "cascade-guard"):
                problems.append(f"TABLE: line {i}: {gid} may not be external (only parser-guard and cascade-guard are)")
            if mp != "-":
                problems.append(f"TABLE: line {i}: an external row is <id><TAB>external:<plan><TAB><case or -><TAB>-<TAB><finding>")
        else:
            m = re.fullmatch(r"just (\S+)", target)
            if m:
                if m.group(1) not in recipes:
                    problems.append(f"DRIFT: row {gid} targets the recipe {m.group(1)}, which does not exist")
                covered.add(m.group(1))
            elif not re.fullmatch(r"(make|bash) \S+", target):
                problems.append(f"TABLE: line {i}: target {target!r} is not `just <recipe>`, `make <target>`, `bash <script>` or external:<plan>")
        out.append("\t".join((gid, target, mf)))
    for r in sorted(exempt):
        ex, vl = capable.get(r, (False, []))
        if ex or vl:
            why = "it execs another program, whose verdict this check cannot see" if ex else f"e.g. {vl[0][:90]!r}"
            problems.append(f"DRIFT: {r} prints a verdict and cannot be not-a-gate ({why}); it needs a real row")
        elif r in covered:
            problems.append(f"DRIFT: {r} is both a row target and not-a-gate")
    for g in sorted(gates - covered - set(exempt)):
        problems.append(f"DRIFT: {g} can exit non-zero on a check (or renders a verdict) but is neither a row target nor a not-a-gate row of {tsv}")
    for p in problems:
        print(p)
    if problems:
        print(f"DRIFT CHECK FAILED: {len(problems)} problem(s)")
        sys.exit(1)
    open(rows_out, "w").write("\n".join(out) + "\n")
    print(f"DRIFT CHECK OK: {len([n for n in names if n.startswith(('laya', '_laya'))])} laya recipes enumerated (private included), "
          f"{len(gates)} gates, all covered: {len(covered & gates)} by row targets, {len(exempt)} not-a-gate; {len(out)} rows")
    for r in sorted(exempt):
        print(f"  not-a-gate {r}: {exempt[r]}")
    PY
    # 2. The rows. One function per gate_id; an unknown id fails BEFORE any case runs.
    T="crates/aprender-decide/tests/fixtures/laya_tiny"
    GOLDEN="37d65159b2be0fa091aa840cd56c1a84b73c0bcd9e2df5906d1f8218f5448561"
    MAIN="$(cd "$(git rev-parse --git-common-dir)/.." && pwd -P)"
    DEP_APR="$MAIN/models/decide/laya-stance-64.apr"
    DEP_RUN="$MAIN/models/decide/laya-stance-64"
    DEP_DATA="$MAIN/data/decide/tweet-stance-64"
    BASE="{{laya_model_dir}}"
    # The aws recorder, first on PATH for the whole sweep: it must stay unused.
    REC="$W/aws-calls.log"
    : > "$REC"
    mkdir -p "$W/recorder"
    printf '#!/usr/bin/env bash\nprintf "%%s\\n" "$*" >> "%s"\necho "aws recorder: laya-gates-selftest never reaches AWS" >&2\nexit 97\n' "$REC" > "$W/recorder/aws"
    chmod +x "$W/recorder/aws"
    export PATH="$W/recorder:$PATH"
    set +e; aws recorder-control > /dev/null 2>&1; crc=$?; set -e
    [ "$crc" -eq 97 ] && [ "$(wc -l < "$REC" | tr -d ' ')" -eq 1 ] || { echo "LAYA GATES SELFTEST FAILED: the aws recorder did not record its control call" >&2; exit 1; }
    : > "$REC"
    # The fake aws for the IAM / teardown rows (FAKE_AWS_MODE selects the failure). Never reaches AWS.
    mkdir -p "$W/fakeaws"
    cat > "$W/fakeaws/aws" <<'SH'
    #!/usr/bin/env bash
    printf '%s\n' "$*" >> "${FAKE_AWS_LOG:-/dev/null}"
    mode="${FAKE_AWS_MODE:-ok}"
    B="aprender-decide-weights-123456789012-dev"
    case "$1 $2" in
        "lambda get-function") echo "arn:aws:iam::123456789012:role/decide-fn-role" ;;
        "sts get-caller-identity") echo "123456789012" ;;
        "iam list-role-policies")
            [ "$mode" = list-fail ] && { echo "An error occurred (AccessDenied) when calling the ListRolePolicies operation" >&2; exit 255; }
            echo '["pmcp-declared"]' ;;
        "iam get-role-policy") echo "{\"Statement\":[{\"Effect\":\"Allow\",\"Action\":\"s3:GetObject\",\"Resource\":\"arn:aws:s3:::$B/decide/aprender-mcp-decide/*\"}]}" ;;
        "iam list-attached-role-policies")
            [ "$mode" = attached-fail ] && { echo "An error occurred (Throttling) when calling the ListAttachedRolePolicies operation" >&2; exit 255; }
            echo '["arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole"]' ;;
        "iam get-policy") echo v1 ;;
        "iam get-policy-version") echo '{"Statement":[{"Effect":"Allow","Action":["logs:CreateLogStream","logs:PutLogEvents"],"Resource":"*"}]}' ;;
        "lambda put-function-concurrency") echo '{}' ;;
        "lambda get-function-concurrency") echo 0 ;;
        "iam delete-role-policy")
            case "$mode" in
                teardown-nosuch) echo "An error occurred (NoSuchEntity) when calling the DeleteRolePolicy operation: The role policy with name aprender-decide-weights-dev cannot be found." >&2; exit 254 ;;
                teardown-denied) echo "An error occurred (AccessDenied) when calling the DeleteRolePolicy operation: not authorized" >&2; exit 254 ;;
                *) exit 0 ;;
            esac ;;
        *) echo "fake aws: unexpected call: $*" >&2; exit 97 ;;
    esac
    SH
    chmod +x "$W/fakeaws/aws"
    FAKE="$W/fakeaws"
    export FAKE_AWS_LOG="$W/fake-aws-calls.log"
    CUR=""; ROW_RED=0; RC=0; FULL_SKIPPED=""
    run() { local name="$1"; shift; set +e; "$@" < /dev/null > "$LOGD/$CUR-$name.log" 2>&1; RC=$?; set -e; }
    red() { echo "  RED  $CUR: $1" >&2; ROW_RED=1; }
    logof() { printf '%s' "$LOGD/$CUR-$1.log"; }
    # must_fail <label> <ERE> <cmd...>: non-zero AND the expected message, or the row is RED.
    must_fail() {
        local label="$1" pat="$2"; shift 2
        run "$label" "$@"
        if [ "$RC" -ne 0 ] && grep -Eq -- "$pat" "$(logof "$label")"; then
            echo "  must-fail $label: exit $RC, $(grep -Eo -m 1 -- "$pat.*" "$(logof "$label")" | cut -c 1-150)"
        else
            red "must-fail $label: exit $RC, expected non-zero with a line matching '$pat' (log $(logof "$label"))"
            tail -4 "$(logof "$label")" >&2 || true
        fi
    }
    # must_pass <label> <ERE> <cmd...>: exit 0 AND the expected evidence line, or the row is RED.
    must_pass() {
        local label="$1" pat="$2"; shift 2
        run "$label" "$@"
        if [ "$RC" -eq 0 ] && grep -Eq -- "$pat" "$(logof "$label")"; then
            echo "  must-pass $label: exit 0, $(grep -Eo -m 1 -- "$pat.*" "$(logof "$label")" | cut -c 1-150)"
        else
            red "must-pass $label: exit $RC, expected 0 with a line matching '$pat' (log $(logof "$label"))"
            tail -4 "$(logof "$label")" >&2 || true
        fi
    }
    full_only() { FULL_SKIPPED="${FULL_SKIPPED:+$FULL_SKIPPED, }$CUR ($1)"; echo "  FULL tier not run: $1 (LAYA_GATES_FULL=1)"; }
    need_deployed() {
        local p
        for p in "$DEP_APR" "$DEP_RUN/gate-report.json" "$DEP_DATA/eval.jsonl" "$BASE/model.safetensors"; do
            [ -e "$p" ] || { red "the deployed artifact's inputs are absent ($p): this case cannot measure anything"; return 1; }
        done
    }
    # Shared, memoized: the synthetic fixture artifact, and ONE default-armed deploy selftest.
    tiny() {
        [ -f "$W/tiny.apr" ] && return 0
        just laya-pack-fixture "$T" "$T/data" "$W/tiny.apr" > "$W/tiny.log" 2>&1 && grep -q "sha256=$GOLDEN" "$W/tiny.log" \
            || { echo "LAYA GATES SELFTEST FAILED: laya-pack-fixture did not reproduce the golden $GOLDEN" >&2; tail -4 "$W/tiny.log" >&2; exit 1; }
    }
    ARMED_RC=""
    armed_deploy_selftest() {
        [ -n "$ARMED_RC" ] && return 0
        echo "  (running the default-armed laya-deploy-selftest once: laya-verify of the deployed artifact, multi-GB)"
        set +e
        env -u LAYA_ELIGIBLE_APR -u LAYA_ELIGIBLE_RUN -u LAYA_ELIGIBLE_DATA -u LAYA_ELIGIBLE_BASE -u LAYA_DEPLOY_SELFTEST_POSITIVE \
            just laya-deploy-selftest > "$W/armed-selftest.log" 2>&1
        ARMED_RC=$?
        set -e
        cp models/decide/selftest/cases/positive.log "$W/armed-positive.log" 2>/dev/null || : > "$W/armed-positive.log"
        cp models/decide/eligibility-aprender-mcp-decide.log "$W/armed-eligibility.log" 2>/dev/null || : > "$W/armed-eligibility.log"
    }
    set_pin() { python3 -c 'import re, sys; p, key, value = sys.argv[1:4]; t = open(p).read(); t, n = re.subn(r"^(" + key + r" = )\"[^\"]*\"$", lambda m: m.group(1) + chr(34) + value + chr(34), t, count=1, flags=re.M); assert n == 1, key; open(p, "w").write(t)' "$CFG" "$1" "$2"; }
    pinned() { python3 -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["environment"]["APRENDER_DECIDE_SHA256"])' "$CFG"; }
    # PATH with every directory holding an `rtk` replaced by a symlink mirror without it.
    nortk_path() {
        local out="" d m f
        local -a dirs
        IFS=: read -r -a dirs <<< "$PATH"
        for d in "${dirs[@]}"; do
            if [ -n "$d" ] && [ -x "$d/rtk" ]; then
                m="$W/nortk/$(printf '%s' "$d" | tr '/' '_')"
                mkdir -p "$m"
                for f in "$d"/*; do [ "$(basename "$f")" = rtk ] || ln -sf "$f" "$m/"; done
                d="$m"
            fi
            out="${out:+$out:}$d"
        done
        printf '%s' "$out"
    }
    # ---- one function per gate_id --------------------------------------------------------------
    row_leg_verdict() {
        local d="$W/leg"; mkdir -p "$d"
        printf 'MEASURED ids 14/14\nMEASURED probs max_abs 3.8e-6\nSKIP ladder rung: LAYA_LADDER_BIN not set\n' > "$d/skip-ladder.log"
        printf 'MEASURED ids 14/14 argmax 14/14\ntest full_model_reproduces_spike_025_fixture ... SKIP: LAYA_MODEL_DIR not set\nMEASURED probs max_abs 3.8e-6\nMEASURED ladder 32 blocks within bars\n' > "$d/libtest-skip.log"
        printf 'MEASURED ids 14/14 argmax 14/14\nMEASURED probs max_abs 3.8e-6\n' > "$d/no-ladder.log"
        printf 'test full_model_reproduces_spike_025_fixture ... load: 3.1 s through the F16 .apr path\nMEASURED ids 14/14 argmax 14/14 truncated 1\nMEASURED probs max_abs 3.841e-6 bar 1e-5 logits max_abs 2.146e-5 bar 1e-4 ARCH aarch64\nMEASURED ladder 32 blocks within bars\nok\n' > "$d/laya_parity.log"
        printf 'VECTOR a verify: REFUSED GateFailed\nFAIL-CLOSED VECTORS REFUSED 2/2 (412 s, ARCH aarch64)\n' > "$d/fail_closed_vectors.log"
        printf 'DEMO gate_pass sha256=24a44d7e\nDEMO OUTCOME gate_pass decided on the exact bytes (203 s, ARCH aarch64)\n' > "$d/demo_run.log"
        printf 'NOISE which=fine_tuned rust=8.3e-6 python=8.3e-6 bound=1e-5\nMEDIAN rust=17 python=17\n' > "$d/python_records.log"
        must_fail skip-ladder 'FAIL: laya_parity is armed but printed SKIP' just _laya-leg-verdict laya_parity "$d/skip-ladder.log"
        must_fail libtest-skip 'FAIL: laya_parity is armed but printed SKIP' just _laya-leg-verdict laya_parity "$d/libtest-skip.log"
        must_fail no-ladder "FAIL: laya_parity printed no positive evidence matching 'MEASURED ladder" just _laya-leg-verdict laya_parity "$d/no-ladder.log"
        must_fail unknown-leg "FAIL: unknown leg 'no_such_leg'" just _laya-leg-verdict no_such_leg "$d/laya_parity.log"
        local leg
        for leg in laya_parity fail_closed_vectors demo_run python_records; do
            must_pass "$leg" "LEG OK: $leg\$" just _laya-leg-verdict "$leg" "$d/$leg.log"
        done
    }
    SCOPED='{"Effect":"Allow","Action":"s3:GetObject","Resource":"arn:aws:s3:::dry-run-bucket/decide/aprender-mcp-decide/*"}'
    LOGS='{"Effect":"Allow","Action":["logs:CreateLogStream","logs:PutLogEvents"],"Resource":"*"}'
    grant() { just _laya-grant-check "$1" dry-run-bucket aprender-mcp-decide aprender-decide-weights-dev; }
    row_grant_check() {
        local inl="{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED]}}"
        local att='"kind":"attached","name":"arn:aws:iam::aws:policy/X","document":{"Statement":'
        must_fail attached-s3-any 'REFUSED grant: attached policy .* an attached managed policy reaches S3' \
            grant "[$inl,{$att[{\"Effect\":\"Allow\",\"Action\":\"s3:GetObject\",\"Resource\":\"arn:aws:s3:::*\"}]}}]"
        must_fail get-star 'REFUSED grant: .*a wildcard S3 action' \
            grant "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED,{\"Effect\":\"Allow\",\"Action\":\"s3:Get*\",\"Resource\":\"arn:aws:s3:::dry-run-bucket/decide/aprender-mcp-decide/*\"}]}}]"
        must_fail decide-prefix-star 'REFUSED grant: .*a wildcard S3 resource' \
            grant "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED,{\"Effect\":\"Allow\",\"Action\":\"s3:GetObject\",\"Resource\":\"arn:aws:s3:::dry-run-bucket/decide/*\"}]}}]"
        must_fail not-action 'REFUSED grant: .*NotAction/NotResource' \
            grant "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED,{\"Effect\":\"Allow\",\"NotAction\":\"iam:*\",\"Resource\":\"*\"}]}}]"
        must_pass stack-declared 'grant ok: checked 1 inline and 1 attached policies' grant "[$inl,{$att[$LOGS]}}]"
    }
    row_grant_listing_failure() {
        must_fail list-fail 'ERROR: IAM read failed: listing the inline policies' env PATH="$FAKE:$PATH" FAKE_AWS_MODE=list-fail just laya-grant
        if grep -q 'REFUSED grant' "$(logof list-fail)"; then red "a failed listing was reported as a grant verdict"; fi
        must_fail attached-fail 'ERROR: IAM read failed: listing the attached managed policies' env PATH="$FAKE:$PATH" FAKE_AWS_MODE=attached-fail just laya-grant
        must_pass ok 'grant ok: checked 1 inline and 1 attached policies' env PATH="$FAKE:$PATH" FAKE_AWS_MODE=ok just laya-grant
    }
    row_teardown_classify() {
        must_fail access-denied 'ERROR: deleting the legacy policy aprender-decide-weights-dev from decide-fn-role failed' env PATH="$FAKE:$PATH" FAKE_AWS_MODE=teardown-denied just laya-teardown
        must_pass no-such-entity 'no legacy policy aprender-decide-weights-dev on decide-fn-role' env PATH="$FAKE:$PATH" FAKE_AWS_MODE=teardown-nosuch just laya-teardown
    }
    row_sha256_helper() {
        tiny
        local H F NP
        H="$(shasum -a 256 "$W/tiny.apr" | awk '{print $1}')"
        F="$W/foreign-rtk"; mkdir -p "$F"
        printf '#!/bin/sh\necho "rtk 0.1.0 (Rust Type Kit): unknown subcommand $1"\nexit 0\n' > "$F/rtk"; chmod +x "$F/rtk"
        must_pass foreign-rtk "sha256   $H" env PATH="$F:$PATH" just laya-deploy-config "$W/tiny.apr" off
        [ "$RC" -eq 0 ] && [ "$(pinned)" = "$H" ] || red "with a foreign rtk first on PATH the pin is '$(pinned 2>/dev/null || echo none)', not shasum's $H"
        NP="$(nortk_path)"
        if env PATH="$NP" sh -c 'command -v rtk' > /dev/null 2>&1; then red "could not build a PATH without rtk"; fi
        must_pass rtk-absent "sha256   $H" env PATH="$NP" just laya-deploy-config "$W/tiny.apr" off
        [ "$RC" -eq 0 ] && [ "$(pinned)" = "$H" ] || red "with rtk absent the pin is '$(pinned 2>/dev/null || echo none)', not shasum's $H"
        set_pin APRENDER_DECIDE_SHA256 0000000000000000000000000000000000000000000000000000000000000000
        must_fail sha-pin 'REFUSED sha-pin:' env PATH="$F:$PATH" just laya-deploy "$W/tiny.apr" "$T" "$T/data" "$T/checkpoint"
    }
    row_resolver_proof_sed() {
        local d="$W/rp"; mkdir -p "$d"
        printf 'running 1 test\ntest deployment::builder::aprender_resolver_proof::aprender_decide_resolves_from_shared_crates_root ... RESOLVED root=crates server=aprender-mcp-decide -> crates/aprender-mcp-decide-lambda\nCONTROL root=crates/aprender-mcp-decide-lambda server=aprender-mcp-decide -> crates/aprender-mcp-chronos-lambda\nok\n' > "$d/prefixed.log"
        printf 'running 1 test\ntest deployment::builder::aprender_resolver_proof::aprender_decide_resolves_from_shared_crates_root ... ok\n' > "$d/none.log"
        must_fail no-resolved 'REFUSED resolver-parse: no RESOLVED line' just _laya-resolver-parse "$d/none.log"
        must_pass libtest-prefixed '^RESOLVED=crates/aprender-mcp-decide-lambda$' just _laya-resolver-parse "$d/prefixed.log"
        grep -qx 'CONTROL=crates/aprender-mcp-chronos-lambda' "$(logof libtest-prefixed)" || red "the CONTROL value was not read"
    }
    row_resolver_proof() {
        local nogit="$W/not-a-checkout"; mkdir -p "$nogit"
        if git -C "$nogit" rev-parse --git-dir > /dev/null 2>&1; then red "environment: $nogit is inside a git checkout"; return; fi
        must_fail not-git 'is not a git checkout' just laya-resolver-proof "$nogit" 0000000 "$W/rp-scratch"
        local body; body="$(just --show laya-resolver-proof)"
        if printf '%s' "$body" | grep -qF 'PARSED="$(just _laya-resolver-parse "$LOG")"' && ! printf '%s' "$body" | grep -qF "s/^RESOLVED root="; then
            echo "  must-pass wiring: RESOLVED/CONTROL come from _laya-resolver-parse (no column-0 sed)"
        else
            red "laya-resolver-proof does not read its tokens through _laya-resolver-parse"
        fi
        if [ "$FULL" = "1" ]; then
            local sdk commit
            sdk="${LAYA_GATES_SDK:-$HOME/Development/mcp/sdk/rust-mcp-sdk}"
            commit="$(sed -n 's/^sdk_commit=//p' "$BK/$PROOF")"
            must_pass real 'RESOLVER PROOF: crates/aprender-mcp-decide-lambda ' just laya-resolver-proof "$sdk" "$commit" "$W/rp-scratch"
        else
            full_only "the real resolver proof (cargo-pmcp test in an SDK archive)"
        fi
    }
    row_deploy_selftest_skip() {
        tiny
        must_fail synthetic-armed 'FAIL positive dry run: exit 2 without DRY-RUN OK' \
            env LAYA_ELIGIBLE_APR="$W/tiny.apr" LAYA_ELIGIBLE_RUN="$T" LAYA_ELIGIBLE_DATA="$T/data" LAYA_ELIGIBLE_BASE="$T/checkpoint" just laya-deploy-selftest
        must_pass disarmed '^DEPLOY SELFTEST OK \(positive dry run SKIPPED: disarmed by LAYA_DEPLOY_SELFTEST_POSITIVE=0\)$' \
            env LAYA_DEPLOY_SELFTEST_POSITIVE=0 just laya-deploy-selftest
        if grep -qx 'DEPLOY SELFTEST OK' "$(logof disarmed)"; then red "a skipped positive dry run printed the bare DEPLOY SELFTEST OK"; fi
        need_deployed || return 0
        armed_deploy_selftest < /dev/null
        if [ "$ARMED_RC" -eq 0 ] && grep -q '^DRY-RUN OK aprender-mcp-decide' "$W/armed-selftest.log" && grep -qx 'DEPLOY SELFTEST OK' "$W/armed-selftest.log"; then
            echo "  must-pass default-armed: exit 0, $(grep -m 1 '^DRY-RUN OK' "$W/armed-selftest.log" | cut -c 1-120), then the bare DEPLOY SELFTEST OK"
        else
            red "must-pass default-armed: exit $ARMED_RC without DRY-RUN OK and the bare OK line (log $W/armed-selftest.log)"
            tail -4 "$W/armed-selftest.log" >&2 || true
        fi
    }
    row_deploy_refusals() {
        tiny
        run setup just laya-deploy-config "$W/tiny.apr" off
        [ "$RC" -eq 0 ] || { red "laya-deploy-config on the synthetic fixture failed"; return; }
        set_pin APRENDER_DECIDE_S3_URI UNSET-run-just-laya-deploy-config
        must_fail placeholder 'REFUSED placeholder:' just laya-deploy "$W/tiny.apr" "$T" "$T/data" "$T/checkpoint"
        run regen just laya-deploy-config "$W/tiny.apr" off
        must_fail eligibility 'REFUSED eligibility: .*SyntheticNotDeployable' just laya-deploy "$W/tiny.apr" "$T" "$T/data" "$T/checkpoint"
        need_deployed || return 0
        armed_deploy_selftest
        if grep -q '^DRY-RUN OK aprender-mcp-decide' "$W/armed-positive.log"; then
            echo "  must-pass deployed: $(grep -m 1 '^DRY-RUN OK' "$W/armed-positive.log" | cut -c 1-120) (the default-armed selftest's positive dry run)"
        else
            red "must-pass deployed: the positive dry run printed no DRY-RUN OK (log $W/armed-positive.log)"
        fi
    }
    row_upload_eligibility() {
        tiny
        must_fail synthetic 'REFUSED eligibility: .*SyntheticNotDeployable' just laya-upload "$W/tiny.apr" "$T" "$T/data" "$T/checkpoint"
        if [ "$FULL" = "1" ]; then
            need_deployed || return 0
            must_pass deployed '^DRY-RUN: would upload ' just laya-upload "$DEP_APR" "$DEP_RUN" "$DEP_DATA" "$BASE"
        else
            full_only "the DRY_RUN upload of the deployed artifact (laya-verify, multi-GB)"
        fi
    }
    row_verify() {
        tiny
        must_fail synthetic 'REFUSED SyntheticNotDeployable' just laya-verify "$W/tiny.apr" "$T" "$T/data" "$T/checkpoint"
        need_deployed || return 0
        armed_deploy_selftest
        local H; H="$(shasum -a 256 "$DEP_APR" | awk '{print $1}')"
        if python3 -c 'import json, sys; v = json.loads([l for l in open(sys.argv[1]) if l.startswith("{")][-1]); sys.exit(0 if v.get("deploy_eligible") is True and v.get("artifact_sha256") == sys.argv[2] else 1)' "$W/armed-eligibility.log" "$H" 2> /dev/null; then
            echo "  must-pass deployed: laya-verify accepted $H (deploy_eligible true, via the default-armed selftest)"
        else
            red "must-pass deployed: no deploy_eligible true for $H in $W/armed-eligibility.log"
        fi
    }
    row_pack() {
        must_fail synthetic 'REFUSED SyntheticNotDeployable' just laya-pack "$T" "$T/data" "$T/checkpoint" "$W/pack-synthetic.apr"
        [ ! -e "$W/pack-synthetic.apr" ] || red "a refused pack wrote $W/pack-synthetic.apr"
        if [ "$FULL" = "1" ]; then
            need_deployed || return 0
            must_pass deployed '^PACKED .* sha256=[0-9a-f]{64} ' just laya-pack "$DEP_RUN" "$DEP_DATA" "$BASE" "$W/pack-deployed.apr"
            rm -f "$W/pack-deployed.apr"
        else
            full_only "a real repack of the deployed run (multi-GB)"
        fi
    }
    row_pack_fixture() {
        mkdir -p "$W/empty-run/data"
        must_fail no-task 'REFUSED Pack pack: read .*task.json' just laya-pack-fixture "$W/empty-run" "$W/empty-run/data" "$W/pf-refused.apr"
        [ ! -e "$W/pf-refused.apr" ] || red "a refused pack-fixture wrote $W/pf-refused.apr"
        must_pass golden "PACKED-FIXTURE .* sha256=$GOLDEN variant=synthetic-fixture" just laya-pack-fixture "$T" "$T/data" "$W/pf.apr"
    }
    row_inspect() {
        tiny
        head -c 1000 "$W/tiny.apr" > "$W/truncated.apr"
        must_fail truncated 'REFUSED Artifact load ladder' just laya-inspect "$W/truncated.apr"
        must_pass tiny "\"artifact_sha256\":\"$GOLDEN\".*\"variant\":\"synthetic-fixture\"" just laya-inspect "$W/tiny.apr"
    }
    row_verify_suite() {
        mkdir -p "$W/no-model"
        must_fail no-model 'model.safetensors is missing' just laya-verify-suite "$W/no-model"
        if [ -f "$BASE/model.safetensors" ]; then
            must_fail no-ladder 'ERROR: the ladder dump .* is missing' env LAYA_LADDER_BIN="$W/no-ladder.bin" just laya-verify-suite "$BASE"
        else
            red "the base snapshot $BASE is absent: the ladder refusal cannot be reached"
        fi
        local body; body="$(just --show laya-verify-suite)"
        if printf '%s' "$body" | grep -qF 'just _laya-leg-verdict "$name" "$log" || return 1' \
            && printf '%s' "$body" | grep -qF 'leg laya_parity LAYA_MODEL_DIR="$model" LAYA_LADDER_BIN="$ladder"' \
            && ! printf '%s' "$body" | grep -qF "grep -q '^SKIP:'"; then
            echo "  must-pass wiring: every leg's verdict is _laya-leg-verdict, and laya_parity gets LAYA_LADDER_BIN"
        else
            red "laya-verify-suite does not route every leg through _laya-leg-verdict with the ladder armed"
        fi
        if [ "$FULL" = "1" ]; then
            must_pass real '^LAYA VERIFY SUITE OK$' just laya-verify-suite "$BASE"
            local n; n="$(grep -c '^LEG OK: ' "$(logof real)" || true)"
            [ "${n:-0}" -eq 4 ] || red "the armed suite printed $n LEG OK lines, not 4"
        else
            full_only "the armed laya-verify-suite (four real-weights legs, about 15 min)"
        fi
    }
    row_train_selftest() {
        local S="$W/uvshim" U; U="$(command -v uv)"; mkdir -p "$S"
        printf '#!/bin/sh\ncase "$*" in *gate.py*--selftest*) echo "uv shim: gate selftest failed" >&2; exit 1 ;; esac\nexec "%s" "$@"\n' "$U" > "$S/uv"; chmod +x "$S/uv"
        must_fail gate-step-fails 'uv shim: gate selftest failed' env PATH="$S:$PATH" just laya-train-selftest
        if grep -q 'LAYA TRAIN SELFTEST OK' "$(logof gate-step-fails)"; then red "a failing step still printed LAYA TRAIN SELFTEST OK"; fi
        must_pass real '^LAYA TRAIN SELFTEST OK$' just laya-train-selftest
    }
    row_deploy_verify() {
        need_deployed || return 0
        must_fail one-sample 'samples must be >= 2' just laya-deploy-verify "$DEP_APR" aprender-mcp-decide ze-kasher-dev 1
        must_fail missing-apr 'does not exist' just laya-deploy-verify "$W/no-such.apr"
        must_pass dry-run '^DRY-RUN: stopped before the network' just laya-deploy-verify "$DEP_APR"
    }
    row_bootstrap_build() {
        local Z="$W/zigshim"; mkdir -p "$Z"
        printf '#!/bin/sh\necho "cargo-zigbuild shim: built nothing"\nexit 0\n' > "$Z/cargo-zigbuild"; chmod +x "$Z/cargo-zigbuild"
        must_fail stale 'ERROR: .*(is older than this build|does not exist after the build)' env PATH="$Z:$PATH" just laya-build-bootstrap
        must_pass real '^BOOTSTRAP aarch64 OK ' just laya-build-bootstrap
    }
    row_edge_health_url() {
        must_fail health-path 'REFUSED edge-health-url' just _laya-edge-health-url https://aprender-mcp-decide.us-east.true-mcp.com/health
        must_fail http 'REFUSED edge-health-url' just _laya-edge-health-url http://aprender-mcp-decide.us-east.true-mcp.com/mcp
        must_pass mcp '^https://aprender-mcp-decide.us-east.true-mcp.com/health$' just _laya-edge-health-url https://aprender-mcp-decide.us-east.true-mcp.com/mcp
    }
    row_edge_health_check() {
        must_fail chronos "REFUSED edge-health: serverId is 'chronos-forecaster'" just _laya-edge-health-check '{"serverId":"chronos-forecaster"}' aprender-mcp-decide
        must_fail bootstrap-body 'REFUSED edge-health: serverId is None' just _laya-edge-health-check '{"package":"aprender-mcp-decide-lambda","server":"aprender-mcp-decide"}' aprender-mcp-decide
        must_pass decide '^edge-health ok: serverId aprender-mcp-decide$' just _laya-edge-health-check '{"status":"healthy","serverId":"aprender-mcp-decide"}' aprender-mcp-decide
    }
    row_crates_root_swap() {
        local R="$W/swaproot" C="$W/swapcfg.toml" before after bkdir
        mkdir -p "$R/.pmcp" "$R/deploy/lib"
        echo 'name = "setfit-train"' > "$R/.pmcp/deploy.toml"; echo 'stack' > "$R/deploy/lib/stack.ts"; echo 'name = "decide"' > "$C"
        before="$(cd "$R" && find . -type f -print0 | LC_ALL=C sort -z | xargs -0 shasum -a 256 | shasum -a 256)"
        must_fail not-a-dir 'REFUSED swap: deploy root .* is not a directory' just _laya-crates-root-swap "$W/no-such-root" "$C" "$W/snap" true
        must_pass swap '^RESTORED .* byte-identical' just _laya-crates-root-swap "$R" "$C" "$W/snap" \
            sh -c 'cmp -s "$1/.pmcp/deploy.toml" "$2" && test ! -e "$1/deploy"' _ "$R" "$C"
        after="$(cd "$R" && find . -type f -print0 | LC_ALL=C sort -z | xargs -0 shasum -a 256 | shasum -a 256)"
        [ "$before" = "$after" ] || red "the swap root was not restored byte-identically"
        bkdir="models/decide/swap-backup/$(printf '%s' "$R" | tr '/.' '__')"
        mkdir -p "$bkdir"
        must_fail leftover-backup 'REFUSED swap: .* exists' just _laya-crates-root-swap "$R" "$C" "$W/snap" true
        rm -rf "$bkdir"
    }
    row_iam_check() {
        local ok="$W/iam-ok.toml" star="$W/iam-star.toml" two="$W/iam-two.toml"
        printf '[server]\nname = "aprender-mcp-decide"\n[environment]\nAPRENDER_DECIDE_S3_URI = "s3://b/decide/aprender-mcp-decide/h.apr"\n[[iam.statements]]\neffect = "Allow"\nactions = ["s3:GetObject"]\nresources = ["arn:aws:s3:::b/decide/aprender-mcp-decide/*"]\n' > "$ok"
        sed 's/actions = \["s3:GetObject"\]/actions = ["s3:*"]/' "$ok" > "$star"
        { cat "$ok"; printf '[[iam.statements]]\neffect = "Allow"\nactions = ["s3:GetObject"]\nresources = ["arn:aws:s3:::other/x"]\n'; } > "$two"
        must_fail s3-star 'REFUSED iam: the statement' just _laya-iam-check "$star"
        must_fail two-statements 'REFUSED iam: expected exactly one' just _laya-iam-check "$two"
        must_pass scoped '^iam ok: Allow s3:GetObject arn:aws:s3:::b/decide/aprender-mcp-decide/\*' just _laya-iam-check "$ok"
    }
    row_resolver_ere() {
        must_fail pre-08-22-fn 'FAIL fn expected no-match, got match: /// fn name' \
            make --no-print-directory contract-audit-phase8-selftest 'P8_FN_ERE=(^|[^[:alnum:]_])fn[[:space:]]+$$name[[:space:]]*[(<]'
        must_fail pre-08-22-recipe 'FAIL recipe expected no-match, got match: name := value' \
            make --no-print-directory contract-audit-phase8-selftest 'P8_RECIPE_ERE=^$$name([[:space:]][^:]*)?:'
        must_pass committed 'every resolver ERE as the table expects' make --no-print-directory contract-audit-phase8-selftest
    }
    row_dup_bin_names() {
        grep -v '^bootstrap ' scripts/duplicate_bin_names_allowlist.txt > "$W/allow-no-bootstrap.txt"
        if cmp -s "$W/allow-no-bootstrap.txt" scripts/duplicate_bin_names_allowlist.txt; then
            red "the allowlist has no bootstrap line to remove (the intent is not declared)"
        elif ! cargo metadata --no-deps --format-version 1 > "$W/root-md.json" 2> /dev/null \
            || ! (cd crates/facades && cargo metadata --no-deps --format-version 1) > "$W/facades-md.json" 2> /dev/null; then
            red "cargo metadata failed: the guard's engine has nothing to scan"
        else
            must_fail no-bootstrap-line 'D  `bootstrap` is declared by 4 packages' \
                python3 scripts/lib/bin_names.py "$W/allow-no-bootstrap.txt" "root=$W/root-md.json" "facades=$W/facades-md.json"
        fi
        must_pass guard '^PASS  every duplicated bin name is declared intentional' bash scripts/check_duplicate_bin_names.sh
        must_pass self-test '^SELF-TEST PASSED' bash scripts/check_duplicate_bin_names.sh --self-test
    }
    row_gates_selftest() {
        if [ "${LAYA_GATES_DEPTH:-0}" != "0" ]; then red "a nested sweep reached the gates-selftest row (recursion)"; return; fi
        cp "$TSV" "$W/t-unknown.tsv"; printf 'no-such-gate\tjust laya-verify\tplanted\tplanted\tplanted by gates-selftest\n' >> "$W/t-unknown.tsv"
        cp "$TSV" "$W/t-exempt.tsv"; printf 'not-a-gate\tlaya-verify\t-\t-\tplanted by gates-selftest\n' >> "$W/t-exempt.tsv"
        must_fail unknown-id 'unknown gate_id no-such-gate' env LAYA_GATES_DEPTH=1 LAYA_GATES_TSV="$W/t-unknown.tsv" just laya-gates-selftest
        grep -q '^  must-' "$(logof unknown-id)" && red "a case ran before the unknown gate_id was refused"
        must_fail verdict-exempt 'DRIFT: laya-verify prints a verdict and cannot be not-a-gate' env LAYA_GATES_DEPTH=1 LAYA_GATES_TSV="$W/t-exempt.tsv" just laya-gates-selftest
        must_pass one-row '^LAYA GATES ROWS OK 1 of ' env LAYA_GATES_DEPTH=1 LAYA_GATES_ONLY=edge-health-url just laya-gates-selftest
    }
    row_claims_check() {
        # The committed ledger with its FIRST row's anchor planted (and renamed), the rest intact: the
        # recipe's own three self-cases must still pass (3 `self-case` lines), then the ledger fails.
        awk -F '\t' 'BEGIN { OFS = "\t" } NR == 2 { $1 = "gates-selftest-anchor"; $3 = "planted by gates-selftest: an anchor in no file" } { print }' scripts/laya_claims.tsv > "$W/claims-anchor.tsv"
        must_fail missing-anchor '^ANCHOR MISSING gates-selftest-anchor: ' env LAYA_CLAIMS_TSV="$W/claims-anchor.tsv" just laya-claims-check
        [ "$(grep -c '^self-case [a-z]*: exit ' "$(logof missing-anchor)")" -eq 3 ] || red "missing-anchor: the recipe's three self-cases did not all run and fail as designed before the ledger"
        must_pass ledger '^LAYA CLAIMS OK [0-9]+ rows' just laya-claims-check
    }
    row_gap_regression() {
        # A cargo shim first on PATH: `zero` exits 0 having run no test (the CR-02 shape of a stale name
        # filter), `fail` exits 101. Class A must FAIL on both; the real class A must pass.
        local S="$W/cargoshim"; mkdir -p "$S"
        printf '#!/bin/sh\nif [ "${GAP_SHIM_MODE:-zero}" = fail ]; then echo "cargo shim: a test failed" >&2; exit 101; fi\necho "running 0 tests"\necho "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.00s"\nexit 0\n' > "$S/cargo"
        chmod +x "$S/cargo"
        must_fail zero-match '^FAIL CLASS A manifest-and-run-fields: 0 passed' env PATH="$S:$PATH" LAYA_GAP_ONLY=A just laya-gap-regression
        must_fail stage-exit '^FAIL CLASS A manifest-and-run-fields: exit 101' env PATH="$S:$PATH" GAP_SHIM_MODE=fail LAYA_GAP_ONLY=A just laya-gap-regression
        if grep -q 'LAYA GAP' "$(logof zero-match)" "$(logof stage-exit)"; then red "a failing class still printed a LAYA GAP OK line"; fi
        must_fail unknown-class "^FAIL LAYA_GAP_ONLY: unknown class 'F'" env LAYA_GAP_ONLY=F just laya-gap-regression
        # HYGIENE must FAIL when its first gate exits non-zero (the cargo shim's `fail` mode drives `pv lint`)...
        must_fail hygiene-stage-exit '^FAIL HYGIENE pv-lint: exit 101' env PATH="$S:$PATH" GAP_SHIM_MODE=fail LAYA_GAP_ONLY=HYGIENE just laya-gap-regression
        # ...and when the tracked graph reads stale: a `pv` shim answers lint PASS but `extract --check` exits 1,
        # so a stale graph cannot ride through on the lint verdict (the real pv is measured in the 08-34 evidence).
        local P="$W/pvshim"; mkdir -p "$P"
        printf '#!/bin/sh\ncase "$1" in lint) echo "armed meet: Pass"; echo "Result: PASS"; exit 0 ;; extract) echo "shim: the tracked graph differs from a fresh extraction" >&2; exit 1 ;; esac\nexit 2\n' > "$P/pv"
        chmod +x "$P/pv"
        must_fail hygiene-stale-graph '^FAIL HYGIENE graph-fresh: exit 1' env PATH="$P:$PATH" LAYA_GAP_ONLY=HYGIENE just laya-gap-regression
        must_pass class-a '^LAYA GAP STAGES OK \(A; not the full regression\)' env LAYA_GAP_ONLY=A just laya-gap-regression
        grep -q '^PASS CLASS A$' "$(logof class-a)" || red "class-a: no PASS CLASS A line"
    }
    # ---- dispatch -------------------------------------------------------------------------------
    UNKNOWN=""
    while IFS=$'\t' read -r -u 3 gid target mf; do
        case "$target" in external:*) continue ;; esac
        declare -F "row_${gid//-/_}" > /dev/null || UNKNOWN="$UNKNOWN $gid"
    done 3< "$W/rows.tsv"
    if [ -n "$UNKNOWN" ]; then
        for gid in $UNKNOWN; do echo "FAIL: unknown gate_id $gid: no dispatcher case in laya-gates-selftest" >&2; done
        echo "LAYA GATES SELFTEST FAILED: unknown gate_id(s):$UNKNOWN (no case ran)" >&2
        exit 1
    fi
    if [ -n "$ONLY" ]; then
        for gid in ${ONLY//,/ }; do
            cut -f 1 "$W/rows.tsv" | grep -qx "$gid" || { echo "LAYA GATES SELFTEST FAILED: LAYA_GATES_ONLY names $gid, which is not a row" >&2; exit 1; }
        done
    fi
    TOTAL=0; RAN=0; FAILS=0; REDS=""
    while IFS=$'\t' read -r -u 3 gid target mf; do
        TOTAL=$((TOTAL + 1))
        if [ -n "$ONLY" ] && ! printf ',%s,' "$ONLY" | grep -qF ",$gid,"; then continue; fi
        RAN=$((RAN + 1))
        CUR="$gid"; ROW_RED=0; S0=$(date +%s)
        echo "ROW $gid ($target)"
        case "$target" in
            external:*)
                if [ "$mf" = "-" ]; then
                    echo "  external: owner plan ${target#external:}; no local case"
                else
                    run external sh -c "$mf"
                    if [ "$RC" -eq 0 ]; then echo "  external case: '$mf' exit 0 (owner plan ${target#external:})"; else red "external case '$mf' exit $RC (log $(logof external))"; tail -4 "$(logof external)" >&2 || true; fi
                fi
                ;;
            *) "row_${gid//-/_}" ;;
        esac
        if [ "$ROW_RED" -eq 0 ]; then echo "ROW $gid: GREEN ($(( $(date +%s) - S0 )) s)"; else echo "ROW $gid: RED" >&2; FAILS=$((FAILS + 1)); REDS="$REDS $gid"; fi
    done 3< "$W/rows.tsv"
    CALLS="$(wc -l < "$REC" | tr -d ' ')"
    echo "AWS CALLS: $CALLS"
    [ "$CALLS" -eq 0 ] || { echo "LAYA GATES SELFTEST FAILED: the aws recorder was called: $(head -3 "$REC" | tr '\n' ';')" >&2; exit 1; }
    if [ "$FAILS" -ne 0 ]; then
        echo "LAYA GATES SELFTEST FAILED: $FAILS row(s) RED:$REDS" >&2
        exit 1
    fi
    if [ -n "$ONLY" ]; then
        echo "LAYA GATES ROWS OK $RAN of $TOTAL (LAYA_GATES_ONLY=$ONLY; not a full sweep)"
    elif [ -n "$FULL_SKIPPED" ]; then
        echo "LAYA GATES SELFTEST OK $TOTAL rows (FULL-tier must-pass cases SKIPPED: $FULL_SKIPPED; set LAYA_GATES_FULL=1)"
    else
        echo "LAYA GATES SELFTEST OK $TOTAL rows"
    fi

# ---------------------------------------------------------------------------
# Class D, claim honesty (plan 08-31): every normative claim the 08-19..08-30 gap round corrected or
# enforced is a row of scripts/laya_claims.tsv, and the row names what enforces it. This branch's CI
# strict-binding guard is vacuous (D-ITEM-08-01-A), so this ledger is also the non-vacuous check that
# a contract's named tests exist.
# ---------------------------------------------------------------------------

# Check scripts/laya_claims.tsv (or $LAYA_CLAIMS_TSV). Header:
#   claim_id<TAB>file<TAB>anchor<TAB>kind<TAB>owner<TAB>test<TAB>finding
# Per row: `anchor` must occur in `file` (a fixed-string match, like `grep -F`), then by `kind`:
#   rust      every name in `test` (one, or several joined by `,`) is in `cargo test -p <owner> --
#             --list` (listed once per owner and cached); an owner `<crate>:lib` lists `--lib` only and
#             `<crate>:test=<target>` that one target only, so a contract command's `--lib` / `--test`
#             selector is checked as written (and aprender-core's full list is never built). A name
#             matches a listed test exactly or as its `::`-suffix, never as a substring: cargo's
#             substring filter runs zero tests on a stale name and passes, which is the defect this
#             check exists to see
#   just      every name in `test` (one, or several joined by `,`) is a recipe (`just --dump`:
#             `just --summary` hides the private `_laya-*` recipes)
#   python    `test` appears in `just laya-train-selftest` output (run once and cached; needs uv + torch)
#   pv        `pv validate <file>` exits 0 with `0 error(s)`, and `test` (a FALSIFY id, or `-`) is in the file
#   evidence  `test` is `<json file>#<dotted.key>` and that key exists in that JSON file
# An unknown kind, a malformed row or a repeated claim_id FAILS. FIRST, before the ledger itself, three
# built-in MUST-FAIL self-cases run against temp copies of it — a missing anchor, a missing test, an
# unknown kind — and each must exit non-zero naming its planted row, or the check fails. Prints
# `LAYA CLAIMS OK <n> rows`. pv is `pv` on PATH, else built with cargo (the Makefile's PV_BIN).
laya-claims-check:
    #!/usr/bin/env bash
    set -euo pipefail
    TSV="${LAYA_CLAIMS_TSV:-scripts/laya_claims.tsv}"
    [ -f "$TSV" ] || { echo "LAYA CLAIMS FAILED: the ledger $TSV does not exist" >&2; exit 1; }
    W="$(mktemp -d)"
    trap 'rm -rf "$W"' EXIT
    mkdir -p "$W/cache"
    if command -v pv > /dev/null 2>&1; then export LAYA_CLAIMS_PV="pv"; else export LAYA_CLAIMS_PV="cargo run --release -q -p aprender-contracts-cli --bin pv --"; fi
    cat > "$W/claims.py" <<'PY'
    import json, os, re, shlex, subprocess, sys
    HEADER = ["claim_id", "file", "anchor", "kind", "owner", "test", "finding"]
    KINDS = ("rust", "just", "python", "pv", "evidence")
    def rows_of(tsv):
        lines = open(tsv, encoding="utf-8").read().split("\n")
        if lines and lines[-1] == "":
            lines.pop()
        return lines
    def safe(s):
        return re.sub(r"[^A-Za-z0-9_.=-]", "_", s)
    def cached(cache, key, argv):
        path = os.path.join(cache, safe(key))
        if not os.path.exists(path + ".rc"):
            p = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
            open(path + ".out", "w").write(p.stdout)
            open(path + ".rc", "w").write(str(p.returncode))
        return int(open(path + ".rc").read()), open(path + ".out").read()
    def rust_tests(cache, owner):
        crate, _, sel = owner.partition(":")
        argv = ["cargo", "test", "-q", "-p", crate]
        if sel == "lib":
            argv.append("--lib")
        elif sel.startswith("test="):
            argv += ["--test", sel[len("test="):]]
        elif sel:
            return None, f"owner selector {sel!r} is not lib or test=<target>"
        rc, out = cached(cache, "rust-" + owner, argv + ["--", "--list"])
        if rc != 0:
            return None, f"`{' '.join(argv)} -- --list` exited {rc}"
        return {m.group(1) for m in re.finditer(r"^(\S+): (?:test|bench)$", out, re.M)}, None
    def check(tsv, cache):
        lines = rows_of(tsv)
        problems = []
        if not lines or lines[0].split("\t") != HEADER:
            print(f"TABLE: the first line of {tsv} is not the header {chr(9).join(HEADER)!r}")
            return 1
        ids, n = set(), 0
        for i, raw in enumerate(lines[1:], 2):
            f = raw.split("\t")
            if len(f) != 7 or any(not x.strip() for x in f):
                problems.append(f"TABLE: line {i} does not have seven non-empty tab-separated fields: {raw[:120]!r}")
                continue
            cid, path, anchor, kind, owner, test, finding = f
            n += 1
            if cid in ids:
                problems.append(f"TABLE: line {i}: claim_id {cid} is repeated")
            ids.add(cid)
            if kind not in KINDS:
                problems.append(f"UNKNOWN KIND {cid}: {kind!r} is not one of {', '.join(KINDS)}")
                continue
            if not os.path.isfile(path):
                problems.append(f"FILE MISSING {cid}: {path}")
                continue
            text = open(path, encoding="utf-8").read()
            if anchor not in text:
                problems.append(f"ANCHOR MISSING {cid}: {path} no longer contains {anchor[:100]!r}")
                continue
            if kind == "rust":
                names, err = rust_tests(cache, owner)
                if err:
                    problems.append(f"TEST UNLISTABLE {cid}: {err}")
                else:
                    gone = [x for x in test.split(",") if not x or not any(t == x or t.endswith("::" + x) for t in names)]
                    if gone:
                        problems.append(f"TEST MISSING {cid}: {', '.join(gone)} not in the test list of {owner} ({len(names)} tests)")
            elif kind == "just":
                rc, out = cached(cache, "just-dump", ["just", "--dump", "--dump-format", "json"])
                gone = [x for x in test.split(",") if rc != 0 or x not in json.loads(out)["recipes"]]
                if gone:
                    problems.append(f"TEST MISSING {cid}: {', '.join(gone)} not a just recipe")
            elif kind == "python":
                rc, out = cached(cache, "python-selftest", ["just", "laya-train-selftest"])
                if rc != 0 or "LAYA TRAIN SELFTEST OK" not in out:
                    problems.append(f"TEST UNLISTABLE {cid}: `just laya-train-selftest` exited {rc} without LAYA TRAIN SELFTEST OK")
                elif test not in out:
                    problems.append(f"TEST MISSING {cid}: {test!r} is not in the laya-train-selftest output")
            elif kind == "pv":
                rc, out = cached(cache, "pv-" + path, shlex.split(os.environ["LAYA_CLAIMS_PV"]) + ["validate", path])
                if rc != 0 or not re.search(r"^0 error\(s\)", out, re.M):
                    problems.append(f"PV INVALID {cid}: `pv validate {path}` exited {rc} without 0 error(s)")
                elif test != "-" and test not in text:
                    problems.append(f"TEST MISSING {cid}: {test} is not in {path}")
            elif kind == "evidence":
                jpath, sep, key = test.partition("#")
                if not sep or not key:
                    problems.append(f"TABLE: line {i}: an evidence test is <json file>#<dotted.key>, got {test!r}")
                    continue
                try:
                    node = json.load(open(jpath, encoding="utf-8"))
                except (OSError, ValueError) as e:
                    problems.append(f"TEST MISSING {cid}: {jpath} is not readable JSON ({e})")
                    continue
                for part in key.split("."):
                    if isinstance(node, dict) and part in node:
                        node = node[part]
                    elif isinstance(node, list) and part.isdigit() and int(part) < len(node):
                        node = node[int(part)]
                    else:
                        problems.append(f"TEST MISSING {cid}: {jpath} has no key {key}")
                        break
        for p in problems:
            print(p)
        if problems:
            print(f"LAYA CLAIMS FAILED: {len(problems)} problem(s) in {n} rows")
            return 1
        print(f"LAYA CLAIMS OK {n} rows")
        return 0
    def plant(case, tsv, out):
        lines = rows_of(tsv)
        rows = [l.split("\t") for l in lines[1:]]
        if not rows:
            sys.exit("the ledger has no rows to plant into")
        if case == "anchor":
            r = rows[0]
            r[0], r[2] = "selftest-anchor", "LAYA-CLAIMS-SELFTEST planted anchor, in no file"
        elif case == "test":
            # A rust row whose anchor is present, so the planted row fails on its TEST, never earlier.
            def anchored(r):
                return os.path.isfile(r[1]) and r[2] in open(r[1], encoding="utf-8").read()
            r = next((r for r in rows if len(r) == 7 and r[3] == "rust" and anchored(r)), None)
            if r is None:
                sys.exit("the ledger has no anchored rust row to plant a missing test into")
            r[0], r[5] = "selftest-test", "no_such_test_planted_by_laya_claims_check"
        elif case == "kind":
            rows.append(["selftest-kind"] + rows[0][1:3] + ["bogus"] + rows[0][4:])
        else:
            sys.exit(f"unknown self-case {case}")
        open(out, "w").write("\n".join([lines[0]] + ["\t".join(r) for r in rows]) + "\n")
    if sys.argv[1] == "check":
        sys.exit(check(sys.argv[2], sys.argv[3]))
    plant(sys.argv[2], sys.argv[3], sys.argv[4])
    PY
    # 1. The must-fail self-cases, before the ledger itself. Each shares the list cache.
    for c in anchor:ANCHOR test:TEST kind:UNKNOWN; do
        case_id="${c%%:*}"; want="${c#*:}"
        python3 "$W/claims.py" plant "$case_id" "$TSV" "$W/planted-$case_id.tsv"
        set +e; python3 "$W/claims.py" check "$W/planted-$case_id.tsv" "$W/cache" > "$W/self-$case_id.log" 2>&1; rc=$?; set -e
        if [ "$rc" -ne 0 ] && grep -Eq "^$want (MISSING|KIND) selftest-$case_id:" "$W/self-$case_id.log"; then
            echo "self-case $case_id: exit $rc, $(grep -E -m 1 "^$want (MISSING|KIND) selftest-$case_id:" "$W/self-$case_id.log" | cut -c 1-140)"
        else
            tail -5 "$W/self-$case_id.log" >&2 || true
            echo "LAYA CLAIMS FAILED: self-case $case_id did not fail as designed (exit $rc, no '$want ... selftest-$case_id' line)" >&2
            exit 1
        fi
    done
    # 2. The ledger.
    python3 "$W/claims.py" check "$TSV" "$W/cache"

# ---------------------------------------------------------------------------
# The gap round as a whole (plan 08-32): each of plans 08-19..08-31 proved its own class slice; this is
# the goal-backward check that they compose, on the final tree and the real deployed artifact.
# ---------------------------------------------------------------------------

# Re-prove every class invariant of the 08-19..08-31 gap round, then the phase regression, in one run:
#   CLASS A  manifest leaf sweep, run-field sweep, served-fields sweep, the lambda probe checks
#   CLASS B  the artifact / request / Lambda bounds sweeps, apr-format and the ModernBERT bound tests
#   CLASS C  just laya-gates-selftest, make contract-audit-phase8 (with its ERE case table)
#   CLASS D  just laya-claims-check
#   CLASS E  the bit-for-bit numeric replay (Rust side) and laya-train-selftest (Python side, with
#            METRICS SELFTEST OK and the PYTHON REFUSALS sweep line)
#   HYGIENE  the contract-hygiene class 08-32's regression never reached (plan 08-34): `pv lint contracts`
#            (Result: PASS), `pv extract contracts --check` on a clean export of the tracked tree (graph
#            freshness, see below), and the aprender-contracts lib and corpus tests. 08-32 reported PASS
#            while six of those lib tests and two corpus tests were red.
#   REGRESSION  the three decide crates' tests, clippy -D warnings, fmt, pv validate on the four Phase 8
#            contracts, monorepo_invariants + readme_contract, the aprender-contracts-cli tests (the
#            contract-cycle guard, then the WHOLE crate with nothing skipped, on the clean export),
#            laya-verify-suite (real weights, every LEG OK, the ladder rung MEASURED) and laya-verify of
#            the deployed artifact from the MAIN checkout (deploy_eligible true, the pinned sha256
#            below, shipped seed 17).
# The clean export: `pv extract` and `cargo test -p aprender-contracts-cli` walk every directory except
# target/.git/.lake/node_modules, so untracked agent worktrees (.claude/worktrees) make the tracked graph
# read as stale in a working tree (`the_tracked_repo_graph_is_fresh`): a host artifact CI's checkout
# cannot reproduce. Both graph-sensitive checks therefore run on a clone of HEAD (plus any uncommitted
# tracked edits) at <target>/laya-gap-export, which is what a clean checkout holds. Nothing is skipped
# by name.
# Every stage logs to one temp dir (printed first). A stage whose exit is non-zero, whose evidence line
# is absent, or whose named-test run passes a different number of tests than it names prints
# `FAIL <CLASS> <stage>: <why> (log <path>)` and the recipe exits 1: a name filter that matches nothing
# exits 0 in cargo (REVIEW CR-02), so the passed count is the check, never the exit status alone.
# Success prints `PASS CLASS A` .. `PASS CLASS E`, `PASS HYGIENE`, `PASS REGRESSION` and
# `LAYA GAP REGRESSION OK`.
# LAYA_GAP_ONLY=<A,B,C,D,E,HYGIENE,REGRESSION> runs only those classes and prints `LAYA GAP STAGES OK`, never
# the full OK line (row gap-regression of scripts/laya_gates.tsv uses it). The default run reads real
# weights (laya-gates-selftest's armed leg, laya-verify-suite, laya-verify), so run it under the host's
# real-weights lock (`lockf -k /tmp/aprender-laya-real-weights.lock just laya-gap-regression`); it does
# not take the lock itself, because laya-gates-selftest re-enters it for class A.
laya-gap-regression:
    #!/usr/bin/env bash
    set -euo pipefail
    ONLY="${LAYA_GAP_ONLY:-}"
    CLASSES="A B C D E HYGIENE REGRESSION"
    for c in ${ONLY//,/ }; do
        case " $CLASSES " in *" $c "*) ;; *) echo "FAIL LAYA_GAP_ONLY: unknown class '$c' (known: $CLASSES)" >&2; exit 1 ;; esac
    done
    DEPLOYED_SHA="24a44d7e050166c9b64e2716f2bcb3ce91747f7a3b927d03d6eeae5f89b6275a"
    DEPLOYED_SEED=17
    MAIN="$(cd "$(git rev-parse --git-common-dir)/.." && pwd -P)"
    L="$(mktemp -d "${TMPDIR:-/tmp}/laya-gap-regression.XXXXXX")"
    echo "LOGS $L"
    CLS=""
    wanted() { [ -z "$ONLY" ] && return 0; case ",$ONLY," in *",$1,"*) return 0 ;; esac; return 1; }
    fail() { echo "FAIL $CLS $1: $2 (log $3)" >&2; tail -8 "$3" >&2 || true; exit 1; }
    # The sum of libtest's `N passed` over every `test result:` line of a log.
    passed() { awk '/^test result: /{ for (i = 2; i <= NF; i++) if ($i == "passed;") s += $(i - 1) } END { print s + 0 }' "$1"; }
    # run <stage> <cmd...>: the command's status, read on its own line; non-zero is a FAIL.
    run() {
        local stage="$1" rc; shift
        LOG="$L/${CLS// /-}-$stage.log"
        set +e
        "$@" < /dev/null > "$LOG" 2>&1
        rc=$?
        set -e
        [ "$rc" -eq 0 ] || fail "$stage" "exit $rc" "$LOG"
    }
    # need <stage> <ERE>: the last run's log must carry a line matching ERE.
    need() { grep -Eq -- "$2" "$LOG" || fail "$1" "no line matching '$2'" "$LOG"; }
    # tests <stage> <want> <cargo args...>: exit 0 AND exactly <want> tests passed (`+` = at least one).
    tests() {
        local stage="$1" want="$2" n; shift 2
        run "$stage" cargo test "$@"
        n="$(passed "$LOG")"
        if [ "$want" = "+" ]; then
            [ "$n" -ge 1 ] || fail "$stage" "0 passed: the run matched no test and proves nothing" "$LOG"
        else
            [ "$n" -eq "$want" ] || fail "$stage" "$n passed, but the stage names exactly $want test(s)" "$LOG"
        fi
        echo "  ok $stage: $n passed"
    }
    # evidence <stage> <ERE>...: `run` already happened; every ERE must match, the first is echoed.
    evidence() {
        local stage="$1" re; shift
        for re in "$@"; do need "$stage" "$re"; done
        echo "  ok $stage: $(grep -Eo -m 1 -- "$1.*" "$LOG" | cut -c 1-150)"
    }
    # The pv the HYGIENE and REGRESSION stages drive, resolved once. `--manifest-path` lets it run from
    # any directory (the graph check runs from the clean export).
    TOP="$(git rev-parse --show-toplevel)"
    if command -v pv > /dev/null 2>&1; then PV=(pv); else PV=(cargo run --release -q --manifest-path "$TOP/Cargo.toml" -p aprender-contracts-cli --bin pv --); fi
    # export_tree: X = a fresh clone of the TRACKED tree (HEAD + tracked edits) inside the cargo target dir (a
    # stable path, so cargo reuses its fingerprints run to run; target/ is skipped by every tree walker).
    X=""
    TARGET=""
    export_tree() {
        TARGET="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')" \
            || fail export "cargo metadata gave no target directory" /dev/null
        X="$TARGET/laya-gap-export"
        rm -rf "$X"
        # A clone at HEAD, not a bare copy of the files: scripts that read the tree ask git for it
        # (parity_receipt_denominator.sh lists `git ls-files`), so the export must be a repository.
        git clone -q --local --no-checkout "$TOP" "$X" || fail export "the clone failed" /dev/null
        git -C "$X" checkout -q --detach "$(git -C "$TOP" rev-parse HEAD)" || fail export "the checkout failed" /dev/null
        # Overlay the tracked edits that are not committed yet, so what is checked is what this tree holds.
        (cd "$TOP" && git diff -z --name-only --diff-filter=AM HEAD) > "$X/../laya-gap-export.edits"
        if [ -s "$X/../laya-gap-export.edits" ]; then
            (cd "$TOP" && tar --null -T "$X/../laya-gap-export.edits" -cf -) | tar -x -C "$X" || fail export "the edits could not be overlaid" /dev/null
        fi
        rm -f "$X/../laya-gap-export.edits"
    }
    trap '[ -z "$X" ] || rm -rf "$X"' EXIT
    if wanted A; then
        CLS="CLASS A"
        tests manifest-and-run-fields 4 -p aprender-decide --lib -- --exact \
            artifact::ladder::every_manifest_leaf_is_bound artifact::ladder::manifest_bindings_table_matches_manifest_leaves \
            verify::tests::every_run_field_is_bound_or_report_only verify::tests::run_field_bindings_table_matches_fixture_leaves
        tests served-fields 1 -p aprender-mcp-decide --lib -- --exact tests::every_served_field_has_a_bound_source
        tests lambda-probe + -p aprender-mcp-decide-lambda --lib -- probe::
        echo "PASS CLASS A"
    fi
    if wanted B; then
        CLS="CLASS B"
        tests artifact-bounds 1 -p aprender-decide --lib -- --exact artifact::ladder::artifact_bounds_table_is_swept
        tests request-bounds 1 -p aprender-mcp-decide --lib -- --exact tests::request_bounds_table_is_swept
        tests lambda-request-rows 1 -p aprender-mcp-decide-lambda --lib -- --exact tests::lambda_request_rows_are_swept
        # golden_v2_f32_writer_is_byte_identical is red at every commit since before 08-20 (writer 516 B,
        # fixture 1092 B; deferred-items.md "Found during plan 08-20", open, not Phase 8's): skipped by its
        # exact path, never by a substring, and the skip is printed so it is never silent.
        tests apr-format + -p apr-format -- --exact --skip golden_v2_f32_writer_is_byte_identical
        echo "  skipped apr-format: golden_v2_f32_writer_is_byte_identical (deferred-items 08-20, pre-existing writer/fixture drift)"
        tests modernbert + -p aprender-core --lib models::modernbert
        echo "PASS CLASS B"
    fi
    if wanted C; then
        CLS="CLASS C"
        run gates-selftest just laya-gates-selftest
        evidence gates-selftest '^LAYA GATES SELFTEST OK [0-9]+ rows' '^AWS CALLS: 0$' '^DRIFT CHECK OK: '
        run contract-audit-phase8 make --no-print-directory contract-audit-phase8
        evidence contract-audit-phase8 '^Phase 8 binding audit: 4 contract\(s\) audited' \
            'every resolver ERE as the table expects' '^Phase 8 source resolution: resolved [1-9][0-9]* '
        echo "PASS CLASS C"
    fi
    if wanted D; then
        CLS="CLASS D"
        run claims-check just laya-claims-check
        evidence claims-check '^LAYA CLAIMS OK [1-9][0-9]* rows' '^self-case anchor: exit ' '^self-case test: exit ' '^self-case kind: exit '
        echo "PASS CLASS D"
    fi
    if wanted E; then
        CLS="CLASS E"
        tests numeric-replay 1 -p aprender-decide --lib -- --exact verify::tests::gate_numeric_cases_agree_bit_for_bit
        run train-selftest just laya-train-selftest
        evidence train-selftest '^METRICS SELFTEST OK ' '^PYTHON REFUSALS swept=[1-9]' '^LAYA TRAIN SELFTEST OK$'
        echo "PASS CLASS E"
    fi
    if wanted HYGIENE; then
        CLS="HYGIENE"
        run pv-lint "${PV[@]}" lint contracts
        evidence pv-lint '^Result: PASS' '^armed meet: Pass'
        # Graph freshness: the tracked contracts.nt/shapes.ttl equal a fresh extraction, on the clean export.
        export_tree
        run graph-fresh bash -c 'cd "$1" && shift && exec "$@"' _ "$X" "${PV[@]}" extract contracts --check
        evidence graph-fresh '"check": \[\]'
        rm -rf "$X"
        tests contracts-lib + -p aprender-contracts --lib
        tests contracts-corpus + -p aprender-contracts --test validate_contracts
        echo "PASS HYGIENE"
    fi
    if wanted REGRESSION; then
        CLS="REGRESSION"
        tests decide-crates + -p aprender-decide -p aprender-mcp-decide -p aprender-mcp-decide-lambda
        run clippy cargo clippy -p aprender-decide -p aprender-mcp-decide -p aprender-mcp-decide-lambda -p apr-format \
            --all-targets --no-deps -- -D warnings
        echo "  ok clippy: -D warnings clean on the three decide crates and apr-format"
        run fmt cargo fmt -p aprender-decide -p aprender-mcp-decide -p aprender-mcp-decide-lambda -p apr-format -p aprender-core -- --check
        echo "  ok fmt: rustfmt --check clean"
        for c in contracts/decide-tool-boundary-v1.yaml contracts/laya-finetune-gate-v1.yaml contracts/laya-parity-v1.yaml contracts/decide-apr-v1.yaml; do
            run "pv-$(basename "$c" .yaml)" "${PV[@]}" validate "$c"
            evidence "pv-$(basename "$c" .yaml)" '^0 error\(s\)'
        done
        tests invariants + -p aprender-core --test monorepo_invariants --test readme_contract
        # The contract-cycle guard by name (the three tests the 08-01 dependency cycle broke, plan 08-12), then
        # the WHOLE crate with nothing skipped (plan 08-34): every_contract_generates_book_page is green since
        # the upstream merge fixed its walker (D-ITEM-08-32-A, closed), so the old exemption hid nothing today
        # and would have hidden a future regression of that test. The crate runs on the clean export (see the
        # header): its the_tracked_repo_graph_is_fresh reads the tree, and untracked worktrees falsify it.
        tests contract-cycle 3 -p aprender-contracts-cli --lib -- --exact commands::certify::tests::certify_on_real_contracts \
            commands::verify_pipeline::tests::verify_pipeline_on_real_contracts commands::verify_pipeline::tests::verify_pipeline_json_on_real_contracts
        export_tree
        CARGO_TARGET_DIR="$TARGET" CARGO_INCREMENTAL=0 tests contracts-cli + --manifest-path "$X/Cargo.toml" -p aprender-contracts-cli
        rm -rf "$X"
        run verify-suite just laya-verify-suite
        evidence verify-suite '^LAYA VERIFY SUITE OK$' '^LEG OK: laya_parity$' '^LEG OK: fail_closed_vectors$' '^LEG OK: demo_run$' \
            '^LEG OK: python_records$' 'MEASURED ladder [0-9]+ blocks within bars'
        run laya-verify just laya-verify "$MAIN/models/decide/laya-stance-64.apr" "$MAIN/models/decide/laya-stance-64" \
            "$MAIN/data/decide/tweet-stance-64" "{{laya_model_dir}}"
        python3 - "$LOG" "$DEPLOYED_SHA" "$DEPLOYED_SEED" <<'PY' || fail laya-verify "no JSON line with deploy_eligible true, sha256 $DEPLOYED_SHA and shipped_seed $DEPLOYED_SEED" "$LOG"
    import json, sys
    log, sha, seed = sys.argv[1], sys.argv[2], int(sys.argv[3])
    v = json.loads([l for l in open(log) if l.startswith("{")][-1])
    sys.exit(0 if v.get("deploy_eligible") is True and v.get("artifact_sha256") == sha and v.get("shipped_seed") == seed else 1)
    PY
        echo "  ok laya-verify: deploy_eligible true, sha256 $DEPLOYED_SHA, shipped_seed $DEPLOYED_SEED"
        echo "PASS REGRESSION"
    fi
    if [ -n "$ONLY" ]; then
        echo "LAYA GAP STAGES OK ($ONLY; not the full regression)"
    else
        echo "LAYA GAP REGRESSION OK"
    fi

# ---------------------------------------------------------------------------------------------
# Phase 9: internal LLM endpoint (deploy/llm-endpoint)
# ---------------------------------------------------------------------------------------------

# Build the one arm64 image for the internal OpenAI-compatible endpoint (D-13/D-02): the RELEASE
# `apr` named by deploy/llm-endpoint/pin.env plus the Lambda Web Adapter pinned by digest. Refusals,
# in order: tag (APR_TAG shape), sha-release (the release's own .sha256), sha-pin (pin.env's
# APR_TARBALL_SHA256 -- a second, independent check), lwa-digest (the Dockerfile's adapter digest
# literal vs pin.env's LWA_IMAGE_DIGEST; the Dockerfile cannot read pin.env), identity (the built
# image's `apr --version` must carry the pinned tag). No AWS resource is touched unless push=1 AND
# .ids/aws.json exists (written by llm-endpoint-up): then it pushes to the ENV-SPECIFIC repository
# recorded there, never a shared one. LLM_ENDPOINT_DOCKERFILE overrides the Dockerfile (the verify
# feeds it a digest-mutated copy).
llm-endpoint-image push="0" env="dev" variant="":
    #!/usr/bin/env bash
    set -euo pipefail
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    PUSH="{{ trim_start_match(push, "push=") }}"
    ENV="{{ trim_start_match(env, "env=") }}"
    VARIANT="{{ trim_start_match(variant, "variant=") }}"
    DIR=deploy/llm-endpoint
    case "$ENV" in
        dev|prod) ;;
        *) refuse env "$ENV is not a known environment, expected dev or prod" ;;
    esac
    case "$PUSH" in
        variant=*) refuse args "variant= must be the last positional parameter (got it as push)" ;;
    esac
    # the build is shared (one local image per pin tag); the variant picks only where the push goes and records
    . "$DIR/lib/names.sh" || exit 1
    llm_endpoint_names "$VARIANT" "$ENV" || refuse variant "$LLM_ENDPOINT_NAMES_ERR"
    IDS="${LLM_ENDPOINT_IDS_DIR:-$IDS_DEFAULT}"
    BIN="${LLM_ENDPOINT_BIN_DIR:-$DIR/bin}"
    # shasum directly (as laya-deploy): the pin must not depend on which sha tool the PATH offers.
    . "${LLM_ENDPOINT_PIN_FILE:-$DIR/pin.env}"
    DOCKERFILE="${LLM_ENDPOINT_DOCKERFILE:-$DIR/Dockerfile}"
    TAG="$IMAGE_NAME:cpu-$APR_TAG"
    printf '%s' "$APR_TAG" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' \
        || refuse tag "APR_TAG '$APR_TAG' is not vMAJOR.MINOR.PATCH"
    printf '%s' "$APR_TARBALL_SHA256" | grep -Eq '^[0-9a-f]{64}$' \
        || refuse sha-pin "APR_TARBALL_SHA256 in pin.env is not 64 lowercase hex"
    printf '%s' "$LWA_IMAGE_DIGEST" | grep -Eq '^sha256:[0-9a-f]{64}$' \
        || refuse lwa-digest "LWA_IMAGE_DIGEST in pin.env is not sha256:<64 hex>"
    mkdir -p "$BIN" "$IDS"
    TAR="$BIN/$APR_ASSET"
    if [ ! -f "$TAR" ] || [ ! -f "$TAR.sha256" ]; then
        gh release download "$APR_TAG" --repo paiml/aprender --pattern "$APR_ASSET" --pattern "$APR_ASSET.sha256" \
            --dir "$BIN" --clobber > "$IDS/download.log" 2>&1 && rc=0 || rc=$?
        [ "$rc" -eq 0 ] || refuse download "gh release download $APR_TAG failed, rc=$rc, see $IDS/download.log" 3
    fi
    # (1) the release's own checksum file, checked from inside the directory it was written for
    (cd "$BIN" && shasum -a 256 -c "$APR_ASSET.sha256") > "$IDS/sha-release.log" 2>&1 && rc=0 || rc=$?
    [ "$rc" -eq 0 ] || refuse sha-release "$APR_ASSET does not match its release .sha256: $(tail -1 "$IDS/sha-release.log")" 4
    # (2) pin.env's own copy of the hash: a second, independent check
    GOT="$(shasum -a 256 "$TAR" | awk '{print $1}')"
    [ "$GOT" = "$APR_TARBALL_SHA256" ] \
        || refuse sha-pin "$APR_ASSET hashes to $GOT but pin.env APR_TARBALL_SHA256 is $APR_TARBALL_SHA256" 4
    # (3) the adapter digest exists twice (pin.env, Dockerfile FROM line); compare the two copies
    FROMS="$(grep -E '^FROM ' "$DOCKERFILE" || true)"
    case "$FROMS" in
        *"${LWA_IMAGE}@${LWA_IMAGE_DIGEST}"*) ;;
        *) refuse lwa-digest "$DOCKERFILE has no FROM line naming ${LWA_IMAGE}@${LWA_IMAGE_DIGEST} (pin.env and the Dockerfile disagree)" 4 ;;
    esac
    tar xzf "$TAR" -C "$BIN" --strip-components=1 "${APR_ASSET%.tar.gz}/apr"
    chmod 0755 "$BIN/apr"
    docker buildx build --platform linux/arm64 --provenance=false \
        --build-arg APR_TAG="$APR_TAG" --build-arg APR_TARBALL_SHA256="$APR_TARBALL_SHA256" \
        -f "$DOCKERFILE" -t "$TAG" --load "$DIR" > "$IDS/build.log" 2>&1 && rc=0 || rc=$?
    [ "$rc" -eq 0 ] || { tail -15 "$IDS/build.log" >&2; refuse build "docker buildx build failed, rc=$rc, see $IDS/build.log" 5; }
    # prove the identity from the binary inside the image, never from the tag we gave it
    docker run --rm --entrypoint /usr/local/bin/apr "$TAG" --version > "$IDS/identity.log" 2>&1 && rc=0 || rc=$?
    VLINE="$(head -1 "$IDS/identity.log")"
    [ "$rc" -eq 0 ] || refuse identity "the apr --version of the image exited $rc: $VLINE" 6
    case "$VLINE" in
        *"${APR_TAG#v}"*) ;;
        *) refuse identity "the apr --version of the image prints $VLINE, which does not carry ${APR_TAG#v}" 6 ;;
    esac
    IMAGE_FORMAT='{{{{.Id}}'
    docker image inspect --format "$IMAGE_FORMAT" "$TAG" > "$IDS/image-id.txt"
    IMAGE_ID="$(cat "$IDS/image-id.txt")"
    python3 -I -c 'import datetime, json, sys; out, image, tag, image_id, line = sys.argv[1:6]; now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"); json.dump({"image": image, "tag": tag, "image_id": image_id, "apr_version_line": line, "built_at": now}, open(out, "w"), indent=2)' \
        "$IDS/image-local.json" "$TAG" "$APR_TAG" "$IMAGE_ID" "$VLINE"
    if [ "$PUSH" = "1" ]; then
        [ -f "$IDS/aws.json" ] || refuse ids "push=1 needs $IDS/aws.json, written by llm-endpoint-up" 7
        llm_endpoint_check_record "$IDS/aws.json" || refuse ids "$LLM_ENDPOINT_NAMES_ERR" 7
        FIELDS="$(python3 -I -c 'import json, sys; a = json.load(open(sys.argv[1])); print(a["profile"], a["region"], a["ecr_host"], a["ecr_repo"], a["ecr_repo_name"])' "$IDS/aws.json")" \
            || refuse ids "$IDS/aws.json lacks profile/region/ecr_host/ecr_repo/ecr_repo_name" 7
        read -r PROFILE REGION ECR_HOST ECR_REPO ECR_REPO_NAME <<< "$FIELDS"
        PW="$(aws ecr get-login-password --profile "$PROFILE" --region "$REGION")" \
            || refuse push-login "aws ecr get-login-password failed" 8
        printf '%s' "$PW" | docker login --username AWS --password-stdin "$ECR_HOST" > "$IDS/login.log" 2>&1 \
            || refuse push-login "docker login to $ECR_HOST failed (see $IDS/login.log)" 8
        docker tag "$TAG" "$ECR_REPO:cpu-$APR_TAG"
        docker push "$ECR_REPO:cpu-$APR_TAG" > "$IDS/push.log" 2>&1 && rc=0 || rc=$?
        [ "$rc" -eq 0 ] || refuse push "docker push failed, rc=$rc, see $IDS/push.log" 8
        DIGEST="$(aws ecr describe-images --profile "$PROFILE" --region "$REGION" --repository-name "$ECR_REPO_NAME" --image-ids "imageTag=cpu-$APR_TAG" --query 'imageDetails[0].imageDigest' --output text)" \
            || refuse push "aws ecr describe-images failed" 8
        python3 -I -c 'import json, sys; out, repo, tag, digest = sys.argv[1:5]; json.dump({"repo": repo, "tag": tag, "digest": digest}, open(out, "w"), indent=2)' \
            "$IDS/image-pushed.json" "$ECR_REPO" "cpu-$APR_TAG" "$DIGEST"
        echo "PUSHED $ECR_REPO:cpu-$APR_TAG $DIGEST"
    fi
    echo "IMAGE OK $TAG (apr $VLINE)"

# Deploy the endpoint function (D-13): a fail-closed ladder in the laya-deploy shape. ONE mode
# contract, worded identically in the plan, here, in the self-test and in the README:
#   Ten rungs, each printing `REFUSED <check>: ...` before any AWS write. Rungs 1-7 are LOCAL (env,
#   pin, tarball, image, ladder, auth, ids); rungs 8-10 are read-only AWS calls (account = sts,
#   s3-sha = head-object, digest = ecr describe-images).
#   DRY_RUN=1   = OFFLINE static validation: runs rungs 1-7 only, makes ZERO AWS calls of any kind,
#                 and ends `DRY-RUN OK ... (offline rungs 1-7; account, s3-sha and digest NOT checked; nothing deployed)`.
#   PREFLIGHT=1 = CONNECTED read-only preflight: runs rungs 1-10 (only the three read calls) and
#                 ends `PREFLIGHT OK ... (rungs 1-10; read-only AWS calls only; nothing deployed)`.
#   Neither flag = rungs 1-10 then the live deploy. Both flags = `REFUSED mode`.
# Test seams: LLM_ENDPOINT_IDS_DIR (default deploy/llm-endpoint/.ids), LLM_ENDPOINT_PIN_FILE (default
# deploy/llm-endpoint/pin.env), LLM_ENDPOINT_BIN_DIR (default deploy/llm-endpoint/bin). RESERVED_CONCURRENCY
# (an integer) is applied after the deploy and read back. APR_API_KEY_HASH comes from the environment
# and is only ever printed as the word `set`.
llm-endpoint-deploy env="dev" profile="ze-kasher-dev" variant="":
    #!/usr/bin/env bash
    set -euo pipefail
    ENV="{{ trim_start_match(env, "env=") }}"
    PROFILE="{{profile}}"
    VARIANT="{{ trim_start_match(variant, "variant=") }}"
    DIR=deploy/llm-endpoint
    BIN="${LLM_ENDPOINT_BIN_DIR:-$DIR/bin}"
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    # shasum directly, as laya-deploy: the pin must not depend on which sha tool the PATH offers.
    sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
    jget() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2"; }
    has_keys() { python3 -I -c 'import json, sys; d = json.load(open(sys.argv[1])); [d[k] for k in sys.argv[2:]]' "$@" 2> /dev/null; }
    # rung 0: mode
    if [ "${DRY_RUN:-0}" = "1" ] && [ "${PREFLIGHT:-0}" = "1" ]; then
        refuse mode "DRY_RUN=1 (offline, rungs 1-7) and PREFLIGHT=1 (connected, read-only, rungs 1-10) are different modes; set one"
    fi
    # rung 1: env
    case "$ENV" in
        dev|prod) ;;
        *) refuse env "'$ENV' is not a known environment (expected dev or prod)" ;;
    esac
    case "$PROFILE" in
        variant=*) refuse args "variant= must be the last positional parameter (got it as profile)" ;;
    esac
    # every name below comes from the one library; a bad variant is refused before anything is read
    . "$DIR/lib/names.sh" || exit 1
    llm_endpoint_names "$VARIANT" "$ENV" || refuse variant "$LLM_ENDPOINT_NAMES_ERR"
    IDS="${LLM_ENDPOINT_IDS_DIR:-$IDS_DEFAULT}"
    PIN_FILE="${LLM_ENDPOINT_PIN_FILE:-$PIN_DEFAULT}"
    # rung 2: pin
    [ -f "$PIN_FILE" ] || refuse pin "$PIN_FILE does not exist"
    . "$PIN_FILE"
    printf '%s' "${APR_TAG:-}" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' \
        || refuse pin "APR_TAG '${APR_TAG:-}' in $PIN_FILE is not vMAJOR.MINOR.PATCH"
    case "${APR_PIN_PROFILE:-}" in
        baseline|fixed) ;;
        *) refuse pin "APR_PIN_PROFILE '${APR_PIN_PROFILE:-}' in $PIN_FILE is not baseline or fixed" ;;
    esac
    [ -n "${APR_ASSET:-}" ] && [ -n "${GGUF_RUNG:-}" ] && [ -n "${GGUF_OBJECT:-}" ] \
        || refuse pin "$PIN_FILE lacks APR_ASSET, GGUF_RUNG or GGUF_OBJECT"
    # rung 3: tarball
    [ -f "$BIN/$APR_ASSET" ] || refuse tarball "$BIN/$APR_ASSET does not exist -- run: just llm-endpoint-image"
    GOT="$(sha256 "$BIN/$APR_ASSET")"
    [ "$GOT" = "${APR_TARBALL_SHA256:-}" ] \
        || refuse tarball "$APR_ASSET hashes to $GOT but the pin is ${APR_TARBALL_SHA256:-unset}"
    # rung 4: image
    [ -f "$IDS/image-local.json" ] || refuse image "$IDS/image-local.json does not exist -- run: just llm-endpoint-image"
    VLINE="$(jget "$IDS/image-local.json" apr_version_line 2> /dev/null)" \
        || refuse image "$IDS/image-local.json has no apr_version_line"
    case "$VLINE" in
        *"${APR_TAG#v}"*) ;;
        *) refuse image "the local image apr version line $VLINE does not carry ${APR_TAG#v}" ;;
    esac
    # rung 5: ladder (the hash is derived, never typed here)
    GGUF_SHA256="$(python3 -I "$DIR/ladder_pin.py" contracts/model-capability-ladder-v1.yaml "$GGUF_RUNG" 2> "$IDS/ladder.err")" \
        || refuse ladder "ladder_pin.py refused rung $GGUF_RUNG: $(tail -1 "$IDS/ladder.err")"
    printf '%s' "$GGUF_SHA256" | grep -Eq '^[0-9a-f]{64}$' || refuse ladder "the ladder pin for $GGUF_RUNG is not 64 lowercase hex"
    # rung 6: auth (the value is checked and never printed)
    KEYHASH="${APR_API_KEY_HASH:-}"
    KEYLEN="$(printf '%s' "$KEYHASH" | wc -c | tr -d ' ')"
    [ "$KEYLEN" -eq 64 ] || refuse auth "APR_API_KEY_HASH must be set to 64 hex chars, otherwise the server would start OPEN"
    case "$KEYHASH" in
        *[!0-9a-f]*) refuse auth "APR_API_KEY_HASH is 64 chars but not lowercase hex" ;;
    esac
    echo "APR_API_KEY_HASH=set"
    # rung 7: ids
    [ -f "$IDS/aws.json" ] || refuse ids "$IDS/aws.json does not exist -- run: just llm-endpoint-up"
    has_keys "$IDS/aws.json" account region bucket ecr_repo ecr_repo_name role_arn log_group \
        || refuse ids "$IDS/aws.json lacks one of account, region, bucket, ecr_repo, ecr_repo_name, role_arn, log_group"
    llm_endpoint_check_record "$IDS/aws.json" || refuse ids "$LLM_ENDPOINT_NAMES_ERR"
    WANT_ACCOUNT="$(jget "$IDS/aws.json" account)"
    REGION="$(jget "$IDS/aws.json" region)"
    AP=(--profile "$PROFILE" --region "$REGION")
    BUCKET="$(jget "$IDS/aws.json" bucket)"
    ECR_REPO="$(jget "$IDS/aws.json" ecr_repo)"
    ECR_REPO_NAME="$(jget "$IDS/aws.json" ecr_repo_name)"
    ROLE_ARN="$(jget "$IDS/aws.json" role_arn)"
    if [ "${DRY_RUN:-0}" = "1" ]; then
        echo "DRY-RUN OK $FN (offline rungs 1-7; account, s3-sha and digest NOT checked; apr $APR_TAG, gguf $GGUF_SHA256; nothing deployed)"
        exit 0
    fi
    # rung 8: account -- never deploy into another account
    ACCOUNT="$(aws sts get-caller-identity --profile "$PROFILE" --query Account --output text)" \
        || refuse account "cannot read the AWS account for profile $PROFILE"
    [ "$ACCOUNT" = "$WANT_ACCOUNT" ] \
        || refuse account "profile $PROFILE is account $ACCOUNT but $IDS/aws.json was written for $WANT_ACCOUNT"
    # rung 9: s3-sha -- the staged object carries the hash of the ladder pin as metadata
    REMOTE_SHA="$(aws s3api head-object "${AP[@]}" --bucket "$BUCKET" --key "$GGUF_OBJECT" --query Metadata.sha256 --output text)" \
        || refuse s3-sha "s3://$BUCKET/$GGUF_OBJECT is absent -- stage it: just llm-endpoint-weights"
    [ "$REMOTE_SHA" = "$GGUF_SHA256" ] \
        || refuse s3-sha "s3://$BUCKET/$GGUF_OBJECT carries a sha256 that is not the ladder pin for $GGUF_RUNG"
    # rung 10: digest -- the pushed tag still points at what was recorded
    [ -f "$IDS/image-pushed.json" ] || refuse digest "$IDS/image-pushed.json does not exist -- run: just llm-endpoint-image 1"
    has_keys "$IDS/image-pushed.json" repo tag digest || refuse digest "$IDS/image-pushed.json lacks repo, tag or digest"
    PUSHED_REPO="$(jget "$IDS/image-pushed.json" repo)"
    PUSHED_TAG="$(jget "$IDS/image-pushed.json" tag)"
    PUSHED_DIGEST="$(jget "$IDS/image-pushed.json" digest)"
    [ "$PUSHED_TAG" = "cpu-$APR_TAG" ] || refuse digest "the pushed tag is $PUSHED_TAG, the pin is cpu-$APR_TAG"
    [ "$PUSHED_REPO" = "$ECR_REPO" ] \
        || refuse digest "the image was pushed to $PUSHED_REPO but the repository of this environment is $ECR_REPO"
    printf '%s' "$PUSHED_DIGEST" | grep -Eq '^sha256:[0-9a-f]{64}$' || refuse digest "the recorded digest is not sha256:<64 hex>"
    LIVE_DIGEST="$(aws ecr describe-images "${AP[@]}" --repository-name "$ECR_REPO_NAME" --image-ids "imageTag=cpu-$APR_TAG" --query 'imageDetails[0].imageDigest' --output text)" \
        || refuse digest "cannot read tag cpu-$APR_TAG in $ECR_REPO_NAME"
    [ "$LIVE_DIGEST" = "$PUSHED_DIGEST" ] \
        || refuse digest "tag cpu-$APR_TAG now resolves to $LIVE_DIGEST, not the recorded $PUSHED_DIGEST"
    if [ "${PREFLIGHT:-0}" = "1" ]; then
        echo "PREFLIGHT OK $FN (rungs 1-10; read-only AWS calls only; apr $APR_TAG; nothing deployed)"
        exit 0
    fi
    # ---- live ----
    TAGS="project=aprender-llm-endpoint,phase=09,env=$ENV,$TAG_VARIANT"
    # step <what> <cmd...>: run, keep the log, refuse on failure (the status is read after a redirect, never a pipe)
    step() {
        local what="$1" rc
        shift
        set +e
        "$@" > "$IDS/step.log" 2>&1
        rc=$?
        set -e
        if [ "$rc" -ne 0 ]; then
            tail -5 "$IDS/step.log" >&2
            refuse "$what" "failed (rc=$rc)" 9
        fi
    }
    ENVF="$(mktemp)"
    trap 'rm -f "$ENVF"' EXIT
    chmod 0600 "$ENVF"
    python3 -I -c 'import json, os, sys; json.dump({"Variables": {"MODEL_S3_URI": sys.argv[2], "MODEL_SHA256": sys.argv[3], "APR_API_KEY_HASH": os.environ["APR_API_KEY_HASH"], "RAYON_NUM_THREADS": "6", "PLATFORM": "lambda"}}, open(sys.argv[1], "w"))' \
        "$ENVF" "s3://$BUCKET/$GGUF_OBJECT" "$GGUF_SHA256"
    set +e
    aws lambda get-function --profile "$PROFILE" --region "$REGION" --function-name "$FN" > "$IDS/get-function.log" 2>&1
    grc=$?
    set -e
    if [ "$grc" -eq 0 ]; then
        echo "UPDATING $FN"
        step update-config aws lambda update-function-configuration --profile "$PROFILE" --region "$REGION" --function-name "$FN" \
            --memory-size 10240 --timeout 900 --ephemeral-storage Size=10240 --environment "file://$ENVF"
        step wait-config aws lambda wait function-updated-v2 --profile "$PROFILE" --region "$REGION" --function-name "$FN"
        step update-code aws lambda update-function-code --profile "$PROFILE" --region "$REGION" --function-name "$FN" \
            --image-uri "$ECR_REPO@$PUSHED_DIGEST"
        step wait-code aws lambda wait function-updated-v2 --profile "$PROFILE" --region "$REGION" --function-name "$FN"
    elif grep -q 'ResourceNotFoundException' "$IDS/get-function.log"; then
        echo "CREATING $FN"
        step create aws lambda create-function --profile "$PROFILE" --region "$REGION" --function-name "$FN" \
            --package-type Image --code "ImageUri=$ECR_REPO@$PUSHED_DIGEST" --role "$ROLE_ARN" \
            --architectures arm64 --memory-size 10240 --timeout 900 --ephemeral-storage Size=10240 \
            --environment "file://$ENVF" --tags "$TAGS"
        step wait-active aws lambda wait function-active-v2 --profile "$PROFILE" --region "$REGION" --function-name "$FN"
        step wait-updated aws lambda wait function-updated-v2 --profile "$PROFILE" --region "$REGION" --function-name "$FN"
    else
        tail -3 "$IDS/get-function.log" >&2
        refuse lambda "aws lambda get-function failed for a reason other than the function being absent (rc=$grc)" 9
    fi
    if [ -n "${RESERVED_CONCURRENCY:-}" ]; then
        case "$RESERVED_CONCURRENCY" in
            ''|*[!0-9]*) refuse concurrency "RESERVED_CONCURRENCY=$RESERVED_CONCURRENCY is not an integer" ;;
        esac
        step reserve aws lambda put-function-concurrency --profile "$PROFILE" --region "$REGION" --function-name "$FN" \
            --reserved-concurrent-executions "$RESERVED_CONCURRENCY"
    fi
    GOT_RC="$(aws lambda get-function-concurrency "${AP[@]}" --function-name "$FN" --query ReservedConcurrentExecutions --output text)" \
        || refuse readback "cannot read the reserved concurrency of $FN" 9
    # readback: what Lambda actually holds, not what we asked for
    CFG="$(aws lambda get-function-configuration "${AP[@]}" --function-name "$FN" --query '[State, MemorySize, Timeout, EphemeralStorage.Size, Architectures[0]]' --output text)" \
        || refuse readback "cannot read the configuration of $FN" 9
    ENV_KEYS="$(aws lambda get-function-configuration "${AP[@]}" --function-name "$FN" --query 'keys(Environment.Variables)' --output text)" \
        || refuse readback "cannot read the environment keys of $FN" 9
    read -r CSTATE CMEM CTIMEOUT CEPH CARCH <<< "$CFG"
    [ "$CMEM" = "10240" ] && [ "$CTIMEOUT" = "900" ] && [ "$CEPH" = "10240" ] && [ "$CARCH" = "arm64" ] \
        || refuse readback "$FN reads memory=$CMEM timeout=$CTIMEOUT ephemeral=$CEPH arch=$CARCH (want 10240 / 900 / 10240 / arm64)" 9
    echo "  readback: state=$CSTATE memory=$CMEM timeout=$CTIMEOUT ephemeral=$CEPH arch=$CARCH env keys: $ENV_KEYS"
    FN_ARN="arn:aws:lambda:$REGION:$ACCOUNT:function:$FN"
    python3 -I -c 'import datetime, json, sys; out, fn, arn, env, variant, suffix, digest, conc, tag, gguf = sys.argv[1:11]; now = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"); rec = {"function": fn, "arn": arn, "env": env, "variant": variant, "suffix": suffix, "image_digest": digest, "memory_mb": 10240, "timeout_s": 900, "ephemeral_mb": 10240, "reserved_concurrency": int(conc) if conc.isdigit() else None, "deployed_at": now, "apr_tag": tag, "gguf_sha256": gguf}; json.dump(rec, open(out, "w"), indent=2)' \
        "$IDS/function.json" "$FN" "$FN_ARN" "$ENV" "$VARIANT" "$SUFFIX" "$PUSHED_DIGEST" "$GOT_RC" "$APR_TAG" "$GGUF_SHA256"
    echo "DEPLOY OK $FN (image $PUSHED_DIGEST, apr $APR_TAG)"

# Create the environment-scoped resources (idempotent; D-13): role, log group, weights bucket and the
# ECR repository aprender-llm-endpoint-<env>. No repository is shared across environments, so tearing
# one environment down cannot touch the images of another. Writes $IDS/aws.json (gitignored) and
# prints UP OK. LLM_ENDPOINT_IDS_DIR overrides the ids directory (the self-test seam).
llm-endpoint-up env="dev" profile="ze-kasher-dev" variant="":
    #!/usr/bin/env bash
    set -euo pipefail
    ENV="{{ trim_start_match(env, "env=") }}"
    PROFILE="{{profile}}"
    VARIANT="{{ trim_start_match(variant, "variant=") }}"
    REGION=us-east-1
    DIR=deploy/llm-endpoint
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    case "$ENV" in
        dev|prod) ;;
        *) refuse env "$ENV is not a known environment, expected dev or prod" ;;
    esac
    case "$PROFILE" in
        variant=*) refuse args "variant= must be the last positional parameter (got it as profile)" ;;
    esac
    . "$DIR/lib/names.sh" || exit 1
    llm_endpoint_names "$VARIANT" "$ENV" || refuse variant "$LLM_ENDPOINT_NAMES_ERR"
    IDS="${LLM_ENDPOINT_IDS_DIR:-$IDS_DEFAULT}"
    mkdir -p "$IDS"
    # tolerate <pattern> <what> <cmd...>: success, or a failure whose output names the exact exception
    tolerate() {
        local pat="$1" what="$2" rc
        shift 2
        set +e
        "$@" > "$IDS/step.log" 2>&1
        rc=$?
        set -e
        if [ "$rc" -eq 0 ]; then
            return 0
        fi
        if grep -q "$pat" "$IDS/step.log"; then
            echo "  already in place, $pat: $what"
            return 0
        fi
        tail -5 "$IDS/step.log" >&2
        refuse "$what" "failed, rc=$rc" 9
    }
    ACCOUNT="$(aws sts get-caller-identity --profile "$PROFILE" --query Account --output text)" \
        || refuse account "cannot read the AWS account for profile $PROFILE"
    llm_endpoint_names "$VARIANT" "$ENV" "$ACCOUNT" || refuse variant "$LLM_ENDPOINT_NAMES_ERR"
    # a record already in the ids dir must be this function's own (legacy serves only the default variant)
    if [ -f "$IDS/aws.json" ]; then
        llm_endpoint_check_record "$IDS/aws.json" || refuse ids "$LLM_ENDPOINT_NAMES_ERR"
    fi
    NAME="$FN"
    ECR_HOST="$ACCOUNT.dkr.ecr.$REGION.amazonaws.com"
    ECR_REPO="$ECR_HOST/$REPO_NAME"
    AP=(--profile "$PROFILE" --region "$REGION")
    # bucket: private, encrypted, tagged
    if aws s3api head-bucket "${AP[@]}" --bucket "$BUCKET" > /dev/null 2>&1; then
        echo "  exists:  s3://$BUCKET"
    else
        aws s3api create-bucket "${AP[@]}" --bucket "$BUCKET" > /dev/null
        echo "  created: s3://$BUCKET"
    fi
    aws s3api put-public-access-block "${AP[@]}" --bucket "$BUCKET" \
        --public-access-block-configuration BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true
    aws s3api put-bucket-encryption "${AP[@]}" --bucket "$BUCKET" \
        --server-side-encryption-configuration '{"Rules":[{"ApplyServerSideEncryptionByDefault":{"SSEAlgorithm":"AES256"}}]}'
    aws s3api put-bucket-tagging "${AP[@]}" --bucket "$BUCKET" \
        --tagging "TagSet=[{Key=project,Value=aprender-llm-endpoint},{Key=phase,Value=09},{Key=env,Value=$ENV},{Key=variant,Value=$VARIANT}]"
    # ECR repository, one per environment
    if aws ecr describe-repositories "${AP[@]}" --repository-names "$REPO_NAME" > /dev/null 2>&1; then
        echo "  exists:  ECR $REPO_NAME"
    else
        aws ecr create-repository "${AP[@]}" --repository-name "$REPO_NAME" \
            --tags Key=project,Value=aprender-llm-endpoint Key=phase,Value=09 Key=env,Value="$ENV" Key=variant,Value="$VARIANT" > /dev/null
        echo "  created: ECR $REPO_NAME"
    fi
    # log group, 7 days
    tolerate ResourceAlreadyExistsException log-group aws logs create-log-group "${AP[@]}" --log-group-name "$LOG_GROUP" \
        --tags "project=aprender-llm-endpoint,phase=09,env=$ENV,$TAG_VARIANT"
    aws logs put-retention-policy "${AP[@]}" --cli-input-json "{\"logGroupName\":\"$LOG_GROUP\",\"retentionInDays\":7}"
    # role: trust from a file, the managed basic-execution policy, the inline S3 read from a file
    CREATED_ROLE=0
    if aws iam get-role --profile "$PROFILE" --role-name "$NAME" > /dev/null 2>&1; then
        echo "  exists:  role $NAME"
    else
        aws iam create-role --profile "$PROFILE" --role-name "$NAME" \
            --assume-role-policy-document "file://$DIR/iam/lambda-trust.json" \
            --tags Key=project,Value=aprender-llm-endpoint Key=phase,Value=09 Key=env,Value="$ENV" Key=variant,Value="$VARIANT" > /dev/null
        CREATED_ROLE=1
        echo "  created: role $NAME"
    fi
    aws iam attach-role-policy --profile "$PROFILE" --role-name "$NAME" \
        --policy-arn arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole
    POL="$(mktemp)"
    trap 'rm -f "${POL:?}"' EXIT
    sed "s/__BUCKET__/$BUCKET/g" "$DIR/iam/lambda-s3-read.json" > "$POL"
    aws iam put-role-policy --profile "$PROFILE" --role-name "$NAME" --policy-name aprender-llm-endpoint-s3-read \
        --policy-document "file://$POL"
    ROLE_ARN="$(aws iam get-role --profile "$PROFILE" --role-name "$NAME" --query Role.Arn --output text)" \
        || refuse role "cannot read the ARN of role $NAME"
    if [ "$CREATED_ROLE" = "1" ]; then
        echo "  waiting 10 s for IAM propagation of the new role"
        sleep 10
    fi
    python3 -I -c 'import datetime, json, sys; keys = ["account", "region", "profile", "env", "bucket", "ecr_repo", "ecr_repo_name", "ecr_host", "role_arn", "log_group", "name", "variant", "suffix"]; rec = dict(zip(keys, sys.argv[2:15])); rec["created_at"] = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"); json.dump(rec, open(sys.argv[1], "w"), indent=2)' \
        "$IDS/aws.json" "$ACCOUNT" "$REGION" "$PROFILE" "$ENV" "$BUCKET" "$ECR_REPO" "$REPO_NAME" "$ECR_HOST" "$ROLE_ARN" "$LOG_GROUP" "$FN" "$VARIANT" "$SUFFIX"
    echo "UP OK $NAME (account $ACCOUNT, bucket $BUCKET, repository $REPO_NAME)"

# Stage the ladder-pinned GGUF into the endpoint bucket and verify its Metadata.sha256 against the
# ladder (D-13). mode=copy: server-side copy of the spike-031 object (metadata carried by the COPY
# directive); refuses `source-meta` when the source has no matching hash, then use mode=codebuild.
# mode=codebuild: a CodeBuild ARM job downloads the file from Hugging Face, checks its sha256 and
# uploads it with the hash as metadata (never crosses the operator uplink). Refusals: mode, env, pin,
# ladder, ids, account, source-meta, codebuild, s3-sha.
llm-endpoint-weights mode="copy" env="dev" profile="ze-kasher-dev" variant="":
    #!/usr/bin/env bash
    set -euo pipefail
    MODE="{{ trim_start_match(mode, "mode=") }}"
    ENV="{{ trim_start_match(env, "env=") }}"
    PROFILE="{{profile}}"
    DIR=deploy/llm-endpoint
    VARIANT="{{ trim_start_match(variant, "variant=") }}"
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    jget() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2"; }
    case "$MODE" in
        copy|codebuild) ;;
        *) refuse mode "$MODE is not copy or codebuild" ;;
    esac
    case "$ENV" in
        dev|prod) ;;
        *) refuse env "$ENV is not a known environment, expected dev or prod" ;;
    esac
    case "$MODE" in
        variant=*) refuse args "variant= must be the last positional parameter (got it as mode)" ;;
    esac
    case "$PROFILE" in
        variant=*) refuse args "variant= must be the last positional parameter (got it as profile)" ;;
    esac
    # every name below comes from the one library; a bad variant is refused before anything is read
    . "$DIR/lib/names.sh" || exit 1
    llm_endpoint_names "$VARIANT" "$ENV" || refuse variant "$LLM_ENDPOINT_NAMES_ERR"
    IDS="${LLM_ENDPOINT_IDS_DIR:-$IDS_DEFAULT}"
    PIN_FILE="${LLM_ENDPOINT_PIN_FILE:-$PIN_DEFAULT}"
    [ -f "$PIN_FILE" ] || refuse pin "$PIN_FILE does not exist"
    . "$PIN_FILE"
    [ -n "${GGUF_RUNG:-}" ] && [ -n "${GGUF_OBJECT:-}" ] || refuse pin "$PIN_FILE lacks GGUF_RUNG or GGUF_OBJECT"
    mkdir -p "$IDS"
    GGUF_SHA256="$(python3 -I "$DIR/ladder_pin.py" contracts/model-capability-ladder-v1.yaml "$GGUF_RUNG" 2> "$IDS/ladder.err")" \
        || refuse ladder "ladder_pin.py refused rung $GGUF_RUNG: $(tail -1 "$IDS/ladder.err")"
    [ -f "$IDS/aws.json" ] || refuse ids "$IDS/aws.json does not exist, run: just llm-endpoint-up"
    llm_endpoint_check_record "$IDS/aws.json" || refuse ids "$LLM_ENDPOINT_NAMES_ERR"
    WANT_ACCOUNT="$(jget "$IDS/aws.json" account)" || refuse ids "$IDS/aws.json lacks account"
    REGION="$(jget "$IDS/aws.json" region)" || refuse ids "$IDS/aws.json lacks region"
    BUCKET="$(jget "$IDS/aws.json" bucket)" || refuse ids "$IDS/aws.json lacks bucket"
    AP=(--profile "$PROFILE" --region "$REGION")
    ACCOUNT="$(aws sts get-caller-identity --profile "$PROFILE" --query Account --output text)" \
        || refuse account "cannot read the AWS account for profile $PROFILE"
    [ "$ACCOUNT" = "$WANT_ACCOUNT" ] || refuse account "profile $PROFILE is account $ACCOUNT but $IDS/aws.json was written for $WANT_ACCOUNT"
    # object_sha <bucket>: the Metadata.sha256 of the GGUF object, or empty when absent
    object_sha() {
        aws s3api head-object "${AP[@]}" --bucket "$1" --key "$GGUF_OBJECT" --query Metadata.sha256 --output text 2> /dev/null || true
    }
    # step <what> <cmd...>: run, keep the log, refuse on failure; the status is read after a redirect
    step() {
        local what="$1" rc
        shift
        set +e
        "$@" > "$IDS/step.log" 2>&1
        rc=$?
        set -e
        if [ "$rc" -ne 0 ]; then
            tail -5 "$IDS/step.log" >&2
            refuse "$what" "failed, rc=$rc" 9
        fi
    }
    verify_object() {
        local got
        got="$(object_sha "$BUCKET")"
        [ "$got" = "$GGUF_SHA256" ] || refuse s3-sha "s3://$BUCKET/$GGUF_OBJECT does not carry the ladder pin for $GGUF_RUNG as Metadata.sha256"
        python3 -I -c 'import json, sys; json.dump({"mode": sys.argv[2], "bucket": sys.argv[3], "key": sys.argv[4], "codebuild_project": sys.argv[5], "codebuild_role": sys.argv[6]}, open(sys.argv[1], "w"), indent=2)' \
            "$IDS/weights.json" "$MODE" "$BUCKET" "$GGUF_OBJECT" "${CB_PROJECT:-}" "${CB_ROLE:-}"
        echo "WEIGHTS OK s3://$BUCKET/$GGUF_OBJECT sha256 $GGUF_SHA256"
    }
    # the library's names are used only by mode=codebuild; copy mode records none
    LIB_CB_PROJECT="$CB_PROJECT"
    LIB_CB_ROLE="$CB_ROLE"
    CB_PROJECT=""
    CB_ROLE=""
    if [ "$(object_sha "$BUCKET")" = "$GGUF_SHA256" ] && [ "$MODE" = "copy" ]; then
        echo "  already staged: s3://$BUCKET/$GGUF_OBJECT carries the ladder pin"
        verify_object
        exit 0
    fi
    if [ "$MODE" = "copy" ]; then
        SRC="llm-spike-031-$ACCOUNT"
        SRC_SHA="$(object_sha "$SRC")"
        [ "$SRC_SHA" = "$GGUF_SHA256" ] \
            || refuse source-meta "s3://$SRC/$GGUF_OBJECT is absent or carries no matching sha256 metadata, use mode=codebuild"
        # s3api copy-object (a 532 MB object is far under its 5 GB limit): `aws s3 cp` between buckets
        # dropped the sha256 metadata on the first live run, and verify_object then refused s3-sha.
        step copy aws s3api copy-object "${AP[@]}" --copy-source "$SRC/$GGUF_OBJECT" --bucket "$BUCKET" --key "$GGUF_OBJECT" \
            --metadata-directive COPY --server-side-encryption AES256
        verify_object
        exit 0
    fi
    # mode=codebuild
    CB_ROLE="$LIB_CB_ROLE"
    CB_PROJECT="$LIB_CB_PROJECT"
    CREATED_ROLE=0
    if aws iam get-role --profile "$PROFILE" --role-name "$CB_ROLE" > /dev/null 2>&1; then
        echo "  exists:  role $CB_ROLE"
    else
        step cb-role aws iam create-role --profile "$PROFILE" --role-name "$CB_ROLE" \
            --assume-role-policy-document "file://$DIR/iam/codebuild-trust.json" \
            --tags Key=project,Value=aprender-llm-endpoint Key=phase,Value=09 Key=env,Value="$ENV" Key=variant,Value="$VARIANT"
        CREATED_ROLE=1
    fi
    POL="$(mktemp)"
    PROJ_JSON="$(mktemp)"
    trap 'rm -f "${POL:?}" "${PROJ_JSON:?}"' EXIT
    sed "s/__BUCKET__/$BUCKET/g" "$DIR/iam/codebuild-s3-logs.json" > "$POL"
    step cb-policy aws iam put-role-policy --profile "$PROFILE" --role-name "$CB_ROLE" --policy-name aprender-llm-endpoint-s3-logs \
        --policy-document "file://$POL"
    if [ "$CREATED_ROLE" = "1" ]; then
        echo "  waiting 10 s for IAM propagation of the new role"
        sleep 10
    fi
    CB_ROLE_ARN="$(aws iam get-role --profile "$PROFILE" --role-name "$CB_ROLE" --query Role.Arn --output text)" \
        || refuse codebuild "cannot read the ARN of role $CB_ROLE"
    # the project JSON is generated by python so the multi-line buildspec survives quoting intact
    python3 -I -c 'import json, sys; spec = open(sys.argv[2]).read(); env = [{"name": "BUCKET", "value": sys.argv[5]}, {"name": "GGUF_SHA256", "value": sys.argv[6]}]; proj = {"name": sys.argv[3], "serviceRole": sys.argv[4], "tags": [{"key": "project", "value": "aprender-llm-endpoint"}, {"key": "phase", "value": "09"}, {"key": "variant", "value": sys.argv[7]}], "source": {"type": "NO_SOURCE", "buildspec": spec}, "artifacts": {"type": "NO_ARTIFACTS"}, "environment": {"type": "ARM_CONTAINER", "image": "aws/codebuild/amazonlinux-aarch64-standard:3.0", "computeType": "BUILD_GENERAL1_SMALL", "environmentVariables": env}, "timeoutInMinutes": 30}; json.dump(proj, open(sys.argv[1], "w"))' \
        "$PROJ_JSON" "$DIR/buildspec-weights.yaml" "$CB_PROJECT" "$CB_ROLE_ARN" "$BUCKET" "$GGUF_SHA256" "$VARIANT"
    EXISTING="$(aws codebuild batch-get-projects "${AP[@]}" --names "$CB_PROJECT" --query 'projects[0].name' --output text 2> /dev/null || true)"
    if [ "$EXISTING" = "$CB_PROJECT" ]; then
        step cb-project aws codebuild update-project "${AP[@]}" --cli-input-json "file://$PROJ_JSON"
    else
        step cb-project aws codebuild create-project "${AP[@]}" --cli-input-json "file://$PROJ_JSON"
    fi
    BUILD_ID="$(aws codebuild start-build "${AP[@]}" --project-name "$CB_PROJECT" --query build.id --output text)" \
        || refuse codebuild "start-build failed for $CB_PROJECT"
    echo "  build $BUILD_ID started"
    STATUS=IN_PROGRESS
    for _ in $(seq 1 180); do
        STATUS="$(aws codebuild batch-get-builds "${AP[@]}" --ids "$BUILD_ID" --query 'builds[0].buildStatus' --output text)"
        if [ "$STATUS" != "IN_PROGRESS" ]; then
            break
        fi
        sleep 10
    done
    echo "  build status: $STATUS"
    GROUP_STREAM="$(aws codebuild batch-get-builds "${AP[@]}" --ids "$BUILD_ID" --query 'builds[0].logs.[groupName,streamName]' --output text 2> /dev/null || true)"
    read -r CB_GROUP CB_STREAM <<< "$GROUP_STREAM"
    if [ -n "${CB_GROUP:-}" ] && [ -n "${CB_STREAM:-}" ]; then
        aws logs get-log-events "${AP[@]}" --log-group-name "$CB_GROUP" --log-stream-name "$CB_STREAM" --start-from-head \
            --query 'events[].message' --output text > "$IDS/codebuild.log" 2> /dev/null || true
        grep -E 'DOWNLOAD|SHA|UPLOAD|MISMATCH' "$IDS/codebuild.log" | head -20 || true
    fi
    [ "$STATUS" = "SUCCEEDED" ] || refuse codebuild "the build finished as $STATUS, see $IDS/codebuild.log" 9
    verify_object

# Prove the DEPLOYED function enforces the key (D-12): invoke it through an API Gateway v2 style event
# with NO authorization header and record the status code in $IDS/prove-401.json. Exit 0 only on 401;
# any other status exits 3 and no URL may be created. The first call is a cold start, about 40 s.
llm-endpoint-prove-401 env="dev" profile="ze-kasher-dev" variant="":
    #!/usr/bin/env bash
    set -euo pipefail
    ENV="{{ trim_start_match(env, "env=") }}"
    PROFILE="{{profile}}"
    DIR=deploy/llm-endpoint
    VARIANT="{{ trim_start_match(variant, "variant=") }}"
    EVENT="$DIR/events/apigw-v2-no-auth.json"
    # the AWS CLI spelling of the raw payload format (raw, in, base64, out) is built in two pieces so the bashrs lexer does not read the word in as a keyword
    BINFMT="raw-i""n-base64-out"
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    jget() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2"; }
    case "$ENV" in
        dev|prod) ;;
        *) refuse env "$ENV is not a known environment, expected dev or prod" ;;
    esac
    case "$PROFILE" in
        variant=*) refuse args "variant= must be the last positional parameter (got it as profile)" ;;
    esac
    # every name below comes from the one library; a bad variant is refused before anything is read
    . "$DIR/lib/names.sh" || exit 1
    llm_endpoint_names "$VARIANT" "$ENV" || refuse variant "$LLM_ENDPOINT_NAMES_ERR"
    IDS="${LLM_ENDPOINT_IDS_DIR:-$IDS_DEFAULT}"
    [ -f "$IDS/function.json" ] || refuse function "$IDS/function.json does not exist, run: just llm-endpoint-deploy"
    llm_endpoint_check_record "$IDS/function.json" || refuse ids "$LLM_ENDPOINT_NAMES_ERR"
    [ -f "$EVENT" ] || refuse event "$EVENT does not exist"
    FN="$(jget "$IDS/function.json" function)" || refuse function "$IDS/function.json lacks function"
    REGION=us-east-1
    if [ -f "$IDS/aws.json" ]; then
        REGION="$(jget "$IDS/aws.json" region)" || refuse ids "$IDS/aws.json lacks region"
    fi
    AP=(--profile "$PROFILE" --region "$REGION")
    RESOLVED="$(aws lambda get-function "${AP[@]}" --function-name "$FN" --query Code.ResolvedImageUri --output text)" \
        || refuse function "cannot read the resolved image of $FN"
    DIGEST="${RESOLVED##*@}"
    OUT="$IDS/prove-401.out.json"
    rm -f "$OUT"
    set +e
    aws lambda invoke "${AP[@]}" --function-name "$FN" --cli-binary-format "$BINFMT" --cli-read-timeout 900 \
        --payload "file://$EVENT" --log-type Tail --query LogResult --output text "$OUT" > "$IDS/prove-401.invoke.log" 2>&1
    rc=$?
    set -e
    STATUS=none
    if [ "$rc" -eq 0 ] && [ -f "$OUT" ]; then
        STATUS="$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1])).get("statusCode", "none"))' "$OUT" 2> /dev/null || echo none)"
    fi
    EVENT_SHA="$(shasum -a 256 "$EVENT" | awk '{print $1}')"
    python3 -I -c 'import datetime, json, sys; out, fn, digest, status, sha = sys.argv[1:6]; rec = {"function": fn, "image_digest": digest, "statusCode": int(status) if status.isdigit() else status, "measured_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"), "event_sha256": sha}; json.dump(rec, open(out, "w"), indent=2)' \
        "$IDS/prove-401.json" "$FN" "$DIGEST" "$STATUS" "$EVENT_SHA"
    if [ "$STATUS" = "401" ]; then
        echo "401 PROVEN on $FN (image $DIGEST)"
        exit 0
    fi
    echo "401 NOT PROVEN: statusCode=$STATUS, invoke rc=$rc. The deployed pin does not enforce the key; no URL may be created (D-12)" >&2
    exit 3

# Create the public Function URL (AuthType NONE, BUFFERED), D-12. Refuses `proof` unless
# $IDS/prove-401.json records 401 for the CURRENTLY deployed image digest within 24 h, and refuses
# `concurrency` unless the function has bounded reserved concurrency 1-5 (ACCEPT_UNRESERVED_URL=1
# overrides and records risk_accepted). Creates the URL and BOTH permission statements, immediately
# re-checks the URL WITHOUT a key (POST and GET /health must both answer 401 within 180 s) and removes
# the URL and both statements otherwise. The URL is written only to the gitignored $IDS/url.json; only
# its host is printed.
llm-endpoint-url env="dev" profile="ze-kasher-dev" variant="":
    #!/usr/bin/env bash
    set -euo pipefail
    ENV="{{ trim_start_match(env, "env=") }}"
    PROFILE="{{profile}}"
    DIR=deploy/llm-endpoint
    VARIANT="{{ trim_start_match(variant, "variant=") }}"
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    jget() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2"; }
    case "$ENV" in
        dev|prod) ;;
        *) refuse env "$ENV is not a known environment, expected dev or prod" ;;
    esac
    case "$PROFILE" in
        variant=*) refuse args "variant= must be the last positional parameter (got it as profile)" ;;
    esac
    # every name below comes from the one library; a bad variant is refused before anything is read
    . "$DIR/lib/names.sh" || exit 1
    llm_endpoint_names "$VARIANT" "$ENV" || refuse variant "$LLM_ENDPOINT_NAMES_ERR"
    IDS="${LLM_ENDPOINT_IDS_DIR:-$IDS_DEFAULT}"
    # the proof, from local files only: nothing below reaches AWS unless it is fresh and says 401
    [ -f "$IDS/function.json" ] || refuse proof "$IDS/function.json does not exist, run: just llm-endpoint-deploy"
    [ -f "$IDS/prove-401.json" ] || refuse proof "$IDS/prove-401.json does not exist, run: just llm-endpoint-prove-401"
    llm_endpoint_check_record "$IDS/function.json" || refuse ids "$LLM_ENDPOINT_NAMES_ERR"
    FN="$(jget "$IDS/function.json" function)" || refuse proof "$IDS/function.json lacks function"
    PROOF="$(python3 -I -c 'import datetime, json, sys; p = json.load(open(sys.argv[1])); t = datetime.datetime.strptime(p["measured_at"], "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=datetime.timezone.utc); print(p["statusCode"], p["image_digest"], int((datetime.datetime.now(datetime.timezone.utc) - t).total_seconds()))' "$IDS/prove-401.json" 2> /dev/null)" \
        || refuse proof "$IDS/prove-401.json is not a valid proof record"
    read -r PSTATUS PDIGEST PAGE <<< "$PROOF"
    [ "$PSTATUS" = "401" ] || refuse proof "the recorded statusCode is $PSTATUS, not 401"
    [ "$PAGE" -le 86400 ] || refuse proof "the 401 proof is $PAGE s old, older than 24 h: run just llm-endpoint-prove-401 again"
    REGION=us-east-1
    WANT_ACCOUNT=""
    if [ -f "$IDS/aws.json" ]; then
        REGION="$(jget "$IDS/aws.json" region)" || refuse ids "$IDS/aws.json lacks region"
        WANT_ACCOUNT="$(jget "$IDS/aws.json" account)" || refuse ids "$IDS/aws.json lacks account"
    fi
    AP=(--profile "$PROFILE" --region "$REGION")
    ACCOUNT="$(aws sts get-caller-identity --profile "$PROFILE" --query Account --output text)" \
        || refuse account "cannot read the AWS account for profile $PROFILE"
    if [ -n "$WANT_ACCOUNT" ] && [ "$ACCOUNT" != "$WANT_ACCOUNT" ]; then
        refuse account "profile $PROFILE is account $ACCOUNT but $IDS/aws.json was written for $WANT_ACCOUNT"
    fi
    RESOLVED="$(aws lambda get-function "${AP[@]}" --function-name "$FN" --query Code.ResolvedImageUri --output text)" \
        || refuse proof "cannot read the resolved image of $FN"
    [ "${RESOLVED##*@}" = "$PDIGEST" ] \
        || refuse proof "the 401 was measured on image $PDIGEST but $FN now runs ${RESOLVED##*@}: run just llm-endpoint-prove-401 again"
    # the concurrency gate: a NONE URL on an unbounded function lets anyone who learns the address buy cold starts
    CONC="$(aws lambda get-function-concurrency "${AP[@]}" --function-name "$FN" --query ReservedConcurrentExecutions --output text)" \
        || refuse concurrency "cannot read the reserved concurrency of $FN"
    RISK=false
    case "$CONC" in
        1|2|3|4|5) ;;
        *)
            if [ "${ACCEPT_UNRESERVED_URL:-0}" = "1" ]; then
                echo "WARNING: reserved concurrency of $FN is $CONC, not 1-5; ACCEPT_UNRESERVED_URL=1 accepts the cost-amplification risk" >&2
                RISK=true
            else
                refuse concurrency "reserved concurrency of $FN is $CONC. A NONE URL on an unbounded function lets anyone who learns the address buy cold starts: set RESERVED_CONCURRENCY=1 at deploy or run aws lambda put-function-concurrency with 1"
            fi
            ;;
    esac
    tolerate() {
        local pat="$1" what="$2" rc
        shift 2
        set +e
        "$@" > "$IDS/step.log" 2>&1
        rc=$?
        set -e
        if [ "$rc" -eq 0 ]; then
            return 0
        fi
        if grep -q "$pat" "$IDS/step.log"; then
            echo "  already in place, $pat: $what"
            return 0
        fi
        tail -5 "$IDS/step.log" >&2
        return "$rc"
    }
    remove_url() {
        tolerate ResourceNotFoundException delete-url aws lambda delete-function-url-config "${AP[@]}" --function-name "$FN" \
            || echo "ERROR: the URL config of $FN was not removed: run just llm-endpoint-teardown" >&2
        tolerate ResourceNotFoundException remove-public aws lambda remove-permission "${AP[@]}" --function-name "$FN" --statement-id llm-endpoint-url-public \
            || echo "ERROR: statement llm-endpoint-url-public was not removed: run just llm-endpoint-teardown" >&2
        tolerate ResourceNotFoundException remove-invoke aws lambda remove-permission "${AP[@]}" --function-name "$FN" --statement-id llm-endpoint-invoke-via-url \
            || echo "ERROR: statement llm-endpoint-invoke-via-url was not removed: run just llm-endpoint-teardown" >&2
    }
    set +e
    URL="$(aws lambda create-function-url-config "${AP[@]}" --function-name "$FN" --auth-type NONE --invoke-mode BUFFERED --query FunctionUrl --output text 2> "$IDS/step.log")"
    crc=$?
    set -e
    if [ "$crc" -ne 0 ]; then
        if grep -q 'ResourceConflictException' "$IDS/step.log"; then
            URL="$(aws lambda get-function-url-config "${AP[@]}" --function-name "$FN" --query FunctionUrl --output text)" \
                || refuse url-create "the URL config of $FN exists but cannot be read"
        else
            tail -5 "$IDS/step.log" >&2
            refuse url-create "create-function-url-config failed for $FN, rc=$crc" 9
        fi
    fi
    URL="${URL%/}"
    HOST="${URL#https://}"
    HOST="${HOST%%/*}"
    # NONE needs BOTH statements (the rule since October 2025)
    if ! tolerate ResourceConflictException add-public aws lambda add-permission "${AP[@]}" --function-name "$FN" \
        --statement-id llm-endpoint-url-public --action lambda:InvokeFunctionUrl --principal '*' --function-url-auth-type NONE; then
        remove_url
        refuse permission "add-permission llm-endpoint-url-public failed, the URL was removed" 9
    fi
    if ! tolerate ResourceConflictException add-invoke aws lambda add-permission "${AP[@]}" --function-name "$FN" \
        --statement-id llm-endpoint-invoke-via-url --action lambda:InvokeFunction --principal '*' --invoked-via-function-url; then
        remove_url
        refuse permission "add-permission llm-endpoint-invoke-via-url failed, the URL was removed" 9
    fi
    # IMMEDIATELY re-check without a key, from the outside
    BODY='{"model":"x","messages":[{"role":"user","content":"Hi"}],"max_tokens":1}'
    http_code() {
        if [ "$1" = POST ]; then
            curl -s -o /dev/null -w '%{http_code}' --max-time 120 -X POST -H 'content-type: application/json' --data "$BODY" "$URL/v1/chat/completions" || true
        else
            curl -s -o /dev/null -w '%{http_code}' --max-time 120 "$URL/health" || true
        fi
    }
    retryable() {
        case "$1" in
            000|403|5??) return 0 ;;
            *) return 1 ;;
        esac
    }
    DEADLINE=$((SECONDS + 180))
    VERDICT=waiting
    PC=000
    GC=000
    while [ "$VERDICT" = waiting ]; do
        PC="$(http_code POST)"
        GC="$(http_code GET)"
        BAD=0
        if [ "$PC" != 401 ] && ! retryable "$PC"; then
            BAD=1
        fi
        if [ "$GC" != 401 ] && ! retryable "$GC"; then
            BAD=1
        fi
        if [ "$PC" = 401 ] && [ "$GC" = 401 ]; then
            VERDICT=ok
        elif [ "$BAD" -eq 1 ]; then
            VERDICT=open
        elif [ "$SECONDS" -ge "$DEADLINE" ]; then
            VERDICT=open
        else
            sleep 5
        fi
    done
    if [ "$VERDICT" != ok ]; then
        remove_url
        echo "URL REMOVED: open endpoint, POST answered $PC and GET /health answered $GC without a key, not 401" >&2
        exit 1
    fi
    python3 -I -c 'import datetime, json, sys; out, fn, url, conc, risk = sys.argv[1:6]; rec = {"function": fn, "url": url, "created_at": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"), "auth_type": "NONE", "invoke_mode": "BUFFERED", "statement_ids": ["llm-endpoint-url-public", "llm-endpoint-invoke-via-url"], "reserved_concurrency": int(conc) if conc.isdigit() else None, "risk_accepted": risk == "true"}; json.dump(rec, open(out, "w"), indent=2)' \
        "$IDS/url.json" "$FN" "$URL" "$CONC" "$RISK"
    chmod 0600 "$IDS/url.json"
    echo "URL OK (401 without key verified; reserved concurrency $CONC; risk_accepted $RISK)"
    echo "  host: $HOST (the full URL is only in $IDS/url.json, gitignored)"

# Pre-warm the function with ONE authenticated 1-token invoke so the demo turn is not a cold start (D-10:
# the platform turn must stay under 120 s). The key comes from APR_PROBE_KEY, is written into a mktemp
# event that is deleted afterwards, and is never printed.
llm-endpoint-warm env="dev" profile="ze-kasher-dev" variant="":
    #!/usr/bin/env bash
    set -euo pipefail
    ENV="{{ trim_start_match(env, "env=") }}"
    PROFILE="{{profile}}"
    DIR=deploy/llm-endpoint
    VARIANT="{{ trim_start_match(variant, "variant=") }}"
    EVENT="$DIR/events/apigw-v2-no-auth.json"
    # the AWS CLI spelling of the raw payload format (raw, in, base64, out) is built in two pieces so the bashrs lexer does not read the word in as a keyword
    BINFMT="raw-i""n-base64-out"
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    jget() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2"; }
    case "$ENV" in
        dev|prod) ;;
        *) refuse env "$ENV is not a known environment, expected dev or prod" ;;
    esac
    case "$PROFILE" in
        variant=*) refuse args "variant= must be the last positional parameter (got it as profile)" ;;
    esac
    # every name below comes from the one library; a bad variant is refused before anything is read
    . "$DIR/lib/names.sh" || exit 1
    llm_endpoint_names "$VARIANT" "$ENV" || refuse variant "$LLM_ENDPOINT_NAMES_ERR"
    IDS="${LLM_ENDPOINT_IDS_DIR:-$IDS_DEFAULT}"
    [ -n "${APR_PROBE_KEY:-}" ] || refuse probe-key "APR_PROBE_KEY is not set, the warm-up call is authenticated"
    [ -f "$IDS/function.json" ] || refuse function "$IDS/function.json does not exist, run: just llm-endpoint-deploy"
    llm_endpoint_check_record "$IDS/function.json" || refuse ids "$LLM_ENDPOINT_NAMES_ERR"
    FN="$(jget "$IDS/function.json" function)" || refuse function "$IDS/function.json lacks function"
    REGION=us-east-1
    if [ -f "$IDS/aws.json" ]; then
        REGION="$(jget "$IDS/aws.json" region)" || refuse ids "$IDS/aws.json lacks region"
    fi
    EVF="$(mktemp)"
    OUT="$(mktemp)"
    trap 'rm -f "${EVF:?}" "${OUT:?}"' EXIT
    chmod 0600 "$EVF"
    python3 -I -c 'import json, os, sys; e = json.load(open(sys.argv[1])); e["headers"]["authorization"] = "Bearer " + os.environ["APR_PROBE_KEY"]; json.dump(e, open(sys.argv[2], "w"))' "$EVENT" "$EVF"
    T0=$SECONDS
    set +e
    aws lambda invoke --profile "$PROFILE" --region "$REGION" --function-name "$FN" --cli-binary-format "$BINFMT" --cli-read-timeout 900 \
        --payload "file://$EVF" --log-type Tail --query LogResult --output text "$OUT" > "$IDS/warm.invoke.log" 2>&1
    rc=$?
    set -e
    DT=$((SECONDS - T0))
    STATUS=none
    if [ "$rc" -eq 0 ]; then
        STATUS="$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1])).get("statusCode", "none"))' "$OUT" 2> /dev/null || echo none)"
    fi
    if [ "$STATUS" != "200" ]; then
        echo "WARM FAILED: $FN answered statusCode=$STATUS after $DT s, invoke rc=$rc" >&2
        exit 3
    fi
    echo "WARM OK $FN (statusCode 200, $DT s)"

# Read the live function back and record it, so two endpoints can be compared (readback_equal.py) as the
# isolation evidence. READ CALLS ONLY (get-function, get-function-concurrency, get-function-url-config);
# writes only $IDS/readback-<UTC>.json. The environment is recorded as sorted KEY NAMES, never values (the
# projection happens inside the aws CLI, so no value reaches this shell). function_url is `absent` when the
# function has no URL config, otherwise `present` (the address, host included, is secret-equivalent and the
# record gets committed, so it is never written).
llm-endpoint-readback env="dev" profile="ze-kasher-dev" variant="":
    #!/usr/bin/env bash
    set -euo pipefail
    ENV="{{ trim_start_match(env, "env=") }}"
    PROFILE="{{profile}}"
    DIR=deploy/llm-endpoint
    VARIANT="{{ trim_start_match(variant, "variant=") }}"
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    jget() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2"; }
    case "$ENV" in
        dev|prod) ;;
        *) refuse env "$ENV is not a known environment, expected dev or prod" ;;
    esac
    case "$PROFILE" in
        variant=*) refuse args "variant= must be the last positional parameter (got it as profile)" ;;
    esac
    # every name below comes from the one library; a bad variant is refused before anything is read
    . "$DIR/lib/names.sh" || exit 1
    llm_endpoint_names "$VARIANT" "$ENV" || refuse variant "$LLM_ENDPOINT_NAMES_ERR"
    IDS="${LLM_ENDPOINT_IDS_DIR:-$IDS_DEFAULT}"
    [ -f "$IDS/function.json" ] || refuse function "$IDS/function.json does not exist, run: just llm-endpoint-deploy"
    llm_endpoint_check_record "$IDS/function.json" || refuse ids "$LLM_ENDPOINT_NAMES_ERR"
    FN="$(jget "$IDS/function.json" function)" || refuse function "$IDS/function.json lacks function"
    REGION=us-east-1
    if [ -f "$IDS/aws.json" ]; then
        REGION="$(jget "$IDS/aws.json" region)" || refuse ids "$IDS/aws.json lacks region"
    fi
    AP=(--profile "$PROFILE" --region "$REGION")
    CFG="$(mktemp)"
    ERR="$(mktemp)"
    trap 'rm -f "${CFG:?}" "${ERR:?}"' EXIT
    # the projection keeps the environment as key names only
    PROJ='{function_name:Configuration.FunctionName,state:Configuration.State,memory_mb:Configuration.MemorySize,timeout_s:Configuration.Timeout,ephemeral_mb:Configuration.EphemeralStorage.Size,architecture:Configuration.Architectures[0],package_type:Configuration.PackageType,code_sha256:Configuration.CodeSha256,last_modified:Configuration.LastModified,revision_id:Configuration.RevisionId,image_uri:Code.ImageUri,env_keys:keys(Configuration.Environment.Variables || `{}`)}'
    set +e
    aws lambda get-function "${AP[@]}" --function-name "$FN" --query "$PROJ" --output json > "$CFG" 2> "$ERR"
    rc=$?
    set -e
    [ "$rc" -eq 0 ] || refuse readback "get-function failed for $FN, rc=$rc: $(tr '\n' ' ' < "$ERR")" 3
    set +e
    CONC="$(aws lambda get-function-concurrency "${AP[@]}" --function-name "$FN" --query ReservedConcurrentExecutions --output text 2> "$ERR")"
    rc=$?
    set -e
    [ "$rc" -eq 0 ] || refuse readback "get-function-concurrency failed for $FN, rc=$rc: $(tr '\n' ' ' < "$ERR")" 3
    set +e
    URL="$(aws lambda get-function-url-config "${AP[@]}" --function-name "$FN" --query FunctionUrl --output text 2> "$ERR")"
    rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then
        grep -q 'ResourceNotFoundException' "$ERR" || refuse readback "get-function-url-config failed for $FN, rc=$rc: $(tr '\n' ' ' < "$ERR")" 3
        URL=absent
    fi
    STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
    OUT="$IDS/readback-$STAMP.json"
    python3 -I -c 'import datetime, json, sys; cfg, out, conc, url = sys.argv[1:5]; rec = json.load(open(cfg)); rec["env_keys"] = sorted(rec["env_keys"] or []); rec["reserved_concurrency"] = int(conc) if conc.isdigit() else None; rec["function_url"] = "absent" if url == "absent" else "present"; rec["read_at"] = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"); json.dump(rec, open(out, "w"), indent=2)' \
        "$CFG" "$OUT" "$CONC" "$URL"
    chmod 0600 "$OUT"
    echo "READBACK OK $FN $OUT"

# Tear an environment down (D-12, Pitfall 6), contain first. Order: reserved concurrency 0, delete the
# URL config, remove-permission for BOTH statement ids, verify get-policy lists neither (refuses
# `policy`), delete the function, delete ONLY the ECR image tag this deployment recorded and only while
# it still resolves to the recorded digest (otherwise SKIPPED image), the log group, the bucket (only if
# its sole object is the GGUF, unless KEEP_BUCKET=1), the CodeBuild project and role when
# `llm-endpoint-weights codebuild` made them, and the role. The ECR REPOSITORY is never deleted by this
# recipe: its removal is printed under NOT RUN for the human to decide. KEEP_IMAGE=1 keeps the image
# tag. The spike-031 leftovers are printed, and removed only with TEARDOWN_SPIKE_031=1 (never its ECR repository).
llm-endpoint-teardown env="dev" profile="ze-kasher-dev" variant="":
    #!/usr/bin/env bash
    set -euo pipefail
    ENV="{{ trim_start_match(env, "env=") }}"
    PROFILE="{{profile}}"
    DIR=deploy/llm-endpoint
    VARIANT="{{ trim_start_match(variant, "variant=") }}"
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    jget() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2"; }
    case "$ENV" in
        dev|prod) ;;
        *) refuse env "$ENV is not a known environment, expected dev or prod" ;;
    esac
    case "$PROFILE" in
        variant=*) refuse args "variant= must be the last positional parameter (got it as profile)" ;;
    esac
    # every name below comes from the one library; a bad variant is refused before anything is read
    . "$DIR/lib/names.sh" || exit 1
    llm_endpoint_names "$VARIANT" "$ENV" || refuse variant "$LLM_ENDPOINT_NAMES_ERR"
    IDS="${LLM_ENDPOINT_IDS_DIR:-$IDS_DEFAULT}"
    PIN_FILE="${LLM_ENDPOINT_PIN_FILE:-$PIN_DEFAULT}"
    [ -f "$IDS/aws.json" ] || refuse ids "$IDS/aws.json does not exist, nothing to tear down"
    # no delete may run against a record that is another function's, so both are checked before anything else
    llm_endpoint_check_record "$IDS/aws.json" || refuse ids "$LLM_ENDPOINT_NAMES_ERR"
    if [ -f "$IDS/function.json" ]; then
        llm_endpoint_check_record "$IDS/function.json" || refuse ids "$LLM_ENDPOINT_NAMES_ERR"
    fi
    if [ -f "$IDS/weights.json" ] && [ "$(jget "$IDS/weights.json" mode 2> /dev/null || true)" = "codebuild" ]; then
        WCB_PROJECT="$(jget "$IDS/weights.json" codebuild_project)" || refuse ids "$IDS/weights.json lacks codebuild_project"
        WCB_ROLE="$(jget "$IDS/weights.json" codebuild_role)" || refuse ids "$IDS/weights.json lacks codebuild_role"
        # exactly the names this variant derives; empty (copy mode) passes
        if [ -n "$WCB_PROJECT" ] && [ "$WCB_PROJECT" != "$CB_PROJECT" ]; then
            refuse ids "weights.json names a CodeBuild project ($WCB_PROJECT) that does not belong to $FN"
        fi
        if [ -n "$WCB_ROLE" ] && [ "$WCB_ROLE" != "$CB_ROLE" ]; then
            refuse ids "weights.json names a CodeBuild role ($WCB_ROLE) that does not belong to $FN"
        fi
    fi
    [ -f "$PIN_FILE" ] || refuse pin "$PIN_FILE does not exist"
    . "$PIN_FILE"
    [ -n "${GGUF_OBJECT:-}" ] || refuse pin "$PIN_FILE lacks GGUF_OBJECT"
    WANT_ACCOUNT="$(jget "$IDS/aws.json" account)" || refuse ids "$IDS/aws.json lacks account"
    REGION="$(jget "$IDS/aws.json" region)" || refuse ids "$IDS/aws.json lacks region"
    BUCKET="$(jget "$IDS/aws.json" bucket)" || refuse ids "$IDS/aws.json lacks bucket"
    ECR_REPO="$(jget "$IDS/aws.json" ecr_repo)" || refuse ids "$IDS/aws.json lacks ecr_repo"
    REPO_NAME="$(jget "$IDS/aws.json" ecr_repo_name)" || refuse ids "$IDS/aws.json lacks ecr_repo_name"
    # the record matched this function by name; its other names are cross-checked against the derived ones
    # before any write (a hand-edited bucket or repository would otherwise be deleted as if it were ours)
    REC_BUCKET="$BUCKET"
    REC_REPO_NAME="$REPO_NAME"
    llm_endpoint_names "$VARIANT" "$ENV" "$WANT_ACCOUNT" || refuse variant "$LLM_ENDPOINT_NAMES_ERR"
    [ "$REC_BUCKET" = "$BUCKET" ] || refuse ids "$IDS/aws.json bucket $REC_BUCKET is not $BUCKET, the bucket of $FN"
    [ "$REC_REPO_NAME" = "$REPO_NAME" ] || refuse ids "$IDS/aws.json ecr_repo_name $REC_REPO_NAME is not $REPO_NAME, the repository of $FN"
    case "$ECR_REPO" in
        */"$REPO_NAME") ;;
        *) refuse ids "$IDS/aws.json ecr_repo $ECR_REPO does not end in /$REPO_NAME" ;;
    esac
    REC_LOG_GROUP="$(jget "$IDS/aws.json" log_group 2> /dev/null || true)"
    if [ -n "$REC_LOG_GROUP" ] && [ "$REC_LOG_GROUP" != "$LOG_GROUP" ]; then
        refuse ids "$IDS/aws.json log_group $REC_LOG_GROUP is not $LOG_GROUP, the log group of $FN"
    fi
    REC_ROLE_ARN="$(jget "$IDS/aws.json" role_arn 2> /dev/null || true)"
    if [ -n "$REC_ROLE_ARN" ]; then
        case "$REC_ROLE_ARN" in
            arn:aws:iam::"$WANT_ACCOUNT":role/"$FN") ;;
            *) refuse ids "$IDS/aws.json role_arn $REC_ROLE_ARN is not the role $FN of account $WANT_ACCOUNT" ;;
        esac
    fi
    AP=(--profile "$PROFILE" --region "$REGION")
    ACCOUNT="$(aws sts get-caller-identity --profile "$PROFILE" --query Account --output text)" \
        || refuse account "cannot read the AWS account for profile $PROFILE"
    [ "$ACCOUNT" = "$WANT_ACCOUNT" ] || refuse account "profile $PROFILE is account $ACCOUNT but $IDS/aws.json was written for $WANT_ACCOUNT"
    # tolerate <pattern> <what> <cmd...>: success, or a failure whose output names the exact exception
    tolerate() {
        local pat="$1" what="$2" rc
        shift 2
        set +e
        "$@" > "$IDS/step.log" 2>&1
        rc=$?
        set -e
        if [ "$rc" -eq 0 ]; then
            return 0
        fi
        if grep -q "$pat" "$IDS/step.log"; then
            echo "  absent, $pat: $what"
            return 0
        fi
        tail -5 "$IDS/step.log" >&2
        refuse "$what" "failed, rc=$rc" 9
    }
    # (1) contain first: no invocation runs, warm instances included
    set +e
    aws lambda put-function-concurrency "${AP[@]}" --function-name "$FN" --reserved-concurrent-executions 0 > "$IDS/step.log" 2>&1
    crc=$?
    set -e
    if [ "$crc" -eq 0 ]; then
        GOT="$(aws lambda get-function-concurrency "${AP[@]}" --function-name "$FN" --query ReservedConcurrentExecutions --output text)" \
            || refuse contain "cannot read back the reserved concurrency of $FN" 9
        [ "$GOT" = "0" ] || refuse contain "reserved concurrency of $FN reads $GOT, not 0: it may still be serving" 9
        echo "  CONTAINED: $FN reserved concurrency 0"
    elif grep -q 'ResourceNotFoundException' "$IDS/step.log"; then
        echo "  absent: function $FN (nothing to contain)"
    else
        tail -5 "$IDS/step.log" >&2
        refuse contain "put-function-concurrency failed for $FN, rc=$crc" 9
    fi
    # (2) the URL config, then BOTH statements (deleting the URL does not delete the policy)
    tolerate ResourceNotFoundException delete-url-config aws lambda delete-function-url-config "${AP[@]}" --function-name "$FN"
    tolerate ResourceNotFoundException remove-public aws lambda remove-permission "${AP[@]}" --function-name "$FN" --statement-id llm-endpoint-url-public
    tolerate ResourceNotFoundException remove-invoke aws lambda remove-permission "${AP[@]}" --function-name "$FN" --statement-id llm-endpoint-invoke-via-url
    set +e
    POLICY="$(aws lambda get-policy "${AP[@]}" --function-name "$FN" --query Policy --output text 2>&1)"
    prc=$?
    set -e
    if [ "$prc" -ne 0 ]; then
        case "$POLICY" in
            *ResourceNotFoundException*) POLICY="" ;;
            *) refuse policy "get-policy failed for $FN, rc=$prc: $POLICY" 9 ;;
        esac
    fi
    case "$POLICY" in
        *llm-endpoint-url-public*|*llm-endpoint-invoke-via-url*)
            refuse policy "the resource policy of $FN still lists a URL statement, the function stays contained but is NOT clean" 9
            ;;
    esac
    echo "  policy clean: neither URL statement remains on $FN"
    # (3) the function, then the image tag this deployment recorded
    tolerate ResourceNotFoundException delete-function aws lambda delete-function "${AP[@]}" --function-name "$FN"
    if [ "${KEEP_IMAGE:-0}" = "1" ]; then
        echo "  KEEP_IMAGE=1: the image tag is kept"
    elif [ -f "$IDS/image-pushed.json" ]; then
        REC_REPO="$(jget "$IDS/image-pushed.json" repo)" || refuse ids "$IDS/image-pushed.json lacks repo"
        REC_TAG="$(jget "$IDS/image-pushed.json" tag)" || refuse ids "$IDS/image-pushed.json lacks tag"
        REC_DIGEST="$(jget "$IDS/image-pushed.json" digest)" || refuse ids "$IDS/image-pushed.json lacks digest"
        set +e
        CUR_DIGEST="$(aws ecr describe-images "${AP[@]}" --repository-name "$REPO_NAME" --image-ids "imageTag=$REC_TAG" --query 'imageDetails[0].imageDigest' --output text 2> /dev/null)"
        drc=$?
        set -e
        if [ "$REC_REPO" != "$ECR_REPO" ]; then
            echo "  SKIPPED image: the recorded repository $REC_REPO is not the repository of this environment $ECR_REPO"
        elif [ "$drc" -eq 0 ] && [ "$CUR_DIGEST" = "$REC_DIGEST" ]; then
            FAILCODE="$(aws ecr batch-delete-image "${AP[@]}" --repository-name "$REPO_NAME" --image-ids "imageTag=$REC_TAG" --query 'failures[0].failureCode' --output text)" \
                || refuse image "batch-delete-image failed for tag $REC_TAG" 9
            case "$FAILCODE" in
                ""|None) echo "  deleted image tag $REC_TAG ($REC_DIGEST) from $REPO_NAME" ;;
                *) echo "  WARNING: batch-delete-image reported $FAILCODE for tag $REC_TAG" >&2 ;;
            esac
        else
            echo "  SKIPPED image: $REC_TAG does not resolve to the recorded digest, nothing deleted"
        fi
    else
        echo "  no $IDS/image-pushed.json: no image tag recorded, nothing to delete"
    fi
    # (4) the repository itself is the human decision: print its state and the command, never run it
    COUNT="$(aws ecr list-images "${AP[@]}" --repository-name "$REPO_NAME" --query 'length(imageIds)' --output text 2> /dev/null || echo unknown)"
    echo "  NOT RUN -- for the human to decide:"
    echo "    repository $REPO_NAME now holds $COUNT image(s)"
    if [ "$COUNT" = "0" ]; then
        echo "      aws ecr delete-repository --repository-name $REPO_NAME --profile $PROFILE --region $REGION"
    else
        echo "      aws ecr delete-repository --repository-name $REPO_NAME --force --profile $PROFILE --region $REGION   # --force is needed while images remain"
    fi
    # (5) log group
    tolerate ResourceNotFoundException delete-log-group aws logs delete-log-group "${AP[@]}" --log-group-name "$LOG_GROUP"
    # (6) the bucket, only when its sole object is the GGUF
    if [ "${KEEP_BUCKET:-0}" = "1" ]; then
        echo "  KEEP_BUCKET=1: s3://$BUCKET is kept"
    else
        set +e
        KEYS="$(aws s3api list-objects-v2 "${AP[@]}" --bucket "$BUCKET" --query 'Contents[].Key' --output text 2> "$IDS/step.log")"
        lrc=$?
        set -e
        if [ "$lrc" -ne 0 ] && grep -q 'NoSuchBucket' "$IDS/step.log"; then
            echo "  absent, NoSuchBucket: s3://$BUCKET"
        elif [ "$lrc" -ne 0 ]; then
            tail -5 "$IDS/step.log" >&2
            refuse bucket "list-objects-v2 failed for s3://$BUCKET, rc=$lrc" 9
        elif [ -z "$KEYS" ] || [ "$KEYS" = "None" ]; then
            tolerate NoSuchBucket delete-bucket aws s3api delete-bucket "${AP[@]}" --bucket "$BUCKET"
            echo "  deleted empty bucket s3://$BUCKET"
        elif [ "$KEYS" = "$GGUF_OBJECT" ]; then
            tolerate NoSuchKey delete-object aws s3api delete-object "${AP[@]}" --bucket "$BUCKET" --key "$GGUF_OBJECT"
            tolerate NoSuchBucket delete-bucket aws s3api delete-bucket "${AP[@]}" --bucket "$BUCKET"
            echo "  deleted s3://$BUCKET with its sole object $GGUF_OBJECT"
        else
            echo "  KEPT s3://$BUCKET: it holds more than the GGUF, keys found: $KEYS"
        fi
    fi
    # (7) the CodeBuild project and role, when llm-endpoint-weights codebuild made them
    if [ -f "$IDS/weights.json" ]; then
        WMODE="$(jget "$IDS/weights.json" mode)" || refuse ids "$IDS/weights.json lacks mode"
        if [ "$WMODE" = "codebuild" ]; then
            CB_PROJECT="$(jget "$IDS/weights.json" codebuild_project)" || refuse ids "$IDS/weights.json lacks codebuild_project"
            CB_ROLE="$(jget "$IDS/weights.json" codebuild_role)" || refuse ids "$IDS/weights.json lacks codebuild_role"
            tolerate ResourceNotFoundException delete-codebuild aws codebuild delete-project "${AP[@]}" --name "$CB_PROJECT"
            tolerate ResourceNotFoundException delete-codebuild-logs aws logs delete-log-group "${AP[@]}" --log-group-name "/aws/codebuild/$CB_PROJECT"
            tolerate NoSuchEntity delete-codebuild-policy aws iam delete-role-policy --profile "$PROFILE" --role-name "$CB_ROLE" --policy-name aprender-llm-endpoint-s3-logs
            tolerate NoSuchEntity delete-codebuild-role aws iam delete-role --profile "$PROFILE" --role-name "$CB_ROLE"
        fi
        rm -f "$IDS/weights.json"
    fi
    # (8) the function role: inline policy, managed policy, role
    tolerate NoSuchEntity delete-role-policy aws iam delete-role-policy --profile "$PROFILE" --role-name "$FN" --policy-name aprender-llm-endpoint-s3-read
    tolerate NoSuchEntity detach-role-policy aws iam detach-role-policy --profile "$PROFILE" --role-name "$FN" \
        --policy-arn arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole
    tolerate NoSuchEntity delete-role aws iam delete-role --profile "$PROFILE" --role-name "$FN"
    rm -f "$IDS/function.json" "$IDS/url.json" "$IDS/prove-401.json"
    # (9) the spike-031 leftovers: listed, and removed only on request; their ECR repository is never executed
    SP=llm-spike-031
    SPIKE_ROLES="$SP-lambda $SP-ecs-exec $SP-ecs-task $SP-lmi-operator $SP-codebuild"
    echo "  NOT RUN -- for the human to decide (D-15), spike-031 leftovers; TEARDOWN_SPIKE_031=1 removes all but the ECR repository:"
    echo "    aws lambda delete-function --function-name $SP-lambda-0-8b --profile $PROFILE --region $REGION"
    echo "    aws lambda delete-function --function-name $SP-lambda-2b --profile $PROFILE --region $REGION"
    echo "    aws codebuild delete-project --name $SP-fetch-weights --profile $PROFILE --region $REGION"
    echo "    aws ecs delete-cluster --cluster $SP --profile $PROFILE --region $REGION"
    echo "    aws s3 rm s3://$SP-$ACCOUNT --recursive --profile $PROFILE --region $REGION , afterwards: aws s3api delete-bucket --bucket $SP-$ACCOUNT"
    echo "    roles: $SPIKE_ROLES (detach and delete their policies first), security group $SP"
    echo "    aws ecr delete-repository --repository-name $SP --force --profile $PROFILE --region $REGION   # this recipe never runs it"
    if [ "${TEARDOWN_SPIKE_031:-0}" = "1" ]; then
        # best_effort <cmd...>: a leftover that is already gone is not an error
        best_effort() {
            set +e
            "$@" > "$IDS/step.log" 2>&1
            local rc=$?
            set -e
            if [ "$rc" -ne 0 ]; then
                echo "  not removed (already gone or refused): $*"
            fi
        }
        best_effort aws lambda delete-function --function-name "$SP-lambda-0-8b" "${AP[@]}"
        best_effort aws lambda delete-function --function-name "$SP-lambda-2b" "${AP[@]}"
        best_effort aws codebuild delete-project --name "$SP-fetch-weights" "${AP[@]}"
        best_effort aws ecs delete-cluster --cluster "$SP" "${AP[@]}"
        best_effort aws s3 rm "s3://$SP-$ACCOUNT" --recursive "${AP[@]}"
        best_effort aws s3api delete-bucket --bucket "$SP-$ACCOUNT" "${AP[@]}"
        for role in $SPIKE_ROLES; do
            for pol in $(aws iam list-attached-role-policies --profile "$PROFILE" --role-name "$role" --query 'AttachedPolicies[].PolicyArn' --output text 2> /dev/null || true); do
                best_effort aws iam detach-role-policy --profile "$PROFILE" --role-name "$role" --policy-arn "$pol"
            done
            for pol in $(aws iam list-role-policies --profile "$PROFILE" --role-name "$role" --query PolicyNames --output text 2> /dev/null || true); do
                best_effort aws iam delete-role-policy --profile "$PROFILE" --role-name "$role" --policy-name "$pol"
            done
            best_effort aws iam delete-role --profile "$PROFILE" --role-name "$role"
        done
        SG="$(aws ec2 describe-security-groups "${AP[@]}" --filters "Name=group-name,Values=$SP" --query 'SecurityGroups[0].GroupId' --output text 2> /dev/null || true)"
        if [ -n "$SG" ] && [ "$SG" != "None" ]; then
            best_effort aws ec2 delete-security-group "${AP[@]}" --group-id "$SG"
        fi
        echo "  TEARDOWN_SPIKE_031=1: the spike-031 leftovers above were removed where they existed"
    fi
    echo "TEARDOWN OK $FN"

# Run the conformance probe against a server and bind the record to the Rust reader (D-07, plan 09-13).
# Accepted labels: release-local, interim, deployed-https. This HTTP wrapper NEVER writes the label
# baseline (the committed spike-029 transcription) or deployed (written only by plan 09-07 through
# lambda:Invoke); both are refused. The key is read from APR_PROBE_KEY by the probe, never from argv.
# File name and pin follow the probe-record contract in deploy/llm-endpoint/probe/README.md:
# interim -> conformance-interim-upstream-main-plus-fixes.json with pin upstream-main+PRs, every other
# label -> conformance-<APR_TAG>-<label>.json with pin APR_TAG. APR_UNRELEASED_FIXES is a semicolon
# separated list of probe=reference, forwarded as repeated --unreleased flags (fixed profile, and only
# for the two unreleased-eligible probes). LLM_ENDPOINT_PROBE_DRY=1 prints what would be written and exits 0
# before contacting any server. Seams: LLM_ENDPOINT_PIN_FILE, LLM_ENDPOINT_RESULTS_DIR.
llm-endpoint-probe $base $model $label $server_version:
    #!/usr/bin/env bash
    set -euo pipefail
    DIR=deploy/llm-endpoint
    PIN_FILE="${LLM_ENDPOINT_PIN_FILE:-$DIR/pin.env}"
    RESULTS="${LLM_ENDPOINT_RESULTS_DIR:-$DIR/probe/results}"
    BASE="$base"
    MODEL="$model"
    LABEL="$label"
    SERVER_VERSION="$server_version"
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    [ -n "${APR_PROBE_KEY:-}" ] || refuse key "APR_PROBE_KEY is not set, the probe reads the key from the environment only"
    case "$LABEL" in
        release-local|interim|deployed-https) ;;
        *) refuse label "$LABEL is not accepted, use release-local, interim or deployed-https; baseline is the committed transcription and deployed is written only by plan 09-07" ;;
    esac
    ENV_PROFILE="${APR_PIN_PROFILE:-}"
    [ -f "$PIN_FILE" ] || refuse pin "$PIN_FILE does not exist"
    . "$PIN_FILE"
    printf '%s' "${APR_TAG:-}" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' \
        || refuse pin "APR_TAG in $PIN_FILE is not vMAJOR.MINOR.PATCH"
    PIN_PROFILE="${APR_PIN_PROFILE:-}"
    PROFILE="$PIN_PROFILE"
    if [ -n "$ENV_PROFILE" ]; then PROFILE="$ENV_PROFILE"; fi
    case "$PROFILE" in
        baseline|fixed) ;;
        *) refuse profile "the profile $PROFILE is not baseline or fixed" ;;
    esac
    UNREL=()
    N=0
    if [ -n "${APR_UNRELEASED_FIXES:-}" ]; then
        [ "$PROFILE" = fixed ] || refuse unreleased "APR_UNRELEASED_FIXES is accepted only in the fixed profile, this one is $PROFILE"
        ENTRIES="$(printf '%s' "$APR_UNRELEASED_FIXES" | tr ';' '\n')"
        while IFS= read -r entry; do
            [ -n "$entry" ] || continue
            case "$entry" in
                "tool result -> final answer="?*|"max_completion_tokens honoured="?*) ;;
                *) refuse unreleased "$entry is not probe=reference for tool result -> final answer or max_completion_tokens honoured, no other probe may be recorded as unreleased" ;;
            esac
            UNREL+=(--unreleased "$entry")
            N=$((N + 1))
        done <<< "$ENTRIES"
    fi
    if [ "$LABEL" = interim ]; then
        FILE="conformance-interim-upstream-main-plus-fixes.json"
        PIN="upstream-main+PRs"
    else
        FILE="conformance-$APR_TAG-$LABEL.json"
        PIN="$APR_TAG"
    fi
    if [ "${LLM_ENDPOINT_PROBE_DRY:-0}" = 1 ]; then
        echo "WOULD WRITE $FILE pin=$PIN profile=$PROFILE unreleased=$N"
        exit 0
    fi
    mkdir -p "$RESULTS"
    OUTFILE="$RESULTS/$FILE"
    CMD=(python3 -I "$DIR/probe/probe_openai.py" --base "$BASE" --model "$MODEL" --profile "$PROFILE" --label "$LABEL" --pin "$PIN" --server-version "$SERVER_VERSION" --out "$OUTFILE")
    if [ "$N" -gt 0 ]; then
        CMD+=("${UNREL[@]}")
    fi
    set +e
    "${CMD[@]}"
    PRC=$?
    set -e
    [ -f "$OUTFILE" ] || refuse probe "the probe wrote no record, exit $PRC" 3
    echo "RECORD $OUTFILE (probe exit $PRC)"
    # the reader runs on every record, so a record the contract rejects can never sit unnoticed in results/
    set +e
    CARGO_INCREMENTAL=0 cargo test -p aprender-serve --lib api::llm_endpoint_conformance_tests::conformance_record_matches_contract
    RRC=$?
    set -e
    [ "$RRC" -eq 0 ] || refuse reader "the reader test rejected the results directory, exit $RRC; the record is left in place for diagnosis" 1
    [ "$PRC" -eq 0 ] || { echo "PROBE CROSS: the probe exited $PRC, see the table above" >&2; exit "$PRC"; }
    echo "PROBE OK $FILE pin=$PIN profile=$PROFILE"

# Measure the DEPLOYED function (D-14): the spike-031 shapes through aws lambda invoke, recording client
# wall AND billed time, the price at 0.0000133334 per GB-s and cpu_part on every record. `budget` (USD,
# optional) makes bench.py refuse each sample before it would overspend; when it stops, the partial file is
# still written and this recipe exits 4. Writes deploy/llm-endpoint/.ids/bench-<label>-<utc>.json (gitignored);
# plan 09-07 composes the evidence record from it. No key on any command line: APR_PROBE_KEY is read by bench.py.
llm-endpoint-measure env="dev" profile="ze-kasher-dev" label="deployed" cold="1" long="1" conc="2" budget="" variant="":
    #!/usr/bin/env bash
    set -euo pipefail
    ENV="{{ trim_start_match(env, "env=") }}"
    PROFILE="{{profile}}"
    LABEL="{{label}}"
    COLD="{{cold}}"
    LONG="{{long}}"
    CONC="{{conc}}"
    BUDGET="{{budget}}"
    DIR=deploy/llm-endpoint
    VARIANT="{{ trim_start_match(variant, "variant=") }}"
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    jget() { python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))[sys.argv[2]])' "$1" "$2"; }
    case "$ENV" in
        dev|prod) ;;
        *) refuse env "$ENV is not a known environment, expected dev or prod" ;;
    esac
    case "$PROFILE" in
        variant=*) refuse args "variant= must be the last positional parameter (got it as profile)" ;;
    esac
    case "$LABEL" in
        variant=*) refuse args "variant= must be the last positional parameter (got it as label)" ;;
    esac
    # every name below comes from the one library; a bad variant is refused before anything is read
    . "$DIR/lib/names.sh" || exit 1
    llm_endpoint_names "$VARIANT" "$ENV" || refuse variant "$LLM_ENDPOINT_NAMES_ERR"
    IDS="${LLM_ENDPOINT_IDS_DIR:-$IDS_DEFAULT}"
    PIN_FILE="${LLM_ENDPOINT_PIN_FILE:-$PIN_DEFAULT}"
    [ -n "${APR_PROBE_KEY:-}" ] || refuse key "APR_PROBE_KEY is not set, every sample except the 401 case is authenticated"
    [ -f "$IDS/function.json" ] || refuse function "$IDS/function.json does not exist, run: just llm-endpoint-deploy"
    llm_endpoint_check_record "$IDS/function.json" || refuse ids "$LLM_ENDPOINT_NAMES_ERR"
    FN="$(jget "$IDS/function.json" function)" || refuse function "$IDS/function.json lacks function"
    [ -f "$PIN_FILE" ] || refuse pin "$PIN_FILE does not exist"
    . "$PIN_FILE"
    [ -n "${GGUF_OBJECT:-}" ] || refuse pin "$PIN_FILE lacks GGUF_OBJECT"
    MODEL="${GGUF_OBJECT%.gguf}"
    printf '%s' "$LABEL" | grep -Eq '^[A-Za-z0-9._-]+$' || refuse label "the label must be letters, digits, dot, underscore or dash"
    printf '%s' "$CONC" | grep -Eq '^[0-9]+$' || refuse conc "conc must be a whole number"
    if [ -n "$BUDGET" ]; then
        printf '%s' "$BUDGET" | grep -Eq '^[0-9]+(\.[0-9]+)?$' || refuse budget "budget must be a positive number of USD"
    fi
    ARGS=(--function "$FN" --ids-dir "$IDS" --profile "$PROFILE" --model "$MODEL" --label "$LABEL" --conc "$CONC")
    if [ "$COLD" = 1 ]; then
        ARGS+=(--cold)
    fi
    if [ "$LONG" = 1 ]; then
        ARGS+=(--long)
    fi
    if [ -n "$BUDGET" ]; then
        ARGS+=(--budget-usd "$BUDGET")
    fi
    OUT="$IDS/bench-$LABEL-$(date -u +%Y%m%dT%H%M%SZ).json"
    set +e
    python3 -I "$DIR/probe/bench.py" "${ARGS[@]}" --out "$OUT"
    RC=$?
    set -e
    if [ "$RC" -eq 0 ]; then
        echo "MEASURE OK $OUT"
    elif [ "$RC" -eq 4 ] && [ -f "$OUT" ]; then
        STOP="$(python3 -I -c 'import json, sys; s = json.load(open(sys.argv[1]))["budget_stop"]; print("%s spent=%s est=%s budget=%s" % (s["before"], s["spent_usd"], s["est_usd"], s["budget_usd"]))' "$OUT")"
        echo "MEASURE STOPPED (budget): $STOP, partial record $OUT" >&2
        exit 4
    else
        echo "MEASURE FAILED: bench.py exited $RC" >&2
        exit "$RC"
    fi

# Offline proof of llm-endpoint-deploy (and the other llm-endpoint recipes): an `aws` shim is FIRST on
# PATH and the real aws is never executed. Offline cases run in shim mode `refuse` (log and exit 97), so
# their log must stay empty (`AWS CALLS: 0`); the PREFLIGHT cases run in mode `preflight`, which answers
# only the three read calls (`FIXTURE READS: n`); a negative control proves the counter can be non-zero.
llm-endpoint-selftest:
    #!/usr/bin/env bash
    set -euo pipefail
    DIR=deploy/llm-endpoint
    T="$(mktemp -d)"
    # SELFTEST_KEEP=1 keeps the temp directory (argv logs, per-case outputs) for debugging
    cleanup() {
        if [ "${SELFTEST_KEEP:-0}" = 1 ]; then
            echo "kept $T"
        else
            rm -rf "${T:?}"
        fi
    }
    trap cleanup EXIT
    SH="$T/shim"
    IDS="$T/ids"
    mkdir -p "$SH" "$IDS" "$T/bin"
    # write_shim <name> <script line>...: an executable bash shim; lines are written verbatim
    write_shim() {
        local name="$1"
        shift
        printf '%s\n' '#!/usr/bin/env bash' "$@" > "$SH/$name"
        chmod 0755 "$SH/$name"
    }
    write_shim aws \
        'echo "$*" >> "$SHIM_LOG"' \
        'key="${1:-} ${2:-}"; last=""' \
        'for a in "$@"; do last="$a"; done ' \
        'case "${SHIM_MODE:-refuse}" in ' \
        '  refuse) echo "aws shim: refused (the self-test never reaches AWS)" >&2; exit 97 ;;' \
        '  preflight)' \
        '    case "$key" in ' \
        '      "sts get-caller-identity") echo "$key" >> "${SHIM_READS:-/dev/null}"; echo "${SHIM_STS_ACCOUNT:-}"; exit 0 ;;' \
        '      "s3api head-object") echo "$key" >> "${SHIM_READS:-/dev/null}"; echo "${SHIM_S3_SHA:-}"; exit 0 ;;' \
        '      "ecr describe-images") echo "$key" >> "${SHIM_READS:-/dev/null}"; echo "${SHIM_ECR_DIGEST:-}"; exit 0 ;;' \
        '      "lambda get-function-concurrency") echo "$key" >> "${SHIM_READS:-/dev/null}"; echo "${SHIM_CONCURRENCY:-None}"; exit 0 ;;' \
        '      "lambda get-function") echo "$key" >> "${SHIM_READS:-/dev/null}"; echo "${SHIM_RESOLVED_IMAGE_URI:-}"; exit 0 ;;' \
        '    esac ' \
        '    echo "aws shim: unanswered call in preflight mode: $key" >&2; exit 97 ;;' \
        '  permissive)' \
        '    case "$key" in ' \
        '      "sts get-caller-identity") echo "${SHIM_STS_ACCOUNT:-}" ;;' \
        '      "s3api head-object")' \
        '        case "$*" in ' \
        '          *llm-spike-031*) echo "${SHIM_S3_SHA:-}" ;;' \
        '          *) if [ "${SHIM_DST_EMPTY:-0}" = 1 ] && ! grep -q "s3api copy-object" "$SHIM_LOG"; then echo None; else echo "${SHIM_S3_SHA:-}"; fi ;;' \
        '        esac ;;' \
        '      "s3api list-objects-v2") echo "${SHIM_S3_KEY:-}" ;;' \
        '      "ecr describe-images") echo "${SHIM_ECR_DIGEST:-}" ;;' \
        '      "ecr list-images") echo "${SHIM_IMAGE_COUNT:-1}" ;;' \
        '      "lambda get-function-concurrency") echo "${SHIM_CONCURRENCY:-None}" ;;' \
        '      "lambda get-function")' \
        '        if [ "${SHIM_FN_EXISTS:-1}" = 0 ]; then echo "An error occurred, ResourceNotFoundException, when calling GetFunction" >&2; exit 254; fi ' \
        '        case "$*" in ' \
        '          *function_name:*) echo "{\"function_name\":\"${SHIM_RB_NAME:-aprender-llm-endpoint-dev}\",\"state\":\"Active\",\"memory_mb\":10240,\"timeout_s\":900,\"ephemeral_mb\":10240,\"architecture\":\"arm64\",\"package_type\":\"Image\",\"code_sha256\":\"shacode\",\"last_modified\":\"2026-10-18T10:00:00.000+0000\",\"revision_id\":\"rev-1\",\"image_uri\":\"img@sha256:1\",\"env_keys\":[\"MODEL_S3_URI\",\"APR_API_KEY_HASH\"]}" ;;' \
        '          *) echo "${SHIM_RESOLVED_IMAGE_URI:-}" ;;' \
        '        esac ;;' \
        '      "lambda get-function-url-config")' \
        '        if [ "${SHIM_URL_ABSENT:-0}" = 1 ]; then echo "An error occurred (ResourceNotFoundException) when calling the GetFunctionUrlConfig operation: nope" >&2; exit 254; fi ' \
        '        echo "${SHIM_URL:-https://shimhost0.lambda-url.us-east-1.on.aws/}" ;;' \
        '      "lambda get-function-configuration")' \
        '        case "$*" in ' \
        '          *keys*) echo "APR_API_KEY_HASH MODEL_S3_URI MODEL_SHA256 PLATFORM RAYON_NUM_THREADS" ;;' \
        '          *) echo "${SHIM_CFG:-Active 10240 900 10240 arm64}" ;;' \
        '        esac ;;' \
        '      "lambda get-policy")' \
        '        if [ "${SHIM_POLICY_IDS:-0}" = 1 ]; then echo "{\"Sid\":\"llm-endpoint-url-public\"}"; else echo "{\"Sid\":\"other\"}"; fi ;;' \
        '      "codebuild start-build") echo "build-selftest-1" ;;' \
        '      "codebuild batch-get-builds")' \
        '        case "$*" in ' \
        '          *buildStatus*) echo "SUCCEEDED" ;;' \
        '          *) echo "selftest-group selftest-stream" ;;' \
        '        esac ;;' \
        '      "logs get-log-events") echo "DOWNLOAD x 1s 100 bytes" ;;' \
        '      "lambda create-function-url-config") echo "https://shimhost0.lambda-url.us-east-1.on.aws/" ;;' \
        '      "lambda invoke")' \
        '        if [ -n "${SHIM_INVOKE_PAYLOAD:-}" ]; then cp "$SHIM_INVOKE_PAYLOAD" "$last"; fi ;;' \
        '      "iam get-role")' \
        '        if [ "${SHIM_ROLE_EXISTS:-1}" = 0 ] && ! grep -q "iam create-role" "$SHIM_LOG"; then echo "An error occurred, NoSuchEntity, when calling GetRole" >&2; exit 254; fi ' \
        '        rn=shim; prev=""; for a in "$@"; do if [ "$prev" = "--role-name" ]; then rn="$a"; fi; prev="$a"; done ' \
        '        echo "arn:aws:iam::${SHIM_STS_ACCOUNT:-0}:role/$rn" ;;' \
        '    esac ' \
        '    exit 0 ;;' \
        'esac ' \
        'exit 97'
    write_shim curl \
        'echo "$*" >> "${SHIM_CURL_LOG:-/dev/null}"' \
        'printf "%s" "${SHIM_CURL_CODE:-401}"'
    write_shim docker \
        'echo "$*" >> "$SHIM_LOG"' \
        'case "${1:-}" in ' \
        '  run) echo "apr 0.71.4 (selftest)" ;;' \
        '  image) echo "sha256:$(printf "e%.0s" $(seq 1 64))" ;;' \
        'esac ' \
        'exit 0'
    export PATH="$SH:$PATH"
    [ "$(command -v aws)" = "$SH/aws" ] || { echo "ERROR: the aws shim is not first on PATH" >&2; exit 1; }
    # positive control: a zero count means nothing unless the shim demonstrably records
    export SHIM_LOG="$T/offline.log"
    : > "$SHIM_LOG"
    set +e
    aws recorder-control > /dev/null 2>&1
    crc=$?
    set -e
    [ "$crc" -eq 97 ] && [ "$(wc -l < "$SHIM_LOG" | tr -d ' ')" -eq 1 ] \
        || { echo "ERROR: the aws shim did not record its control call (exit $crc)" >&2; exit 1; }
    : > "$SHIM_LOG"
    echo "AWS SHIM: control call recorded and cleared"
    # ---- fixtures ----
    ACCT=111111111111
    GGUF_OBJECT="Qwen3.5-0.8B-Q4_K_M.gguf"
    LADDER_SHA="$(python3 -I "$DIR/ladder_pin.py" contracts/model-capability-ladder-v1.yaml qwen35-0.8b-q4km)"
    printf 'selftest fake release tarball\n' > "$T/bin/fake.tar.gz"
    FAKE_SHA="$(shasum -a 256 "$T/bin/fake.tar.gz" | awk '{print $1}')"
    PIN="$T/pin.env"
    printf '%s\n' 'APR_TAG=v0.71.4' 'APR_ASSET=fake.tar.gz' "APR_TARBALL_SHA256=$FAKE_SHA" 'APR_PIN_PROFILE=baseline' \
        'IMAGE_NAME=aprender-llm-endpoint' 'GGUF_RUNG=qwen35-0.8b-q4km' "GGUF_OBJECT=$GGUF_OBJECT" > "$PIN"
    REPO_NAME="aprender-llm-endpoint-dev"
    REPO="$ACCT.dkr.ecr.us-east-1.amazonaws.com/$REPO_NAME"
    BUCKET="aprender-llm-endpoint-weights-$ACCT-dev"
    DIGEST="sha256:$(printf 'c%.0s' $(seq 1 64))"
    OTHER_DIGEST="sha256:$(printf 'd%.0s' $(seq 1 64))"
    FN=aprender-llm-endpoint-dev
    write_aws_json() {
        printf '{"account":"%s","region":"us-east-1","profile":"p","env":"dev","bucket":"%s","ecr_repo":"%s","ecr_repo_name":"%s","ecr_host":"%s.dkr.ecr.us-east-1.amazonaws.com","role_arn":"arn:aws:iam::%s:role/%s","log_group":"/aws/lambda/%s"}\n' \
            "$ACCT" "$BUCKET" "$REPO" "$REPO_NAME" "$ACCT" "$ACCT" "$FN" "$FN" > "$1/aws.json"
    }
    write_aws_json "$IDS"
    printf '{"image":"aprender-llm-endpoint:cpu-v0.71.4","tag":"v0.71.4","apr_version_line":"apr 0.71.4 (selftest)"}\n' > "$IDS/image-local.json"
    printf '{"repo":"%s","tag":"cpu-v0.71.4","digest":"%s"}\n' "$REPO" "$DIGEST" > "$IDS/image-pushed.json"
    HASH64="$(printf 'ab%.0s' $(seq 1 32))"
    export LLM_ENDPOINT_IDS_DIR="$IDS" LLM_ENDPOINT_PIN_FILE="$PIN" LLM_ENDPOINT_BIN_DIR="$T/bin" APR_API_KEY_HASH="$HASH64"
    # ---- harness ----
    PASS=0
    FAILS=0
    RC=0
    # run_deploy <log> [env args...]: the recipe under the base environment plus the given overrides
    run_deploy() {
        local log="$1"
        shift
        set +e
        env "$@" just llm-endpoint-deploy > "$log" 2>&1
        RC=$?
        set -e
    }
    # row <n> <label> <log> <zero|nonzero> <pattern> [extra 1|0]: one PASS/FAIL row
    row() {
        local n="$1" label="$2" log="$3" want="$4" pat="$5" extra="${6:-1}" ok=1
        if [ "$want" = zero ]; then
            [ "$RC" -eq 0 ] || ok=0
        else
            [ "$RC" -ne 0 ] || ok=0
        fi
        grep -Eq -- "$pat" "$log" || ok=0
        [ "$extra" = 1 ] || ok=0
        if [ "$ok" -eq 1 ]; then
            PASS=$((PASS + 1))
            echo "PASS $n $label"
        else
            FAILS=$((FAILS + 1))
            echo "FAIL $n $label (rc=$RC, want $want, pattern '$pat', extra=$extra)" >&2
            tail -4 "$log" >&2
        fi
    }
    lines() {
        if [ -f "$1" ]; then
            wc -l < "$1" | tr -d ' '
        else
            echo 0
        fi
    }
    # ---- name library (V1, V2): the case table of the one name rule, and its lint ----
    set +e
    bash "$DIR/lib/names.sh" --self-test > "$T/v1.log" 2>&1
    RC=$?
    set -e
    V1_CLEAN=1
    ! grep -q '^FAIL' "$T/v1.log" || V1_CLEAN=0
    row V1 "names.sh case table passes with no FAIL row" "$T/v1.log" zero '^PASS' "$V1_CLEAN"
    set +e
    bashrs lint "$DIR/lib/names.sh" > "$T/v2.log" 2>&1
    RC=$?
    set -e
    row V2 "bashrs lint names.sh is clean" "$T/v2.log" zero '.'
    # ---- offline cases (shim mode refuse; the log must stay empty) ----
    export SHIM_MODE=refuse
    run_deploy "$T/c1.log" LLM_ENDPOINT_PIN_FILE="$T/absent.env"
    row 1 "pin file absent -> REFUSED pin" "$T/c1.log" nonzero '^REFUSED pin:'
    printf 'tampered\n' > "$T/bin/fake.tar.gz"
    run_deploy "$T/c2.log"
    row 2 "tarball sha mismatch -> REFUSED tarball" "$T/c2.log" nonzero '^REFUSED tarball:'
    printf 'selftest fake release tarball\n' > "$T/bin/fake.tar.gz"
    mkdir -p "$T/ids-img"
    cp "$IDS/aws.json" "$IDS/image-pushed.json" "$T/ids-img/"
    printf '{"apr_version_line":"apr 0.70.0 (old)"}\n' > "$T/ids-img/image-local.json"
    run_deploy "$T/c3.log" LLM_ENDPOINT_IDS_DIR="$T/ids-img"
    row 3 "image-local.json with a wrong version line -> REFUSED image" "$T/c3.log" nonzero '^REFUSED image:'
    run_deploy "$T/c4.log" -u APR_API_KEY_HASH
    row 4 "APR_API_KEY_HASH unset -> REFUSED auth" "$T/c4.log" nonzero '^REFUSED auth:'
    run_deploy "$T/c5.log" APR_API_KEY_HASH="g$(printf 'ab%.0s' $(seq 1 31))a"
    row 5 "64 chars that are not hex -> REFUSED auth" "$T/c5.log" nonzero '^REFUSED auth:'
    mkdir -p "$T/ids-noaws"
    cp "$IDS/image-local.json" "$IDS/image-pushed.json" "$T/ids-noaws/"
    run_deploy "$T/c6.log" LLM_ENDPOINT_IDS_DIR="$T/ids-noaws"
    row 6 "aws.json absent -> REFUSED ids" "$T/c6.log" nonzero '^REFUSED ids:'
    run_deploy "$T/c7.log" DRY_RUN=1 PREFLIGHT=1
    row 7 "DRY_RUN=1 together with PREFLIGHT=1 -> REFUSED mode" "$T/c7.log" nonzero '^REFUSED mode:'
    run_deploy "$T/c8.log" DRY_RUN=1
    x=1
    [ "$(lines "$SHIM_LOG")" -eq 0 ] || x=0
    row 8 "DRY_RUN=1, consistent fixtures -> DRY-RUN OK (offline rungs 1-7; ... NOT checked ...), zero aws calls" "$T/c8.log" zero \
        '^DRY-RUN OK aprender-llm-endpoint-dev \(offline rungs 1-7; account, s3-sha and digest NOT checked;' "$x"
    AWS_CALLS="$(lines "$SHIM_LOG")"
    # ---- PREFLIGHT cases (shim mode preflight: only the three read calls are answered) ----
    export SHIM_MODE=preflight
    FIXTURE_READS=0
    pf() { # pf <n> <label> <log> <zero|nonzero> <pattern> <sts> <s3sha> <digest>
        local n="$1" label="$2" log="$3" want="$4" pat="$5" r
        : > "$T/reads$n.log"
        : > "$T/pf$n.log"
        run_deploy "$log" PREFLIGHT=1 SHIM_LOG="$T/pf$n.log" SHIM_READS="$T/reads$n.log" SHIM_STS_ACCOUNT="$6" SHIM_S3_SHA="$7" SHIM_ECR_DIGEST="$8"
        r="$(lines "$T/reads$n.log")"
        FIXTURE_READS=$((FIXTURE_READS + r))
        row "$n" "$label" "$log" "$want" "$pat" "$([ "$(lines "$T/pf$n.log")" -eq "$r" ] && echo 1 || echo 0)"
    }
    pf 9 "PREFLIGHT, sts account differs from aws.json -> REFUSED account" "$T/c9.log" nonzero '^REFUSED account:' 222222222222 "$LADDER_SHA" "$DIGEST"
    pf 10 "PREFLIGHT, right account, wrong object sha -> REFUSED s3-sha" "$T/c10.log" nonzero '^REFUSED s3-sha:' "$ACCT" "$(printf '0%.0s' $(seq 1 64))" "$DIGEST"
    pf 11 "PREFLIGHT, right account and sha, tag moved -> REFUSED digest" "$T/c11.log" nonzero '^REFUSED digest:' "$ACCT" "$LADDER_SHA" "$OTHER_DIGEST"
    pf 12 "PREFLIGHT, all three reads right -> PREFLIGHT OK (rungs 1-10; read-only AWS calls only; ...), three reads" "$T/c12.log" zero \
        '^PREFLIGHT OK aprender-llm-endpoint-dev \(rungs 1-10; read-only AWS calls only;' "$ACCT" "$LADDER_SHA" "$DIGEST"
    if [ "$(lines "$T/reads12.log")" -lt 3 ]; then
        FAILS=$((FAILS + 1))
        echo "FAIL 12 fewer than three fixture reads in the PREFLIGHT happy path" >&2
    fi
    # 13: NEGATIVE CONTROL -- the refuse shim under PREFLIGHT: the first read is refused, and a SEPARATE log shows a call
    export SHIM_MODE=refuse
    : > "$T/neg.log"
    run_deploy "$T/c13.log" PREFLIGHT=1 SHIM_LOG="$T/neg.log"
    row 13 "NEGATIVE CONTROL: PREFLIGHT with the refusing shim -> REFUSED account and a recorded call" "$T/c13.log" nonzero '^REFUSED account:' \
        "$([ "$(lines "$T/neg.log")" -ge 1 ] && echo 1 || echo 0)"
    # ---- the lifecycle recipes (shim mode permissive: canned answers, every call logged) ----
    export SHIM_MODE=permissive SHIM_STS_ACCOUNT="$ACCT" SHIM_RESOLVED_IMAGE_URI="$REPO@$DIGEST" SHIM_ECR_DIGEST="$DIGEST" \
        SHIM_S3_SHA="$LADDER_SHA" SHIM_S3_KEY="$GGUF_OBJECT" SHIM_CONCURRENCY=None SHIM_IMAGE_COUNT=1
    W="$T/w"
    printf '{"statusCode":200,"body":"{}"}\n' > "$T/p200.json"
    printf '{"statusCode":401,"body":"{}"}\n' > "$T/p401.json"
    # run_just <log> [VAR=value ...] just <recipe> ...: run a recipe under the base environment plus overrides
    run_just() {
        local log="$1"
        shift
        set +e
        env "$@" > "$log" 2>&1
        RC=$?
        set -e
    }
    # fresh_ids: a clean ids directory holding aws.json, image-local.json, image-pushed.json and function.json
    fresh_ids() {
        rm -rf "${W:?}"
        mkdir -p "$W"
        cp "$IDS/aws.json" "$IDS/image-local.json" "$IDS/image-pushed.json" "$W/"
        printf '{"function":"%s","image_digest":"%s"}\n' "$FN" "$DIGEST" > "$W/function.json"
    }
    # write_proof [hours]: a 401 proof record measured that many hours ago
    write_proof() {
        local ts
        ts="$(python3 -I -c 'import datetime, sys; print((datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(hours=int(sys.argv[1]))).strftime("%Y-%m-%dT%H:%M:%SZ"))' "${1:-0}")"
        printf '{"function":"%s","image_digest":"%s","statusCode":401,"measured_at":"%s","event_sha256":"x"}\n' "$FN" "$DIGEST" "$ts" > "$W/prove-401.json"
    }
    # jget_selftest <file> <key>: one key of a JSON record, empty when the file or key is absent
    jget_selftest() {
        python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1])).get(sys.argv[2], ""))' "$1" "$2" 2> /dev/null || true
    }
    # seen <log> <pattern>: 1 when the argv log holds the pattern, else 0
    seen() {
        if grep -qE -- "$2" "$1"; then
            echo 1
        else
            echo 0
        fi
    }
    # absent_from <log> <pattern>: 1 when the argv log does NOT hold the pattern
    absent_from() {
        if grep -qE -- "$2" "$1"; then
            echo 0
        else
            echo 1
        fi
    }
    # 14-15: prove-401 exits 3 on a 200 and 0 on a 401
    fresh_ids
    : > "$T/a14.log"
    run_just "$T/c14.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a14.log" SHIM_INVOKE_PAYLOAD="$T/p200.json" just llm-endpoint-prove-401
    x=1
    [ "$RC" -eq 3 ] || x=0
    row 14 "prove-401 on a 200 payload -> exit 3 and 401 NOT PROVEN" "$T/c14.log" nonzero '401 NOT PROVEN' "$x"
    : > "$T/a15.log"
    run_just "$T/c15.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a15.log" SHIM_INVOKE_PAYLOAD="$T/p401.json" just llm-endpoint-prove-401
    x="$(seen "$W/prove-401.json" '"statusCode": 401')"
    row 15 "prove-401 on a 401 payload -> exit 0, 401 PROVEN and a record" "$T/c15.log" zero '^401 PROVEN on aprender-llm-endpoint-dev' "$x"
    # 16: url without a proof refuses before any aws call
    fresh_ids
    : > "$T/a16.log"
    run_just "$T/c16.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_MODE=refuse SHIM_LOG="$T/a16.log" just llm-endpoint-url
    x=1
    [ "$(lines "$T/a16.log")" -eq 0 ] || x=0
    row 16 "url without prove-401.json -> REFUSED proof, zero aws calls" "$T/c16.log" nonzero '^REFUSED proof:' "$x"
    # 17: url with a valid proof but no bounded concurrency
    fresh_ids
    write_proof 0
    : > "$T/a17.log"
    run_just "$T/c17.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_MODE=preflight SHIM_LOG="$T/a17.log" SHIM_CONCURRENCY=None just llm-endpoint-url
    x="$(absent_from "$T/a17.log" 'create-function-url-config')"
    row 17 "url with a valid proof, concurrency None -> REFUSED concurrency, no URL created" "$T/c17.log" nonzero '^REFUSED concurrency:' "$x"
    # 18-19: teardown under the permissive shim
    fresh_ids
    write_proof 0
    printf '{}\n' > "$W/url.json"
    : > "$T/a18.log"
    run_just "$T/c18.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a18.log" SHIM_CONCURRENCY=0 just llm-endpoint-teardown
    x=1
    grep -qE 'lambda remove-permission .*--statement-id llm-endpoint-url-public' "$T/a18.log" || x=0
    grep -qE 'lambda remove-permission .*--statement-id llm-endpoint-invoke-via-url' "$T/a18.log" || x=0
    grep -qE 'ecr batch-delete-image .*imageTag=cpu-v0.71.4' "$T/a18.log" || x=0
    grep -q 'delete-repository' "$T/a18.log" && x=0
    grep -q 's3api delete-bucket' "$T/a18.log" || x=0
    grep -q 'aws ecr delete-repository --repository-name aprender-llm-endpoint-dev' "$T/c18.log" || x=0
    grep -q 'NOT RUN' "$T/c18.log" || x=0
    [ ! -f "$W/url.json" ] && [ ! -f "$W/prove-401.json" ] && [ ! -f "$W/function.json" ] || x=0
    row 18 "teardown: both statements removed, one tag deleted, no delete-repository (printed under NOT RUN)" "$T/c18.log" zero '^TEARDOWN OK' "$x"
    fresh_ids
    : > "$T/a19.log"
    run_just "$T/c19.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a19.log" SHIM_CONCURRENCY=0 SHIM_ECR_DIGEST="$OTHER_DIGEST" just llm-endpoint-teardown
    x="$(absent_from "$T/a19.log" 'batch-delete-image')"
    row 19 "teardown: tag moved -> SKIPPED image, no batch-delete-image" "$T/c19.log" zero 'SKIPPED image' "$x"
    # 20-24: the URL lifecycle
    fresh_ids
    write_proof 0
    : > "$T/a20.log"
    : > "$T/curl20.log"
    run_just "$T/c20.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a20.log" SHIM_CURL_LOG="$T/curl20.log" SHIM_CURL_CODE=401 SHIM_CONCURRENCY=1 just llm-endpoint-url
    x=1
    grep -qE 'lambda add-permission .*--statement-id llm-endpoint-url-public .*--function-url-auth-type NONE' "$T/a20.log" || x=0
    grep -qE 'lambda add-permission .*--statement-id llm-endpoint-invoke-via-url .*--invoked-via-function-url' "$T/a20.log" || x=0
    grep -qE 'create-function-url-config .*--auth-type NONE --invoke-mode BUFFERED' "$T/a20.log" || x=0
    grep -q '"auth_type": "NONE"' "$W/url.json" || x=0
    grep -q 'https://' "$T/c20.log" && x=0
    grep -q '/health' "$T/curl20.log" || x=0
    grep -q 'v1/chat/completions' "$T/curl20.log" || x=0
    grep -qi 'authorization' "$T/curl20.log" && x=0
    row 20 "url: both statements, 401 re-check without a key, url.json written, only the host printed" "$T/c20.log" zero '^URL OK \(401 without key verified; reserved concurrency 1' "$x"
    fresh_ids
    write_proof 0
    : > "$T/a21.log"
    run_just "$T/c21.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a21.log" SHIM_CURL_LOG="$T/curl21.log" SHIM_CURL_CODE=200 SHIM_CONCURRENCY=1 just llm-endpoint-url
    x=1
    grep -q 'lambda delete-function-url-config' "$T/a21.log" || x=0
    grep -qE 'lambda remove-permission .*--statement-id llm-endpoint-url-public' "$T/a21.log" || x=0
    grep -qE 'lambda remove-permission .*--statement-id llm-endpoint-invoke-via-url' "$T/a21.log" || x=0
    [ ! -f "$W/url.json" ] || x=0
    row 21 "url: an open answer (200) -> URL REMOVED, URL config and both statements deleted, no url.json" "$T/c21.log" nonzero 'URL REMOVED: open endpoint' "$x"
    fresh_ids
    write_proof 48
    : > "$T/a22.log"
    run_just "$T/c22.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_MODE=refuse SHIM_LOG="$T/a22.log" just llm-endpoint-url
    x=1
    [ "$(lines "$T/a22.log")" -eq 0 ] || x=0
    row 22 "url: a 48 h old proof -> REFUSED proof, zero aws calls" "$T/c22.log" nonzero '^REFUSED proof:.*older than 24 h' "$x"
    fresh_ids
    write_proof 0
    : > "$T/a23.log"
    run_just "$T/c23.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_MODE=preflight SHIM_LOG="$T/a23.log" SHIM_CONCURRENCY=1 SHIM_RESOLVED_IMAGE_URI="$REPO@$OTHER_DIGEST" just llm-endpoint-url
    x="$(absent_from "$T/a23.log" 'create-function-url-config')"
    row 23 "url: proof measured on another image digest -> REFUSED proof, no URL created" "$T/c23.log" nonzero '^REFUSED proof:' "$x"
    fresh_ids
    write_proof 0
    : > "$T/a24.log"
    run_just "$T/c24.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a24.log" SHIM_CURL_LOG="$T/curl24.log" SHIM_CURL_CODE=401 SHIM_CONCURRENCY=None ACCEPT_UNRESERVED_URL=1 just llm-endpoint-url
    x="$(seen "$W/url.json" '"risk_accepted": true')"
    row 24 "url: ACCEPT_UNRESERVED_URL=1 on an unbounded function -> WARNING and risk_accepted true" "$T/c24.log" zero '^URL OK .*risk_accepted true' "$x"
    # 25: up writes an aws.json that the deploy ladder accepts, with the environment-specific repository
    rm -rf "${T:?}/up"
    mkdir -p "$T/up"
    cp "$IDS/image-local.json" "$IDS/image-pushed.json" "$T/up/"
    : > "$T/a25.log"
    run_just "$T/c25.log" LLM_ENDPOINT_IDS_DIR="$T/up" SHIM_LOG="$T/a25.log" just llm-endpoint-up
    UP_RC=$RC
    x=1
    [ "$UP_RC" -eq 0 ] || x=0
    grep -qE 'repository-names aprender-llm-endpoint-dev( |$)' "$T/a25.log" || x=0
    grep -qE 'repository-names?.aprender-llm-endpoint( |$)' "$T/a25.log" && x=0
    grep -q 'AWSLambdaBasicExecutionRole' "$T/a25.log" || x=0
    grep -q 'put-public-access-block' "$T/a25.log" || x=0
    run_deploy "$T/c25b.log" LLM_ENDPOINT_IDS_DIR="$T/up" DRY_RUN=1
    [ "$RC" -eq 0 ] || x=0
    RC=$UP_RC
    row 25 "up: env-specific repository, posture calls, and an aws.json that DRY_RUN deploy accepts" "$T/c25.log" zero '^UP OK aprender-llm-endpoint-dev' "$x"
    # V3-V8: the variant. A variant names its own function, repository, bucket and ids; a record names the
    # function it was written for, and a reader refuses a record that names another.
    rm -rf "${T:?}/up2b"
    mkdir -p "$T/up2b"
    cp "$IDS/image-local.json" "$IDS/image-pushed.json" "$T/up2b/"
    : > "$T/a-v3.log"
    run_just "$T/c-v3.log" LLM_ENDPOINT_IDS_DIR="$T/up2b" SHIM_LOG="$T/a-v3.log" just llm-endpoint-up dev ze-kasher-dev variant=2b
    x=1
    [ "$(jget_selftest "$T/up2b/aws.json" name)" = "aprender-llm-endpoint-2b-dev" ] || x=0
    [ "$(jget_selftest "$T/up2b/aws.json" variant)" = "2b" ] || x=0
    [ "$(jget_selftest "$T/up2b/aws.json" suffix)" = "2b-dev" ] || x=0
    [ "$(jget_selftest "$T/up2b/aws.json" ecr_repo_name)" = "aprender-llm-endpoint-2b-dev" ] || x=0
    case "$(jget_selftest "$T/up2b/aws.json" bucket)" in
        *-2b-dev) ;;
        *) x=0 ;;
    esac
    grep -qE 'repository-names aprender-llm-endpoint-2b-dev( |$)' "$T/a-v3.log" || x=0
    grep -q 'variant=2b' "$T/a-v3.log" || x=0
    grep -qE 'aprender-llm-endpoint-dev( |$)' "$T/a-v3.log" && x=0
    row V3 "up variant=2b: 2b-dev names in aws.json and in the calls, tagged variant=2b, no default-name call" "$T/c-v3.log" zero '^UP OK aprender-llm-endpoint-2b-dev' "$x"
    # V4: the 2b ids and a 2b pin pass the offline deploy ladder
    cp "$IDS/image-pushed.json" "$IDS/image-local.json" "$T/up2b/"
    sed -e 's/^GGUF_RUNG=.*/GGUF_RUNG=qwen35-2b-q4km/' -e 's/^GGUF_OBJECT=.*/GGUF_OBJECT=Qwen3.5-2B-Q4_K_M.gguf/' "$PIN" > "$T/pin-2b.env"
    : > "$T/a-v4.log"
    run_just "$T/c-v4.log" LLM_ENDPOINT_IDS_DIR="$T/up2b" LLM_ENDPOINT_PIN_FILE="$T/pin-2b.env" SHIM_MODE=refuse SHIM_LOG="$T/a-v4.log" DRY_RUN=1 \
        just llm-endpoint-deploy dev ze-kasher-dev variant=2b
    row V4 "deploy variant=2b DRY_RUN with the up2b ids and a 2b pin -> DRY-RUN OK aprender-llm-endpoint-2b-dev, zero aws calls" "$T/c-v4.log" zero \
        '^DRY-RUN OK aprender-llm-endpoint-2b-dev ' "$([ "$(lines "$T/a-v4.log")" -eq 0 ] && echo 1 || echo 0)"
    # V5: the default-variant record cannot serve variant 2b, and the 2b record cannot serve the default
    : > "$T/a-v5.log"
    run_just "$T/c-v5.log" LLM_ENDPOINT_IDS_DIR="$IDS" SHIM_MODE=refuse SHIM_LOG="$T/a-v5.log" DRY_RUN=1 just llm-endpoint-deploy dev ze-kasher-dev variant=2b
    row V5 "deploy variant=2b against the legacy dev record -> REFUSED ids, zero aws calls" "$T/c-v5.log" nonzero '^REFUSED ids:' \
        "$([ "$(lines "$T/a-v5.log")" -eq 0 ] && echo 1 || echo 0)"
    : > "$T/a-v5b.log"
    run_just "$T/c-v5b.log" LLM_ENDPOINT_IDS_DIR="$T/up2b" SHIM_MODE=refuse SHIM_LOG="$T/a-v5b.log" DRY_RUN=1 just llm-endpoint-deploy
    row V5b "deploy with no variant against the 2b record -> REFUSED ids, zero aws calls" "$T/c-v5b.log" nonzero '^REFUSED ids:' \
        "$([ "$(lines "$T/a-v5b.log")" -eq 0 ] && echo 1 || echo 0)"
    # V6: a bad variant is refused before any pin, ids or aws work
    : > "$T/a-v6.log"
    run_just "$T/c-v6.log" SHIM_MODE=refuse SHIM_LOG="$T/a-v6.log" DRY_RUN=1 just llm-endpoint-deploy dev ze-kasher-dev variant=2B
    row V6 "deploy variant=2B (uppercase) -> REFUSED variant, zero aws calls" "$T/c-v6.log" nonzero '^REFUSED variant:' \
        "$([ "$(lines "$T/a-v6.log")" -eq 0 ] && echo 1 || echo 0)"
    : > "$T/a-v6b.log"
    run_just "$T/c-v6b.log" SHIM_MODE=refuse SHIM_LOG="$T/a-v6b.log" DRY_RUN=1 just llm-endpoint-deploy dev ze-kasher-dev variant=aprender-llm-endpoint-2b
    row V6b "deploy variant=aprender-llm-endpoint-2b (a whole name) -> REFUSED variant, zero aws calls" "$T/c-v6b.log" nonzero '^REFUSED variant:' \
        "$([ "$(lines "$T/a-v6b.log")" -eq 0 ] && echo 1 || echo 0)"
    : > "$T/a-v6c.log"
    run_just "$T/c-v6c.log" SHIM_MODE=refuse SHIM_LOG="$T/a-v6c.log" just llm-endpoint-up dev ze-kasher-dev variant=2B
    row V6c "up variant=2B (uppercase) -> REFUSED variant, zero aws calls" "$T/c-v6c.log" nonzero '^REFUSED variant:' \
        "$([ "$(lines "$T/a-v6c.log")" -eq 0 ] && echo 1 || echo 0)"
    # V7: a named default-variant record (the new format) is accepted with no variant
    rm -rf "${T:?}/named-default"
    mkdir -p "$T/named-default"
    cp "$IDS/image-local.json" "$IDS/image-pushed.json" "$T/named-default/"
    python3 -I -c 'import json, sys; d = json.load(open(sys.argv[1])); d.update({"name": "aprender-llm-endpoint-dev", "variant": "", "suffix": "dev"}); json.dump(d, open(sys.argv[2], "w"))' \
        "$IDS/aws.json" "$T/named-default/aws.json"
    : > "$T/a-v7.log"
    run_just "$T/c-v7.log" LLM_ENDPOINT_IDS_DIR="$T/named-default" SHIM_MODE=refuse SHIM_LOG="$T/a-v7.log" DRY_RUN=1 just llm-endpoint-deploy
    row V7 "deploy, no variant, a record that names aprender-llm-endpoint-dev -> DRY-RUN OK, zero aws calls" "$T/c-v7.log" zero \
        '^DRY-RUN OK aprender-llm-endpoint-dev ' "$([ "$(lines "$T/a-v7.log")" -eq 0 ] && echo 1 || echo 0)"
    # V7b-V7d: up checks the record already in its ids dir before it creates anything
    # only_sts <log>: 1 when every recorded aws call is the account read, i.e. nothing was created or written
    only_sts() {
        if grep -qvE '^sts get-caller-identity' "$1"; then echo 0; else echo 1; fi
    }
    cp "$T/up2b/aws.json" "$T/up2b-aws.before"
    : > "$T/a-v7b.log"
    run_just "$T/c-v7b.log" LLM_ENDPOINT_IDS_DIR="$T/up2b" SHIM_LOG="$T/a-v7b.log" just llm-endpoint-up
    row V7b "up with no variant over the 2b record -> REFUSED ids, only the account read reached the shim, record untouched" "$T/c-v7b.log" nonzero '^REFUSED ids:' \
        "$(if [ "$(only_sts "$T/a-v7b.log")" = 1 ] && cmp -s "$T/up2b/aws.json" "$T/up2b-aws.before"; then echo 1; else echo 0; fi)"
    rm -rf "${T:?}/up-legacy"
    mkdir -p "$T/up-legacy"
    cp "$IDS/aws.json" "$T/up-legacy/"
    : > "$T/a-v7c.log"
    run_just "$T/c-v7c.log" LLM_ENDPOINT_IDS_DIR="$T/up-legacy" SHIM_LOG="$T/a-v7c.log" just llm-endpoint-up dev ze-kasher-dev variant=2b
    row V7c "up variant=2b over a legacy dev record -> REFUSED ids, only the account read reached the shim" "$T/c-v7c.log" nonzero '^REFUSED ids:' "$(only_sts "$T/a-v7c.log")"
    : > "$T/a-v7d.log"
    run_just "$T/c-v7d.log" LLM_ENDPOINT_IDS_DIR="$T/up2b" SHIM_LOG="$T/a-v7d.log" just llm-endpoint-up dev ze-kasher-dev variant=2b
    row V7d "up variant=2b over its own 2b record -> UP OK (the check accepts a matching record)" "$T/c-v7d.log" zero '^UP OK aprender-llm-endpoint-2b-dev' \
        "$([ "$(only_sts "$T/a-v7d.log")" = 0 ] && echo 1 || echo 0)"
    : > "$T/a-v7e.log"
    run_just "$T/c-v7e.log" LLM_ENDPOINT_IDS_DIR="$T/up-legacy" SHIM_LOG="$T/a-v7e.log" just llm-endpoint-up
    row V7e "up with no variant over a legacy dev record -> UP OK (legacy still serves the default)" "$T/c-v7e.log" zero '^UP OK aprender-llm-endpoint-dev' \
        "$([ "$(only_sts "$T/a-v7e.log")" = 0 ] && echo 1 || echo 0)"
    # V8: image push=1 with a variant pushes to the variant's repository and records in the variant's ids
    # (positional: just binds in order, so `1 dev variant=2b`)
    rm -rf "${T:?}/v8bin"
    mkdir -p "$T/v8bin" "$T/v8src/fake"
    printf 'selftest apr\n' > "$T/v8src/fake/apr"
    tar czf "$T/v8bin/fake.tar.gz" -C "$T/v8src" fake/apr
    (cd "$T/v8bin" && shasum -a 256 fake.tar.gz > fake.tar.gz.sha256)
    V8SHA="$(shasum -a 256 "$T/v8bin/fake.tar.gz" | awk '{print $1}')"
    sed -e "s/^APR_ASSET=.*/APR_ASSET=fake.tar.gz/" -e "s/^APR_TARBALL_SHA256=.*/APR_TARBALL_SHA256=$V8SHA/" -e 's/^APR_TAG=.*/APR_TAG=v0.71.4/' "$DIR/pin.env" > "$T/pin-v8.env"
    cp "$IDS/image-pushed.json" "$T/v8-default-pushed.before"
    rm -f "$T/up2b/image-pushed.json"
    : > "$T/a-v8.log"
    run_just "$T/c-v8.log" LLM_ENDPOINT_IDS_DIR="$T/up2b" LLM_ENDPOINT_BIN_DIR="$T/v8bin" LLM_ENDPOINT_PIN_FILE="$T/pin-v8.env" SHIM_LOG="$T/a-v8.log" \
        just llm-endpoint-image 1 dev variant=2b
    x=1
    case "$(jget_selftest "$T/up2b/image-pushed.json" repo)" in
        */aprender-llm-endpoint-2b-dev) ;;
        *) x=0 ;;
    esac
    cmp -s "$IDS/image-pushed.json" "$T/v8-default-pushed.before" || x=0
    grep -qE '^push .*/aprender-llm-endpoint-2b-dev:' "$T/a-v8.log" || x=0
    row V8 "image 1 dev variant=2b -> pushed to the 2b-dev repository, recorded in the 2b ids, default record untouched" "$T/c-v8.log" zero '^PUSHED .*/aprender-llm-endpoint-2b-dev:' "$x"
    # V9-V13: the remaining lifecycle recipes take the variant as their LAST positional (just binds in order,
    # so `dev ze-kasher-dev variant=2b`); teardown derives its function name from the library.
    # V9: teardown variant=2b removes only 2b-dev-named resources
    rm -rf "${T:?}/td2b"
    mkdir -p "$T/td2b"
    cp "$T/up2b/aws.json" "$T/up2b/image-local.json" "$T/up2b/image-pushed.json" "$T/td2b/"
    printf '{"function":"aprender-llm-endpoint-2b-dev","name":"aprender-llm-endpoint-2b-dev","image_digest":"%s"}\n' "$DIGEST" > "$T/td2b/function.json"
    cp -R "$T/td2b" "$T/td2b-keep"
    : > "$T/a-v9.log"
    run_just "$T/c-v9.log" LLM_ENDPOINT_IDS_DIR="$T/td2b" SHIM_LOG="$T/a-v9.log" SHIM_CONCURRENCY=0 just llm-endpoint-teardown dev ze-kasher-dev variant=2b
    x=1
    grep -qE -- '--function-name aprender-llm-endpoint-2b-dev( |$)' "$T/a-v9.log" || x=0
    grep -qE -- '--role-name aprender-llm-endpoint-2b-dev( |$)' "$T/a-v9.log" || x=0
    grep -qE -- '--log-group-name /aws/lambda/aprender-llm-endpoint-2b-dev( |$)' "$T/a-v9.log" || x=0
    grep -qE -- "--bucket aprender-llm-endpoint-weights-$ACCT-2b-dev( |\$)" "$T/a-v9.log" || x=0
    grep -qE 'aprender-llm-endpoint-dev( |$)' "$T/a-v9.log" && x=0
    row V9 "teardown variant=2b: only 2b-dev names reach the shim (function, role, log group, bucket), none of the default name" "$T/c-v9.log" zero '^TEARDOWN OK aprender-llm-endpoint-2b-dev' "$x"
    # V10: a legacy record cannot be torn down as variant 2b; V11: a 2b record cannot be torn down as the default.
    # check_record runs before the account read, so the shim log is empty.
    : > "$T/a-v10.log"
    run_just "$T/c-v10.log" LLM_ENDPOINT_IDS_DIR="$IDS" SHIM_LOG="$T/a-v10.log" SHIM_CONCURRENCY=0 just llm-endpoint-teardown dev ze-kasher-dev variant=2b
    row V10 "teardown variant=2b over the legacy dev record -> REFUSED ids, nothing reached the shim (not even sts)" "$T/c-v10.log" nonzero '^REFUSED ids:' \
        "$([ "$(lines "$T/a-v10.log")" -eq 0 ] && echo 1 || echo 0)"
    : > "$T/a-v11.log"
    run_just "$T/c-v11.log" LLM_ENDPOINT_IDS_DIR="$T/td2b-keep" SHIM_LOG="$T/a-v11.log" SHIM_CONCURRENCY=0 just llm-endpoint-teardown
    row V11 "teardown with no variant over a 2b record -> REFUSED ids, nothing reached the shim" "$T/c-v11.log" nonzero '^REFUSED ids:' \
        "$([ "$(lines "$T/a-v11.log")" -eq 0 ] && echo 1 || echo 0)"
    # V12: weights codebuild variant=2b names its role and project from the library and tags the role
    L2B="$(python3 -I "$DIR/ladder_pin.py" contracts/model-capability-ladder-v1.yaml qwen35-2b-q4km)"
    rm -rf "${T:?}/w2b"
    mkdir -p "$T/w2b"
    cp "$T/up2b/aws.json" "$T/w2b/"
    : > "$T/a-v12.log"
    run_just "$T/c-v12.log" LLM_ENDPOINT_IDS_DIR="$T/w2b" LLM_ENDPOINT_PIN_FILE="$T/pin-2b.env" SHIM_S3_SHA="$L2B" SHIM_ROLE_EXISTS=0 SHIM_LOG="$T/a-v12.log" \
        just llm-endpoint-weights codebuild dev ze-kasher-dev variant=2b
    x=1
    grep -qE 'iam create-role .*--role-name aprender-llm-endpoint-2b-dev-codebuild( |$)' "$T/a-v12.log" || x=0
    grep -qE 'iam create-role .*Key=variant,Value=2b' "$T/a-v12.log" || x=0
    grep -qE 'codebuild create-project ' "$T/a-v12.log" || x=0
    grep -qE 'codebuild start-build .*--project-name aprender-llm-endpoint-weights-2b-dev( |$)' "$T/a-v12.log" || x=0
    [ "$(jget_selftest "$T/w2b/weights.json" codebuild_role)" = "aprender-llm-endpoint-2b-dev-codebuild" ] || x=0
    [ "$(jget_selftest "$T/w2b/weights.json" codebuild_project)" = "aprender-llm-endpoint-weights-2b-dev" ] || x=0
    row V12 "weights codebuild variant=2b: role ...-2b-dev-codebuild (tagged variant=2b) and project ...-weights-2b-dev, both recorded" "$T/c-v12.log" zero '^WEIGHTS OK' "$x"
    # V13: no literal endpoint name survives in the nine lifecycle recipes. The pattern must also match
    # in a file that does hold such literals (the self-test's own fixtures), or a zero count proves nothing.
    LITPAT='aprender-llm-endpoint-[a-z-]*(\$\{?ENV\}?|dev|prod)\b'
    x=1
    : > "$T/c-v13.log"
    for rc_name in image deploy up weights prove-401 url warm teardown measure; do
        just --show "llm-endpoint-$rc_name" 2> /dev/null | grep -vE '^[[:space:]]*#' > "$T/show-$rc_name.txt" || true
        n="$(grep -cE "$LITPAT" "$T/show-$rc_name.txt" || true)"
        echo "literal count llm-endpoint-$rc_name: $n" >> "$T/c-v13.log"
        if [ "$n" != 0 ]; then
            grep -nE "$LITPAT" "$T/show-$rc_name.txt" >> "$T/c-v13.log" || true
            x=0
        fi
        [ -s "$T/show-$rc_name.txt" ] || x=0
    done
    just --show llm-endpoint-selftest 2> /dev/null > "$T/show-selftest.txt" || true
    ctl="$(grep -cE "$LITPAT" "$T/show-selftest.txt" || true)"
    echo "must-match control (selftest): $ctl" >> "$T/c-v13.log"
    [ "$ctl" -ge 1 ] || x=0
    RC=0
    row V13 "no recipe of the nine holds an endpoint-name literal; the pattern does match in the self-test (control)" "$T/c-v13.log" zero '^must-match control' "$x"
    # V14-V16: teardown deletes the CodeBuild project and role that weights.json names, so those names must
    # belong to this function before any delete runs
    write_weights() {
        printf '{"mode":"codebuild","bucket":"b","key":"k","codebuild_project":"%s","codebuild_role":"%s"}\n' "$2" "$3" > "$1/weights.json"
    }
    write_weights "$T/td2b-keep" aprender-llm-endpoint-weights-dev aprender-llm-endpoint-dev-codebuild
    : > "$T/a-v14.log"
    run_just "$T/c-v14.log" LLM_ENDPOINT_IDS_DIR="$T/td2b-keep" SHIM_LOG="$T/a-v14.log" SHIM_CONCURRENCY=0 just llm-endpoint-teardown dev ze-kasher-dev variant=2b
    row V14 "teardown variant=2b with a weights.json naming the default's CodeBuild project and role -> REFUSED ids, no delete call" "$T/c-v14.log" nonzero '^REFUSED ids:.*does not belong to' \
        "$(if grep -qE ' delete-' "$T/a-v14.log"; then echo 0; else echo 1; fi)"
    fresh_ids
    write_weights "$W" aprender-llm-endpoint-weights-dev aprender-llm-endpoint-codebuild-dev
    : > "$T/a-v15.log"
    run_just "$T/c-v15.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a-v15.log" SHIM_CONCURRENCY=0 just llm-endpoint-teardown
    row V15 "teardown, default variant, a weights.json naming aprender-llm-endpoint-codebuild-dev (variant codebuild's function role) -> REFUSED ids, no delete call" "$T/c-v15.log" nonzero '^REFUSED ids:.*does not belong to' \
        "$(if grep -qE ' delete-' "$T/a-v15.log"; then echo 0; else echo 1; fi)"
    write_weights "$T/td2b-keep" aprender-llm-endpoint-weights-2b-dev aprender-llm-endpoint-2b-dev-codebuild
    : > "$T/a-v16.log"
    run_just "$T/c-v16.log" LLM_ENDPOINT_IDS_DIR="$T/td2b-keep" SHIM_LOG="$T/a-v16.log" SHIM_CONCURRENCY=0 just llm-endpoint-teardown dev ze-kasher-dev variant=2b
    x=1
    grep -qE 'codebuild delete-project .*--name aprender-llm-endpoint-weights-2b-dev( |$)' "$T/a-v16.log" || x=0
    grep -qE 'iam delete-role .*--role-name aprender-llm-endpoint-2b-dev-codebuild( |$)' "$T/a-v16.log" || x=0
    row V16 "teardown variant=2b with its own derived CodeBuild names -> TEARDOWN OK, deletes those" "$T/c-v16.log" zero '^TEARDOWN OK aprender-llm-endpoint-2b-dev' "$x"
    # V17: readback reads and records, it never writes, and it never records a value of an environment variable
    rm -rf "${T:?}/rb2b"
    mkdir -p "$T/rb2b"
    cp "$T/up2b/aws.json" "$T/rb2b/"
    python3 -I -c 'import json, sys; json.dump({"function": sys.argv[2], "name": sys.argv[2], "variant": "2b", "env": "dev"}, open(sys.argv[1], "w"))' "$T/rb2b/function.json" aprender-llm-endpoint-2b-dev
    : > "$T/a-v17.log"
    run_just "$T/c-v17.log" LLM_ENDPOINT_IDS_DIR="$T/rb2b" SHIM_LOG="$T/a-v17.log" SHIM_RB_NAME=aprender-llm-endpoint-2b-dev SHIM_URL_ABSENT=1 SHIM_CONCURRENCY=1 \
        APR_API_KEY_HASH="$HASH64" just llm-endpoint-readback dev ze-kasher-dev variant=2b
    x=1
    [ "$(find "$T/rb2b" -name 'readback-*.json' | wc -l | tr -d ' ')" -eq 1 ] || x=0
    RBF="$(find "$T/rb2b" -name 'readback-*.json' | head -1)"
    [ "$(jget_selftest "$RBF" function_url)" = absent ] || x=0
    [ "$(jget_selftest "$RBF" function_name)" = aprender-llm-endpoint-2b-dev ] || x=0
    grep -q "$HASH64" "$RBF" && x=0
    grep -q "$HASH64" "$T/c-v17.log" && x=0
    grep -q 'lambda get-function ' "$T/a-v17.log" || x=0
    grep -q 'lambda get-function-url-config ' "$T/a-v17.log" || x=0
    grep -qE '^[a-z0-9-]+ (create|update|put|delete|add|remove|tag)' "$T/a-v17.log" && x=0
    row V17 "readback variant=2b: one readback-*.json with function_url absent and no env value, read calls only (no write verb in the shim log)" "$T/c-v17.log" zero '^READBACK OK aprender-llm-endpoint-2b-dev ' "$x"
    # V18: the real 2b pin file. Its non-GGUF lines equal pin.env (the image build reads pin.env, deploy reads
    # pin-2b.env, so a drift in APR_* or LWA_* would deploy an image the pin does not describe). The deploy
    # derives its pin path (no LLM_ENDPOINT_PIN_FILE): it runs in a scratch working directory whose
    # deploy/llm-endpoint links to the repository's, except pin-2b.env, a copy of the real file with only the
    # two APR_ASSET/APR_TARBALL_SHA256 lines pointed at the V8 fixture tarball (the real release tarball is not offline).
    rm -rf "${T:?}/v18wd"
    mkdir -p "$T/v18wd/deploy/llm-endpoint"
    for f in "$DIR"/* "$DIR"/.[!.]*; do
        case "$(basename "$f")" in pin-2b.env | .ids) continue ;; esac
        ln -s "$PWD/$f" "$T/v18wd/deploy/llm-endpoint/$(basename "$f")"
    done
    ln -s "$PWD/contracts" "$T/v18wd/contracts"
    sed -e "s/^APR_ASSET=.*/APR_ASSET=fake.tar.gz/" -e "s/^APR_TARBALL_SHA256=.*/APR_TARBALL_SHA256=$V8SHA/" "$DIR/pin-2b.env" > "$T/v18wd/deploy/llm-endpoint/pin-2b.env"
    : > "$T/a-v18.log"
    run_just "$T/c-v18.log" -u LLM_ENDPOINT_PIN_FILE LLM_ENDPOINT_IDS_DIR="$T/up2b" LLM_ENDPOINT_BIN_DIR="$T/v8bin" SHIM_MODE=refuse SHIM_LOG="$T/a-v18.log" DRY_RUN=1 \
        just --justfile "$PWD/justfile" --working-directory "$T/v18wd" llm-endpoint-deploy dev ze-kasher-dev variant=2b
    x=1
    [ "$(grep -vE '^(#|GGUF_(RUNG|OBJECT)=)' "$DIR/pin.env" | cksum)" = "$(grep -vE '^(#|GGUF_(RUNG|OBJECT)=)' "$DIR/pin-2b.env" | cksum)" ] || x=0
    [ "$(grep -c '^GGUF_RUNG=qwen35-2b-q4km$' "$DIR/pin-2b.env")" -eq 1 ] || x=0
    [ "$(lines "$T/a-v18.log")" -eq 0 ] || x=0
    row V18 "pin-2b.env equals pin.env but for the two GGUF keys; deploy variant=2b derives and reads it -> DRY-RUN OK with the 2b gguf hash" "$T/c-v18.log" zero \
        '^DRY-RUN OK aprender-llm-endpoint-2b-dev .*gguf aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223' "$x"
    # V19: readback records only that a Function URL is present, never its host (the record gets committed)
    : > "$T/a-v19.log"
    rm -f "$T"/rb2b/readback-*.json
    run_just "$T/c-v19.log" LLM_ENDPOINT_IDS_DIR="$T/rb2b" SHIM_LOG="$T/a-v19.log" SHIM_RB_NAME=aprender-llm-endpoint-2b-dev SHIM_URL=https://secrethost9z.lambda-url.us-east-1.on.aws/ SHIM_CONCURRENCY=1 \
        just llm-endpoint-readback dev ze-kasher-dev variant=2b
    x=1
    RBF="$(find "$T/rb2b" -name 'readback-*.json' | head -1)"
    [ "$(jget_selftest "$RBF" function_url)" = present ] || x=0
    grep -q 'secrethost9z' "$RBF" && x=0
    grep -q 'secrethost9z' "$T/c-v19.log" && x=0
    row V19 "readback with a URL config: function_url is 'present', the host appears in neither the record nor stdout" "$T/c-v19.log" zero '^READBACK OK aprender-llm-endpoint-2b-dev ' "$x"
    # V20-V22: variant= in the wrong positional slot is refused, never silently bound to the default name
    : > "$T/a-v20.log"
    run_just "$T/c-v20.log" LLM_ENDPOINT_IDS_DIR="$IDS" SHIM_LOG="$T/a-v20.log" SHIM_CONCURRENCY=0 just llm-endpoint-teardown env=dev variant=iso
    row V20 "teardown env=dev variant=iso (variant lands in the profile slot) -> REFUSED args, nothing reached the shim" "$T/c-v20.log" nonzero '^REFUSED args: variant= must be the last positional' \
        "$([ "$(lines "$T/a-v20.log")" -eq 0 ] && echo 1 || echo 0)"
    : > "$T/a-v21.log"
    run_just "$T/c-v21.log" LLM_ENDPOINT_IDS_DIR="$T/rb2b" SHIM_LOG="$T/a-v21.log" APR_PROBE_KEY=k just llm-endpoint-measure dev ze-kasher-dev variant=2b
    row V21 "measure dev ze-kasher-dev variant=2b (variant lands in the label slot) -> REFUSED args, nothing reached the shim" "$T/c-v21.log" nonzero '^REFUSED args: variant= must be the last positional' \
        "$([ "$(lines "$T/a-v21.log")" -eq 0 ] && echo 1 || echo 0)"
    : > "$T/a-v22.log"
    run_just "$T/c-v22.log" LLM_ENDPOINT_IDS_DIR="$T/up-legacy" SHIM_LOG="$T/a-v22.log" just llm-endpoint-up dev variant=iso
    row V22 "up dev variant=iso (variant lands in the profile slot) -> REFUSED args, nothing reached the shim" "$T/c-v22.log" nonzero '^REFUSED args: variant= must be the last positional' \
        "$([ "$(lines "$T/a-v22.log")" -eq 0 ] && echo 1 || echo 0)"
    # V23: teardown cross-checks the names a name-matching aws.json carries before any write
    x=1
    : > "$T/c-v23.log"
    for field in bucket ecr_repo_name log_group role_arn; do
        rm -rf "${T:?}/tp"
        mkdir -p "$T/tp"
        cp "$T/rb2b/aws.json" "$T/rb2b/function.json" "$T/tp/"
        python3 -I -c 'import json, sys; p, k = sys.argv[1:3]; d = json.load(open(p)); d[k] = "tampered-" + str(d.get(k, "x")); json.dump(d, open(p, "w"))' "$T/tp/aws.json" "$field"
        : > "$T/a-v23.log"
        run_just "$T/c-v23-$field.log" LLM_ENDPOINT_IDS_DIR="$T/tp" SHIM_LOG="$T/a-v23.log" SHIM_CONCURRENCY=0 just llm-endpoint-teardown dev ze-kasher-dev variant=2b
        [ "$RC" -ne 0 ] || x=0
        grep -q '^REFUSED ids:' "$T/c-v23-$field.log" || x=0
        [ "$(lines "$T/a-v23.log")" -eq 0 ] && ! grep -q ' delete-' "$T/a-v23.log" || x=0
        echo "tampered $field: rc=$RC" >> "$T/c-v23.log"
    done
    RC=0
    row V23 "teardown variant=2b: a tampered bucket, ecr_repo_name, log_group or role_arn in aws.json -> REFUSED ids each, no delete call" "$T/c-v23.log" zero '^tampered role_arn: rc=2' "$x"
    # 26-28: the live deploy under the permissive shim
    fresh_ids
    rm -f "$W/function.json"
    : > "$T/a26.log"
    run_deploy "$T/c26.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a26.log" SHIM_FN_EXISTS=0
    x=1
    grep -qE 'lambda create-function .*--memory-size 10240 --timeout 900 --ephemeral-storage Size=10240' "$T/a26.log" || x=0
    grep -qE 'lambda create-function .*--architectures arm64' "$T/a26.log" || x=0
    grep -q 'lambda wait function-active-v2' "$T/a26.log" || x=0
    grep -q '"reserved_concurrency": null' "$W/function.json" || x=0
    grep -q "$HASH64" "$T/a26.log" && x=0
    grep -q "$HASH64" "$T/c26.log" && x=0
    grep -q "$HASH64" "$W/function.json" && x=0
    row 26 "deploy live, function absent -> create-function with the readback values, no key hash anywhere" "$T/c26.log" zero '^DEPLOY OK aprender-llm-endpoint-dev' "$x"
    fresh_ids
    rm -f "$W/function.json"
    : > "$T/a27.log"
    run_deploy "$T/c27.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a27.log" SHIM_FN_EXISTS=1 RESERVED_CONCURRENCY=1 SHIM_CONCURRENCY=1
    x=1
    grep -q 'lambda update-function-configuration' "$T/a27.log" || x=0
    grep -q "lambda update-function-code .*--image-uri $REPO@$DIGEST" "$T/a27.log" || x=0
    grep -q 'lambda create-function ' "$T/a27.log" && x=0
    grep -q 'put-function-concurrency .*--reserved-concurrent-executions 1' "$T/a27.log" || x=0
    grep -q '"reserved_concurrency": 1' "$W/function.json" || x=0
    row 27 "deploy live, function present -> update path, reserved concurrency applied and recorded" "$T/c27.log" zero '^DEPLOY OK' "$x"
    fresh_ids
    rm -f "$W/function.json"
    : > "$T/a28.log"
    run_deploy "$T/c28.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a28.log" SHIM_FN_EXISTS=1 "SHIM_CFG=Active 3008 900 10240 arm64"
    x=1
    [ ! -f "$W/function.json" ] || x=0
    row 28 "deploy live, memory reads 3008 -> REFUSED readback, no function.json" "$T/c28.log" nonzero '^REFUSED readback:' "$x"
    # 29-30: teardown refusals and the spike-031 guard
    fresh_ids
    : > "$T/a29.log"
    run_just "$T/c29.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a29.log" SHIM_CONCURRENCY=0 SHIM_POLICY_IDS=1 just llm-endpoint-teardown
    x="$(absent_from "$T/a29.log" 'lambda delete-function ')"
    row 29 "teardown: get-policy still lists a URL statement -> REFUSED policy, function not deleted" "$T/c29.log" nonzero '^REFUSED policy:' "$x"
    fresh_ids
    : > "$T/a30.log"
    run_just "$T/c30.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a30.log" SHIM_CONCURRENCY=0 TEARDOWN_SPIKE_031=1 just llm-endpoint-teardown
    x=1
    grep -q 'lambda delete-function --function-name llm-spike-031-lambda-0-8b' "$T/a30.log" || x=0
    grep -q 'delete-repository' "$T/a30.log" && x=0
    grep -q 'aws ecr delete-repository --repository-name llm-spike-031' "$T/c30.log" || x=0
    row 30 "teardown: TEARDOWN_SPIKE_031=1 removes the spike leftovers but never runs delete-repository" "$T/c30.log" zero '^TEARDOWN OK' "$x"
    fresh_ids
    : > "$T/a30b.log"
    run_just "$T/c31.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a30b.log" SHIM_CONCURRENCY=0 just llm-endpoint-teardown
    x="$(absent_from "$T/a30b.log" 'llm-spike-031')"
    row 31 "teardown: without TEARDOWN_SPIKE_031 no spike-031 resource is touched" "$T/c31.log" zero '^TEARDOWN OK' "$x"
    # 32: warm
    fresh_ids
    : > "$T/a32.log"
    run_just "$T/c32.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a32.log" SHIM_INVOKE_PAYLOAD="$T/p200.json" APR_PROBE_KEY=probe-key-selftest just llm-endpoint-warm
    x=1
    grep -q 'probe-key-selftest' "$T/a32.log" && x=0
    grep -q 'probe-key-selftest' "$T/c32.log" && x=0
    row 32 "warm: authenticated 1-token invoke, the key appears in no argv and no output" "$T/c32.log" zero '^WARM OK' "$x"
    : > "$T/a33.log"
    run_just "$T/c33.log" -u APR_PROBE_KEY LLM_ENDPOINT_IDS_DIR="$W" SHIM_MODE=refuse SHIM_LOG="$T/a33.log" just llm-endpoint-warm
    x=1
    [ "$(lines "$T/a33.log")" -eq 0 ] || x=0
    row 33 "warm without APR_PROBE_KEY -> REFUSED probe-key, zero aws calls" "$T/c33.log" nonzero '^REFUSED probe-key:' "$x"
    # 34-36: weights
    fresh_ids
    : > "$T/a34.log"
    run_just "$T/c34.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a34.log" just llm-endpoint-weights
    x="$(absent_from "$T/a34.log" 's3 cp')"
    row 34 "weights copy: object already carries the ladder pin -> WEIGHTS OK without a copy" "$T/c34.log" zero '^WEIGHTS OK' "$x"
    fresh_ids
    : > "$T/a43.log"
    run_just "$T/c43.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a43.log" SHIM_DST_EMPTY=1 just llm-endpoint-weights
    x=1
    grep -qE 's3api copy-object .*--copy-source llm-spike-031-.*--metadata-directive COPY' "$T/a43.log" || x=0
    grep -qE 's3 cp ' "$T/a43.log" && x=0
    row 43 "weights copy: destination empty -> one copy-object with the COPY directive, then the metadata is re-read -> WEIGHTS OK" "$T/c43.log" zero '^WEIGHTS OK' "$x"
    fresh_ids
    : > "$T/a35.log"
    run_just "$T/c35.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a35.log" SHIM_S3_SHA=notthepin just llm-endpoint-weights
    x="$(absent_from "$T/a35.log" 's3 cp')"
    row 35 "weights copy: source without the matching hash -> REFUSED source-meta, nothing copied" "$T/c35.log" nonzero '^REFUSED source-meta:' "$x"
    fresh_ids
    : > "$T/a36.log"
    run_just "$T/c36.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a36.log" just llm-endpoint-weights codebuild
    x=1
    grep -q 'codebuild create-project' "$T/a36.log" || x=0
    grep -q 'codebuild start-build' "$T/a36.log" || x=0
    grep -q '"mode": "codebuild"' "$W/weights.json" || x=0
    row 36 "weights codebuild: role, project, build and the metadata check -> WEIGHTS OK, recorded" "$T/c36.log" zero '^WEIGHTS OK' "$x"
    : > "$T/a37.log"
    run_just "$T/c37.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a37.log" SHIM_CONCURRENCY=0 just llm-endpoint-teardown
    x=1
    grep -qE 'codebuild delete-project .*--name aprender-llm-endpoint-weights-dev' "$T/a37.log" || x=0
    grep -qE 'iam delete-role .*--role-name aprender-llm-endpoint-dev-codebuild' "$T/a37.log" || x=0
    row 37 "teardown removes the CodeBuild project and role that weights codebuild recorded" "$T/c37.log" zero '^TEARDOWN OK' "$x"
    # 38-41: guards that a mutation probe showed were not yet pinned by a case
    fresh_ids
    printf '{"function":"%s","image_digest":"%s","statusCode":200,"measured_at":"%s","event_sha256":"x"}\n' "$FN" "$DIGEST" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$W/prove-401.json"
    : > "$T/a38.log"
    run_just "$T/c38.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_MODE=refuse SHIM_LOG="$T/a38.log" just llm-endpoint-url
    x=1
    [ "$(lines "$T/a38.log")" -eq 0 ] || x=0
    row 38 "url: a recorded statusCode of 200 -> REFUSED proof, zero aws calls" "$T/c38.log" nonzero '^REFUSED proof:.*not 401' "$x"
    fresh_ids
    : > "$T/a39.log"
    run_just "$T/c39.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a39.log" SHIM_CONCURRENCY=0 SHIM_S3_KEY=other-object.bin just llm-endpoint-teardown
    x=1
    grep -qE 's3api (delete-bucket|delete-object)' "$T/a39.log" && x=0
    row 39 "teardown: a bucket holding more than the GGUF is kept, never emptied" "$T/c39.log" zero 'KEPT s3://' "$x"
    fresh_ids
    : > "$T/a40.log"
    run_just "$T/c40.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a40.log" SHIM_CONCURRENCY=5 just llm-endpoint-teardown
    x="$(absent_from "$T/a40.log" 'lambda delete-function ')"
    row 40 "teardown: containment reads back 5, not 0 -> REFUSED contain, nothing deleted" "$T/c40.log" nonzero '^REFUSED contain:' "$x"
    fresh_ids
    : > "$T/a41.log"
    run_just "$T/c41.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a41.log" SHIM_STS_ACCOUNT=222222222222 just llm-endpoint-teardown
    x=1
    [ "$(lines "$T/a41.log")" -eq 1 ] || x=0
    row 41 "teardown: profile is another account -> REFUSED account after the single sts call" "$T/c41.log" nonzero '^REFUSED account:' "$x"
    # 42: the bench (plan 09-13) proves itself offline, with its own shimmed aws; the shim above never sees it
    set +e
    python3 -I "$DIR/probe/bench.py" --selftest > "$T/c42.log" 2>&1
    RC=$?
    set -e
    x=1
    grep -q 'price=0.009473' "$T/c42.log" || x=0
    grep -q 'cpu_part=0xd40' "$T/c42.log" || x=0
    grep -q 'budget guard refused tools2_turn1 #2' "$T/c42.log" || x=0
    row 42 "bench-selftest: fixture LogResult parsed and priced, cpu_part read, budget guard refuses the second two-tool turn" "$T/c42.log" zero 'BENCH SELFTEST OK' "$x"
    echo "AWS CALLS: $AWS_CALLS"
    echo "FIXTURE READS: $FIXTURE_READS"
    if [ "$AWS_CALLS" -ne 0 ]; then
        FAILS=$((FAILS + 1))
        echo "FAIL the offline cases reached the aws shim $AWS_CALLS time(s)" >&2
    fi
    if [ "$FAILS" -ne 0 ]; then
        echo "LLM-ENDPOINT SELFTEST FAILED ($FAILS failure(s), $PASS passed)" >&2
        exit 1
    fi
    echo "LLM-ENDPOINT SELFTEST OK $PASS/$PASS"
