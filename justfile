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

# GoldStandard2024 (antisemitism-v1, phase 10): the partner CSV is read IN PLACE (never copied) and written as a
# Laya data dir by the rule prepare_goldstandard.py declares (nfc-trim-ws-v1 grouping, conflicting-label texts
# dropped, a seed-13 stratified group-disjoint 20 % eval draw) -> data/decide/goldstandard-2024. The same script
# measures a task's prefix with the SERVED tokenizer: `just laya-prepare-goldstandard --measure --task <task.json>
# [--csv <csv>] [--max-texts N]`, and runs its own `--selftest` / `--selftest-tokenizer`. A thin wrapper: the
# script owns the default CSV path and every refusal. Output is under the root-anchored, gitignored /data/.
laya-prepare-goldstandard *args:
    #!/usr/bin/env bash
    set -euo pipefail
    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_goldstandard.py {{args}}

# The motivation CSV (motivation-v1, phase 12, a PILOT on the 25 October 2026 guide draft): the partner CSV is read IN
# PLACE (never copied) and written as a Laya data dir by the rule prepare_motivation.py declares (nfc-trim-ws-v1
# grouping, conflicting-label texts dropped, a seed-13 stratified text-disjoint 25 % eval draw) -> data/decide/motivation-v1.
# The same script measures a task's prefix with the SERVED tokenizer: `just laya-prepare-motivation --measure
# (--draft a|b | --task <task.json>) [--csv <csv>] [--max-texts N]`, and runs its own `--selftest` /
# `--selftest-tokenizer`. A thin wrapper: the script owns the default CSV path and every refusal. Output is under the
# root-anchored, gitignored /data/. The arguments are forwarded as "$@" (plan 13-01): a path with
# spaces reaches the script whole. Row prepare-motivation-args.
[positional-arguments]
laya-prepare-motivation *args:
    #!/usr/bin/env bash
    set -euo pipefail
    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/prepare_motivation.py "$@"

# Pull ONE finished training run out of S3 (plan 14-11, D-20): `just laya-fetch NAME TASK_ID BUCKET [--dest DIR]
# [--profile ze-kasher-dev]` reads runs/<NAME>/<TASK_ID>/manifest.json, verifies EVERY entry's sha256 and size in a
# staging directory, and only then renames it to DIR (default models/decide/<NAME>/): manifest.json, verdict.json and
# <NAME>.apr at its root, run/ and data/ as the trainer wrote them, the top-level logs under logs/. An existing
# non-empty DIR is refused, never merged. It prints the owner's next commands (`just laya-verify`, `just laya-deploy`)
# and never runs them; no decide server is touched. A thin wrapper: the script owns every refusal. Row fetch.
[positional-arguments]
laya-fetch *args:
    #!/usr/bin/env bash
    set -euo pipefail
    exec python3 -I scripts/laya_deploy/laya_fetch.py "$@"

# Name the CLASS of the SC4 margin miss from evidence already on disk (plan 15-09, D-13): `just laya-margin-diagnosis
# snapshot --out FILE [--local-root DIR]` (the only step that reads the gitignored fetched runs; it writes a text-free
# fixture of class indices and floats), `verify-inputs --inputs FILE` (bit-exact recompute of the nine recorded
# margins and ECEs), `analyze`, `check-diagnosis`, `check-brief`, `check-reply`, `write-disposition`,
# `check-disposition`, and `--selftest`. Spends nothing, sets no trust. A thin wrapper: the script owns every refusal.
# Row gap diagnosis.
[positional-arguments]
laya-margin-diagnosis *args:
    #!/usr/bin/env bash
    set -euo pipefail
    exec python3 -I scripts/laya_deploy/laya_margin_diagnosis.py "$@"

# The cross-CPU re-score drift study (plan 14-20; 14-VERIFICATION gap 2; D-14, D-15): `just laya-drift local [--only
# LEG,..] [--repeats N] [--out DIR] [--controls] [--evidence FILE]` measures the retained runs on THIS machine with the
# pinned pack_laya binary (path from cargo's own message, sha256 and commit recorded, a dirty closure refused) and a
# positive control that must reproduce the recorded Mac refusal; `aws-plan --source smoke:TASK_ID [--source
# motivation:TASK_ID]` WRITES the owner-run Graviton script (gitignored; it runs no aws); `collect` merges the legs into
# 14-DRIFT-EVIDENCE.json; `bashrs-gate FILE..` is the repository's bashrs judgement for a gitignored shell file; and
# `--selftest`. No contract is amended. A thin wrapper: the script owns every refusal. Row drift.
[positional-arguments]
laya-drift *args:
    #!/usr/bin/env bash
    set -euo pipefail
    exec python3 -I scripts/laya_deploy/laya_drift.py "$@"

# The finalizer and state-machine failure surface (plan 14-16): `just laya-failure-surface --check [--artifact PATH]`
# binds crates/aprender-mcp-laya-train-lambda/fixtures/failure-surface-v1.json and 14-FAILURE-SURFACE.md to the code
# (axes, catch-fed classes and the KNOWN_DEFECTS ratchet out of finalizer.rs, the exit classes by running run_step.py's
# job_exit_code, every sink count by a fresh scan, the state machine out of laya-trainer-stack.ts); `--selftest` proves
# each rule refuses its planted mutation; `--sweep` runs the finalizer sweep tests; `--mutations` is the source-mutation
# runner 14-17 and 14-18 fill. No AWS call. A thin wrapper: the script owns every refusal. Row failure-surface.
[positional-arguments]
laya-failure-surface *args:
    #!/usr/bin/env bash
    set -euo pipefail
    exec python3 -I scripts/laya_deploy/laya_failure_surface.py "$@"

# Synthesize the Laya training server's two CDK stacks OFFLINE and assert their shape (Phase 14, plan 14-08; D-01, D-02,
# D-12, D-13, D-14, D-19). Cross-builds the finalizer for aarch64 (glibc floor <= 2.34), installs it as the Lambda asset
# deploy-extensions/assets/laya-finalizer/bootstrap with its build record bootstrap.build.json beside it (plan 14-19: commit,
# cargo-tree source closure, sha256; `laya-trainer-infra diff|deploy` refuse an asset it cannot attribute to a clean,
# committed closure), synthesizes the images stack and the trainer stack with no
# credentials, no profile and no notices (so no account is ever contacted and the templates do not depend on whose
# machine ran them), runs scripts/laya_deploy/laya_trainer_stack_check.py on both templates and prints TRAINER SYNTH OK.
# env is dev or prod only. With no digests, all-zero placeholders are used and said so: that is for a synth only, never a
# deploy (14-09 passes the real digests the CodeBuild builds printed). Creates nothing, deploys nothing. Row trainer-synth.
laya-trainer-synth env="dev" train_digest="" pack_digest="":
    #!/usr/bin/env bash
    set -euo pipefail
    refuse() { echo "REFUSED $1: $2" >&2; exit 2; }
    ENV_NAME="{{ trim_start_match(env, "env=") }}"
    case "$ENV_NAME" in dev|prod) ;; *) refuse env "environment must be dev or prod, got '$ENV_NAME'" ;; esac
    TRAIN_D="{{ trim_start_match(train_digest, "train_digest=") }}"
    PACK_D="{{ trim_start_match(pack_digest, "pack_digest=") }}"
    if [ -z "$TRAIN_D" ] && [ -z "$PACK_D" ]; then
        ZERO="sha256:$(printf '0%.0s' $(seq 64))"
        TRAIN_D="$ZERO"; PACK_D="$ZERO"
        echo "PLACEHOLDER DIGESTS (synth only): both images are the all-zero digest; a deploy refuses them"
    fi
    for D in "$TRAIN_D" "$PACK_D"; do
        echo "$D" | grep -Eq '^sha256:[0-9a-f]{64}$' || refuse digest "an image digest is sha256:<64 hex> (both digests or neither), got '$D'"
    done
    command -v cargo-zigbuild >/dev/null || refuse tool "cargo-zigbuild is not installed (cargo install cargo-zigbuild)"
    cargo zigbuild --release --target {{target}} -p aprender-mcp-laya-train-lambda --bin aprender-laya-train-finalizer
    BIN="target/{{target}}/release/aprender-laya-train-finalizer"
    file "$BIN" | grep -q 'ARM aarch64' || { echo "ERROR: the finalizer is not an aarch64 binary"; exit 1; }
    python3 -I scripts/embed_endpoint/glibc_floor.py 34 "$BIN"
    ASSET="deploy-extensions/assets/laya-finalizer"
    mkdir -p "$ASSET"
    cp "$BIN" "$ASSET/bootstrap"
    chmod 0755 "$ASSET/bootstrap"
    # Plan 14-19 (WR-04): record what the asset was built from (commit, the cargo-tree source closure, sha256, target,
    # UTC time) in $ASSET/bootstrap.build.json; `laya-trainer-infra diff|deploy` refuse an asset it cannot attribute.
    python3 -I scripts/laya_deploy/laya_trainer_ops.py finalizer-provenance write "$ASSET" --target {{target}}
    OFFLINE_ENV="AWS_CONFIG_FILE=/dev/null AWS_SHARED_CREDENTIALS_FILE=/dev/null AWS_EC2_METADATA_DISABLED=true"
    synth() {
        (cd deploy-extensions && env -u AWS_PROFILE -u AWS_ACCESS_KEY_ID -u AWS_SECRET_ACCESS_KEY -u AWS_SESSION_TOKEN -u AWS_REGION -u AWS_DEFAULT_REGION \
            $OFFLINE_ENV npx cdk synth --no-notices --no-version-reporting --context=env="$ENV_NAME" --context=stack="$1" \
            --context=trainImageDigest="$TRAIN_D" --context=packImageDigest="$PACK_D" --output "cdk.out/$1" --quiet)
    }
    synth laya-trainer-images
    synth laya-trainer
    python3 -I scripts/laya_deploy/laya_trainer_stack_check.py \
        "deploy-extensions/cdk.out/laya-trainer/aprender-laya-trainer-$ENV_NAME.template.json" \
        "deploy-extensions/cdk.out/laya-trainer-images/aprender-laya-trainer-images-$ENV_NAME.template.json"
    echo "TRAINER SYNTH OK env=$ENV_NAME"

# The owner-run AWS recipes of the Laya training server's first deployment (Phase 14, plan 14-09; D-01, D-02). Each is a thin
# wrapper over scripts/laya_deploy/laya_trainer_ops.py, which owns every refusal: only profile ze-kasher-dev and env dev|prod,
# `_laya-account-check` as the first command of a live run, `--state-dir DIR` anywhere (default models/laya-trainer, which is
# gitignored), DRY_RUN=1 to print every command and execute none (`AWS CALLS: 0`). Nothing here runs by itself.
#
# `just laya-trainer-image train|pack ENV PROFILE`: zips the image's own context list from `git archive HEAD` (refused when a path
# of that list has uncommitted changes), uploads it to the images stack's context bucket, starts the CodeBuild project, waits up to
# 90 min and records {repo_uri, repo_name, digest, commit, build_id} in images-<env>.json. It outlasts the Bash tool's 10-minute
# cap: run it in the background and judge it by its LAST line, `IMAGE OK <which> <digest>` or `IMAGE FAILED <which> <status>`.
# Row trainer-image.
[positional-arguments]
laya-trainer-image *args:
    #!/usr/bin/env bash
    set -euo pipefail
    exec python3 -I scripts/laya_deploy/laya_trainer_ops.py image "$@"

# `just laya-trainer-infra diff-images|deploy-images|outputs-images|diff|deploy|outputs ENV PROFILE`: the two CDK stacks. `deploy`
# refuses unless images-<env>.json holds real digests for both images and runs `cdk diff` immediately before `cdk deploy
# --require-approval never` in the same invocation (the deploy-training rationale). `outputs-images` reads the images stack alone
# (the first-deployment bootstrap the image builds need); `outputs` writes outputs-<env>.json from both stacks and refuses before
# the trainer stack exists. Row trainer-infra.
[positional-arguments]
laya-trainer-infra *args:
    #!/usr/bin/env bash
    set -euo pipefail
    exec python3 -I scripts/laya_deploy/laya_trainer_ops.py infra "$@"

# `just laya-trainer-stage base ENV PROFILE [--tar FILE]` uploads the locally built laya-base.tar (default <state-dir>/laya-base.tar) to
# base/laya-55cf4c4e/ only when its size and sha256 equal scripts/laya_train/container/base-stage.json, then verifies the uploaded
# object's size. `just laya-trainer-stage dataset DIR PREFIX ENV PROFILE [--expect motivation-v1]` uploads exactly task.json,
# train.jsonl, eval.jsonl and an optional shift.jsonl to datasets/<PREFIX>/ (the trainer's prefix grammar), records each file's sha256 in
# datasets-<env>.json, and refuses a prefix that already holds any object (dataset prefixes are immutable; a corrected dataset gets a
# new prefix). `--expect motivation-v1` binds the three files to Phase 12's inputs_sha256. Row trainer-stage.
[positional-arguments]
laya-trainer-stage *args:
    #!/usr/bin/env bash
    set -euo pipefail
    exec python3 -I scripts/laya_deploy/laya_trainer_ops.py stage "$@"

# `just laya-trainer-probe cuda|sigterm ENV PROFILE` creates ONE ml.g5.xlarge training job (MaxRuntime 1800 s, the base channel only):
# `cuda` runs the CUDA/base-load/one-training-step probe, `sigterm` stops the job 120 s after InProgress (once the container is ready)
# and measures SageMaker's SIGTERM grace from the uploaded heartbeats. A live run costs money and refuses unless
# LAYA_TRAINER_APPROVED_SPEND=probe-<mode>. It outlasts the Bash tool's 10-minute cap: run it in the background and judge it by its
# LAST line, `PROBE DONE <mode> <final status>` or `PROBE FAILED <mode> <detail>`. Results land in <state-dir>/probe-<id>.json. Row trainer-probe.
[positional-arguments]
laya-trainer-probe *args:
    #!/usr/bin/env bash
    set -euo pipefail
    exec python3 -I scripts/laya_deploy/laya_trainer_ops.py probe "$@"

# The owner-run PUBLISH of the Laya training server's request function (Phase 14, plan 14-10; D-10, D-01). Both recipes are thin
# wrappers over scripts/laya_deploy/laya_trainer_ops.py (profile ze-kasher-dev only, DRY_RUN=1 prints every command and executes no aws).
#
# `just laya-trainer-config ENV PROFILE` fills the tracked template crates/aprender-mcp-laya-train-lambda/.pmcp/deploy.toml.template from
# <state-dir>/outputs-<env>.json (each of its three placeholders exactly once) into <state-dir>/deploy-config-<env>.toml, mode 600. The
# template carries NO [[iam.statements]] and `[auth] enabled = false` (cargo-pmcp cannot render OAuth for pmcp-run); a template that
# declares IAM is refused. Row trainer-config.
[positional-arguments]
laya-trainer-config *args:
    #!/usr/bin/env bash
    set -euo pipefail
    exec python3 -I scripts/laya_deploy/laya_trainer_ops.py config "$@"

# `just laya-trainer-deploy ENV PROFILE` publishes the request function through the shared crates root: `_laya-crates-root-swap crates
# <config> <snap> cargo pmcp deploy --manifest-path crates ... --no-oauth` (crates/.pmcp and crates/deploy come back byte-identical), and
# refuses to record a deployment whose build log does not name aprender-mcp-laya-train-lambda (`REFUSED deploy: wrong package`). Because a
# `--no-oauth` redeploy may reset OAuth, it first deletes an existing inline grant aprender-laya-train-<env> from the function's role,
# FAIL-CLOSED (an absent function or policy proceeds; any answer it cannot classify is `REFUSED deploy: cannot confirm the grant is
# absent`). It writes <state-dir>/deploy-<env>.json {server_id, package, endpoint, deployed_at}. A live run needs
# LAYA_TRAINER_APPROVED_DEPLOY=<env> and runs `_laya-account-check` first. Row trainer-deploy.
[positional-arguments]
laya-trainer-deploy *args:
    #!/usr/bin/env bash
    set -euo pipefail
    exec python3 -I scripts/laya_deploy/laya_trainer_ops.py deploy "$@"

# `just laya-trainer-grant ENV PROFILE` attaches the stack's RequestLambdaPolicy to the published function's role (inline policy
# aprender-laya-train-<env>), and ONLY after it has seen OAuth itself. It takes NO url: the endpoint is read from the verified deploy
# state <state-dir>/deploy-<env>.json, its host's first label must be the server id aprender-mcp-laya-train and GET /health on it must
# answer 200 naming that server (`REFUSED grant: endpoint not bound to aprender-mcp-laya-train`); then, in this same invocation and
# immediately before the IAM call, `_laya-auth-refusal-check <that endpoint> 0 train_status` must see an unauthenticated tools/list and
# a train_status tools/call refused with 401 (`REFUSED grant: OAuth not confirmed (tools/list answered <code>)`, or `REFUSED grant:
# endpoint unreachable (transport error; retry)` when curl could not connect). A 401 seen on an unrelated endpoint can never authorize
# this function's grant. A live grant needs LAYA_TRAINER_APPROVED_GRANT=<env>. DRY_RUN=1 still makes the two read-only HTTP probes
# (run it only against a deployed endpoint, or with a curl shim as the gate row does) and prints the IAM commands without running them.
# Row trainer-grant.
[positional-arguments]
laya-trainer-grant *args:
    #!/usr/bin/env bash
    set -euo pipefail
    exec python3 -I scripts/laya_deploy/laya_trainer_ops.py grant "$@"

# `just laya-trainer-call PROFILE URL list-tools|validate URI|train URI NAME [EPOCHS]|status TASK_ID|result TASK_ID|cancel TASK_ID|
# call-tool TOOL JSON_ARGS` calls the Laya training server with an M2M bearer (plan 14-10; the live checks of 14-13). The token is
# minted by scripts/laya_deploy/m2m_token.py from the mode-600 client file LAYA_M2M_FILE (outside the checkout) and reaches
# scripts/laya_deploy/train_call.py only in APRENDER_DECIDE_PROBE_TOKEN, never on argv and never printed; each answer is ONE JSON line
# (exit 0 only when ok), `call-tool` sends exactly the given argument object, and the client refuses every redirect (a bearer must never
# follow a 3xx to another host). Refused (`REFUSED call:`, exit 2, nothing run) when the profile is not ze-kasher-dev, LAYA_M2M_FILE is
# unset, APRENDER_DECIDE_PROBE_TOKEN is already set, an argument starts with --bearer / --url, or the URL is not
# https://aprender-mcp-laya-train<tenant suffix>/mcp (loopback http://127.0.0.1:<port>/mcp only under DRY_RUN=1, for the offline chain).
# The token host comes from the helper's --token-host mode: LAYA_M2M_TOKEN_HOST, else <state dir>/m2m-token-host (models/laya-trainer),
# else the URL's host. LAYA_M2M_MIN_EXPIRES (300..86400, default 900) is the expires_in floor. Needs SSL_CERT_FILE=/etc/ssl/cert.pem on a
# python.org build with no CA bundle (the client falls back to it). Row trainer-call.
[positional-arguments]
laya-trainer-call profile url *args:
    #!/usr/bin/env bash
    set -euo pipefail
    P="$1"
    URL="$2"
    shift 2
    refuse() { echo "REFUSED call: $1" >&2; exit 2; }
    [ "$P" = "ze-kasher-dev" ] || refuse "profile $P is not allowed in this phase (only ze-kasher-dev)"
    [ -n "${LAYA_M2M_FILE:-}" ] || refuse "LAYA_M2M_FILE is not set: name the mode-600 M2M client file (outside the checkout)"
    [ -z "${APRENDER_DECIDE_PROBE_TOKEN+x}" ] \
        || refuse "APRENDER_DECIDE_PROBE_TOKEN is already set in the caller's environment; unset it (the token is minted from LAYA_M2M_FILE)"
    for a in "$@"; do
        case "$a" in
            --bearer*) refuse "an argument starts with --bearer; the token comes only from LAYA_M2M_FILE (the argument is not echoed)" ;;
            --url*|--allow-loopback-http*) refuse "an argument starts with --url or --allow-loopback-http; the URL is this recipe's second argument" ;;
        esac
    done
    MIN="${LAYA_M2M_MIN_EXPIRES:-900}"
    case "$MIN" in ''|*[!0-9]*) refuse "config: LAYA_M2M_MIN_EXPIRES is not a whole number of seconds" ;; esac
    if [ "${#MIN}" -gt 5 ] || [ "$((10#$MIN))" -lt 300 ] || [ "$((10#$MIN))" -gt 86400 ]; then
        refuse "config: LAYA_M2M_MIN_EXPIRES must be in 300..86400"
    fi
    # The endpoint under this profile's tenant and naming THIS server (pure checks: no AWS call is made by a call).
    TLINE="$(just _laya-target-check "$P" "")"
    read -r T_TARGET T_REGION _ _ <<< "$TLINE"
    export PMCP_TARGET="$T_TARGET" AWS_PROFILE="$P" AWS_REGION="$T_REGION"
    unset PMCP_API_URL AWS_DEFAULT_REGION
    G="$(just _laya-endpoint-check "$URL" "$P")"
    if [ "$G" != "loopback" ] && [ "$G" != "aprender-mcp-laya-train" ]; then
        refuse "the endpoint names server $G, not aprender-mcp-laya-train"
    fi
    UH="${URL#*://}"
    UH="${UH%%/*}"
    UH="${UH%%:*}"
    TH="$(python3 -I scripts/laya_deploy/m2m_token.py --token-host --state-dir models/laya-trainer --endpoint-host "$UH")"
    FLAGS=(--file "$LAYA_M2M_FILE" --expect-host "$TH" --min-expires "$((10#$MIN))")
    CALLFLAGS=()
    if [ "$G" = "loopback" ]; then FLAGS+=(--allow-loopback-http); CALLFLAGS+=(--allow-loopback-http); fi
    if [ "${LAYA_M2M_ALLOW_BASIC:-0}" = "1" ]; then FLAGS+=(--allow-basic); fi
    set +x
    exec python3 -I scripts/laya_deploy/m2m_token.py "${FLAGS[@]}" -- python3 -I scripts/laya_deploy/train_call.py --url "$URL" ${CALLFLAGS[@]+"${CALLFLAGS[@]}"} "$@"

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

# The D-11 SetFit comparison cell (plan 10-05): s<shots>e1b16 at seed 13 on the Laya splits as adapted by
# crates/aprender-contrastive-data/examples/laya_dir_to_canonical.rs (test == the Laya eval set). select (only when
# no selection manifest exists; written to a temporary sibling and moved) -> train with the tracked explicit config
# (CPU, about 30 min) -> eval validation (--lock-out) -> eval test (--selection-lock). `apr eval` reports ACCURACY and,
# from the same pass, writes <out>/test-probabilities.json (--dump-probabilities: nine keys with row ids, per-row
# text sha256 and the dataset fingerprint; plan 12-06). Exit 0 = SETFIT CELL OK, 2 = SETFIT CELL REFUSED <step>:
# <detail>. The args are <data> <out> <config> <sel> <shots>; an empty <sel> means <data>/selection-s<shots>-seed13,
# and <shots> is one of 8 16 32 64 (default 64: Phase 10's cell). Phase 12 runs s32 (D-25): its train pools after the
# validation carve are below 64.
laya-setfit-cell data="data/decide/goldstandard-2024-setfit" out="models/setfit/goldstandard-2024-s64e1b16-seed13" config="scripts/laya_train/setfit_cell_e1b16_cpu.json" sel="" shots="64":
    #!/usr/bin/env bash
    set -euo pipefail
    refuse() { echo "SETFIT CELL REFUSED $1: $2"; exit 2; }
    case "{{shots}}" in 8|16|32|64) ;; *) refuse select "shots {{shots}} is not one of 8 16 32 64" ;; esac
    test -f "{{data}}/benchmark-manifest.json" || refuse select "no benchmark-manifest.json under {{data}} (run the laya_dir_to_canonical adapter, plan 10-05)"
    test -f "{{config}}" || refuse config "no config file {{config}} (apr setfit train requires --config)"
    MINILM="{{minilm_dir}}"
    test -f "$MINILM/full_model.apr" || refuse train "no full_model.apr under $MINILM (the pinned 1110a243 checkout; set APRENDER_MINILM_DIR)"
    OUT="{{out}}"
    test ! -e "$OUT/cell.apr" || refuse train "$OUT/cell.apr already exists; move it aside, apr setfit train will not replace it"
    . scripts/apr_bin.sh || exit 1
    export CARGO_INCREMENTAL=0
    SELDIR="{{sel}}"; [ -n "$SELDIR" ] || SELDIR="{{data}}/selection-s{{shots}}-seed13"
    SEL="$SELDIR/selection-manifest.json"
    mkdir -p "$OUT"
    if [ ! -f "$SEL" ]; then
        test ! -e "$SELDIR" || refuse select "$SELDIR exists but holds no selection-manifest.json"
        TMPSEL="$SELDIR.tmp.$$"
        rc=0
        "$APR" data select --data "{{data}}" --shots {{shots}} --seed 13 --output "$TMPSEL" > "$OUT/select.log" 2>&1 || rc=$?
        if [ "$rc" -ne 0 ]; then rm -rf "$TMPSEL"; refuse select "$(grep -m 1 -E 'rror|REFUSED|efus' "$OUT/select.log" || echo "exit $rc")"; fi
        mv "$TMPSEL" "$SELDIR"
    fi
    T0="$(date +%s)"
    rc=0
    "$APR" setfit train --config "{{config}}" --data "{{data}}" --selection "$SEL" --model-dir "$MINILM" \
        --output "$OUT/cell.apr" --seed 13 > "$OUT/train.log" 2>&1 || rc=$?
    TRAIN_SECONDS=$(( $(date +%s) - T0 ))
    if [ "$rc" -ne 0 ]; then refuse train "$(grep -m 1 -E 'setfit training: |rror|REFUSED|efus' "$OUT/train.log" || echo "exit $rc")"; fi
    rc=0
    "$APR" eval "$OUT/cell.apr" --task classify --data "{{data}}" --selection "$SEL" --split validation \
        --lock-out "$OUT/selection.lock" --json > "$OUT/eval-validation.json" 2> "$OUT/eval-validation.log" || rc=$?
    if [ "$rc" -ne 0 ]; then refuse eval-validation "$(grep -m 1 -E 'rror|REFUSED|efus' "$OUT/eval-validation.log" || echo "exit $rc")"; fi
    rc=0
    "$APR" eval "$OUT/cell.apr" --task classify --data "{{data}}" --selection "$SEL" --split test \
        --selection-lock "$OUT/selection.lock" --dump-probabilities "$OUT/test-probabilities.json" \
        --json > "$OUT/eval-test.json" 2> "$OUT/eval-test.log" || rc=$?
    if [ "$rc" -ne 0 ]; then refuse eval-test "$(grep -m 1 -E 'rror|REFUSED|efus' "$OUT/eval-test.log" || echo "exit $rc")"; fi
    python3 -I -c 'import json, sys; r = json.load(open(sys.argv[1])); print("SETFIT CELL OK cell=s%se1b16 seed=13 artifact_sha256=%s metric_tag=%s value=%s n_rows=%s train_seconds=%s" % (sys.argv[3], r["artifact_sha256"], r["metric"], r["value"], r["n_rows"], sys.argv[2]))' "$OUT/eval-test.json" "$TRAIN_SECONDS" "{{shots}}"

# The D-10 one-epoch probe: a MEASUREMENT, not a gate run. One epoch of seed 13 through train.train_seed on the
# real fit rows, the device read back, the per-step time, the per-epoch monitor cost, the row-length record and
# the host load -> the evidence JSON <out> (only). No run dir, recipe or checkpoint. A thin wrapper: its own exit is
# the missing-task.json error; the run-dir guard (REFUSED run-dir-exists once models/decide/laya-antisemitism-v1
# exists) and the measurement are probe_epoch.py's. `--dry-run` validates and prints the planned steps.
laya-probe-epoch data out *args:
    #!/usr/bin/env bash
    set -euo pipefail
    test -f "{{data}}/task.json" || { echo "ERROR: {{data}}/task.json does not exist" >&2; exit 2; }
    uv run --project scripts/laya_train --frozen python -I scripts/laya_train/probe_epoch.py \
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
# is the eligibility check of a run with NO pack record, and an advisory cross-CPU check when a valid
# Graviton pack record exists (Phase 15 D-08: scripts/laya_deploy/laya_eligibility.py decides what a
# refusal means); the policy is read from the contracts.
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
# --manifest-path crates` with the SERVER ID as the server name. cargo-pmcp's
# `find_lambda_package_dir` returns `<root>/<server>-lambda` when it exists, BEFORE its
# workspace-wide search, so this root resolves a server id to the crate `crates/<server>-lambda`
# by construction. The per-crate root misses that branch and falls through to the FIRST
# `*-lambda` package with a `bootstrap` bin, which is aprender-mcp-chronos-lambda: the Chronos
# binary would ship under the decide name, healthy-looking (RESEARCH Pitfall 1). `just
# laya-resolver-proof` EXECUTES that resolver on this workspace for one server id;
# `laya-deploy` refuses without that server's proof.
#
# ONE SERVER ID, ONE THIN CRATE (owner statement 2026-10-22, D-19, plan 10-12): several
# classifiers from different datasets serve in parallel, each under its own id
# `aprender-mcp-decide-<dataset>` and its own thin crate `<id>-lambda` (a shim `bootstrap` over
# the model-agnostic runtime library aprender-mcp-decide-lambda). The id is a REQUIRED argument of
# every deploy recipe, with no default, so a stale call site fails loudly instead of silently
# targeting another server, and `_laya-server-check` refuses an id with no thin crate. The
# retired id `aprender-mcp-decide` has no thin crate (its crate is a library), so no recipe can
# deploy to it; `laya-teardown` still names it, because containment must reach any deployed function.
#
# Limit of this choice: SERVING is parallel, DEPLOYING is one at a time. `crates/.pmcp/` +
# `crates/deploy/` belong to the setfit training server, so `laya-deploy` swaps them out and
# restores them byte-identically on every exit path (`_laya-crates-root-swap`, proven by
# `laya-deploy-selftest`); the swap refuses while a leftover backup exists, so a second concurrent
# deploy fails closed. The durable fix is upstream: in cargo-pmcp, return the project root when it is
# itself a `*-lambda` package with a `bootstrap` bin (recommended future SDK work, not done here).
#
# Nothing here writes to AWS unless `just laya-verify` accepted the exact file first.
# ---------------------------------------------------------------------------

# Refuse a server id the deploy path must not touch (plan 10-12, D-19). <mode> `named`: the id is a
# cargo-pmcp server name that is also a crate directory and an S3 prefix, so it is lower-case ASCII
# letters, digits and hyphens, starts with a letter, does not end with a hyphen and is at most 64
# characters (cargo-pmcp's own limit; lower-case only is stricter, because the id also names a crate).
# <mode> `deployable`: the same AND `crates/<server>-lambda/Cargo.toml` exists AND declares the
# `bootstrap` bin, so an id with no thin crate (and the retired shared crate, which ships no bin) is
# refused before any file or AWS action. Exit 2 with `REFUSED server: ...`. Pure: no network.
[positional-arguments]
_laya-server-check server mode:
    #!/usr/bin/env bash
    set -euo pipefail
    SERVER="$1"
    MODE="$2"
    refuse() { echo "REFUSED server: $1" >&2; exit 2; }
    case "$MODE" in
        named|deployable) ;;
        *) refuse "unknown mode '$MODE' (expected named or deployable)" ;;
    esac
    [ -n "$SERVER" ] || refuse "the server id is empty"
    [ "${#SERVER}" -le 64 ] || refuse "'$SERVER' is ${#SERVER} characters; a server name is at most 64"
    # A glob over the WHOLE string, in the C locale: `grep -Eq '^...$'` is line-based, so an id such as
    # "good-id<newline>../x" would pass on its first line.
    export LC_ALL=C
    case "$SERVER" in
        [!a-z]* | *[!a-z0-9-]*)
            refuse "'$SERVER' is not a server id: lower-case ASCII letters, digits and hyphens, starting with a letter" ;;
    esac
    case "$SERVER" in
        *-) refuse "'$SERVER' ends with a hyphen" ;;
    esac
    if [ "$MODE" = "deployable" ]; then
        MANIFEST="crates/$SERVER-lambda/Cargo.toml"
        [ -f "$MANIFEST" ] || refuse "'$SERVER' has no thin crate: $MANIFEST does not exist (one server id, one crate named <id>-lambda)"
        grep -Eq '^name = "bootstrap"$' "$MANIFEST" \
            || refuse "$MANIFEST declares no bootstrap bin, so cargo-pmcp could not deploy it as '$SERVER'"
    fi
    echo "server ok: $SERVER ($MODE)"

# Execute cargo-pmcp's own `find_lambda_package_dir` on THIS workspace (root `crates`, server id
# <server>) from a `git archive` of <sdk> at <commit> unpacked under <work> (a scratch dir; the SDK
# checkout is only read). The source version must equal the installed `cargo pmcp --version`.
# The server id is REQUIRED (plan 10-12, D-19): it must resolve to `crates/<server>-lambda`, the
# thin per-server crate. Writes models/decide/resolver-proof-<server>.txt, which `laya-deploy`
# requires for that server.
laya-resolver-proof sdk commit work server:
    #!/usr/bin/env bash
    set -euo pipefail
    SDK="{{sdk}}"
    WORK="{{work}}"
    SERVER="{{server}}"
    just _laya-server-check "$SERVER" named
    REPO="$(pwd -P)"
    PROOF="models/decide/resolver-proof-$SERVER.txt"
    EXPECT="crates/$SERVER-lambda"
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
    (cd "$DEST" && APRENDER_WORKSPACE="$REPO" APRENDER_PROOF_SERVER="$SERVER" CARGO_TARGET_DIR="$WORK/target" \
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
    PARSED="$(just _laya-resolver-parse "$LOG" "$SERVER")" || { echo "ERROR: the resolver test printed no RESOLVED line (log: $LOG)" >&2; exit 1; }
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
        echo "server=$SERVER"
        echo "control_per_crate_root=$CONTROL"
        echo "proven_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    } > "$PROOF"
    echo "RESOLVER PROOF: $RESOLVED (control: root $EXPECT -> $CONTROL)"
    echo "  cargo-pmcp $INST_VER, source $FULL (builder.rs last changed in $BUILDER_LAST)"
    echo "  wrote $PROOF (gitignored)"

# The RESOLVED / CONTROL values of a resolver-proof test log (plan 08-22, D3-1) for <server>. The tokens
# are read ANYWHERE on a line: under --nocapture libtest writes `test <name> ... ` onto the first output
# line, so a column-0 anchor can miss them. Prints `RESOLVED=<dir>` and `CONTROL=<dir>` (the first of
# each); no RESOLVED line exits 1. Only lines for exactly this server match (a server id is ASCII
# letters, digits and hyphens, so it is safe inside the pattern). Pure: no cargo. Row
# `resolver-proof-sed` of scripts/laya_gates.tsv.
[positional-arguments]
_laya-resolver-parse log server:
    #!/usr/bin/env bash
    set -euo pipefail
    LOG="$1"
    SERVER="$2"
    [ -f "$LOG" ] || { echo "REFUSED resolver-parse: $LOG does not exist" >&2; exit 1; }
    first() { awk -v pat="$1" 'match($0, pat) { print substr($0, RSTART + RLENGTH); exit }' "$LOG"; }
    RESOLVED="$(first "RESOLVED root=crates server=$SERVER -> ")"
    CONTROL="$(first "CONTROL root=[^ ]* server=$SERVER -> ")"
    [ -n "$RESOLVED" ] || { echo "REFUSED resolver-parse: no RESOLVED line in $LOG" >&2; exit 1; }
    printf 'RESOLVED=%s\nCONTROL=%s\n' "$RESOLVED" "$CONTROL"

# A deployment is the TRIPLE (pmcp.run target, AWS account, region), never the server id alone
# (plan 13-04, D-06). This is the committed table that maps a deploy profile to that triple and to the
# host suffix its endpoints live under; a profile outside it is refused, because the target cannot be
# derived from the profile by search (~/.pmcp/config.toml's `staging-test` also uses ze-kasher-dev).
# Checks, all before any AWS call: the profile is a name (^[a-z0-9][a-z0-9-]{0,62}$) and in the table;
# a non-empty <region> equals the table's (empty means the table's); PMCP_TARGET, AWS_PROFILE,
# AWS_REGION and AWS_DEFAULT_REGION, when set and non-empty, agree with the target / profile / region
# (cargo-pmcp and the AWS CLI would otherwise follow a stale shell value); the pmcp config
# ($HOME/.pmcp/config.toml, or LAYA_PMCP_CONFIG only under DRY_RUN=1) has [targets.<target>] with
# type pmcp-run, this aws_profile, this region and an api_url https://<id>.execute-api.<region>.amazonaws.com;
# and PMCP_API_URL, when set and non-empty, is that api_url (it wins over the target's in cargo-pmcp's
# resolver). Refusals are `REFUSED target: ...` / `REFUSED target-config: ...`, exit 2, naming an
# api_url by its host only. Success prints ONE stdout line `<target> <region> <host_suffix> <account>`.
# Pure: reads one local file, no network. Row target-check.
[positional-arguments]
_laya-target-check profile region:
    #!/usr/bin/env bash
    set -euo pipefail
    CONF="$HOME/.pmcp/config.toml"
    if [ -n "${LAYA_PMCP_CONFIG:-}" ]; then
        if [ "${DRY_RUN:-0}" = "1" ]; then
            CONF="$LAYA_PMCP_CONFIG"
        else
            echo "REFUSED target-config: LAYA_PMCP_CONFIG is honoured only under DRY_RUN=1" >&2
            exit 2
        fi
    fi
    python3 -I - "$1" "$2" "$CONF" <<'PY'
    import os, re, sys, tomllib
    profile, region, conf = sys.argv[1:4]
    # profile: (pmcp.run target, region, endpoint host suffix, AWS account)
    TABLE = {
        "ze-kasher-dev": ("dev", "us-east-1", ".us-east.true-mcp.com", "816069129666"),
        "augury-dev": ("augury-dev", "eu-west-1", ".eu-west.augury.pmcp.run", "783960389328"),
    }
    def refuse(kind, msg):
        print(f"REFUSED {kind}: {msg}", file=sys.stderr)
        sys.exit(2)
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,62}", profile):
        refuse("target", f"the profile argument ({len(profile)} characters) is not a profile name")
    if profile not in TABLE:
        refuse("target", f"profile {profile} is not in the committed deploy-target table ({', '.join(sorted(TABLE))})")
    target, want_region, suffix, account = TABLE[profile]
    if region and region != want_region:
        refuse("target", f"profile {profile} is pmcp.run target {target} in {want_region}; the region argument ({len(region)} characters) disagrees")
    agree = {"PMCP_TARGET": target, "AWS_PROFILE": profile, "AWS_REGION": want_region, "AWS_DEFAULT_REGION": want_region}
    for var, want in agree.items():
        got = os.environ.get(var, "")
        if got and got != want:
            refuse("target", f"{var} is set to another value than {want}, which profile {profile} requires; unset it or set it to {want}")
    try:
        with open(conf, "rb") as fh:
            cfg = tomllib.load(fh)
    except FileNotFoundError:
        refuse("target-config", f"the pmcp config {conf} does not exist")
    except (OSError, tomllib.TOMLDecodeError) as e:
        refuse("target-config", f"the pmcp config {conf} cannot be read as TOML ({type(e).__name__})")
    t = cfg.get("targets", {}).get(target) if isinstance(cfg.get("targets"), dict) else None
    if not isinstance(t, dict):
        refuse("target-config", f"{conf} has no [targets.{target}] for profile {profile}")
    if t.get("type") != "pmcp-run":
        refuse("target-config", f"[targets.{target}] is not of type pmcp-run")
    if t.get("aws_profile") != profile:
        refuse("target-config", f"[targets.{target}] names another aws_profile than {profile}")
    if t.get("region") != want_region:
        refuse("target-config", f"[targets.{target}] names another region than {want_region}")
    api = t.get("api_url")
    host = api[len("https://"):].split("/", 1)[0][:80] if isinstance(api, str) and api.startswith("https://") else "<not https>"
    if not isinstance(api, str) or not re.fullmatch(r"https://[a-z0-9]+\.execute-api\." + re.escape(want_region) + r"\.amazonaws\.com/?", api):
        refuse("target-config", f"[targets.{target}] api_url (host {host}) is not an execute-api control plane in {want_region}")
    env_api = os.environ.get("PMCP_API_URL", "")
    if env_api and env_api.rstrip("/") != api.rstrip("/"):
        env_host = env_api.split("://", 1)[-1].split("/", 1)[0][:80]
        refuse("target", f"PMCP_API_URL names another control plane (host {env_host}) than [targets.{target}] ({host}); it would win over the target in cargo-pmcp's resolver: unset it")
    print(f"{target} {want_region} {suffix} {account}")
    print(f"target-check ok: profile {profile} -> pmcp.run target {target}, region {want_region}, account {account}, endpoints *{suffix}", file=sys.stderr)
    PY

# The LIVE half of the triple (plan 13-04, D-06): the profile must resolve to the table's account.
# `_laya-target-check <profile> <region>` first (its refusals propagate), then, when DRY_RUN is not 1
# (or DRY_RUN=1 with LAYA_ACCOUNT_CHECK_FORCE=1, honoured only there, for a selftest's scripted fake
# aws), `aws sts get-caller-identity --profile <profile>` must answer exactly the table's 12 digits:
# `REFUSED target-account: ...` (exit 2) otherwise, naming only the last four digits of a foreign
# account. Under DRY_RUN=1 without the force flag it makes no AWS call (`target-account: not checked`).
# Every AWS-touching laya recipe calls it before its first AWS call. Row account-check.
[positional-arguments]
_laya-account-check profile region:
    #!/usr/bin/env bash
    set -euo pipefail
    TLINE="$(just _laya-target-check "$1" "$2")"
    read -r _ T_REGION _ T_ACCOUNT <<< "$TLINE"
    export AWS_PROFILE="$1" AWS_REGION="$T_REGION"
    if [ "${DRY_RUN:-0}" = "1" ] && [ "${LAYA_ACCOUNT_CHECK_FORCE:-0}" != "1" ]; then
        echo "target-account: not checked (DRY_RUN=1)" >&2
        exit 0
    fi
    set +e
    GOT="$(aws sts get-caller-identity --profile "$1" --query Account --output text 2> /dev/null)"
    rc=$?
    set -e
    if [ "$rc" -ne 0 ]; then
        echo "REFUSED target-account: cannot read the account for profile $1" >&2
        exit 2
    fi
    if [ "$GOT" != "$T_ACCOUNT" ]; then
        if (LC_ALL=C; [[ "$GOT" =~ ^[0-9]{12}$ ]]); then
            echo "REFUSED target-account: profile $1 resolves to an account ending ${GOT: -4}, not the table's $T_ACCOUNT" >&2
        else
            echo "REFUSED target-account: profile $1 did not answer a 12-digit account id (${#GOT} characters), not the table's $T_ACCOUNT" >&2
        fi
        exit 2
    fi
    echo "target-account ok: profile $1 is account $T_ACCOUNT" >&2

# The local state directory of a deploy profile (plan 13-04, D-06): ze-kasher-dev keeps models/decide,
# so every existing path is unchanged byte for byte; any other profile name gets models/decide/<profile>,
# so a second deployment of the SAME server id never overwrites the first one's config, log, state
# snapshot, health / probe / eligibility logs, plan files or evidence. Not a name: `REFUSED state-dir:`
# (exit 2). Pure. Row state-dir.
[positional-arguments]
_laya-state-dir profile:
    #!/usr/bin/env bash
    set -euo pipefail
    P="$1"
    if [ "$P" = "ze-kasher-dev" ]; then
        echo "models/decide"
        exit 0
    fi
    PROFILE_ERE='^[a-z0-9][a-z0-9-]{0,62}$'
    if (LC_ALL=C; [[ "$P" =~ $PROFILE_ERE ]]); then
        echo "models/decide/$P"
    else
        echo "REFUSED state-dir: the profile argument (${#P} characters) is not a profile name" >&2
        exit 2
    fi

# An endpoint read from a state file or printed by cargo-pmcp must live under its profile's tenant
# (plan 13-04, D-06): exactly https://<label><host_suffix>/mcp, with the suffix from the table in
# `_laya-target-check` and <label> ^[a-z][a-z0-9-]{0,62}$; prints the label (the server id). Under
# DRY_RUN=1 only, http://127.0.0.1:<port>/mcp is accepted and prints `loopback`. Anything else is
# `REFUSED target-endpoint: ...` (exit 2). Reads the local pmcp config through the target check; no
# network. Row endpoint-check.
[positional-arguments]
_laya-endpoint-check url profile:
    #!/usr/bin/env bash
    set -euo pipefail
    URL="$1"
    P="$2"
    shape() { python3 -I -c 'import re, sys; sys.exit(0 if re.fullmatch(sys.argv[2], sys.argv[1]) else 1)' "$URL" "$1"; }
    if [ "${DRY_RUN:-0}" = "1" ] && shape 'http://127\.0\.0\.1:[0-9]{1,5}/mcp'; then
        echo "loopback"
        exit 0
    fi
    if ! shape 'https://[a-z][a-z0-9-]{0,62}\.[A-Za-z0-9.-]+/mcp'; then
        echo "REFUSED target-endpoint: the endpoint (${#URL} characters) is not https://<server>.<host>/mcp" >&2
        exit 2
    fi
    TLINE="$(just _laya-target-check "$P" "")"
    read -r _ _ T_SUFFIX _ <<< "$TLINE"
    python3 -I - "$URL" "$T_SUFFIX" "$P" <<'PY'
    import re, sys
    url, suffix, profile = sys.argv[1:4]
    m = re.fullmatch(r"https://([a-z][a-z0-9-]{0,62})" + re.escape(suffix) + r"/mcp", url)
    if not m:
        host = url[len("https://"):].split("/", 1)[0][:100]
        print(f"REFUSED target-endpoint: {host} is not under *{suffix}, the tenant of profile {profile}", file=sys.stderr)
        sys.exit(2)
    print(m.group(1))
    PY

# The offline fixture pmcp config (plan 13-04): [targets.dev] and [targets.augury-dev] shaped like
# ~/.pmcp/config.toml, with fixture execute-api hosts, written to <out>. The offline selftests export
# it as LAYA_PMCP_CONFIG (honoured only under DRY_RUN=1), so no case reads the real config.
[positional-arguments]
_laya-pmcp-fixture-config out:
    #!/usr/bin/env bash
    set -euo pipefail
    printf '%s\n' 'schema_version = 1' '' \
        '[targets.augury-dev]' 'type = "pmcp-run"' 'api_url = "https://fixture2.execute-api.eu-west-1.amazonaws.com"' \
        'aws_profile = "augury-dev"' 'region = "eu-west-1"' '' \
        '[targets.dev]' 'type = "pmcp-run"' 'api_url = "https://fixture1.execute-api.us-east-1.amazonaws.com"' \
        'aws_profile = "ze-kasher-dev"' 'region = "us-east-1"' > "$1"

# Write the gitignored models/decide/deploy-config-<server>.toml from the tracked template
# crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template for <apr>: server name,
# s3://<bucket>/decide/<server>/<sha256>.apr, the sha256 pin, and [auth] enabled from <auth> (on|off,
# the plan 08-11 decision). The server id is REQUIRED (D-19): one config file per server, none shared.
# LOCAL ONLY: eligibility is enforced by the recipes that write to AWS. DRY_RUN=1 uses the literal
# bucket dry-run-bucket.
# <region> (plan 13-01, D-05/D-20; `region=<r>` accepted) replaces the template's single
# `region = "us-east-1"` line, so the default render is byte-identical to the pre-region one. Every
# parameter is read POSITIONALLY: a `{{param}}` is pasted into this script as text, so a quote
# in a value would break the recipe's own quoting. The region must match ^[a-z]{2}(-[a-z]+)+-[0-9]$
# before anything else runs (`REFUSED region:`, exit 2, the value is never echoed).
# LAYA_DEPLOY_TEMPLATE overrides the template ONLY under DRY_RUN=1 (the region selftest's zero- and
# two-line templates); set in a live run it is `REFUSED template:` (exit 2) before any AWS call.
# <profile> names the deployment triple (plan 13-04, D-06): `_laya-target-check` and then the live
# `_laya-account-check` run before the sts call, and the config goes to `_laya-state-dir <profile>`
# (models/decide for ze-kasher-dev, as before; models/decide/<profile>/ otherwise).
[positional-arguments]
laya-deploy-config apr auth server env="dev" profile="ze-kasher-dev" region="us-east-1":
    #!/usr/bin/env bash
    set -euo pipefail
    APR="$1"
    AUTH_WORD="$2"
    SERVER="$3"
    ENV="${4#env=}"
    PROFILE="$5"
    REGION="${6#region=}"
    # The whole string, in the C locale: [[ =~ ]] anchors at the string's ends, so a value with an
    # embedded newline cannot pass on its first line (as `grep -Eq` would let it).
    REGION_ERE='^[a-z]{2}(-[a-z]+)+-[0-9]$'
    if ! (LC_ALL=C; [[ "$REGION" =~ $REGION_ERE ]]); then
        echo "REFUSED region: the region argument (${#REGION} characters) is not an AWS region name" >&2
        exit 2
    fi
    TEMPLATE="crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template"
    if [ -n "${LAYA_DEPLOY_TEMPLATE:-}" ]; then
        if [ "${DRY_RUN:-0}" = "1" ]; then
            TEMPLATE="$LAYA_DEPLOY_TEMPLATE"
        else
            echo "REFUSED template: LAYA_DEPLOY_TEMPLATE is honoured only under DRY_RUN=1" >&2
            exit 2
        fi
    fi
    just _laya-server-check "$SERVER" deployable
    case "$ENV" in
        dev|prod) ;;
        *) echo "ERROR: '$ENV' is not a known environment (expected dev or prod)" >&2; exit 2 ;;
    esac
    case "$AUTH_WORD" in
        on) AUTH=true ;;
        off) AUTH=false ;;
        *) echo "ERROR: auth must be on or off, got '$AUTH_WORD'" >&2; exit 2 ;;
    esac
    test -f "$APR" || { echo "ERROR: $APR does not exist" >&2; exit 2; }
    # The deployment triple (plan 13-04, D-06): the profile names ONE pmcp.run target, account and
    # region, and the region argument must agree with it; then the account is checked live. Both come
    # AFTER the template refusal above, so a refused live template makes no AWS call (13-01's R7).
    TLINE="$(just _laya-target-check "$PROFILE" "$REGION")"
    read -r T_TARGET T_REGION _ _ <<< "$TLINE"
    just _laya-account-check "$PROFILE" "$REGION"
    export PMCP_TARGET="$T_TARGET" AWS_PROFILE="$PROFILE" AWS_REGION="$T_REGION"
    unset PMCP_API_URL AWS_DEFAULT_REGION
    # shasum directly (plan 08-22, V11-c): the rtk hook never rewrites a command inside a recipe, so a
    # proxy branch added nothing and made the identity pin depend on which `rtk` is on PATH.
    sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
    H="$(sha256 "$APR")"
    if [ "${DRY_RUN:-0}" = "1" ]; then
        BUCKET="dry-run-bucket"
    else
        ACCOUNT="$(aws sts get-caller-identity --profile "$PROFILE" --query Account --output text)"
        BUCKET="aprender-decide-weights-${ACCOUNT}-${ENV}"
    fi
    # Per-target local state (D-06): models/decide for ze-kasher-dev (unchanged), models/decide/<profile> otherwise.
    SD="$(just _laya-state-dir "$PROFILE")"
    OUT="$SD/deploy-config-$SERVER.toml"
    mkdir -p "$SD"
    python3 - "$TEMPLATE" "$OUT" "$SERVER" \
        "s3://$BUCKET/decide/$SERVER/$H.apr" "$H" "$AUTH" "$REGION" <<'PY'
    import re, sys, tomllib
    template, out, server, uri, sha, auth, region = sys.argv[1:8]
    text = open(template).read()
    unset = '"UNSET-run-just-laya-deploy-config"'
    for key, value in (("name", server), ("APRENDER_DECIDE_S3_URI", uri), ("APRENDER_DECIDE_SHA256", sha)):
        marker = f"{key} = {unset}"
        if text.count(marker) != 1:
            sys.exit(f"{template}: expected exactly one placeholder for {key}; restore the template")
        text = text.replace(marker, f'{key} = "{value}"')
    # The stack's region: the template carries one literal region line (us-east-1 by default).
    region_line = 'region = "us-east-1"'
    if text.count(region_line) != 1:
        sys.exit(f"{template}: expected exactly one '{region_line}' line; restore the template")
    text = text.replace(region_line, f'region = "{region}"')
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
    assert cfg["aws"]["region"] == region
    open(out, "w").write(text)
    print(f"  server   {server}\n  s3 uri   {uri}\n  sha256   {sha}\n  auth     {auth}\n  region   {region}\n  iam      Allow s3:GetObject {arn}")
    PY
    just _laya-iam-check "$OUT" || { rm -f "$OUT"; echo "ERROR: the generated config failed the iam check; removed it" >&2; exit 1; }
    echo "  wrote $OUT (gitignored) for env=$ENV"

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

# Cross-compile the bootstrap of <server> (the thin crate `<server>-lambda`, D-19) for Lambda arm64
# and prove it: rebuilt now (the output is newer than the build's start), an aarch64 ELF, and
# carrying `<server>-lambda` (08-07 found a stale bootstrap from another crate at this path). A
# BUILD check, never resolver evidence.
laya-build-bootstrap server:
    #!/usr/bin/env bash
    set -euo pipefail
    # Refuse an id with no thin crate before the `touch` and the build below name a path from it.
    just _laya-server-check "{{server}}" deployable
    ulimit -n 65536 || echo "WARNING: could not raise the fd limit; a link may fail with ProcessFdQuotaExceeded" >&2
    command -v cargo-zigbuild >/dev/null || { echo "ERROR: cargo-zigbuild is not installed" >&2; exit 1; }
    TD="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
    OUT="$TD/{{target}}/release/bootstrap"
    STAMP="$(mktemp)"
    trap 'rm -f "$STAMP"' EXIT
    sleep 1
    # A cached build relinks nothing, so "newer than the start" needs the package to rebuild.
    touch "crates/{{server}}-lambda/src/main.rs"
    set +e
    cargo zigbuild --release --target {{target}}.2.34 -p "{{server}}-lambda" --bin bootstrap
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
    grep -aqF -- '{{server}}-lambda' "$OUT" || { echo "ERROR: $OUT does not name {{server}}-lambda -- a stale bootstrap from another crate" >&2; exit 1; }
    echo "BOOTSTRAP aarch64 OK $OUT ($(wc -c < "$OUT" | tr -d ' ') bytes)"

# Deploy <apr> as the decide server, FAIL-CLOSED. Refusals, each `REFUSED <check>: ...` before
# any AWS call: (1) generated config present, (2) no placeholder left, (3) local sha256 ==
# the config's pin and content-addressed key, (4) resolver proof present, naming the decide
# package, for the installed cargo-pmcp, (5) ELIGIBILITY: `just laya-verify` accepts the exact
# file, (6) the S3 object has the local size (live only: first (6a) the config's bucket must be
# this env's, aprender-decide-weights-<account>-<env>). DRY_RUN=1 stops at (6) with DRY-RUN OK. Live:
# touch -> DEPLOYING -> cargo pmcp deploy (crates root, swapped) -> compile-log, edge-health
# serverId and live identity-probe assertions; any identity failure runs `just laya-teardown`
# (containment). The edge /health GET retries HTTP 404 only (a brand-new server id's first
# /health can 404 for minutes, F-10-12-1), bounded by LAYA_EDGE_HEALTH_TRIES / _SLEEP /
# _MAX_SECONDS; any other status still contains at once.
# The deployment is the triple (plan 13-04, D-06): (3c) the profile and the config's [aws] region must
# pass `_laya-target-check`, whose values are then exported (PMCP_TARGET, AWS_PROFILE, AWS_REGION; with
# PMCP_API_URL and AWS_DEFAULT_REGION unset), (5b) `_laya-account-check` must find the table's account,
# every local file lives under `_laya-state-dir <profile>`, and the deployed endpoint must pass
# `_laya-endpoint-check` for the profile or it is contained. `TARGET RECEIPT:` quotes cargo-pmcp's banner.
laya-deploy apr run data base server env="dev" profile="ze-kasher-dev":
    #!/usr/bin/env bash
    set -euo pipefail
    APR="{{apr}}"
    SERVER="{{server}}"
    # The server id is a REQUIRED argument (D-19) and must have a thin crate: the retired id has none.
    just _laya-server-check "$SERVER" deployable
    ENV="{{ trim_start_match(env, "env=") }}"
    PROFILE="{{profile}}"
    # Per-target local state (plan 13-04, D-06): models/decide for ze-kasher-dev, models/decide/<profile>
    # otherwise, so a deploy of the same id on another tenant never touches this one's record.
    SD="$(just _laya-state-dir "$PROFILE")"
    CFG="$SD/deploy-config-$SERVER.toml"
    PROOF="models/decide/resolver-proof-$SERVER.txt"
    EXPECT_PKG="crates/$SERVER-lambda"
    refuse() { echo "REFUSED $1: $2" >&2; exit "${3:-2}"; }
    # shasum directly (plan 08-22, V11-c): the rtk hook never rewrites a command inside a recipe, so a
    # proxy branch added nothing and made the identity pin depend on which `rtk` is on PATH.
    sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
    case "$ENV" in
        dev|prod) ;;
        *) refuse env "'$ENV' is not a known environment (expected dev or prod)" ;;
    esac
    [ -f "$APR" ] || refuse artifact "$APR does not exist"
    # (1) the generated config
    [ -f "$CFG" ] || refuse config "$CFG does not exist -- generate it: just laya-deploy-config <apr> <on|off> $SERVER $ENV $PROFILE <region>"
    # (2) no placeholder left
    if grep -q 'UNSET-run-just-laya-deploy-config' "$CFG"; then
        refuse placeholder "$CFG still holds an UNSET placeholder -- regenerate it with just laya-deploy-config <apr> <on|off> $SERVER"
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
    # (3c) the deployment triple (plan 13-04, D-06): the config's [aws] region and the profile must name
    # one row of the committed target table, and the shell may not point elsewhere. Then the checked
    # values are EXPORTED and PMCP_API_URL / AWS_DEFAULT_REGION unset, so cargo-pmcp (which then injects
    # the target's own api_url) and every aws call below resolve only this target; the repo-root
    # .pmcp/active-target is never consulted and `cargo pmcp configure use` is never run.
    CFG_REGION="$(python3 -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["aws"]["region"])' "$CFG" 2>/dev/null)" \
        || refuse config "$CFG has no [aws] region"
    TLINE="$(just _laya-target-check "$PROFILE" "$CFG_REGION")"
    read -r T_TARGET T_REGION _ _ <<< "$TLINE"
    export PMCP_TARGET="$T_TARGET" AWS_PROFILE="$PROFILE" AWS_REGION="$T_REGION"
    unset PMCP_API_URL AWS_DEFAULT_REGION
    # (4) the resolver proof for the installed cargo-pmcp
    [ -f "$PROOF" ] || refuse resolver-proof "$PROOF does not exist -- run: just laya-resolver-proof <sdk> <commit> <scratch> $SERVER"
    [ "$(head -n 1 "$PROOF")" = "$EXPECT_PKG" ] || refuse resolver-proof "$PROOF names '$(head -n 1 "$PROOF")', not $EXPECT_PKG"
    PROVEN_VER="$(sed -n 's/^cargo_pmcp_version=//p' "$PROOF")"
    INSTALLED_VER="$(cargo pmcp --version 2>/dev/null | awk '{print $2}' || true)"
    if [ -z "$INSTALLED_VER" ] || [ "$INSTALLED_VER" != "$PROVEN_VER" ]; then
        refuse resolver-proof "the proof is for cargo-pmcp ${PROVEN_VER:-?}, the installed tool is ${INSTALLED_VER:-absent}; re-run just laya-resolver-proof <sdk> <commit> <scratch> $SERVER"
    fi
    # (4b) D-07 (plan 13-06): a REDEPLOY of an OAuth-protected server probes its identity with a token minted
    # from LAYA_M2M_FILE; without one the identity probe is refused and this recipe would CONTAIN a healthy
    # function. A token already in the caller's environment is refused. The auth path is account-checked
    # before a token is minted or used (D-08; the (5b) check below still runs). With the file, this recipe
    # re-execs ITSELF, every argument kept, under scripts/laya_deploy/m2m_token.py (the child carries the token
    # in APRENDER_DECIDE_PROBE_TOKEN and LAYA_M2M_MINTED=1, so it does not re-exec again). The token host is the
    # helper's --token-host choice: LAYA_M2M_TOKEN_HOST, else <state dir>/m2m-token-host, else this profile's
    # existing STATE endpoint; with none of them there is nothing to authenticate to (a first publish is auth
    # off) and the run is refused. The expires_in floor is 2400 s, to outlast the build, the deploy, the edge
    # settle and the identity probe. RISK: a token that expires before the identity probe makes this recipe
    # contain a healthy function; LAYA_M2M_MIN_EXPIRES (300..86400) lowers the floor to match a shorter
    # Cognito validity, at that risk.
    if [ -n "${APRENDER_DECIDE_PROBE_TOKEN+x}" ] && [ "${LAYA_M2M_MINTED:-}" != "1" ]; then
        refuse probe-auth "APRENDER_DECIDE_PROBE_TOKEN is set in the caller's environment; unset it and name the client file in LAYA_M2M_FILE"
    fi
    if [ -n "${LAYA_M2M_FILE:-}" ] || [ "${LAYA_M2M_MINTED:-}" = "1" ]; then
        just _laya-account-check "$PROFILE" "$CFG_REGION"
    fi
    if [ -n "${LAYA_M2M_FILE:-}" ] && [ "${LAYA_M2M_MINTED:-}" != "1" ]; then
        MINEXP="${LAYA_M2M_MIN_EXPIRES:-2400}"
        case "$MINEXP" in ''|*[!0-9]*) refuse probe-auth "config: LAYA_M2M_MIN_EXPIRES is not a whole number of seconds" ;; esac
        if [ "${#MINEXP}" -gt 5 ] || [ "$((10#$MINEXP))" -lt 300 ] || [ "$((10#$MINEXP))" -gt 86400 ]; then
            refuse probe-auth "config: LAYA_M2M_MIN_EXPIRES must be in 300..86400"
        fi
        STATE_FILE="$SD/deploy-$SERVER.state/deployment.toml"
        EH=""
        if [ -f "$STATE_FILE" ]; then
            EP="$(python3 -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["deployment"]["endpoint"])' "$STATE_FILE" 2> /dev/null)" \
                || refuse probe-auth "$STATE_FILE has no [deployment] endpoint"
            GOT="$(just _laya-endpoint-check "$EP" "$PROFILE")"
            [ "$GOT" = "$SERVER" ] || refuse probe-auth "$STATE_FILE names server $GOT, not $SERVER"
            EH="${EP#https://}"
            EH="${EH%%/*}"
        fi
        if [ -z "$EH" ] && [ -z "${LAYA_M2M_TOKEN_HOST:-}" ] && [ ! -e "$SD/m2m-token-host" ]; then
            refuse probe-auth "LAYA_M2M_FILE was given but no endpoint exists yet (a first publish is auth off, D-07)"
        fi
        TH="$(python3 -I scripts/laya_deploy/m2m_token.py --token-host --state-dir "$SD" --endpoint-host "$EH")"
        if [ "${DRY_RUN:-0}" = "1" ]; then
            echo "DRY-RUN: LAYA_M2M_FILE is set; the live run mints a token for $TH before the first probe"
        else
            exec python3 -I scripts/laya_deploy/m2m_token.py --file "$LAYA_M2M_FILE" --expect-host "$TH" --min-expires "$((10#$MINEXP))" -- just laya-deploy "$APR" "{{run}}" "{{data}}" "{{base}}" "$SERVER" "$ENV" "$PROFILE"
        fi
    fi
    # (5) ELIGIBILITY (decide-apr-v1 deploy_eligibility; Phase 15 D-08/D-09) -- a run packed by the training
    # server's Graviton pack task is judged by its recorded verify (logs/pack-outcome.json, bound to THESE bytes
    # by apr_sha256), the local Rust verifier on the exact file still runs, and scripts/laya_deploy/
    # laya_eligibility.py decides what its outcome means (never `inspect`). LAYA_PACK_OUTCOME names a record
    # EXPLICITLY (an absent file refuses); otherwise a `<dir>/run` argument is looked up at
    # <dir>/logs/pack-outcome.json, and an absent convention record is no record: the local gate decides, as
    # before. Nothing below reaches AWS unless it accepted.
    mkdir -p "$SD"
    VLOG="$SD/eligibility-$SERVER.log"
    ELIG_PY="scripts/laya_deploy/laya_eligibility.py"
    REC="${LAYA_PACK_OUTCOME:-}"
    if [ -z "$REC" ]; then
        REC="$(python3 -I "$ELIG_PY" record-path --run "{{run}}")" || refuse eligibility "pack record discovery failed for {{run}}"
    fi
    if [ -n "$REC" ]; then
        set +e
        CROUT="$(python3 -I "$ELIG_PY" check-record --record "$REC" --apr-sha256 "$H" 2>&1)"
        crc=$?
        set -e
        if [ "$crc" -ne 0 ]; then
            echo "$CROUT" >&2
            exit "$crc"
        fi
        echo "  record: $CROUT"
    fi
    set +e
    just laya-verify "$APR" "{{run}}" "{{data}}" "{{base}}" > "$VLOG" 2>&1
    vrc=$?
    if [ -n "$REC" ]; then
        JOUT="$(python3 -I "$ELIG_PY" judge --apr-sha256 "$H" --verify-rc "$vrc" --verify-log "$VLOG" --record "$REC" --finding-log "$SD/cross-cpu-finding-$SERVER.log" --server "$SERVER" 2>&1)"
    else
        JOUT="$(python3 -I "$ELIG_PY" judge --apr-sha256 "$H" --verify-rc "$vrc" --verify-log "$VLOG" 2>&1)"
    fi
    jrc=$?
    set -e
    if [ "$jrc" -ne 0 ]; then
        echo "$JOUT" >&2
        exit "$jrc"
    fi
    echo "  eligible: $JOUT"
    # (5b) the profile resolves to the table's account (live sts; a no-op under DRY_RUN=1).
    just _laya-account-check "$PROFILE" "$CFG_REGION"
    # (6) the uploaded object
    BK_KEY="${CFG_URI#s3://}"
    BUCKET="${BK_KEY%%/*}"
    KEY="${BK_KEY#*/}"
    SIZE="$(wc -c < "$APR" | tr -d ' ')"
    if [ "${DRY_RUN:-0}" = "1" ]; then
        echo "DRY-RUN: skipping the head-object check of s3://$BUCKET/$KEY (expected ContentLength $SIZE)"
        just laya-build-bootstrap "$SERVER"
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
        || refuse s3-object "s3://$BUCKET/$KEY is absent -- upload it: just laya-upload $APR {{run}} {{data}} {{base}} $SERVER"
    [ "$REMOTE" = "$SIZE" ] || refuse s3-object "s3://$BUCKET/$KEY is $REMOTE bytes, $APR is $SIZE"
    # Live. The touch makes the decide package ALWAYS recompile, so its absence from the log
    # means cargo-pmcp built something else (a cached build prints no Compiling line).
    ulimit -n 65536 || echo "WARNING: could not raise the fd limit; a link may fail with ProcessFdQuotaExceeded" >&2
    touch "crates/$SERVER-lambda/src/main.rs"
    LOG="$SD/deploy-$SERVER.log"
    SNAP="$SD/deploy-$SERVER.state"
    rm -rf "$SNAP"
    contain() {
        echo "IDENTITY FAILURE: $1 -- containing (reserved concurrency 0)" >&2
        just laya-teardown "$SERVER" "$ENV" "$PROFILE" \
            || echo "ERROR: containment failed -- throttle it by hand: aws lambda put-function-concurrency --profile $PROFILE --region $AWS_REGION --function-name $SERVER --reserved-concurrent-executions 0" >&2
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
    # A receipt, not a gate: cargo-pmcp's own banner names the target it resolved.
    BANNER="$(grep -m 1 '→ Using target:' "$LOG" || true)"
    if [ -n "$BANNER" ]; then
        echo "TARGET RECEIPT: $BANNER"
    else
        echo "WARNING: no cargo-pmcp target banner in $LOG" >&2
    fi
    grep -q "Compiling $SERVER-lambda " "$LOG" || contain "the compile log does not name $SERVER-lambda"
    # The shared runtime library aprender-mcp-decide-lambda is the one other -lambda crate a thin crate builds.
    OTHER="$(grep -oE 'Compiling [A-Za-z0-9_-]+-lambda ' "$LOG" | grep -v -e "Compiling $SERVER-lambda " -e 'Compiling aprender-mcp-decide-lambda ' | sort -u | tr '\n' ' ' || true)"
    [ -z "$OTHER" ] || contain "the compile log also builds $OTHER"
    ENDPOINT="$(python3 -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["deployment"]["endpoint"])' "$SNAP/deployment.toml" 2>/dev/null)" \
        || contain "no endpoint in $SNAP/deployment.toml"
    # The endpoint must be this server under THIS profile's tenant (D-06): a deploy that landed on
    # another tenant's host is contained, not verified.
    GOT="$(just _laya-endpoint-check "$ENDPOINT" "$PROFILE")" || contain "the deployed endpoint is not under the tenant of profile $PROFILE"
    [ "$GOT" = "$SERVER" ] || contain "the deployed endpoint names server $GOT, not $SERVER"
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
    # F-10-12-1: a brand-new server id's first /health can answer 404 for minutes after cargo-pmcp
    # reports success (Phase 10: about 8 minutes). The helper retries that 404 and nothing else, within
    # LAYA_EDGE_HEALTH_TRIES / _SLEEP and the elapsed cap LAYA_EDGE_HEALTH_MAX_SECONDS; every other
    # outcome contains at once, and a refusal leaves no body file behind.
    just _laya-edge-health-get "$HEALTH_URL" "$SD/deploy-health-$SERVER.json" \
        || contain "GET $HEALTH_URL did not return 200 (404 retried at most ${LAYA_EDGE_HEALTH_TRIES:-40} times within ${LAYA_EDGE_HEALTH_MAX_SECONDS:-900} s, F-10-12-1)"
    HEALTH="$(cat "$SD/deploy-health-$SERVER.json")" || contain "no edge health body was kept for $HEALTH_URL"
    just _laya-edge-health-check "$HEALTH" "$SERVER" \
        || contain "the edge health body at $HEALTH_URL does not name $SERVER as serverId: $HEALTH"
    echo "  edge health: $HEALTH_URL names serverId $SERVER"
    # The edge settles AFTER cargo-pmcp reports success: on 2026-09-27 pmcp.run's own post-deploy
    # invocation loaded the model (24.6 s, success) and the edge still answered POST /mcp with 503
    # "Server is in error state" for under a minute, then forwarded. So THAT refusal, and only
    # that one, is retried (bounded); any other probe failure contains at once. (plan 08-17)
    PLOG="$SD/deploy-probe-$SERVER.log"
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

# GET the edge /health URL into <out>, retrying HTTP 404 and nothing else (F-10-12-1). A brand-new
# server id's first /health can answer 404 for minutes after cargo-pmcp reports success (Phase 10's
# first deploy: about 8 minutes), so a single GET contained a healthy function. Bounds: at most
# LAYA_EDGE_HEALTH_TRIES attempts (default 40) LAYA_EDGE_HEALTH_SLEEP seconds apart (default 15), AND
# a total elapsed cap LAYA_EDGE_HEALTH_MAX_SECONDS (default 900): a retry is not started when
# elapsed + SLEEP + the 30 s curl timeout would pass the cap. The cap, not TRIES x SLEEP, is the worst
# case, because each attempt can itself spend the 30 s curl timeout: the helper never runs past the
# cap (15 min by default) where 40 x (30 s + 15 s) alone would allow about 29.75 min. Any other status
# (500 and 503 included: 503 is the identity probe's retry code, never this GET's), a transport
# failure, an invalid retry configuration (refused before any request, exit 2) or a 404 beyond either
# bound exits 1. <out> is truncated before every attempt and removed on every refusal, so a stale
# deploy-health file can never pass _laya-edge-health-check. Exercised by laya-deploy-selftest's
# EDGE HEALTH get table (a scripted curl) and the edge-health-get gates row.
[positional-arguments]
_laya-edge-health-get url out:
    #!/usr/bin/env bash
    set -euo pipefail
    URL="$1"
    OUT="$2"
    refuse_config() {
        rm -f "$OUT"
        echo "REFUSED edge-health-get: config $1" >&2
        exit 2
    }
    TRIES="${LAYA_EDGE_HEALTH_TRIES:-40}"
    SLEEP="${LAYA_EDGE_HEALTH_SLEEP:-15}"
    MAX="${LAYA_EDGE_HEALTH_MAX_SECONDS:-900}"
    for name in TRIES SLEEP MAX; do
        value="${!name}"
        case "$value" in
            ''|*[!0-9]*) refuse_config "LAYA_EDGE_HEALTH_$name=$value is not a whole number" ;;
        esac
        [ "${#value}" -le 9 ] || refuse_config "LAYA_EDGE_HEALTH_$name=$value is too large"
    done
    TRIES=$((10#$TRIES))
    SLEEP=$((10#$SLEEP))
    MAX=$((10#$MAX))
    [ "$TRIES" -ge 1 ] || refuse_config "LAYA_EDGE_HEALTH_TRIES=$TRIES is below 1"
    CURL_TIMEOUT=30
    START=$SECONDS
    k=0
    while :; do
        k=$((k + 1))
        : > "$OUT"
        set +e
        code="$(curl -sS --max-time "$CURL_TIMEOUT" -o "$OUT" -w '%{http_code}' "$URL")"
        crc=$?
        set -e
        elapsed=$((SECONDS - START))
        if [ "$crc" -ne 0 ]; then
            rm -f "$OUT"
            echo "REFUSED edge-health-get: transport (curl exit $crc) after $k attempt(s)" >&2
            exit 1
        fi
        case "$code" in
            200)
                echo "edge-health-get ok: HTTP 200 after $k attempt(s) in $elapsed s"
                exit 0
                ;;
            404)
                if [ "$k" -ge "$TRIES" ]; then
                    rm -f "$OUT"
                    echo "REFUSED edge-health-get: HTTP 404 after $k attempt(s) (tries $TRIES)" >&2
                    exit 1
                fi
                if [ $((elapsed + SLEEP + CURL_TIMEOUT)) -gt "$MAX" ]; then
                    rm -f "$OUT"
                    echo "REFUSED edge-health-get: HTTP 404 after $k attempt(s) (elapsed cap $MAX s)" >&2
                    exit 1
                fi
                echo "  edge /health not routed yet (HTTP 404), attempt $k/$TRIES after $elapsed s; retrying in $SLEEP s" >&2
                sleep "$SLEEP"
                ;;
            *)
                rm -f "$OUT"
                echo "REFUSED edge-health-get: HTTP $code after $k attempt(s)" >&2
                exit 1
                ;;
        esac
    done

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

# The sha256 and path of every PERSISTENT models/decide file, sorted by path: every regular file directly
# in models/decide whose name does not end in .apr, every file under models/decide/*.state/ and every file
# under models/decide/augury-dev/ (absent directories contribute nothing). models/decide/selftest/, the
# swap backup and the archive directory are scratch or evidence and are not listed. Taken before and after
# an offline selftest, two listings that are not cmp-equal name the persistent file it left changed
# (plan 13-01). Read-only; prints the listing and nothing else.
_laya-state-snapshot:
    #!/usr/bin/env bash
    set -euo pipefail
    python3 -I - <<'PY'
    import hashlib, os
    root = "models/decide"
    paths = []
    if os.path.isdir(root):
        for name in os.listdir(root):
            p = os.path.join(root, name)
            if os.path.isfile(p):
                if not name.endswith(".apr"):
                    paths.append(p)
            elif os.path.isdir(p) and (name.endswith(".state") or name == "augury-dev"):
                for d, _, files in os.walk(p):
                    paths.extend(os.path.join(d, f) for f in files if os.path.isfile(os.path.join(d, f)))
    for p in sorted(paths):
        h = hashlib.sha256()
        with open(p, "rb") as fh:
            for chunk in iter(lambda: fh.read(1 << 20), b""):
                h.update(chunk)
        print(h.hexdigest() + "  " + p)
    PY

# Prove the region parameter of laya-deploy-config and laya-weights-bucket OFFLINE (plan 13-01, D-20):
# DRY_RUN=1, an aws recorder first on PATH that must stay empty (AWS CALLS: 0), a fixed fixture artifact
# and a COPY of the base template passed as LAYA_DEPLOY_TEMPLATE (the tracked template is never edited;
# LAYA_REGION_SELFTEST_TEMPLATE replaces the base). Cases: R1 the default render; R2 an explicit
# us-east-1 render, cmp-equal to R1; R3 an eu-west-1 render that differs from R1 in exactly the region
# line; R4/R5 a template with zero / two `region = "us-east-1"` lines is refused and the pre-seeded output
# is unchanged; R6 malformed and injecting regions (a quote, a newline, a `touch` canary) are refused
# before any interpolation and never echoed, and the `region=eu-west-1` form renders; R7 LAYA_DEPLOY_TEMPLATE
# with DRY_RUN unset is refused with zero recorder calls; BUCKET laya-weights-bucket's create-bucket argv
# per region (a scripted fake aws for that table only). Both config paths a render of the server id can
# reach (models/decide/deploy-config-<id>.toml and
# models/decide/augury-dev/deploy-config-<id>.toml) are backed up and restored on EXIT. Ends with
# `REGION SELFTEST OK`, or `REGION SELFTEST FAILED (<k> failed checks)` and exit 1. Row region-selftest.
_laya-region-selftest:
    #!/usr/bin/env bash
    set -euo pipefail
    export DRY_RUN=1
    # An ambient target cannot steer an offline case (plan 13-04): every case names its profile.
    unset PMCP_TARGET PMCP_API_URL AWS_PROFILE AWS_REGION AWS_DEFAULT_REGION LAYA_ACCOUNT_CHECK_FORCE
    ST="models/decide/selftest"
    SRV="${LAYA_SELFTEST_SERVER:-aprender-mcp-decide-antisemite}"
    BASE_TEMPLATE="${LAYA_REGION_SELFTEST_TEMPLATE:-crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template}"
    CFG="models/decide/deploy-config-$SRV.toml"
    AUG_DIR="$(just _laya-state-dir augury-dev)"
    AUG="$AUG_DIR/deploy-config-$SRV.toml"
    FAILS=0
    fail() { echo "FAIL $1" >&2; FAILS=$((FAILS + 1)); }
    mkdir -p "$ST"
    W="$(mktemp -d "$ST/region.XXXXXX")"
    # Both config paths a render of this id can reach come back byte-identical (or absent) on EXIT.
    AUG_DIR_CREATED=0
    [ -d "$AUG_DIR" ] || AUG_DIR_CREATED=1
    if [ -f "$CFG" ]; then cp -p "$CFG" "$W/keep-default.toml"; fi
    if [ -f "$AUG" ]; then cp -p "$AUG" "$W/keep-augury.toml"; fi
    cleanup() {
        if [ -f "$W/keep-default.toml" ]; then cp -p "$W/keep-default.toml" "$CFG"; else rm -f "$CFG"; fi
        if [ -f "$W/keep-augury.toml" ]; then cp -p "$W/keep-augury.toml" "$AUG"; else rm -f "$AUG"; fi
        if [ "$AUG_DIR_CREATED" = "1" ] && [ -d "$AUG_DIR" ]; then rmdir "$AUG_DIR" 2> /dev/null || true; fi
        rm -rf "$W"
    }
    trap cleanup EXIT
    # The fixture pmcp config (plan 13-04): the target checks read it, never ~/.pmcp/config.toml.
    just _laya-pmcp-fixture-config "$W/pmcp-config.toml"
    export LAYA_PMCP_CONFIG="$W/pmcp-config.toml"
    # The aws recorder: first on PATH, appends its argv, never reaches AWS.
    SHIM="$(pwd -P)/$ST/region-shim"
    REC="$SHIM/aws-calls.log"
    mkdir -p "$SHIM"
    : > "$REC"
    printf '#!/usr/bin/env bash\nprintf "%%s\\n" "$*" >> "%s"\necho "aws recorder: the region selftest never reaches AWS" >&2\nexit 97\n' "$REC" > "$SHIM/aws"
    chmod +x "$SHIM/aws"
    export PATH="$SHIM:$PATH"
    [ "$(command -v aws)" = "$SHIM/aws" ] || { echo "REGION SELFTEST FAILED: the aws recorder is not first on PATH" >&2; exit 1; }
    set +e; aws recorder-control > /dev/null 2>&1; crc=$?; set -e
    [ "$crc" -eq 97 ] && [ "$(wc -l < "$REC" | tr -d ' ')" -eq 1 ] || { echo "REGION SELFTEST FAILED: the aws recorder did not record its control call (exit $crc)" >&2; exit 1; }
    : > "$REC"
    echo "AWS RECORDER: control call recorded and cleared"
    # A fixed fixture artifact (laya-deploy-config only hashes it under DRY_RUN) and the base template copy.
    A="$W/fixture.apr"
    printf 'p13-01 region selftest fixture\n' > "$A"
    [ -f "$BASE_TEMPLATE" ] || { echo "REGION SELFTEST FAILED: the base template $BASE_TEMPLATE does not exist" >&2; exit 1; }
    cp "$BASE_TEMPLATE" "$W/base.toml"
    export LAYA_DEPLOY_TEMPLATE="$W/base.toml"
    RC=0
    run() { local log="$1"; shift; set +e; "$@" > "$log" 2>&1; RC=$?; set -e; }
    # The path a render wrote, from its own `wrote <path>` line (so a later per-target path is followed).
    wrote() { sed -n 's/^  wrote \(.*\) (gitignored) for env=.*$/\1/p' "$1" | tail -n 1; }
    aws_region() { python3 -I -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["aws"]["region"])' "$1" 2> /dev/null || echo "<unparsed>"; }
    # `<n> <old line> => <new line>` for two files of equal line count; `count <a> <b>` when they differ.
    line_delta() {
        python3 -I -c 'import sys
    a = open(sys.argv[1]).read().split("\n"); b = open(sys.argv[2]).read().split("\n")
    if len(a) != len(b):
        print("count", len(a), len(b))
    else:
        d = [(x, y) for x, y in zip(a, b) if x != y]
        print(len(d), " | ".join(x + " => " + y for x, y in d))' "$1" "$2"
    }
    # R1: the default render (no region argument).
    run "$W/r1.log" just laya-deploy-config "$A" off "$SRV"
    O1="$(wrote "$W/r1.log")"
    if [ "$RC" -eq 0 ] && [ -n "$O1" ] && [ -f "$O1" ]; then
        cp "$O1" "$W/r1.toml"
        echo "CASE R1 default: rendered $O1, aws.region $(aws_region "$W/r1.toml"), sha256 $(shasum -a 256 "$W/r1.toml" | cut -c 1-12)"
    else
        fail "CASE R1 default: exit $RC, no rendered config (log: $W/r1.log)"; tail -3 "$W/r1.log" >&2 || true
        : > "$W/r1.toml"
    fi
    # R2: an explicit us-east-1 render is byte-identical to the default one.
    run "$W/r2.log" just laya-deploy-config "$A" off "$SRV" dev ze-kasher-dev us-east-1
    O2="$(wrote "$W/r2.log")"
    if [ "$RC" -eq 0 ] && [ -n "$O2" ] && cmp -s "$O2" "$W/r1.toml" && [ "$(aws_region "$O2")" = "us-east-1" ]; then
        echo "CASE R2 explicit us-east-1: cmp-equal to R1, aws.region us-east-1"
    else
        fail "CASE R2 explicit us-east-1: exit $RC, not cmp-equal to R1 or aws.region $( [ -n "$O2" ] && aws_region "$O2" ) (log: $W/r2.log)"
    fi
    # R3: eu-west-1 changes exactly one line, the region line. The augury-dev render goes to that
    # profile's state dir (plan 13-04, D-06) and leaves the default path's bytes alone.
    R3_DEFAULT_BEFORE="$(if [ -f "$CFG" ]; then shasum -a 256 "$CFG" | awk '{print $1}'; else echo absent; fi)"
    run "$W/r3.log" just laya-deploy-config "$A" off "$SRV" dev augury-dev eu-west-1
    O3="$(wrote "$W/r3.log")"
    R3_DEFAULT_AFTER="$(if [ -f "$CFG" ]; then shasum -a 256 "$CFG" | awk '{print $1}'; else echo absent; fi)"
    if [ "$RC" -eq 0 ] && [ -n "$O3" ] && [ -f "$O3" ]; then
        R3_REGION="$(aws_region "$O3")"
        R3_DELTA="$(line_delta "$W/r1.toml" "$O3")"
        if [ "$R3_REGION" = "eu-west-1" ] && [ "$R3_DELTA" = '1 region = "us-east-1" => region = "eu-west-1"' ] \
            && [ "$O3" = "$AUG" ] && [ "$R3_DEFAULT_AFTER" = "$R3_DEFAULT_BEFORE" ]; then
            echo "CASE R3 eu-west-1: aws.region eu-west-1, exactly one line differs from R1 (region = \"us-east-1\" => region = \"eu-west-1\"), written to $O3, the default path's sha256 unchanged"
        else
            fail "CASE R3 eu-west-1: aws.region $R3_REGION, line delta against R1: $R3_DELTA; wrote '$O3' (want $AUG); default sha256 $R3_DEFAULT_BEFORE -> $R3_DEFAULT_AFTER"
        fi
    else
        fail "CASE R3 eu-west-1: exit $RC, no rendered config (log: $W/r3.log)"; tail -3 "$W/r3.log" >&2 || true
    fi
    # The refusal half. The output file is PRE-SEEDED with known bytes: a refused render leaves it unchanged.
    SEED="p13-01 pre-seeded output: a refused render must leave these bytes"
    seed() { printf '%s\n' "$SEED" > "$CFG"; SEED_SHA="$(shasum -a 256 "$CFG" | awk '{print $1}')"; }
    unchanged() { [ -f "$CFG" ] && [ "$(shasum -a 256 "$CFG" | awk '{print $1}')" = "$SEED_SHA" ]; }
    ONE_LINE="expected exactly one 'region = \"us-east-1\"' line"
    # R4: a template copy with the region line removed; R5: a second exact region line under a new table.
    grep -v '^region = "us-east-1"$' "$W/base.toml" > "$W/zero.toml" || true
    { cat "$W/base.toml"; printf '\n[p13-r5]\nregion = "us-east-1"\n'; } > "$W/two.toml"
    for c in "R4 zero-region-lines:zero" "R5 two-region-lines:two"; do
        label="${c%%:*}"; tpl="${c##*:}"
        seed
        run "$W/$tpl.log" env LAYA_DEPLOY_TEMPLATE="$W/$tpl.toml" just laya-deploy-config "$A" off "$SRV"
        if [ "$RC" -ne 0 ] && grep -qF "$ONE_LINE" "$W/$tpl.log" && unchanged; then
            echo "CASE $label: REFUSED as expected (exit $RC, $ONE_LINE), output sha256 unchanged"
        else
            fail "CASE $label: exit $RC, expected the one-line refusal and an unchanged output (log: $W/$tpl.log)"; tail -3 "$W/$tpl.log" >&2 || true
        fi
    done
    # R6: malformed and injecting regions are refused BEFORE any interpolation, the value never echoed.
    CANARY="$W/canary-created"
    NL="$(printf 'eu-west-1\nx')"
    r6_refuse() {
        local kind="$1" value="$2" log="$W/r6-$1.log"
        seed
        run "$log" just laya-deploy-config "$A" off "$SRV" dev ze-kasher-dev "$value"
        if [ "$RC" -eq 2 ] && grep -q '^REFUSED region: ' "$log" && unchanged && [ ! -e "$CANARY" ] \
            && python3 -I -c 'import sys; v = sys.argv[2]; sys.exit(1 if v and v in open(sys.argv[1]).read() else 0)' "$log" "$value"; then
            echo "CASE R6 $kind: REFUSED as expected ($(grep -m 1 '^REFUSED region: ' "$log")), output unchanged, value not echoed"
        else
            fail "CASE R6 $kind: exit $RC, expected exit 2 REFUSED region:, an unchanged output, no canary and the value absent from the log (log: $log)"
        fi
    }
    r6_refuse upper-case 'EU-WEST-1'
    r6_refuse trailing-quote 'eu-west-1"'
    r6_refuse empty ''
    r6_refuse underscores 'eu_west_1'
    r6_refuse path '../x'
    r6_refuse embedded-newline "$NL"
    r6_refuse canary "x\"; touch $CANARY; \""
    [ ! -e "$CANARY" ] || fail "CASE R6 canary: the injected touch ran ($CANARY exists)"
    run "$W/r6-prefix.log" just laya-deploy-config "$A" off "$SRV" dev augury-dev region=eu-west-1
    O6="$(wrote "$W/r6-prefix.log")"
    if [ "$RC" -eq 0 ] && [ -n "$O6" ] && [ "$(aws_region "$O6")" = "eu-west-1" ]; then
        echo "CASE R6 prefix-form: region=eu-west-1 renders aws.region eu-west-1"
    else
        fail "CASE R6 prefix-form: exit $RC, region=eu-west-1 did not render aws.region eu-west-1 (log: $W/r6-prefix.log)"
    fi
    # R7: LAYA_DEPLOY_TEMPLATE in a LIVE run (DRY_RUN unset) is refused before any AWS call: the
    # recorder is first on PATH, so a broken refusal shows up as a recorder call, never a real one.
    seed
    R7_BEFORE="$(wc -l < "$REC" | tr -d ' ')"
    run "$W/r7.log" env -u DRY_RUN LAYA_DEPLOY_TEMPLATE="$W/base.toml" just laya-deploy-config "$A" off "$SRV"
    R7_AFTER="$(wc -l < "$REC" | tr -d ' ')"
    if [ "$RC" -eq 2 ] && grep -q '^REFUSED template: ' "$W/r7.log" && [ "$R7_AFTER" -eq "$R7_BEFORE" ] && unchanged; then
        echo "CASE R7 live-template: REFUSED as expected ($(grep -m 1 '^REFUSED template: ' "$W/r7.log")), 0 recorder calls, output unchanged"
    else
        fail "CASE R7 live-template: exit $RC, recorder calls $R7_BEFORE -> $R7_AFTER; expected exit 2 REFUSED template: with no call (log: $W/r7.log)"
    fi
    # BUCKET: laya-weights-bucket's create-bucket argv against a scripted fake aws, first on PATH for
    # this table only (the bucket is absent; every argv is recorded; an unknown call exits 97).
    FB="$W/fakeaws"
    FLOG="$W/fake-aws-calls.log"
    mkdir -p "$FB"
    cat > "$FB/aws" <<'SH'
    #!/usr/bin/env bash
    printf '%s\n' "$*" >> "$FAKE_BUCKET_LOG"
    case "$1 $2" in
        "sts get-caller-identity") echo 111122223333 ;;
        "s3api head-bucket") exit 254 ;;
        "s3api create-bucket"|"s3api put-public-access-block"|"s3api put-bucket-encryption"|"s3api put-bucket-tagging") exit 0 ;;
        *) echo "fake aws: unexpected call: $*" >&2; exit 97 ;;
    esac
    SH
    chmod +x "$FB/aws"
    export FAKE_BUCKET_LOG="$FLOG"
    PATH_BEFORE_TABLE="$PATH"
    export PATH="$FB:$PATH"
    create_argv() { grep '^s3api create-bucket ' "$FLOG" || true; }
    : > "$FLOG"
    run "$W/bucket-us.log" just laya-weights-bucket dev ze-kasher-dev us-east-1
    ARGV="$(create_argv)"
    if [ "$RC" -eq 0 ] && printf '%s' "$ARGV" | grep -q -- '--region us-east-1' && ! printf '%s' "$ARGV" | grep -q -- '--create-bucket-configuration'; then
        echo "BUCKET us-east-1: create-bucket --region us-east-1, no --create-bucket-configuration"
    else
        fail "BUCKET us-east-1: exit $RC, create-bucket argv '$ARGV' (log: $W/bucket-us.log)"
    fi
    : > "$FLOG"
    run "$W/bucket-eu.log" just laya-weights-bucket dev augury-dev eu-west-1
    ARGV="$(create_argv)"
    if [ "$RC" -eq 0 ] && printf '%s' "$ARGV" | grep -q -- '--region eu-west-1' && printf '%s' "$ARGV" | grep -q -- '--create-bucket-configuration LocationConstraint=eu-west-1'; then
        echo "BUCKET eu-west-1: create-bucket --region eu-west-1 --create-bucket-configuration LocationConstraint=eu-west-1"
    else
        fail "BUCKET eu-west-1: exit $RC, create-bucket argv '$ARGV' (log: $W/bucket-eu.log)"
    fi
    : > "$FLOG"
    run "$W/bucket-refuse.log" just laya-weights-bucket dev ze-kasher-dev EU-WEST-1
    FCALLS="$(wc -l < "$FLOG" | tr -d ' ')"
    if [ "$RC" -eq 2 ] && grep -q '^REFUSED region: ' "$W/bucket-refuse.log" && [ "$FCALLS" -eq 0 ]; then
        echo "BUCKET refuse: EU-WEST-1 REFUSED as expected (exit 2), 0 fake-aws calls"
    else
        fail "BUCKET refuse: exit $RC with $FCALLS fake-aws call(s); expected exit 2 REFUSED region: and no call (log: $W/bucket-refuse.log)"
    fi
    export PATH="$PATH_BEFORE_TABLE"
    [ "$(command -v aws)" = "$SHIM/aws" ] || fail "the aws recorder is not first on PATH again after the BUCKET table"
    CALLS="$(wc -l < "$REC" | tr -d ' ')"
    [ "$CALLS" -eq 0 ] || fail "the aws recorder was called $CALLS time(s): $(head -n 3 "$REC" | tr '\n' ';')"
    echo "AWS CALLS: $CALLS"
    if [ "$FAILS" -eq 0 ]; then
        echo "REGION SELFTEST OK"
    else
        echo "REGION SELFTEST FAILED ($FAILS failed checks)" >&2
        exit 1
    fi

# Prove the deployment triple OFFLINE (plan 13-04, D-06): DRY_RUN=1, the ambient PMCP_TARGET /
# PMCP_API_URL / AWS_PROFILE / AWS_REGION / AWS_DEFAULT_REGION unset at setup, an aws recorder first on
# PATH that must stay empty (AWS CALLS: 0; scripted fake aws dirs go first on PATH for one table only),
# and a fixture pmcp config exported as LAYA_PMCP_CONFIG (LAYA_TARGET_SELFTEST_CONFIG replaces it).
# ISOLATION 1: an augury-dev render goes to models/decide/augury-dev/ and the default config's sha256 is
# unchanged; ISOLATION 2: an augury-dev laya-deploy with no augury config is REFUSED config even though
# the default config exists; ISOLATION 3: with it, the same deploy reaches REFUSED eligibility
# (SyntheticNotDeployable), every config and target check having passed on the augury file.
# Then the case tables: TARGET (the committed table: must-pass and at least ten must-refuse, among them
# each stale env variable, a conflicting PMCP_API_URL and a wrong-region api_url), ACCOUNT (the live
# account check against a scripted fake sts, forced by LAYA_ACCOUNT_CHECK_FORCE=1), STATE, ENDPOINT
# (anchored suffix, spoof hosts, loopback only under DRY_RUN=1), WIRING (ordered line indices over
# every recipe body: target check < account check < first AWS line), TEARDOWN REACHABLE (laya-deploy's
# exported environment reaches CONTAINED; its wrong-region twin is refused before any put) and VERIFY
# STATE (laya-deploy-verify reads the augury-dev STATE and writes its plan files under its state dir).
# The default and augury-dev config paths of the server id (LAYA_SELFTEST_SERVER, default
# aprender-mcp-decide-antisemite) and every augury-dev file a case can write come back byte-identical
# (or absent) on EXIT. Ends with `TARGET SELFTEST OK`, or `TARGET SELFTEST FAILED (<k> failed checks)`
# and exit 1. Row target-selftest.
_laya-target-selftest:
    #!/usr/bin/env bash
    set -euo pipefail
    export DRY_RUN=1
    unset PMCP_TARGET PMCP_API_URL AWS_PROFILE AWS_REGION AWS_DEFAULT_REGION LAYA_ACCOUNT_CHECK_FORCE
    ST="models/decide/selftest"
    SRV="${LAYA_SELFTEST_SERVER:-aprender-mcp-decide-antisemite}"
    T="crates/aprender-decide/tests/fixtures/laya_tiny"
    GOLDEN="37d65159b2be0fa091aa840cd56c1a84b73c0bcd9e2df5906d1f8218f5448561"
    FAILS=0
    fail() { echo "FAIL $1" >&2; FAILS=$((FAILS + 1)); }
    sha_or_absent() { if [ -f "$1" ]; then shasum -a 256 "$1" | awk '{print $1}'; else echo absent; fi; }
    mkdir -p "$ST"
    W="$(cd "$(mktemp -d "$ST/target.XXXXXX")" && pwd -P)"
    CFG="models/decide/deploy-config-$SRV.toml"
    AUG_DIR="$(just _laya-state-dir augury-dev)"
    AUG="$AUG_DIR/deploy-config-$SRV.toml"
    # Every file a case can write: the default config and the augury-dev state a deploy reaches.
    KEEP=("$CFG" "$AUG" "$AUG_DIR/eligibility-$SRV.log" "$AUG_DIR/plan-concentrated.json" "$AUG_DIR/plan-distributed.json"
        models/decide/plan-concentrated.json models/decide/plan-distributed.json)
    AUG_DIR_CREATED=0
    [ -d "$AUG_DIR" ] || AUG_DIR_CREATED=1
    k=0
    for f in "${KEEP[@]}"; do
        k=$((k + 1))
        if [ -f "$f" ]; then cp -p "$f" "$W/keep-$k"; fi
    done
    cleanup() {
        local f k=0
        for f in "${KEEP[@]}"; do
            k=$((k + 1))
            if [ -f "$W/keep-$k" ]; then cp -p "$W/keep-$k" "$f"; else rm -f "$f"; fi
        done
        if [ "$AUG_DIR_CREATED" = "1" ] && [ -d "$AUG_DIR" ]; then rmdir "$AUG_DIR" 2> /dev/null || true; fi
        rm -rf "$W"
    }
    trap cleanup EXIT
    # The pmcp config every case reads: the fixture, or LAYA_TARGET_SELFTEST_CONFIG (the row's must-fail).
    if [ -n "${LAYA_TARGET_SELFTEST_CONFIG:-}" ]; then
        cp "$LAYA_TARGET_SELFTEST_CONFIG" "$W/pmcp-config.toml"
    else
        just _laya-pmcp-fixture-config "$W/pmcp-config.toml"
    fi
    export LAYA_PMCP_CONFIG="$W/pmcp-config.toml"
    # The aws recorder: first on PATH, appends its argv, never reaches AWS.
    SHIM="$W/recorder"
    REC="$W/aws-calls.log"
    mkdir -p "$SHIM"
    : > "$REC"
    printf '#!/usr/bin/env bash\nprintf "%%s\\n" "$*" >> "%s"\necho "aws recorder: the target selftest never reaches AWS" >&2\nexit 97\n' "$REC" > "$SHIM/aws"
    chmod +x "$SHIM/aws"
    export PATH="$SHIM:$PATH"
    [ "$(command -v aws)" = "$SHIM/aws" ] || { echo "TARGET SELFTEST FAILED: the aws recorder is not first on PATH" >&2; exit 1; }
    set +e; aws recorder-control > /dev/null 2>&1; crc=$?; set -e
    [ "$crc" -eq 97 ] && [ "$(wc -l < "$REC" | tr -d ' ')" -eq 1 ] || { echo "TARGET SELFTEST FAILED: the aws recorder did not record its control call (exit $crc)" >&2; exit 1; }
    : > "$REC"
    echo "AWS RECORDER: control call recorded and cleared"
    RC=0
    run() { local log="$1"; shift; set +e; "$@" < /dev/null > "$log" 2>&1; RC=$?; set -e; }
    # The synthetic tiny artifact: laya-deploy reaches its eligibility check on it, which refuses it.
    A="$W/tiny.apr"
    just laya-pack-fixture "$T" "$T/data" "$A" > "$W/fixture.log" 2>&1 && grep -q "sha256=$GOLDEN" "$W/fixture.log" \
        || { echo "TARGET SELFTEST FAILED: laya-pack-fixture did not reproduce the golden $GOLDEN" >&2; tail -3 "$W/fixture.log" >&2; exit 1; }
    deploy_augury() { just laya-deploy "$A" "$T" "$T/data" "$T/checkpoint" "$SRV" dev augury-dev; }
    # ISOLATION 1: the augury-dev render goes to its own state dir; the default config's bytes stay.
    run "$W/iso-default.log" just laya-deploy-config "$A" off "$SRV"
    [ "$RC" -eq 0 ] && [ -f "$CFG" ] || { fail "ISOLATION setup: the default ze-kasher-dev render failed (exit $RC, log: $W/iso-default.log)"; tail -3 "$W/iso-default.log" >&2 || true; }
    D_BEFORE="$(sha_or_absent "$CFG")"
    rm -f "$AUG"
    run "$W/iso1.log" just laya-deploy-config "$A" off "$SRV" dev augury-dev eu-west-1
    D_AFTER="$(sha_or_absent "$CFG")"
    if [ "$RC" -eq 0 ] && [ -f "$AUG" ] && grep -qF "  wrote $AUG (gitignored)" "$W/iso1.log" && [ "$D_BEFORE" != absent ] && [ "$D_AFTER" = "$D_BEFORE" ]; then
        echo "ISOLATION 1 render: augury-dev wrote $AUG; the default $CFG sha256 unchanged (${D_AFTER:0:12})"
    else
        fail "ISOLATION 1 render: exit $RC, augury config $( [ -f "$AUG" ] && echo present || echo absent ), default sha256 $D_BEFORE -> $D_AFTER (log: $W/iso1.log)"
        tail -3 "$W/iso1.log" >&2 || true
    fi
    # ISOLATION 2: no augury config, the default one present -> the augury deploy refuses, naming its own path.
    rm -f "$AUG"
    run "$W/iso2.log" deploy_augury
    if [ "$RC" -eq 2 ] && [ -f "$CFG" ] && grep -qF "REFUSED config: $AUG does not exist" "$W/iso2.log"; then
        echo "ISOLATION 2 no-augury-config: REFUSED config naming $AUG although $CFG exists (exit 2)"
    else
        fail "ISOLATION 2 no-augury-config: exit $RC, expected exit 2 REFUSED config: $AUG does not exist (log: $W/iso2.log)"
        tail -3 "$W/iso2.log" >&2 || true
    fi
    # ISOLATION 3: with the augury config, every config and target check passes and eligibility refuses the fixture.
    run "$W/iso3-config.log" just laya-deploy-config "$A" off "$SRV" dev augury-dev eu-west-1
    run "$W/iso3.log" deploy_augury
    if [ "$RC" -ne 0 ] && grep -Eq 'REFUSED eligibility: .*SyntheticNotDeployable' "$W/iso3.log" && grep -q '^target-check ok: profile augury-dev -> pmcp.run target augury-dev' "$W/iso3.log"; then
        echo "ISOLATION 3 augury-config: reached $(grep -Eo 'REFUSED eligibility: .*SyntheticNotDeployable' "$W/iso3.log" | head -n 1 | cut -c 1-60) after target-check ok on $AUG"
    else
        fail "ISOLATION 3 augury-config: exit $RC, expected REFUSED eligibility SyntheticNotDeployable after target-check ok (log: $W/iso3.log)"
        tail -3 "$W/iso3.log" >&2 || true
    fi
    # TARGET: the committed table as a must-pass / must-refuse case table (CLAUDE.md rule 7). Every
    # refusal is exit 2 with nothing on stdout; an env disagreement is set on that one command only.
    cfg_get() { python3 -I -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["targets"][sys.argv[2]]["api_url"])' "$LAYA_PMCP_CONFIG" "$1" 2> /dev/null || echo unreadable; }
    API_AUG="$(cfg_get augury-dev)"
    API_US="$(cfg_get dev)"
    # variant <out> <key> <value>: the fixture config with ONE key of [targets.augury-dev] replaced.
    variant() {
        just _laya-pmcp-fixture-config "$1"
        python3 -I - "$1" "$2" "$3" <<'PY'
    import re, sys
    p, key, value = sys.argv[1:4]
    t = open(p).read()
    t, n = re.subn(r'(\[targets\.augury-dev\][^\[]*?^' + key + r' = )"[^"]*"', lambda m: m.group(1) + '"' + value + '"', t, count=1, flags=re.M | re.S)
    assert n == 1, key
    open(p, "w").write(t)
    PY
    }
    variant "$W/cfg-profile.toml" aws_profile ze-kasher-dev
    variant "$W/cfg-api.toml" api_url https://fixture3.execute-api.us-east-1.amazonaws.com
    variant "$W/cfg-type.toml" type aws-lambda
    tcase() {
        local want="$1" label="$2" expect="$3" out
        shift 3
        set +e; "$@" < /dev/null > "$W/tc.out" 2> "$W/tc.err"; RC=$?; set -e
        out="$(cat "$W/tc.out")"
        if [ "$want" = accept ] && [ "$RC" -eq 0 ] && [ "$out" = "$expect" ]; then
            echo "TARGET accept: $label -> $out"
        elif [ "$want" = refuse ] && [ "$RC" -eq 2 ] && [ -z "$out" ] && grep -Eq "^$expect" "$W/tc.err"; then
            echo "TARGET refuse: $label (exit 2, $(grep -E -m 1 "^$expect" "$W/tc.err" | cut -c 1-150))"
        else
            fail "TARGET $want $label: exit $RC, stdout '$out' (stderr: $(head -n 2 "$W/tc.err" | tr '\n' ' ' | cut -c 1-200))"
        fi
    }
    L_US='dev us-east-1 .us-east.true-mcp.com 816069129666'
    L_AUG='augury-dev eu-west-1 .eu-west.augury.pmcp.run 783960389328'
    tcase accept "ze-kasher-dev us-east-1" "$L_US" just _laya-target-check ze-kasher-dev us-east-1
    tcase accept "augury-dev with an empty region (the table's)" "$L_AUG" just _laya-target-check augury-dev ""
    tcase accept "augury-dev eu-west-1 with AWS_PROFILE=augury-dev" "$L_AUG" env AWS_PROFILE=augury-dev just _laya-target-check augury-dev eu-west-1
    tcase accept "augury-dev with PMCP_API_URL equal to the target's own api_url" "$L_AUG" env PMCP_API_URL="$API_AUG" just _laya-target-check augury-dev eu-west-1
    tcase refuse "unknown profile mam-dev" 'REFUSED target: ' just _laya-target-check mam-dev ""
    tcase refuse "augury-dev with region us-east-1" 'REFUSED target: ' just _laya-target-check augury-dev us-east-1
    tcase refuse "augury-dev with PMCP_TARGET=dev" 'REFUSED target: PMCP_TARGET ' env PMCP_TARGET=dev just _laya-target-check augury-dev eu-west-1
    tcase refuse "augury-dev with AWS_PROFILE=ze-kasher-dev" 'REFUSED target: AWS_PROFILE ' env AWS_PROFILE=ze-kasher-dev just _laya-target-check augury-dev eu-west-1
    tcase refuse "augury-dev with AWS_REGION=us-east-1" 'REFUSED target: AWS_REGION ' env AWS_REGION=us-east-1 just _laya-target-check augury-dev eu-west-1
    tcase refuse "augury-dev with AWS_DEFAULT_REGION=us-east-1" 'REFUSED target: AWS_DEFAULT_REGION ' env AWS_DEFAULT_REGION=us-east-1 just _laya-target-check augury-dev eu-west-1
    tcase refuse "augury-dev with PMCP_API_URL = the us-east-1 control plane" 'REFUSED target: PMCP_API_URL ' env PMCP_API_URL="$API_US" just _laya-target-check augury-dev eu-west-1
    tcase refuse "a config whose augury-dev target names profile ze-kasher-dev" 'REFUSED target-config: ' env LAYA_PMCP_CONFIG="$W/cfg-profile.toml" just _laya-target-check augury-dev eu-west-1
    tcase refuse "a config whose augury-dev api_url is an us-east-1 execute-api host" 'REFUSED target-config: ' env LAYA_PMCP_CONFIG="$W/cfg-api.toml" just _laya-target-check augury-dev eu-west-1
    tcase refuse "a config whose augury-dev target is not of type pmcp-run" 'REFUSED target-config: ' env LAYA_PMCP_CONFIG="$W/cfg-type.toml" just _laya-target-check augury-dev eu-west-1
    tcase refuse "an absent config file" 'REFUSED target-config: ' env LAYA_PMCP_CONFIG="$W/no-such-config.toml" just _laya-target-check augury-dev eu-west-1
    tcase refuse "LAYA_PMCP_CONFIG set without DRY_RUN=1" 'REFUSED target-config: LAYA_PMCP_CONFIG ' env -u DRY_RUN just _laya-target-check augury-dev eu-west-1
    tcase refuse "a profile with an embedded newline" 'REFUSED target: ' just _laya-target-check "$(printf 'augury-dev\nx')" ""
    # ACCOUNT: the live account check against a scripted fake aws (first on PATH for this table only),
    # forced under DRY_RUN=1 by LAYA_ACCOUNT_CHECK_FORCE=1; FAKE_ACCOUNT is what its sts answers.
    FA="$W/fake-account"
    FALOG="$W/fake-account-calls.log"
    mkdir -p "$FA"
    cat > "$FA/aws" <<'SH'
    #!/usr/bin/env bash
    printf '%s\n' "$*" >> "$FAKE_ACCOUNT_LOG"
    case "$1 $2" in
        "sts get-caller-identity")
            if [ "$FAKE_ACCOUNT" = fail ]; then echo "An error occurred (ExpiredToken) when calling the GetCallerIdentity operation" >&2; exit 255; fi
            echo "$FAKE_ACCOUNT" ;;
        *) echo "fake aws: unexpected call: $*" >&2; exit 97 ;;
    esac
    SH
    chmod +x "$FA/aws"
    export FAKE_ACCOUNT_LOG="$FALOG"
    PATH_BEFORE_TABLE="$PATH"
    export PATH="$FA:$PATH"
    acase() {
        local want="$1" label="$2" answer="$3" pat="$4" prof="$5" calls
        : > "$FALOG"
        set +e; env FAKE_ACCOUNT="$answer" LAYA_ACCOUNT_CHECK_FORCE=1 just _laya-account-check "$prof" "" < /dev/null > "$W/ac.log" 2>&1; RC=$?; set -e
        calls="$(grep -c "^sts get-caller-identity --profile $prof " "$FALOG" || true)"
        if [ "$want" = accept ] && [ "$RC" -eq 0 ] && grep -Eq "$pat" "$W/ac.log" && [ "$calls" -eq 1 ]; then
            echo "ACCOUNT accept: $label ($(grep -Eo -m 1 "$pat" "$W/ac.log"), 1 sts call)"
        elif [ "$want" = refuse ] && [ "$RC" -eq 2 ] && grep -Eq "$pat" "$W/ac.log" && [ "$calls" -eq 1 ]; then
            echo "ACCOUNT refuse: $label (exit 2, $(grep -Eo -m 1 "$pat" "$W/ac.log" | cut -c 1-140))"
        else
            fail "ACCOUNT $want $label: exit $RC, $calls sts call(s) (log: $(head -n 3 "$W/ac.log" | tr '\n' ' ' | cut -c 1-200))"
        fi
    }
    acase accept "augury-dev, sts answers 783960389328" 783960389328 '^target-account ok: profile augury-dev is account 783960389328$' augury-dev
    acase accept "ze-kasher-dev, sts answers 816069129666" 816069129666 '^target-account ok: profile ze-kasher-dev is account 816069129666$' ze-kasher-dev
    acase refuse "augury-dev, sts answers ze-kasher-dev's account" 816069129666 '^REFUSED target-account: profile augury-dev resolves to an account ending 9666, not the table.s 783960389328$' augury-dev
    acase refuse "ze-kasher-dev, sts answers augury-dev's account" 783960389328 '^REFUSED target-account: profile ze-kasher-dev resolves to an account ending 9328, not the table.s 816069129666$' ze-kasher-dev
    acase refuse "augury-dev, sts exits 255" fail '^REFUSED target-account: cannot read the account for profile augury-dev$' augury-dev
    acase refuse "augury-dev, a non-numeric answer" None '^REFUSED target-account: profile augury-dev did not answer a 12-digit account id' augury-dev
    : > "$FALOG"
    set +e; env FAKE_ACCOUNT=783960389328 just _laya-account-check augury-dev "" < /dev/null > "$W/ac-dry.log" 2>&1; RC=$?; set -e
    if [ "$RC" -eq 0 ] && grep -qx 'target-account: not checked (DRY_RUN=1)' "$W/ac-dry.log" && [ ! -s "$FALOG" ]; then
        echo "ACCOUNT accept: DRY_RUN=1 without LAYA_ACCOUNT_CHECK_FORCE -> not checked, 0 fake-aws calls"
    else
        fail "ACCOUNT dry-run: exit $RC, fake-aws calls $(wc -l < "$FALOG" | tr -d ' '); expected 'not checked' and no call"
    fi
    export PATH="$PATH_BEFORE_TABLE"
    [ "$(command -v aws)" = "$SHIM/aws" ] || fail "the aws recorder is not first on PATH again after the ACCOUNT table"
    # STATE: the per-profile local state dir.
    scase() {
        local want="$1" in="$2" expect="${3:-}" out label
        label="${in:-<empty>}"
        set +e; out="$(just _laya-state-dir "$in" 2> "$W/sd.err")"; RC=$?; set -e
        if [ "$want" = accept ] && [ "$RC" -eq 0 ] && [ "$out" = "$expect" ]; then
            echo "STATE accept: $label -> $out"
        elif [ "$want" = refuse ] && [ "$RC" -eq 2 ] && [ -z "$out" ] && grep -q '^REFUSED state-dir: ' "$W/sd.err"; then
            echo "STATE refuse: '$label' (exit 2, $(head -n 1 "$W/sd.err" | cut -c 1-120))"
        else
            fail "STATE $want '$label': exit $RC, got '$out'"
        fi
    }
    scase accept ze-kasher-dev models/decide
    scase accept augury-dev models/decide/augury-dev
    scase refuse ../x
    scase refuse ""
    scase refuse Augury
    # ENDPOINT: an endpoint must live under its profile's tenant (anchored suffix, no spoof host).
    US_MOT="https://aprender-mcp-decide-motivation.us-east.true-mcp.com/mcp"
    EU_MOT="https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/mcp"
    ecase() {
        local want="$1" url="$2" prof="$3" expect="${4:-}" out
        set +e; out="$(just _laya-endpoint-check "$url" "$prof" 2> "$W/ep.err")"; RC=$?; set -e
        if [ "$want" = accept ] && [ "$RC" -eq 0 ] && [ "$out" = "$expect" ]; then
            echo "ENDPOINT accept: $url for $prof -> $out"
        elif [ "$want" = refuse ] && [ "$RC" -eq 2 ] && [ -z "$out" ] && grep -q '^REFUSED target-endpoint: ' "$W/ep.err"; then
            echo "ENDPOINT refuse: $url for $prof (exit 2)"
        else
            fail "ENDPOINT $want $url for $prof: exit $RC, got '$out' ($(head -n 1 "$W/ep.err" | cut -c 1-160))"
        fi
    }
    ecase accept "$EU_MOT" augury-dev aprender-mcp-decide-motivation
    ecase accept "$US_MOT" ze-kasher-dev aprender-mcp-decide-motivation
    ecase refuse "$US_MOT" augury-dev
    ecase refuse "$EU_MOT" ze-kasher-dev
    ecase refuse "http://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/mcp" augury-dev
    ecase refuse "https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/health" augury-dev
    ecase refuse "https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run.evil.example/mcp" augury-dev
    ecase refuse "https://x.evil.eu-west.augury.pmcp.run/mcp" augury-dev
    ecase accept "http://127.0.0.1:8787/mcp" augury-dev loopback
    set +e; out="$(env -u DRY_RUN -u LAYA_PMCP_CONFIG just _laya-endpoint-check http://127.0.0.1:8787/mcp augury-dev 2> "$W/ep.err")"; RC=$?; set -e
    if [ "$RC" -eq 2 ] && [ -z "$out" ] && grep -q '^REFUSED target-endpoint: ' "$W/ep.err"; then
        echo "ENDPOINT refuse: http://127.0.0.1:8787/mcp for augury-dev without DRY_RUN=1 (exit 2)"
    else
        fail "ENDPOINT refuse loopback without DRY_RUN: exit $RC, got '$out'"
    fi
    # WIRING (ordered): every laya/_laya recipe that runs `aws <s3|s3api|sts|lambda|iam|logs>` or
    # `cargo pmcp deploy` on a code line (not a comment, not an echo/printf) calls the target check and
    # then the account check at SMALLER line indices than that first AWS line. Plus laya-deploy's export
    # before its deploy and its first contain, laya-deploy-verify's endpoint check before its build, and
    # laya-deploy-config's template refusal before both checks. Line indices, never mere presence.
    just --dump --dump-format json > "$W/dump.json"
    set +e
    python3 -I - "$W/dump.json" > "$W/wiring.log" 2>&1 <<'PY'
    import json, re, sys
    recipes = json.load(open(sys.argv[1]))["recipes"]
    render = lambda line: "".join(f if isinstance(f, str) else "<interpolation>" for f in line)
    AWS = re.compile(r"(^|[^A-Za-z0-9_./-])aws (s3|s3api|sts|lambda|iam|logs)\b|\bcargo pmcp deploy\b")
    def code(line):
        s = line.strip()
        if not s or s.startswith("#"):
            return False
        s = re.sub(r"^(\|\||&&)\s*", "", s)
        return not re.match(r"(echo|printf)\b", s)
    def first(body, pred):
        return next((i for i, l in enumerate(body) if pred(l)), None)
    problems = []
    if "_laya-iam-check" not in recipes:
        problems.append("the recipe enumeration lacks _laya-iam-check (must-match control: it is not the full list)")
    found = {}
    for n in recipes:
        if not n.startswith(("laya", "_laya")) or n.endswith("-selftest"):
            continue
        body = [render(l) for l in recipes[n]["body"]]
        k = first(body, lambda l: code(l) and AWS.search(l))
        if k is not None:
            found[n] = (body, k)
    WIRED = ["laya-deploy-config", "laya-deploy", "laya-upload", "laya-grant", "laya-deploy-verify", "laya-teardown", "laya-weights-bucket"]
    for n in WIRED:
        if n not in found:
            problems.append(f"{n} is not in the AWS-touching set: the AWS-line pattern no longer sees it")
    if "_laya-iam-check" in found:
        problems.append("must-not-match control: _laya-iam-check (pure) was classified as AWS-touching")
    for n, (body, k) in sorted(found.items()):
        i = first(body, lambda l: code(l) and "just _laya-target-check" in l)
        if n == "_laya-account-check":
            if i is not None and i < k:
                print(f"WIRING self {n}: target {i} < aws {k}")
            else:
                problems.append(f"{n}: its target check (line {i}) is not before its sts line {k}")
            continue
        j = first(body, lambda l: code(l) and "just _laya-account-check" in l)
        if i is not None and j is not None and i < j < k:
            print(f"WIRING order {n}: target {i} < account {j} < aws {k}")
        else:
            problems.append(f"WIRING order {n}: target {i}, account {j}, first aws {k}: the checks are missing or not before the first AWS line")
    if "laya-deploy" in found:
        body = found["laya-deploy"][0]
        e = first(body, lambda l: code(l) and re.search(r"\bexport PMCP_TARGET=", l))
        u = first(body, lambda l: code(l) and re.search(r"\bunset PMCP_API_URL\b", l))
        d = first(body, lambda l: code(l) and "cargo pmcp deploy" in l)
        cd = first(body, lambda l: code(l) and re.search(r"\bcontain\(\) \{", l))
        c = first(body, lambda l: code(l) and re.search(r"\bcontain \"", l))
        if None not in (e, u, d, cd, c) and max(e, u) < min(d, cd, c):
            print(f"WIRING laya-deploy export: export PMCP_TARGET {e}, unset PMCP_API_URL {u} < contain() {cd}, first contain {c}, cargo pmcp deploy {d}")
        else:
            problems.append(f"WIRING laya-deploy export: export PMCP_TARGET {e}, unset PMCP_API_URL {u}, contain() {cd}, first contain {c}, cargo pmcp deploy {d}")
    else:
        problems.append("must-match control: laya-deploy is not in the AWS-touching set")
    if "laya-deploy-verify" in found:
        body = found["laya-deploy-verify"][0]
        ep = first(body, lambda l: code(l) and "just _laya-endpoint-check" in l)
        cb = first(body, lambda l: code(l) and re.search(r"\bcargo build\b", l))
        if ep is not None and cb is not None and ep < cb:
            print(f"WIRING laya-deploy-verify endpoint: endpoint check {ep} < cargo build {cb}")
        else:
            problems.append(f"WIRING laya-deploy-verify endpoint: endpoint check {ep}, cargo build {cb}: the STATE endpoint is not checked before the probe build")
    if "laya-deploy-config" in found:
        body = found["laya-deploy-config"][0]
        tp = first(body, lambda l: "REFUSED template" in l)
        i = first(body, lambda l: code(l) and "just _laya-target-check" in l)
        j = first(body, lambda l: code(l) and "just _laya-account-check" in l)
        if None not in (tp, i, j) and tp < i and tp < j:
            print(f"WIRING laya-deploy-config template: REFUSED template {tp} < target {i}, account {j}")
        else:
            problems.append(f"WIRING laya-deploy-config template: REFUSED template {tp}, target {i}, account {j}")
    print(f"WIRING controls: {len(found)} AWS-touching recipes; laya-deploy {'in' if 'laya-deploy' in found else 'NOT in'} the set, _laya-iam-check enumerated and {'NOT ' if '_laya-iam-check' in found else ''}outside it")
    for p in problems:
        print(f"FAIL WIRING {p}")
    sys.exit(1 if problems else 0)
    PY
    wrc=$?
    set -e
    grep -v '^FAIL WIRING' "$W/wiring.log" || true
    WP="$(grep -c '^FAIL WIRING' "$W/wiring.log" || true)"
    if [ "$wrc" -ne 0 ] || [ "$WP" -ne 0 ]; then
        grep '^FAIL WIRING' "$W/wiring.log" >&2 || true
        FAILS=$((FAILS + (WP > 0 ? WP : 1)))
    fi
    # TEARDOWN REACHABLE: contain() runs laya-teardown with exactly the environment laya-deploy exports,
    # so containment on augury-dev must pass the target and account checks; the wrong-region twin is
    # refused before any put. A scripted fake aws, first on PATH for this case only.
    FT="$W/fake-teardown"
    FTLOG="$W/fake-teardown-calls.log"
    mkdir -p "$FT"
    cat > "$FT/aws" <<'SH'
    #!/usr/bin/env bash
    printf '%s\n' "$*" >> "$FAKE_TEARDOWN_LOG"
    case "$1 $2" in
        "sts get-caller-identity") echo 783960389328 ;;
        "lambda put-function-concurrency") echo '{}' ;;
        "lambda get-function-concurrency") echo 0 ;;
        "lambda get-function") echo "arn:aws:iam::783960389328:role/fake-decide-fn-role" ;;
        "iam delete-role-policy") echo "An error occurred (NoSuchEntity) when calling the DeleteRolePolicy operation: The role policy cannot be found." >&2; exit 254 ;;
        *) echo "fake aws: unexpected call: $*" >&2; exit 97 ;;
    esac
    SH
    chmod +x "$FT/aws"
    export FAKE_TEARDOWN_LOG="$FTLOG"
    read -r X_TARGET X_REGION _ _ <<< "$(just _laya-target-check augury-dev "" 2> /dev/null)"
    PATH_BEFORE_TABLE="$PATH"
    export PATH="$FT:$PATH"
    : > "$FTLOG"
    run "$W/teardown-ok.log" env PMCP_TARGET="$X_TARGET" AWS_PROFILE=augury-dev AWS_REGION="$X_REGION" LAYA_ACCOUNT_CHECK_FORCE=1 \
        just laya-teardown "$SRV" dev augury-dev
    if [ "$RC" -eq 0 ] && grep -q '^  CONTAINED: ' "$W/teardown-ok.log" && ! grep -q 'REFUSED' "$W/teardown-ok.log" \
        && grep -q '^target-check ok: profile augury-dev ' "$W/teardown-ok.log" && grep -q '^target-account ok: profile augury-dev is account 783960389328$' "$W/teardown-ok.log" \
        && grep -q '^sts get-caller-identity --profile augury-dev ' "$FTLOG" && grep -q '^lambda put-function-concurrency ' "$FTLOG"; then
        echo "TEARDOWN REACHABLE accept: PMCP_TARGET=$X_TARGET AWS_PROFILE=augury-dev AWS_REGION=$X_REGION (laya-deploy's exports) -> $(grep -m 1 'CONTAINED: ' "$W/teardown-ok.log" | sed 's/^ *//' | cut -c 1-90), target and account checks passed"
    else
        fail "TEARDOWN REACHABLE accept: exit $RC, expected CONTAINED and no REFUSED with laya-deploy's exported environment (log: $W/teardown-ok.log)"
        tail -3 "$W/teardown-ok.log" >&2 || true
    fi
    : > "$FTLOG"
    run "$W/teardown-twin.log" env PMCP_TARGET="$X_TARGET" AWS_PROFILE=augury-dev AWS_REGION=us-east-1 LAYA_ACCOUNT_CHECK_FORCE=1 \
        just laya-teardown "$SRV" dev augury-dev
    if [ "$RC" -eq 2 ] && grep -q '^REFUSED target' "$W/teardown-twin.log" && ! grep -q 'put-function-concurrency' "$FTLOG"; then
        echo "TEARDOWN REACHABLE refuse: the twin with AWS_REGION=us-east-1 is REFUSED target (exit 2), no put-function-concurrency call"
    else
        fail "TEARDOWN REACHABLE refuse: exit $RC, expected exit 2 REFUSED target and no put-function-concurrency (log: $W/teardown-twin.log)"
    fi
    export PATH="$PATH_BEFORE_TABLE"
    [ "$(command -v aws)" = "$SHIM/aws" ] || fail "the aws recorder is not first on PATH again after the TEARDOWN case"
    # VERIFY STATE: laya-deploy-verify reads the augury-dev deployment's STATE from its own state dir and
    # writes its plan files there; the default plan files keep their bytes.
    P_BEFORE="$(sha_or_absent models/decide/plan-concentrated.json) $(sha_or_absent models/decide/plan-distributed.json)"
    rm -f "$AUG_DIR/plan-concentrated.json" "$AUG_DIR/plan-distributed.json"
    WANT_STATE="<endpoint from $AUG_DIR/deploy-$SRV.state/deployment.toml>"
    run "$W/verify-state.log" just laya-deploy-verify "$A" "$SRV" augury-dev
    P_AFTER="$(sha_or_absent models/decide/plan-concentrated.json) $(sha_or_absent models/decide/plan-distributed.json)"
    if [ "$RC" -eq 0 ] && grep -qF "probe --url $WANT_STATE --apr" "$W/verify-state.log" \
        && [ -s "$AUG_DIR/plan-concentrated.json" ] && [ -s "$AUG_DIR/plan-distributed.json" ] && [ "$P_AFTER" = "$P_BEFORE" ]; then
        echo "VERIFY STATE augury-dev: the DRY_RUN plan names $WANT_STATE and wrote its plan files under $AUG_DIR/; the default plan files unchanged"
    else
        fail "VERIFY STATE augury-dev: exit $RC, expected $WANT_STATE, plan files under $AUG_DIR/ and the default ones unchanged ($P_BEFORE -> $P_AFTER) (log: $W/verify-state.log)"
        grep -m 1 'probe --url' "$W/verify-state.log" >&2 || tail -3 "$W/verify-state.log" >&2 || true
    fi
    [ "$(command -v aws)" = "$SHIM/aws" ] || fail "the aws recorder is not first on PATH at the end"
    CALLS="$(wc -l < "$REC" | tr -d ' ')"
    [ "$CALLS" -eq 0 ] || fail "the aws recorder was called $CALLS time(s): $(head -n 3 "$REC" | tr '\n' ';')"
    echo "AWS CALLS: $CALLS"
    if [ "$FAILS" -eq 0 ]; then
        echo "TARGET SELFTEST OK"
    else
        echo "TARGET SELFTEST FAILED ($FAILS failed checks)" >&2
        exit 1
    fi

# Prove every deploy refusal OFFLINE on the synthetic tiny artifact (the only one 08-09 writes),
# DRY_RUN=1 throughout, with `aws` shadowed by a recorder that must stay empty. Cases: placeholder,
# sha-pin, resolver-proof, deploy-eligibility, upload-eligibility (the last two from laya-verify:
# SyntheticNotDeployable); the crates-root swap restored byte-identical on success, forced
# failure, SIGTERM and an absent root; the edge-health step's URL and body tables and its wiring
# into laya-deploy (D-ITEM-08-17-A); resolver proof; bootstrap build; and (plan 10-12, D-19) the two-name
# cases: the retired id is REFUSED by laya-deploy and laya-upload before any file or AWS action, the old and
# new S3 prefixes can never satisfy each other's IAM or grant check, and two config files for two server
# names swap in and out of the shared crates root byte-identically. The POSITIVE dry run (a full
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
    # An ambient target cannot steer an offline case (plan 13-04): every case names its profile.
    unset PMCP_TARGET PMCP_API_URL AWS_PROFILE AWS_REGION AWS_DEFAULT_REGION LAYA_ACCOUNT_CHECK_FORCE
    T="crates/aprender-decide/tests/fixtures/laya_tiny"
    ST="models/decide/selftest"
    A="$ST/laya_tiny.apr"
    CASES="$ST/cases"
    # ONE variable for the server id, passed explicitly to every call (D-19); LAYA_SELFTEST_SERVER overrides it.
    SRV="${LAYA_SELFTEST_SERVER:-aprender-mcp-decide-antisemite}"
    RETIRED_SERVER="aprender-mcp-decide"   # the retired id: it has no thin crate, so no deploy recipe may accept it
    CFG="models/decide/deploy-config-$SRV.toml"
    PROOF="models/decide/resolver-proof-$SRV.txt"
    GOLDEN="37d65159b2be0fa091aa840cd56c1a84b73c0bcd9e2df5906d1f8218f5448561"
    FAILS=0
    fail() { echo "FAIL $1" >&2; FAILS=$((FAILS + 1)); }
    sha_or_absent() { if [ -f "$1" ]; then shasum -a 256 "$1" | awk '{print $1}'; else echo absent; fi; }
    rm -rf "$CASES" "$ST/snap" "$ST/root-absent"
    mkdir -p "$CASES" "$ST/shim"
    # The fixture pmcp config (plan 13-04): the target checks read it, never ~/.pmcp/config.toml.
    just _laya-pmcp-fixture-config "$CASES/pmcp-config.toml"
    export LAYA_PMCP_CONFIG="$(pwd -P)/$CASES/pmcp-config.toml"
    # Keep a pre-existing generated decide config and the resolver proof; restore both on EXIT.
    BK="$(mktemp -d "$ST/bk.XXXXXX")"
    [ -f "$CFG" ] && cp -p "$CFG" "$BK/decide-deploy.toml"
    cp -p "$PROOF" "$BK/resolver-proof.txt" 2>/dev/null || true
    # The id's eligibility logs (plan 13-01): the historical files are kept and the live paths REMOVED,
    # so only this run can write them. On EXIT this run's output is EXPORTED first (laya-gates-selftest's
    # row verify reads the export, never the restored history), then the historical files come back.
    ELIG="models/decide/eligibility-$SRV.log"
    ELIG_UP="models/decide/eligibility-upload-$SRV.log"
    if [ -f "$ELIG" ]; then cp -p "$ELIG" "$BK/eligibility.log"; fi
    if [ -f "$ELIG_UP" ]; then cp -p "$ELIG_UP" "$BK/eligibility-upload.log"; fi
    # The cross-CPU finding log (Phase 15 D-08): kept and removed the same way, so only this run's cases write it.
    FIND="models/decide/cross-cpu-finding-$SRV.log"
    if [ -f "$FIND" ]; then cp -p "$FIND" "$BK/cross-cpu-finding.log"; fi
    cleanup() {
        if [ -f "$ELIG" ]; then cp "$ELIG" "$CASES/eligibility-export-$SRV.log"; fi
        if [ -f "$ELIG_UP" ]; then cp "$ELIG_UP" "$CASES/eligibility-upload-export-$SRV.log"; fi
        if [ -f "$BK/eligibility.log" ]; then cp -p "$BK/eligibility.log" "$ELIG"; else rm -f "$ELIG"; fi
        if [ -f "$BK/eligibility-upload.log" ]; then cp -p "$BK/eligibility-upload.log" "$ELIG_UP"; else rm -f "$ELIG_UP"; fi
        if [ -f "$BK/cross-cpu-finding.log" ]; then cp -p "$BK/cross-cpu-finding.log" "$FIND"; else rm -f "$FIND"; fi
        if [ -f "$BK/decide-deploy.toml" ]; then cp -p "$BK/decide-deploy.toml" "$CFG"; else rm -f "$CFG"; fi
        [ -f "$BK/resolver-proof.txt" ] && cp -p "$BK/resolver-proof.txt" "$PROOF"
        rm -rf "$BK"
    }
    trap cleanup EXIT
    rm -f "$ELIG" "$ELIG_UP" "$FIND"
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
    regen() { just laya-deploy-config "$A" off "$SRV" > "$CASES/config-$1.log" 2>&1 || { tail -5 "$CASES/config-$1.log" >&2; exit 1; }; }
    regen setup
    [ -f "$PROOF" ] || { echo "ERROR: $PROOF is missing -- run just laya-resolver-proof <sdk> <commit> <scratch> $SRV first" >&2; exit 1; }
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
    deploy_tiny() { just laya-deploy "$A" "$T" "$T/data" "$T/checkpoint" "$SRV"; }
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
        just laya-upload "$A" "$T" "$T/data" "$T/checkpoint" "$SRV"
    # Phase 15 D-08/D-09: the pack record as the eligibility authority, OFFLINE on the synthetic fixture (no
    # real bytes, no real weights; the real-bytes path is the FULL-tier gates row upload-eligibility). The helper
    # first (its own case table, both roles), then cases a-f through BOTH recipes with the aws recorder empty.
    ELIG_PY="scripts/laya_deploy/laya_eligibility.py"
    if python3 -I "$ELIG_PY" --selftest > "$CASES/eligibility-helper.log" 2>&1 \
        && grep -Eq '^ELIGIBILITY SELFTEST OK [0-9]+ cases .*roles role-drift-advisory,role-all-advisory' "$CASES/eligibility-helper.log"; then
        echo "CASE 60 eligibility-helper: $(tail -n 1 "$CASES/eligibility-helper.log")"
    else
        fail "CASE 60 eligibility-helper: laya_eligibility.py --selftest failed (log: $CASES/eligibility-helper.log)"
        tail -5 "$CASES/eligibility-helper.log" >&2
    fi
    ROLE_NOW="$(python3 -I "$ELIG_PY" role)"
    mkrec() {
        python3 -I -c 'import json, sys; p, sha, acc = sys.argv[1:4]; json.dump({"schema": "laya-train-pack-outcome-v1", "accepted": acc == "true", "deploy_eligible": acc == "true", "apr_sha256": sha, "pack_image_digest": "sha256:" + "5" * 64, "parity_contract_sha256": "7" * 64}, open(p, "w"))' "$@"
    }
    mkrec "$CASES/record-valid.json" "$GOLDEN" true
    mkrec "$CASES/record-zeros.json" "0000000000000000000000000000000000000000000000000000000000000000" true
    mkrec "$CASES/record-unaccepted.json" "$GOLDEN" false
    mkdir -p "$CASES/conv-bad/logs"
    cp -R "$T" "$CASES/conv-absent-run-src" && mkdir -p "$CASES/conv-absent" && mv "$CASES/conv-absent-run-src" "$CASES/conv-absent/run"
    cp -R "$T" "$CASES/conv-bad/run"
    mkrec "$CASES/conv-bad/logs/pack-outcome.json" "$GOLDEN" false
    RN=60
    rcase() {
        local letter="$1" recipe="$2" rec="$3" run="$4" pat="$5" n
        RN=$((RN + 1)); n="$RN"
        expect_refused "$n" "record-$letter-$recipe" "$pat" env LAYA_PACK_OUTCOME="$rec" just "laya-$recipe" "$A" "$run" "$T/data" "$T/checkpoint" "$SRV"
    }
    for recipe in deploy upload; do
        # (a) a valid record: the signed role decides what the local SyntheticNotDeployable refusal means
        if [ "$ROLE_NOW" = "role-drift-advisory" ]; then
            FIND_BEFORE="$(sha_or_absent "$FIND")"
            rcase a "$recipe" "$CASES/record-valid.json" "$T" 'REFUSED eligibility: .*SyntheticNotDeployable'
            [ "$(sha_or_absent "$FIND")" = "$FIND_BEFORE" ] || fail "CASE record-a-$recipe: a non-drift refusal was logged as a cross-CPU finding"
        else
            RN=$((RN + 1)); n="$RN"
            set +e
            env LAYA_PACK_OUTCOME="$CASES/record-valid.json" just "laya-$recipe" "$A" "$T" "$T/data" "$T/checkpoint" "$SRV" > "$CASES/$n-record-a-$recipe.log" 2>&1
            set -e
            if grep -Fq "  eligible: ELIGIBLE pack-record sha256 $GOLDEN (CROSS-CPU FINDING: REFUSED SyntheticNotDeployable" "$CASES/$n-record-a-$recipe.log" \
                && grep -Fq "local=REFUSED SyntheticNotDeployable" "$FIND"; then
                echo "CASE $n record-a-$recipe: ELIGIBLE with a logged finding under $ROLE_NOW (the later DRY_RUN outcome is not asserted)"
            else
                fail "CASE $n record-a-$recipe: expected a logged SyntheticNotDeployable finding under $ROLE_NOW (log: $CASES/$n-record-a-$recipe.log)"
            fi
        fi
        # (b) the record names other bytes; (c) the pack did not accept; (d) an explicit record that is absent
        rcase b "$recipe" "$CASES/record-zeros.json" "$T" 'REFUSED eligibility: pack record apr_sha256 .* differs from the local file'
        rcase c "$recipe" "$CASES/record-unaccepted.json" "$T" 'REFUSED eligibility: pack record accepted is not true'
        rcase d "$recipe" "$CASES/no-such-record.json" "$T" 'REFUSED eligibility: pack record .* does not exist'
        # (e) convention-absent: a `run` dir with no logs/pack-outcome.json beside it is no record, the local gate decides
        rcase e "$recipe" "" "$CASES/conv-absent/run" 'REFUSED eligibility: .*SyntheticNotDeployable'
        if grep -q 'pack record' "$CASES/$RN-record-e-$recipe.log"; then
            fail "CASE $RN record-e-$recipe: an absent convention record was judged as a pack record"
        fi
        # (f) convention-present: the record beside the `run` dir is found and judged
        rcase f "$recipe" "" "$CASES/conv-bad/run" 'REFUSED eligibility: pack record accepted is not true'
    done
    [ "$(wc -l < "$REC" | tr -d ' ')" -eq 0 ] || fail "CASE 6x record cases: the aws recorder was called ($(head -n 3 "$REC" | tr '\n' ';'))"
    # D-19, D-20: the retired id has no thin crate. Both AWS-writing recipes refuse it naming `server`, and they
    # do so BEFORE any file or AWS action: no eligibility log is written for it, and the recorder stays empty.
    expect_refused 5a retired-deploy 'REFUSED server: .*declares no bootstrap bin' \
        just laya-deploy "$A" "$T" "$T/data" "$T/checkpoint" "$RETIRED_SERVER"
    expect_refused 5b retired-upload 'REFUSED server: .*declares no bootstrap bin' \
        just laya-upload "$A" "$T" "$T/data" "$T/checkpoint" "$RETIRED_SERVER"
    expect_refused 5c retired-config 'REFUSED server: .*declares no bootstrap bin' \
        just laya-deploy-config "$A" off "$RETIRED_SERVER"
    expect_refused 5d no-such-thin-crate 'REFUSED server: .*no thin crate' \
        just laya-deploy "$A" "$T" "$T/data" "$T/checkpoint" "$SRV-nonexistent"
    if [ -e "models/decide/eligibility-$RETIRED_SERVER.log" ] && [ "models/decide/eligibility-$RETIRED_SERVER.log" -nt "$CASES/0-fixture.log" ]; then
        fail "CASE 5a: a refused retired id still wrote an eligibility log (the refusal came too late)"
    fi
    if [ -e "models/decide/eligibility-upload-$RETIRED_SERVER.log" ] && [ "models/decide/eligibility-upload-$RETIRED_SERVER.log" -nt "$CASES/0-fixture.log" ]; then
        fail "CASE 5b: a refused retired id still wrote an upload eligibility log (the refusal came too late)"
    fi
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
    # D-19: two DIFFERENT server names, two different config files, one after the other. Each install is that
    # server's config alone, and the shared crates root is byte-identical after EACH (a second deploy for
    # another server finds exactly what the first one found).
    OTHER_CFG="$CASES/deploy-config-other-server.toml"
    python3 -c 'import re, sys; t = open(sys.argv[1]).read(); t, n = re.subn(r"^(name = )\"[^\"]*\"$", lambda m: m.group(1) + chr(34) + sys.argv[3] + chr(34), t, count=1, flags=re.M); assert n == 1; open(sys.argv[2], "w").write(t)' "$CFG" "$OTHER_CFG" "aprender-mcp-decide-second-example"
    cmp -s "$CFG" "$OTHER_CFG" && fail "SWAP 5: the second server's config is identical to the first"
    swap_case 5 server-a 0 crates "$CFG" "$SNAP" \
        sh -c 'cmp -s crates/.pmcp/deploy.toml "$1" && grep -q "^name = \"$2\"" crates/.pmcp/deploy.toml && test ! -e crates/deploy' _ "$CFG" "$SRV"
    swap_case 6 server-b 0 crates "$OTHER_CFG" "$SNAP" \
        sh -c 'cmp -s crates/.pmcp/deploy.toml "$1" && grep -q "^name = \"aprender-mcp-decide-second-example\"" crates/.pmcp/deploy.toml && test ! -e crates/deploy' _ "$OTHER_CFG"
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
    EP="https://$SRV.us-east.true-mcp.com/mcp"
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
    url_case accept "$EP" "https://$SRV.us-east.true-mcp.com/health"
    url_case refuse "https://$SRV.us-east.true-mcp.com/mcp/"
    url_case refuse "https://$SRV.us-east.true-mcp.com/health"
    url_case refuse "http://$SRV.us-east.true-mcp.com/mcp"
    url_case refuse "https://$SRV.us-east.true-mcp.com/x/mcp"
    url_case refuse ""
    body_case() {
        local want="$1" name="$2" body="$3" rc
        set +e; just _laya-edge-health-check "$body" "$SRV" > /dev/null 2>&1; rc=$?; set -e
        if { [ "$want" = accept ] && [ "$rc" -eq 0 ]; } || { [ "$want" = refuse ] && [ "$rc" -ne 0 ]; }; then
            echo "EDGE HEALTH body $want: $name (exit $rc)"
        else
            fail "EDGE HEALTH body $want $name: exit $rc"
        fi
    }
    body_case accept decide-edge "{\"status\":\"healthy\",\"serverId\":\"$SRV\",\"serverName\":\"$SRV\",\"hasDeployment\":true}"
    body_case refuse chronos-edge '{"status":"healthy","serverId":"chronos-forecaster","serverName":"chronos-forecaster","hasDeployment":true}'
    body_case refuse bootstrap-body "{\"package\":\"$SRV-lambda\",\"server\":\"$SRV\"}"
    # The reverse-prefix edge case: the retired server's own healthy edge body must NOT satisfy the new id.
    body_case refuse retired-server-edge "{\"status\":\"healthy\",\"serverId\":\"$RETIRED_SERVER\",\"serverName\":\"$RETIRED_SERVER\",\"hasDeployment\":true}"
    body_case refuse edge-405 '{"jsonrpc":"2.0","error":{"code":-32600,"message":"SSE streams are not offered at this endpoint. Use POST /mcp."},"id":null}'
    body_case refuse prefix-server "{\"serverId\":\"$SRV-2\"}"
    body_case refuse not-an-object "[\"$SRV\"]"
    body_case refuse not-json '<html>503</html>'
    body_case refuse empty ''
    # EDGE HEALTH get (F-10-12-1, plan 12-04): the bounded 404-only retry of the first /health GET,
    # over a scripted curl first on PATH for this table only (restored after it). The Nth call
    # answers line N of the script (the last line repeats); `x7` is a transport failure. The out
    # path is pre-seeded with a STALE healthy body before every case: after any refusal it must be
    # absent or empty, so a leftover deploy-health file can never pass _laya-edge-health-check.
    CURLSHIM="$(pwd -P)/$ST/curlshim"
    CURL_SCRIPT="$CASES/curl-script"
    CURL_CALLS="$CASES/curl-calls.log"
    mkdir -p "$CURLSHIM"
    cat > "$CURLSHIM/curl" <<'SH'
    #!/usr/bin/env bash
    out=""
    while [ $# -gt 0 ]; do
        case "$1" in
            -o) out="$2"; shift 2 ;;
            -w|--max-time) shift 2 ;;
            *) shift ;;
        esac
    done
    n=$(( $(wc -l < "$CURL_CALLS" | tr -d ' ') + 1 ))
    code="$(sed -n "${n}p" "$CURL_SCRIPT")"
    [ -n "$code" ] || code="$(tail -n 1 "$CURL_SCRIPT")"
    echo "$code" >> "$CURL_CALLS"
    if [ "$code" = x7 ]; then echo "curl: (7) shim: transport failure" >&2; printf '000'; exit 7; fi
    if [ "$code" = 200 ]; then
        printf '{"status":"healthy","serverId":"%s","via":"curl-shim"}' "$CURL_SERVER" > "$out"
    else
        printf '{"error":"shim answered %s"}' "$code" > "$out"
    fi
    printf '%s' "$code"
    SH
    chmod +x "$CURLSHIM/curl"
    PATH_BEFORE_CURL="$PATH"
    export PATH="$CURLSHIM:$PATH"
    [ "$(command -v curl)" = "$CURLSHIM/curl" ] || { echo "ERROR: the curl shim is not first on PATH" >&2; exit 1; }
    # hg_run <name> <comma-free script words> [VAR=value ...]: runs the helper, sets HG_RC / HG_CALLS.
    hg_run() {
        HG_NAME="$1"; local script="$2"
        shift 2
        HG_OUT="$CASES/health-get-$HG_NAME.body"; HG_LOG="$CASES/health-get-$HG_NAME.log"
        printf '%s\n' "$script" | tr ' ' '\n' > "$CURL_SCRIPT"
        : > "$CURL_CALLS"
        printf '{"status":"healthy","serverId":"%s"}\n' "$SRV" > "$HG_OUT"
        set +e
        env CURL_SCRIPT="$CURL_SCRIPT" CURL_CALLS="$CURL_CALLS" CURL_SERVER="$SRV" LAYA_EDGE_HEALTH_SLEEP=0 "$@" \
            just _laya-edge-health-get "https://$SRV.us-east.true-mcp.com/health" "$HG_OUT" > "$HG_LOG" 2>&1
        HG_RC=$?
        set -e
        HG_CALLS="$(wc -l < "$CURL_CALLS" | tr -d ' ')"
    }
    # hg_check <accept|refuse> <exit> <ERE> <calls>: sets HG_OK (1/0); an accept leaves the shim's body,
    # a refusal leaves no body file (absent or empty) although a stale healthy one was there before.
    hg_check() {
        local want="$1" wrc="$2" pat="$3" wcalls="$4"
        HG_OK=1
        [ "$HG_RC" -eq "$wrc" ] || HG_OK=0
        grep -Eq -- "$pat" "$HG_LOG" || HG_OK=0
        [ "$HG_CALLS" -eq "$wcalls" ] || HG_OK=0
        if [ "$want" = accept ]; then
            grep -q '"via":"curl-shim"' "$HG_OUT" || HG_OK=0
        else
            [ ! -s "$HG_OUT" ] || HG_OK=0
        fi
    }
    hg_say() {
        local want="$1"
        if [ "$HG_OK" -eq 1 ]; then
            echo "EDGE HEALTH get $want: $HG_NAME ($HG_CALLS calls)"
        else
            fail "EDGE HEALTH get $want $HG_NAME: exit $HG_RC, $HG_CALLS calls, body $( [ -s "$HG_OUT" ] && echo present || echo absent ) (log: $HG_LOG)"
            tail -3 "$HG_LOG" >&2 || true
        fi
    }
    hg_run ok-first 200
    hg_check accept 0 '^edge-health-get ok: HTTP 200 after 1 attempt\(s\)' 1; hg_say accept
    hg_run 404-404-200 "404 404 200"
    hg_check accept 0 '^edge-health-get ok: HTTP 200 after 3 attempt\(s\)' 3; hg_say accept
    hg_run 404-forever 404 LAYA_EDGE_HEALTH_TRIES=3
    hg_check refuse 1 'REFUSED edge-health-get: HTTP 404 after 3 attempt\(s\)' 3; hg_say refuse
    hg_run 500 500
    hg_check refuse 1 'REFUSED edge-health-get: HTTP 500 after 1 attempt\(s\)' 1; hg_say refuse
    hg_run 503 503
    hg_check refuse 1 'REFUSED edge-health-get: HTTP 503 after 1 attempt\(s\)' 1; hg_say refuse
    hg_run transport x7
    hg_check refuse 1 'REFUSED edge-health-get: transport \(curl exit 7\) after 1 attempt\(s\)' 1; hg_say refuse
    HG_CFG_OK=1; HG_CFG_CALLS=0; HG_CFG_N=0
    for kv in LAYA_EDGE_HEALTH_TRIES=0 LAYA_EDGE_HEALTH_TRIES=abc LAYA_EDGE_HEALTH_SLEEP=-1 LAYA_EDGE_HEALTH_MAX_SECONDS=x; do
        HG_CFG_N=$((HG_CFG_N + 1))
        hg_run "config-$HG_CFG_N" 200 "$kv"
        hg_check refuse 2 'REFUSED edge-health-get: config ' 0
        [ "$HG_OK" -eq 1 ] || { HG_CFG_OK=0; fail "EDGE HEALTH get config $kv: exit $HG_RC, $HG_CALLS calls (log: $HG_LOG)"; }
        HG_CFG_CALLS=$((HG_CFG_CALLS + HG_CALLS))
    done
    HG_NAME=config; HG_CALLS="$HG_CFG_CALLS"; HG_OK="$HG_CFG_OK"
    [ "$HG_CFG_N" -eq 4 ] || HG_OK=0
    hg_say refuse
    hg_run elapsed-cap 404 LAYA_EDGE_HEALTH_TRIES=40 LAYA_EDGE_HEALTH_MAX_SECONDS=0
    hg_check refuse 1 'REFUSED edge-health-get: HTTP 404 after 1 attempt\(s\) \(elapsed cap 0 s\)' 1; hg_say refuse
    export PATH="$PATH_BEFORE_CURL"
    [ "$(command -v curl)" != "$CURLSHIM/curl" ] || { echo "ERROR: the curl shim is still on PATH after the table" >&2; exit 1; }
    # Wiring: laya-deploy calls the health-url and health-check helpers, gets /health through the retrying
    # helper (never a bare curl), and never GETs the /mcp endpoint itself.
    DEPLOY_BODY="$(just --show laya-deploy)"
    DEPLOY_CODE="$(printf '%s\n' "$DEPLOY_BODY" | grep -v '^ *#' || true)"
    if printf '%s' "$DEPLOY_BODY" | grep -q 'just _laya-edge-health-url "\$ENDPOINT"' \
        && printf '%s' "$DEPLOY_BODY" | grep -q 'just _laya-edge-health-check "\$HEALTH" "\$SERVER"' \
        && printf '%s' "$DEPLOY_CODE" | grep -qF 'just _laya-edge-health-get "$HEALTH_URL"' \
        && ! printf '%s' "$DEPLOY_CODE" | grep -qF 'curl -fsS' \
        && ! printf '%s' "$DEPLOY_BODY" | grep -q 'curl [^|]*"\$ENDPOINT"'; then
        echo "EDGE HEALTH wiring: laya-deploy GETs \$HEALTH_URL through the retrying helper, checks its serverId and never GETs \$ENDPOINT"
    else
        fail "EDGE HEALTH wiring: laya-deploy does not call the helpers, still runs a bare curl on \$HEALTH_URL, or still GETs \$ENDPOINT"
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
    elif kind == "old-prefix":
        sub(r'^resources = \["arn:aws:s3:::([^/"]+)/decide/[^"]*"\]$', r'resources = ["arn:aws:s3:::\1/decide/aprender-mcp-decide/*"]')
    elif kind == "reverse":
        sub(r'^name = "[^"]*"$', 'name = "aprender-mcp-decide"')  # the retired id
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
    if python3 -c 'import sys, tomllib; c = tomllib.load(open(sys.argv[1], "rb")); s = c["iam"]["statements"]; sys.exit(0 if s == [{"effect": "Allow", "actions": ["s3:GetObject"], "resources": ["arn:aws:s3:::dry-run-bucket/decide/" + sys.argv[2] + "/*"]}] and set(c["iam"]) == {"statements"} and c["server"]["name"] == sys.argv[2] else 1)' "$CFG" "$SRV"; then
        echo "IAM config literal: exactly [Allow s3:GetObject arn:aws:s3:::dry-run-bucket/decide/$SRV/*]"
    else
        fail "IAM config literal: the generated [iam] is not exactly the scoped read"
    fi
    for k in s3-star list-too bucket-wide star-resource other-server old-prefix reverse two-statements bucket-sugar no-iam; do iam_case refuse "$k"; done
    cp "$CASES/iam-s3-star.toml" "$CFG"
    expect_refused 6 iam-broadened 'REFUSED iam:' deploy_tiny
    regen 7
    # GRANT: the read-only role check, over policy documents shaped like `aws iam get-role-policy`.
    GB="dry-run-bucket"
    SCOPED="{\"Effect\":\"Allow\",\"Action\":\"s3:GetObject\",\"Resource\":\"arn:aws:s3:::dry-run-bucket/decide/$SRV/*\"}"
    SCOPED_RETIRED="{\"Effect\":\"Allow\",\"Action\":\"s3:GetObject\",\"Resource\":\"arn:aws:s3:::dry-run-bucket/decide/$RETIRED_SERVER/*\"}"
    XRAY='{"Effect":"Allow","Action":["xray:PutTraceSegments","xray:PutTelemetryRecords"],"Resource":"*"}'
    grant_case() {
        local want="$1" name="$2" docs="$3" srv="${4:-$SRV}" rc
        set +e; just _laya-grant-check "$docs" "$GB" "$srv" aprender-decide-weights-dev > "$CASES/grant-$name.log" 2>&1; rc=$?; set -e
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
    # D-19 prefix collision, both ways: a role holding the retired server's prefix grant does not satisfy the new
    # server's check, and the new prefix does not satisfy the retired server's.
    grant_case refuse old-prefix-for-new "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$XRAY,$SCOPED_RETIRED]}}]"
    grant_case refuse new-prefix-for-old "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$XRAY,$SCOPED]}}]" "$RETIRED_SERVER"
    grant_case refuse both-prefixes "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED,$SCOPED_RETIRED]}}]"
    grant_case refuse get-star "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED,{\"Effect\":\"Allow\",\"Action\":\"s3:Get*\",\"Resource\":\"arn:aws:s3:::dry-run-bucket/decide/$SRV/*\"}]}}]"
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
        just laya-deploy-config "$P_APR" off "$SRV" > "$CASES/positive-config.log" 2>&1 || fail "positive: laya-deploy-config"
        set +e
        just laya-deploy "$P_APR" "$P_RUN" "$P_DATA" "$P_BASE" "$SRV" > "$CASES/positive.log" 2>&1
        rc=$?
        set -e
        # The exact server: the retired id's prefix can no longer satisfy this match.
        if [ "$rc" -eq 0 ] && grep -q "^DRY-RUN OK $SRV " "$CASES/positive.log"; then
            grep "^DRY-RUN OK $SRV " "$CASES/positive.log"
        else
            fail "positive dry run: exit $rc without DRY-RUN OK (log: $CASES/positive.log)"
        fi
    elif [ -n "$POS_SKIPPED" ]; then
        echo "SKIP positive dry run: $POS_SKIPPED"
    fi
    # The region parameter (plan 13-01, D-20): its own offline recipe, required to end REGION SELFTEST OK.
    set +e
    just _laya-region-selftest > "$CASES/region.log" 2>&1
    rrc=$?
    set -e
    if [ "$rrc" -eq 0 ] && grep -qx 'REGION SELFTEST OK' "$CASES/region.log" && grep -qx 'AWS CALLS: 0' "$CASES/region.log"; then
        echo "REGION: $(grep -c '^CASE ' "$CASES/region.log") cases, AWS CALLS: 0, REGION SELFTEST OK"
    else
        fail "the region selftest: exit $rrc without REGION SELFTEST OK (log: $CASES/region.log)"
        tail -4 "$CASES/region.log" >&2 || true
    fi
    # The deployment triple (plan 13-04, D-06): its own offline recipe, required to end TARGET SELFTEST OK.
    set +e
    just _laya-target-selftest > "$CASES/target.log" 2>&1
    trc=$?
    set -e
    if [ "$trc" -eq 0 ] && grep -qx 'TARGET SELFTEST OK' "$CASES/target.log" && grep -qx 'AWS CALLS: 0' "$CASES/target.log"; then
        echo "TARGET: $(grep -c -E '^(ISOLATION|TARGET|ACCOUNT|STATE|ENDPOINT|WIRING|TEARDOWN|VERIFY)' "$CASES/target.log") cases, AWS CALLS: 0, TARGET SELFTEST OK"
    else
        fail "the target selftest: exit $trc without TARGET SELFTEST OK (log: $CASES/target.log)"
        tail -4 "$CASES/target.log" >&2 || true
    fi
    # The D-07 auth tooling (plan 13-06): its own offline recipe, required to end AUTH SELFTEST OK.
    set +e
    just _laya-auth-selftest > "$CASES/auth.log" 2>&1
    arc=$?
    set -e
    if [ "$arc" -eq 0 ] && grep -qx 'AUTH SELFTEST OK' "$CASES/auth.log" && grep -qx 'AWS CALLS: 0' "$CASES/auth.log"; then
        echo "AUTH: $(grep -c -E '^(CHAIN|REFUSAL|PROBE-AUTH|RE-EXEC|TOKEN HOST|WIRING)' "$CASES/auth.log") case lines, AWS CALLS: 0, AUTH SELFTEST OK"
    else
        fail "the auth selftest: exit $arc without AUTH SELFTEST OK (log: $CASES/auth.log)"
        tail -4 "$CASES/auth.log" >&2 || true
    fi
    # Independent checks: the resolver proof on file, and the bootstrap build.
    RP="$(head -n 1 "$PROOF")"
    echo "RESOLVER PROOF: $RP ($(sed -n 's/^cargo_pmcp_version=/cargo-pmcp /p' "$PROOF"), sdk $(sed -n 's/^sdk_commit=//p' "$PROOF" | cut -c 1-12))"
    [ "$RP" = "crates/$SRV-lambda" ] || fail "resolver proof names $RP"
    if just laya-build-bootstrap "$SRV" > "$CASES/bootstrap.log" 2>&1 && grep -q '^BOOTSTRAP aarch64 OK' "$CASES/bootstrap.log"; then
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

# Create (if absent) the decide weights bucket aprender-decide-weights-<account>-<env> in <region>
# (default us-east-1, which takes no LocationConstraint; `region=<r>` accepted) and (re)assert its
# posture on every run: all public access blocked, SSE-S3 default encryption, tags project=aprender
# component=decide-weights env=<env>. No lifecycle expiry: objects are content-addressed
# (decide/<server>/<sha256>.apr) and immutable; a new model is a new key. Parameters are read
# positionally (plan 13-01) and the region must match ^[a-z]{2}(-[a-z]+)+-[0-9]$ before the first
# aws call (`REFUSED region:`, exit 2, the value never echoed). Row weights-bucket-region.
[positional-arguments]
laya-weights-bucket env="dev" profile="ze-kasher-dev" region="us-east-1":
    #!/usr/bin/env bash
    set -euo pipefail
    ENV="${1#env=}"
    P="$2"
    R="${3#region=}"
    REGION_ERE='^[a-z]{2}(-[a-z]+)+-[0-9]$'
    if ! (LC_ALL=C; [[ "$R" =~ $REGION_ERE ]]); then
        echo "REFUSED region: the region argument (${#R} characters) is not an AWS region name" >&2
        exit 2
    fi
    case "$ENV" in
        dev|prod) ;;
        *) echo "ERROR: '$ENV' is not a known environment (expected dev or prod)" >&2; exit 2 ;;
    esac
    # The deployment triple (plan 13-04, D-06): the profile names one pmcp.run target, account and region,
    # checked before the first AWS call; the checked values are exported and PMCP_API_URL / AWS_DEFAULT_REGION
    # unset, so every aws call below resolves only this target.
    TLINE="$(just _laya-target-check "$P" "$R")"
    read -r T_TARGET T_REGION _ _ <<< "$TLINE"
    just _laya-account-check "$P" "$R"
    export PMCP_TARGET="$T_TARGET" AWS_PROFILE="$P" AWS_REGION="$T_REGION"
    unset PMCP_API_URL AWS_DEFAULT_REGION
    ACCOUNT="$(aws sts get-caller-identity --profile "$P" --query Account --output text)"
    BUCKET="aprender-decide-weights-${ACCOUNT}-${ENV}"
    if aws s3api head-bucket --profile "$P" --bucket "$BUCKET" 2>/dev/null; then
        echo "  exists:  $BUCKET"
    else
        # us-east-1 takes no LocationConstraint; every other region must name it.
        if [ "$R" = "us-east-1" ]; then
            aws s3api create-bucket --profile "$P" --region "$R" --bucket "$BUCKET" > /dev/null
        else
            aws s3api create-bucket --profile "$P" --region "$R" --bucket "$BUCKET" \
                --create-bucket-configuration "LocationConstraint=$R" > /dev/null
        fi
        echo "  created: $BUCKET"
    fi
    aws s3api put-public-access-block --profile "$P" --bucket "$BUCKET" \
        --public-access-block-configuration BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true
    aws s3api put-bucket-encryption --profile "$P" --bucket "$BUCKET" \
        --server-side-encryption-configuration '{"Rules":[{"ApplyServerSideEncryptionByDefault":{"SSEAlgorithm":"AES256"}}]}'
    aws s3api put-bucket-tagging --profile "$P" --bucket "$BUCKET" \
        --tagging "TagSet=[{Key=project,Value=aprender},{Key=component,Value=decide-weights},{Key=env,Value=$ENV}]"
    echo "  private, SSE-S3, tagged: s3://$BUCKET (region $R)"

# Upload <apr> content-addressed to s3://aprender-decide-weights-<account>-<env>/decide/<server>/<sha256>.apr.
# Eligibility runs FIRST and must accept the file before any AWS call (sts included): the Graviton pack
# record bound to these bytes (Phase 15 D-08/D-09; a local RescoreDrift is then a logged cross-CPU
# finding), or, for a run with no pack record, `just laya-verify` on the exact file. The key's hash is the
# file's sha256; the upload is checked by head-object size and is idempotent.
laya-upload apr run data base server env="dev" profile="ze-kasher-dev":
    #!/usr/bin/env bash
    set -euo pipefail
    APR="{{apr}}"
    SERVER="{{server}}"
    # REQUIRED server id (D-19): checked before any file or AWS action; the retired id has no thin crate.
    just _laya-server-check "$SERVER" deployable
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
    # The deployment triple (plan 13-04, D-06): the profile names one pmcp.run target, account and region,
    # checked before the first AWS call; the checked values are exported and PMCP_API_URL / AWS_DEFAULT_REGION
    # unset, so every aws call below resolves only this target.
    TLINE="$(just _laya-target-check "$P" "")"
    read -r T_TARGET T_REGION _ _ <<< "$TLINE"
    export PMCP_TARGET="$T_TARGET" AWS_PROFILE="$P" AWS_REGION="$T_REGION"
    unset PMCP_API_URL AWS_DEFAULT_REGION
    SD="$(just _laya-state-dir "$P")"
    H_LOCAL="$(sha256 "$APR")"
    # ELIGIBILITY FIRST (decide-apr-v1 deploy_eligibility; Phase 15 D-08/D-09). A run packed by the training
    # server's Graviton pack task is judged by its recorded verify (logs/pack-outcome.json, bound to THESE bytes
    # by apr_sha256); the local Rust verifier on the exact file still runs, and scripts/laya_deploy/
    # laya_eligibility.py decides what its outcome means. LAYA_PACK_OUTCOME names a record EXPLICITLY (an absent
    # file refuses); otherwise a `<dir>/run` argument is looked up at <dir>/logs/pack-outcome.json, and an absent
    # convention record is no record: the local gate decides, exactly as before.
    mkdir -p "$SD"
    VLOG="$SD/eligibility-upload-$SERVER.log"
    ELIG_PY="scripts/laya_deploy/laya_eligibility.py"
    REC="${LAYA_PACK_OUTCOME:-}"
    if [ -z "$REC" ]; then
        REC="$(python3 -I "$ELIG_PY" record-path --run "{{run}}")" || refuse eligibility "pack record discovery failed for {{run}}"
    fi
    if [ -n "$REC" ]; then
        set +e
        CROUT="$(python3 -I "$ELIG_PY" check-record --record "$REC" --apr-sha256 "$H_LOCAL" 2>&1)"
        crc=$?
        set -e
        if [ "$crc" -ne 0 ]; then
            echo "$CROUT" >&2
            exit "$crc"
        fi
        echo "  record: $CROUT"
    fi
    set +e
    just laya-verify "$APR" "{{run}}" "{{data}}" "{{base}}" > "$VLOG" 2>&1
    vrc=$?
    if [ -n "$REC" ]; then
        JOUT="$(python3 -I "$ELIG_PY" judge --apr-sha256 "$H_LOCAL" --verify-rc "$vrc" --verify-log "$VLOG" --record "$REC" --finding-log "$SD/cross-cpu-finding-$SERVER.log" --server "$SERVER" 2>&1)"
    else
        JOUT="$(python3 -I "$ELIG_PY" judge --apr-sha256 "$H_LOCAL" --verify-rc "$vrc" --verify-log "$VLOG" 2>&1)"
    fi
    jrc=$?
    set -e
    if [ "$jrc" -ne 0 ]; then
        echo "$JOUT" >&2
        exit "$jrc"
    fi
    echo "  eligible: $JOUT"
    H="$H_LOCAL"
    KEY="decide/$SERVER/$H.apr"
    SIZE="$(wc -c < "$APR" | tr -d ' ')"
    # The account, live, only after the verifier accepted the file (a no-op under DRY_RUN=1).
    just _laya-account-check "$P" ""
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
laya-grant server env="dev" profile="ze-kasher-dev":
    #!/usr/bin/env bash
    set -euo pipefail
    SERVER="{{server}}"
    just _laya-server-check "$SERVER" deployable
    ENV="{{ trim_start_match(env, "env=") }}"
    P="{{profile}}"
    case "$ENV" in
        dev|prod) ;;
        *) echo "ERROR: '$ENV' is not a known environment (expected dev or prod)" >&2; exit 2 ;;
    esac
    read_failed() { echo "ERROR: IAM read failed: $1 -- the role's policies are UNKNOWN, not missing; nothing was concluded" >&2; exit 1; }
    # The deployment triple (plan 13-04, D-06): the profile names one pmcp.run target, account and region,
    # checked before the first AWS call; the checked values are exported and PMCP_API_URL / AWS_DEFAULT_REGION
    # unset, so every aws call below resolves only this target.
    TLINE="$(just _laya-target-check "$P" "")"
    read -r T_TARGET T_REGION _ _ <<< "$TLINE"
    just _laya-account-check "$P" ""
    export PMCP_TARGET="$T_TARGET" AWS_PROFILE="$P" AWS_REGION="$T_REGION"
    unset PMCP_API_URL AWS_DEFAULT_REGION
    ROLE_ARN="$(aws lambda get-function --profile "$P" --function-name "$SERVER" \
        --query Configuration.Role --output text 2>/dev/null)" || {
        echo "ERROR: no Lambda named '$SERVER' (or it cannot be read) -- deploy it first: just laya-deploy <apr> <run> <data> <base> $SERVER" >&2
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
# Evidence: <state dir>/deploy-evidence-<server>.json. The env bump is out-of-band drift the next
# `laya-deploy` resets. DRY_RUN=1 sizes both shapes from <apr> offline and prints the plan.
# The deployment triple (plan 13-04, D-06): `_laya-target-check` and `_laya-account-check` run right after
# argument validation; then, live only, STATE is read from `_laya-state-dir <profile>` and its endpoint
# must pass `_laya-endpoint-check` for the profile, all BEFORE the probe build and any AWS call. The plan
# files and the evidence go under the same state dir.
laya-deploy-verify apr server profile="ze-kasher-dev" samples="2":
    #!/usr/bin/env bash
    set -euo pipefail
    APR="{{apr}}"
    SERVER="{{server}}"
    just _laya-server-check "$SERVER" deployable
    P="{{profile}}"
    N="{{samples}}"
    CAP_MS=30000
    [ -f "$APR" ] || { echo "ERROR: $APR does not exist" >&2; exit 2; }
    case "$N" in
        ''|*[!0-9]*) echo "ERROR: samples must be a whole number >= 2 (got '$N')" >&2; exit 2 ;;
    esac
    [ "$N" -ge 2 ] || { echo "ERROR: samples must be >= 2 per shape (accepted_region_cold)" >&2; exit 2; }
    # The deployment triple (plan 13-04, D-06): the profile names one pmcp.run target, account and region,
    # checked before the first AWS call; the checked values are exported and PMCP_API_URL / AWS_DEFAULT_REGION
    # unset, so every aws call below resolves only this target.
    TLINE="$(just _laya-target-check "$P" "")"
    read -r T_TARGET T_REGION _ _ <<< "$TLINE"
    just _laya-account-check "$P" ""
    export PMCP_TARGET="$T_TARGET" AWS_PROFILE="$P" AWS_REGION="$T_REGION"
    unset PMCP_API_URL AWS_DEFAULT_REGION
    SD="$(just _laya-state-dir "$P")"
    STATE="$SD/deploy-$SERVER.state/deployment.toml"
    # Live: the deployment's STATE and its endpoint are checked BEFORE the probe build, the plan sizing
    # and any AWS call: a state file naming another tenant's host (or another server) is refused.
    if [ "${DRY_RUN:-0}" != "1" ]; then
        [ -f "$STATE" ] || { echo "ERROR: $STATE does not exist -- it is written by just laya-deploy <apr> <run> <data> <base> $SERVER <env> $P" >&2; exit 2; }
        ENDPOINT="$(python3 -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["deployment"]["endpoint"])' "$STATE" 2> /dev/null)" \
            || { echo "REFUSED target-endpoint: $STATE has no [deployment] endpoint" >&2; exit 2; }
        GOT="$(just _laya-endpoint-check "$ENDPOINT" "$P")"
        [ "$GOT" = "$SERVER" ] || { echo "REFUSED target-endpoint: $STATE names server $GOT, not $SERVER" >&2; exit 2; }
    fi
    # D-07 (plan 13-06): an OAuth-protected server is probed with a token minted from LAYA_M2M_FILE. A token
    # already in the caller's environment is refused (it would skip the file's checks). With the file, this
    # recipe re-execs ITSELF, every argument kept, under scripts/laya_deploy/m2m_token.py, which execs the child
    # with the token in APRENDER_DECIDE_PROBE_TOKEN and LAYA_M2M_MINTED=1 (so the child does not re-exec again).
    # The token host is the helper's --token-host choice: LAYA_M2M_TOKEN_HOST, else <state dir>/m2m-token-host,
    # else the STATE endpoint's host. The expires_in floor is 1800 s (the cold samples, the identity check and
    # five warm calls); LAYA_M2M_MIN_EXPIRES (300..86400) overrides it for a shorter Cognito validity.
    if [ -n "${APRENDER_DECIDE_PROBE_TOKEN+x}" ] && [ "${LAYA_M2M_MINTED:-}" != "1" ]; then
        echo "REFUSED probe-auth: APRENDER_DECIDE_PROBE_TOKEN is set in the caller's environment; unset it and name the client file in LAYA_M2M_FILE" >&2
        exit 2
    fi
    if [ -n "${LAYA_M2M_FILE:-}" ] && [ "${LAYA_M2M_MINTED:-}" != "1" ]; then
        MINEXP="${LAYA_M2M_MIN_EXPIRES:-1800}"
        case "$MINEXP" in ''|*[!0-9]*) echo "REFUSED probe-auth: config: LAYA_M2M_MIN_EXPIRES is not a whole number of seconds" >&2; exit 2 ;; esac
        if [ "${#MINEXP}" -gt 5 ] || [ "$((10#$MINEXP))" -lt 300 ] || [ "$((10#$MINEXP))" -gt 86400 ]; then
            echo "REFUSED probe-auth: config: LAYA_M2M_MIN_EXPIRES must be in 300..86400" >&2
            exit 2
        fi
        EP="${ENDPOINT:-}"
        if [ -z "$EP" ] && [ -f "$STATE" ]; then
            EP="$(python3 -c 'import sys, tomllib; print(tomllib.load(open(sys.argv[1], "rb"))["deployment"]["endpoint"])' "$STATE" 2> /dev/null || true)"
        fi
        EH="${EP#https://}"
        EH="${EH%%/*}"
        if [ -z "$EH" ] && [ -z "${LAYA_M2M_TOKEN_HOST:-}" ] && [ ! -e "$SD/m2m-token-host" ]; then
            TH="<the endpoint host of $STATE>"
        else
            TH="$(python3 -I scripts/laya_deploy/m2m_token.py --token-host --state-dir "$SD" --endpoint-host "$EH")"
        fi
        if [ "${DRY_RUN:-0}" = "1" ]; then
            echo "DRY-RUN: LAYA_M2M_FILE is set; the live run mints a token for $TH before the first probe"
        else
            exec python3 -I scripts/laya_deploy/m2m_token.py --file "$LAYA_M2M_FILE" --expect-host "$TH" --min-expires "$((10#$MINEXP))" -- just laya-deploy-verify "$APR" "$SERVER" "$P" "$N"
        fi
    fi
    cargo build --release -q -p aprender-mcp-decide-lambda --example probe
    TD="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
    PROBE="$TD/release/examples/probe"
    # Size both shapes from the local artifact under ClassifyLimits::CONTRACTED (no network).
    mkdir -p "$SD"
    for shape in concentrated distributed; do
        "$PROBE" --plan-only --apr "$APR" --maximal "$shape" > "$SD/plan-$shape.json" \
            || { echo "ERROR: cannot build the $shape maximal request for $APR" >&2; exit 1; }
    done
    H="$(python3 -c 'import json, sys; print(json.load(open(sys.argv[1]))["artifact_sha256"])' "$SD/plan-concentrated.json")"
    SHAPES=()
    for _ in $(seq 1 "$N"); do SHAPES+=(concentrated distributed); done
    if [ "${DRY_RUN:-0}" = "1" ]; then
        ENDPOINT="<endpoint from $STATE>"
        echo "DRY-RUN laya-deploy-verify $SERVER: artifact sha256 $H, ${#SHAPES[@]} cold samples, cap $CAP_MS ms each"
        for shape in concentrated distributed; do
            python3 -c 'import json, sys; p = json.load(open(sys.argv[1])); print("  shape %s: %s texts, %s tokens planned, max text %s bytes" % (p["shape"], p["texts"], p["tokens_planned"], p["max_text_bytes"]))' "$SD/plan-$shape.json"
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
        echo "  evidence -> $SD/deploy-evidence-$SERVER.json (per cold sample: shape, tokens_total, elapsed_ms, load_ms, probe_id, cold_evidence cloudwatch, graviton)"
        echo "DRY-RUN: stopped before the network (no AWS call, no HTTP request)"
        exit 0
    fi
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
    python3 - "$EVID" "$SERVER" "$H" "${#SHAPES[@]}" "$SD/deploy-evidence-$SERVER.json" <<'PY'
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

# ---------------------------------------------------------------------------
# D-07 (plan 13-06): the authenticated path to an OAuth-protected decide server. A first publish is
# auth OFF; the owner then turns OAuth on (plan 13-08), and from then on every probe needs a token.
# The token is minted from LAYA_M2M_FILE (a mode-0600 or 0400 file OUTSIDE the checkout, the partner's
# Secrets Manager shape {client_id, client_secret, token_url[, scope]}) by
# scripts/laya_deploy/m2m_token.py, which execs ONE command with the token in
# APRENDER_DECIDE_PROBE_TOKEN (and LAYA_M2M_MINTED=1) only: never on argv, never printed, never in a
# log or an evidence file. The probe reads that variable (examples/probe.rs TOKEN_ENV); its --bearer
# argv form must never be used. Partner-ready acceptance requires `(auth form)` in the helper's line.
# ---------------------------------------------------------------------------

# Run the decide probe against <url> under a token minted from LAYA_M2M_FILE. Target-, account- and
# endpoint-checked first (plan 13-04): the URL must be https://<server><tenant suffix>/mcp of <profile>
# (loopback http://127.0.0.1:<port>/mcp only under DRY_RUN=1, for the offline CHAIN case). Refused
# (`REFUSED probe-auth:`, exit 2, nothing run) when LAYA_M2M_FILE is unset, when
# APRENDER_DECIDE_PROBE_TOKEN is already set, when a probe argument starts with --bearer (the probe
# would quote a `--bearer=<value>` word verbatim in its unknown-argument error) or is a second --url.
# The token host comes from the helper's --token-host mode: LAYA_M2M_TOKEN_HOST, else
# <state dir>/m2m-token-host (the non-secret note plan 13-08 writes when the owner names a Cognito
# host), else the URL's host; a conflict between the first two is refused. A host other than the URL's
# own must be *.pmcp.run or *.amazoncognito.com, which admits any pmcp.run host, the shared-pool
# counter-coach host included: intended, the augury servers share one Cognito pool (RESEARCH A1).
# LAYA_M2M_MIN_EXPIRES (300..86400, default 900) is the expires_in floor; LAYA_M2M_ALLOW_BASIC=1 passes
# --allow-basic (DIAGNOSTIC only, never for acceptance). Row probe-auth.
[positional-arguments]
laya-probe-auth profile url *args:
    #!/usr/bin/env bash
    set -euo pipefail
    P="$1"
    URL="$2"
    shift 2
    refuse() { echo "REFUSED probe-auth: $1" >&2; exit 2; }
    # The caller's environment and argv first: no bearer on a command line, no token from elsewhere.
    [ -n "${LAYA_M2M_FILE:-}" ] || refuse "LAYA_M2M_FILE is not set: name the mode-600 M2M client file (outside the checkout)"
    [ -z "${APRENDER_DECIDE_PROBE_TOKEN+x}" ] \
        || refuse "APRENDER_DECIDE_PROBE_TOKEN is already set in the caller's environment; unset it (the token is minted from LAYA_M2M_FILE)"
    for a in "$@"; do
        case "$a" in
            --bearer*) refuse "a probe argument starts with --bearer; the token comes only from LAYA_M2M_FILE (the argument is not echoed)" ;;
            --url*) refuse "a second --url among the probe arguments; the URL is this recipe's second argument" ;;
        esac
    done
    MIN="${LAYA_M2M_MIN_EXPIRES:-900}"
    case "$MIN" in ''|*[!0-9]*) refuse "config: LAYA_M2M_MIN_EXPIRES is not a whole number of seconds" ;; esac
    if [ "${#MIN}" -gt 5 ] || [ "$((10#$MIN))" -lt 300 ] || [ "$((10#$MIN))" -gt 86400 ]; then
        refuse "config: LAYA_M2M_MIN_EXPIRES must be in 300..86400"
    fi
    # The deployment triple (plan 13-04, D-06), then the endpoint under this profile's tenant.
    TLINE="$(just _laya-target-check "$P" "")"
    read -r T_TARGET T_REGION _ _ <<< "$TLINE"
    just _laya-account-check "$P" ""
    export PMCP_TARGET="$T_TARGET" AWS_PROFILE="$P" AWS_REGION="$T_REGION"
    unset PMCP_API_URL AWS_DEFAULT_REGION
    G="$(just _laya-endpoint-check "$URL" "$P")"
    if [ "$G" != "loopback" ]; then just _laya-server-check "$G" deployable > /dev/null; fi
    UH="${URL#*://}"
    UH="${UH%%/*}"
    UH="${UH%%:*}"
    TH="$(python3 -I scripts/laya_deploy/m2m_token.py --token-host --state-dir "$(just _laya-state-dir "$P")" --endpoint-host "$UH")"
    cargo build --release -q -p aprender-mcp-decide-lambda --example probe
    TD="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')"
    PROBE="$TD/release/examples/probe"
    FLAGS=(--file "$LAYA_M2M_FILE" --expect-host "$TH" --min-expires "$((10#$MIN))")
    if [ "$G" = "loopback" ]; then FLAGS+=(--allow-loopback-http); fi
    if [ "${LAYA_M2M_ALLOW_BASIC:-0}" = "1" ]; then FLAGS+=(--allow-basic); fi
    set +x
    exec python3 -I scripts/laya_deploy/m2m_token.py "${FLAGS[@]}" -- "$PROBE" --url "$URL" "$@"

# Measure where the platform refuses an UNAUTHENTICATED caller (plan 13-06, D-07). <url> must be
# https://<host>/mcp and <wait_s> an integer 0..3600. tools/list is POSTed without a bearer, every
# LAYA_AUTH_POLL_SLEEP seconds (default 15) within <wait_s>, until it answers HTTP 401 (so the poll
# measures when OAuth takes effect); only THEN is one tools/call of the tool classify (one synthetic text)
# sent, never while tools/list is open, so an open server is never asked for a cold classify. Both must
# be 401. initialize (200 on augury, RESEARCH P6) and GET /.well-known/oauth-protected-resource are
# recorded as OBSERVATIONS, never pass criteria. No bearer is ever sent. Prints `auth-refusal ok:
# tools/list 401, tools/call 401 after <k> poll(s) (<s> s); first_401_utc <UTC>; initialize <code>
# (observed, not asserted); oauth-protected-resource <code> (observed)`, whose first_401_utc plan 13-08
# records as the auth window. `REFUSED auth-open:` (exit 1) when tools/list stays open past the wait or
# tools/call is not 401, `REFUSED auth-check: transport` (exit 1) on a curl failure, `REFUSED
# auth-check: config` (exit 2) on a malformed argument. An optional third argument <tool> (classify, the default, or train_status)
# selects the tools/call payload (plan 14-10: `just laya-trainer-grant` uses train_status). Row auth-refusal-check.
[positional-arguments]
_laya-auth-refusal-check url wait_s tool="classify":
    #!/usr/bin/env bash
    set -euo pipefail
    URL="$1"
    WAIT="$2"
    TOOL="$3"
    config() { echo "REFUSED auth-check: config $1" >&2; exit 2; }
    python3 -I -c 'import re, sys; sys.exit(0 if re.fullmatch(r"https://[a-z0-9-]+(\.[a-z0-9-]+)+/mcp", sys.argv[1]) else 1)' "$URL" \
        || config "the URL (${#URL} characters) is not https://<host>/mcp"
    case "$WAIT" in ''|*[!0-9]*) config "wait_s is not a whole number of seconds" ;; esac
    [ "${#WAIT}" -le 4 ] && [ "$((10#$WAIT))" -le 3600 ] || config "wait_s is over 3600"
    WAIT=$((10#$WAIT))
    SLEEP="${LAYA_AUTH_POLL_SLEEP:-15}"
    case "$SLEEP" in ''|*[!0-9]*) config "LAYA_AUTH_POLL_SLEEP is not a whole number of seconds" ;; esac
    [ "${#SLEEP}" -le 4 ] && [ "$((10#$SLEEP))" -le 3600 ] || config "LAYA_AUTH_POLL_SLEEP is over 3600"
    SLEEP=$((10#$SLEEP))
    HOST="${URL#https://}"
    HOST="${HOST%/mcp}"
    TMP="$(mktemp)"
    trap 'rm -f "$TMP"' EXIT
    post() {
        curl -s -m 30 -o "$TMP" -w '%{http_code}' -X POST "$URL" -H 'content-type: application/json' \
            -H 'accept: application/json, text/event-stream' -d "$1"
    }
    LIST='{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}'
    # The CALL payload per tool. classify (the default, every decide caller) is one synthetic text; train_status (the Laya training
    # server, plan 14-10) reads one task id that cannot exist, a call that could never start a job even on an open server.
    case "$TOOL" in
        classify) CALL='{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"classify","arguments":{"texts":["auth refusal probe"]}}}' ;;
        train_status) CALL='{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"train_status","arguments":{"task_id":"auth-refusal-probe"}}}' ;;
        *) config "the tool (${#TOOL} characters) is not classify or train_status" ;;
    esac
    INIT='{"jsonrpc":"2.0","id":3,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"laya-auth-refusal-check","version":"1"}}}'
    START=$SECONDS
    k=0
    while :; do
        k=$((k + 1))
        set +e
        code="$(post "$LIST")"
        crc=$?
        set -e
        elapsed=$((SECONDS - START))
        if [ "$crc" -ne 0 ]; then
            echo "REFUSED auth-check: transport (curl exit $crc) on tools/list, poll $k" >&2
            exit 1
        fi
        if [ "$code" = "401" ]; then
            FIRST="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
            break
        fi
        if [ "$elapsed" -ge "$WAIT" ] || [ $((elapsed + SLEEP)) -gt "$WAIT" ] || [ "$k" -ge 241 ]; then
            echo "REFUSED auth-open: unauthenticated tools/list answered HTTP $code after $k poll(s) ($elapsed s, wait $WAIT s); tools/call was not sent" >&2
            exit 1
        fi
        echo "  tools/list answered HTTP $code without a bearer (poll $k, $elapsed s); polling again in $SLEEP s" >&2
        sleep "$SLEEP"
    done
    # Only now, with tools/list refused, one tools/call: it must be refused too.
    set +e
    ccode="$(post "$CALL")"
    crc=$?
    set -e
    if [ "$crc" -ne 0 ]; then
        echo "REFUSED auth-check: transport (curl exit $crc) on tools/call" >&2
        exit 1
    fi
    if [ "$ccode" != "401" ]; then
        echo "REFUSED auth-open: tools/call answered HTTP $ccode without a bearer although tools/list answered 401" >&2
        exit 1
    fi
    # Observations, never pass criteria.
    set +e
    icode="$(post "$INIT")" || icode="transport"
    pcode="$(curl -s -m 30 -o /dev/null -w '%{http_code}' "https://$HOST/.well-known/oauth-protected-resource")" || pcode="transport"
    set -e
    echo "auth-refusal ok: tools/list 401, tools/call 401 after $k poll(s) ($elapsed s); first_401_utc $FIRST; initialize $icode (observed, not asserted); oauth-protected-resource $pcode (observed)"

# The scripted curl of the auth refusal cases (plan 13-06), written to <out>; used by `_laya-auth-selftest` and the
# gate row auth-refusal-check, never by a live recipe. It logs "<METHOD> <json-rpc method>" (or "GET <url>") to
# $CURL_CALLS and answers a status code: the nth tools/list gets line n of $CURL_LIST (the last line repeats),
# tools/call $CURL_CALL, initialize $CURL_INIT, a GET $CURL_PRM; CURL_MODE=transport is a curl transport error.
[positional-arguments]
_laya-auth-curl-shim out:
    #!/usr/bin/env bash
    set -euo pipefail
    cat > "$1" <<'SH'
    #!/usr/bin/env bash
    out=""; data=""; method=GET; url=""
    while [ $# -gt 0 ]; do
        case "$1" in
            -o) out="$2"; shift 2 ;;
            -d) data="$2"; shift 2 ;;
            -X) method="$2"; shift 2 ;;
            -w|-m|-H) shift 2 ;;
            -*) shift ;;
            *) url="$1"; shift ;;
        esac
    done
    if [ "${CURL_MODE:-}" = "transport" ]; then echo "$method transport-error" >> "$CURL_CALLS"; exit 7; fi
    if [ -n "${CURL_BODIES:-}" ] && [ -n "$data" ]; then printf '%s\n' "$data" >> "$CURL_BODIES"; fi
    rpc="$(printf '%s' "$data" | sed -n 's/.*"method":"\([a-z/]*\)".*/\1/p')"
    case "$rpc" in
        tools/list)
            n=$(( $(grep -c '^POST tools/list$' "$CURL_CALLS") + 1 ))
            code="$(sed -n "${n}p" "$CURL_LIST")"
            [ -n "$code" ] || code="$(tail -n 1 "$CURL_LIST")" ;;
        tools/call) code="$CURL_CALL" ;;
        initialize) code="$CURL_INIT" ;;
        *) code="$CURL_PRM" ;;
    esac
    if [ -n "$rpc" ]; then echo "$method $rpc" >> "$CURL_CALLS"; else echo "$method $url" >> "$CURL_CALLS"; fi
    if [ -n "$out" ]; then printf '{"error":"invalid_request","error_description":"No Bearer token provided"}' > "$out"; fi
    # A GET of an edge /health (plan 14-10, the grant recipe's binding check): $CURL_HEALTH is the body, $CURL_HEALTH_CODE the status.
    case "$url" in
        */health)
            if [ -n "${CURL_HEALTH:-}" ]; then
                if [ -n "$out" ]; then printf '%s' "$CURL_HEALTH" > "$out"; fi
                code="${CURL_HEALTH_CODE:-200}"
            fi ;;
    esac
    printf '%s' "$code"
    SH
    chmod +x "$1"

# Prove the D-07 auth tooling OFFLINE (plan 13-06). DRY_RUN=1 and the ambient target / M2M variables
# unset at setup; an aws recorder first on PATH with a positive control (AWS CALLS: 0); the fixture pmcp
# config as LAYA_PMCP_CONFIG; the tiny fixture packed to its golden sha256. CHAIN: the helper's
# --stub-server on 127.0.0.1 (a token endpoint and an MCP endpoint that logs whether the bearer matches),
# a mode-600 client file, then `laya-probe-auth augury-dev http://127.0.0.1:<port>/mcp --apr <tiny>`:
# the stub's first /mcp line must be authorization_bearer_matches=true and the recipe's output must hold
# neither the issued token nor the client secret (LAYA_AUTH_SELFTEST_STUB_MISMATCH=1 starts the stub with
# --stub-mismatch, so this case FAILS: the must-fail of row auth-selftest). Then the tables, each line
# prefixed by its name: REFUSAL accept/refuse (`_laya-auth-refusal-check` against a scripted curl, with the
# tools/call count while tools/list is open), PROBE-AUTH refuse (laya-probe-auth refuses before any probe),
# RE-EXEC (DRY_RUN mints nothing; a pre-set token is refused), RE-EXEC LIVE (DRY_RUN unset for those runs
# only: a python3 shim that intercepts only the minting helper and a fake aws that answers sts and stops at
# the first lambda call EXECUTE both re-exec branches: one helper invocation, every argument kept, the
# child's AWS calls carry the fixture token), TOKEN HOST (the precedence at the helper's argv) and WIRING
# (the guarded re-exec; no probe line carries the argv bearer). D-06: every file a case can
# write under BOTH state dirs (models/decide and models/decide/augury-dev) is backed up at setup and
# restored with `cp -p` on EXIT (removed when it was absent), and models/decide/augury-dev is removed
# only when this run created it and it is empty, so `_laya-state-snapshot` is unchanged around it. Ends
# `AUTH SELFTEST OK`, or `AUTH SELFTEST FAILED (<k> failed checks)` and exit 1. Row auth-selftest.
_laya-auth-selftest:
    #!/usr/bin/env bash
    set -euo pipefail
    export DRY_RUN=1
    unset PMCP_TARGET PMCP_API_URL AWS_PROFILE AWS_REGION AWS_DEFAULT_REGION LAYA_ACCOUNT_CHECK_FORCE LAYA_M2M_TOKEN_HOST LAYA_M2M_MIN_EXPIRES LAYA_M2M_ALLOW_BASIC
    unset LAYA_M2M_FILE LAYA_M2M_MINTED APRENDER_DECIDE_PROBE_TOKEN
    ST="models/decide/selftest"
    SRV="${LAYA_SELFTEST_SERVER:-aprender-mcp-decide-antisemite}"
    T="crates/aprender-decide/tests/fixtures/laya_tiny"
    GOLDEN="37d65159b2be0fa091aa840cd56c1a84b73c0bcd9e2df5906d1f8218f5448561"
    HELPER="scripts/laya_deploy/m2m_token.py"
    FAILS=0
    fail() { echo "FAIL $1" >&2; FAILS=$((FAILS + 1)); }
    mkdir -p "$ST"
    W="$(cd "$(mktemp -d "$ST/auth.XXXXXX")" && pwd -P)"
    ZK_DIR="$(just _laya-state-dir ze-kasher-dev)"
    AUG_DIR="$(just _laya-state-dir augury-dev)"
    AUG_CREATED=0
    [ -d "$AUG_DIR" ] || AUG_CREATED=1
    AUG_STATE_CREATED=0
    [ -d "$AUG_DIR/deploy-$SRV.state" ] || AUG_STATE_CREATED=1
    # Every file a case can write, under both state dirs (D-06; the list shape of laya-gates-selftest).
    KEEP=()
    for d in "$ZK_DIR" "$AUG_DIR"; do
        KEEP+=("$d/plan-concentrated.json" "$d/plan-distributed.json" "$d/m2m-token-host" "$d/deploy-config-$SRV.toml"
            "$d/eligibility-$SRV.log" "$d/eligibility-upload-$SRV.log" "$d/deploy-$SRV.state/deployment.toml")
    done
    k=0
    for f in "${KEEP[@]}"; do
        k=$((k + 1))
        if [ -f "$f" ]; then cp -p "$f" "$W/keep-$k"; fi
    done
    STUB_PID=""
    cleanup() {
        local f n=0
        if [ -n "$STUB_PID" ]; then kill "$STUB_PID" 2> /dev/null || true; wait "$STUB_PID" 2> /dev/null || true; fi
        for f in "${KEEP[@]}"; do
            n=$((n + 1))
            if [ -f "$W/keep-$n" ]; then mkdir -p "$(dirname "$f")"; cp -p "$W/keep-$n" "$f"; else rm -f "$f"; fi
        done
        if [ "$AUG_STATE_CREATED" = "1" ]; then rmdir "$AUG_DIR/deploy-$SRV.state" 2> /dev/null || true; fi
        if [ "$AUG_CREATED" = "1" ]; then rmdir "$AUG_DIR" 2> /dev/null || true; fi
        rm -rf "$W"
    }
    trap cleanup EXIT
    # No token-host note steers a case unless the case plants it.
    rm -f "$ZK_DIR/m2m-token-host" "$AUG_DIR/m2m-token-host"
    # The aws recorder: first on PATH, appends its argv, never reaches AWS.
    mkdir -p "$W/shim"
    REC="$W/aws-calls.log"
    : > "$REC"
    printf '#!/usr/bin/env bash\nprintf "%%s\\n" "$*" >> "%s"\necho "aws recorder: the auth selftest never reaches AWS" >&2\nexit 97\n' "$REC" > "$W/shim/aws"
    chmod +x "$W/shim/aws"
    export PATH="$W/shim:$PATH"
    [ "$(command -v aws)" = "$W/shim/aws" ] || { echo "ERROR: the aws recorder is not first on PATH" >&2; exit 1; }
    set +e; aws recorder-control > /dev/null 2>&1; crc=$?; set -e
    [ "$crc" -eq 97 ] && [ "$(wc -l < "$REC" | tr -d ' ')" -eq 1 ] || { echo "ERROR: the aws recorder did not record its control call (exit $crc)" >&2; exit 1; }
    : > "$REC"
    echo "AWS RECORDER: control call recorded and cleared"
    just _laya-pmcp-fixture-config "$W/pmcp-config.toml"
    export LAYA_PMCP_CONFIG="$W/pmcp-config.toml"
    A="$W/laya_tiny.apr"
    just laya-pack-fixture "$T" "$T/data" "$A" > "$W/fixture.log" 2>&1 || { tail -5 "$W/fixture.log" >&2; exit 1; }
    grep -q "sha256=$GOLDEN" "$W/fixture.log" || { echo "ERROR: laya-pack-fixture did not reproduce the golden $GOLDEN" >&2; exit 1; }
    # ---- CHAIN: file -> helper -> probe environment -> wire, against the loopback stub ----
    SD="$W/stub"
    mkdir -p "$SD"
    python3 -I - "$SD/client.json" <<'PY'
    import json, os, secrets, sys
    fd = os.open(sys.argv[1], os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as fh:
        json.dump({"client_id": "selftest-client-" + secrets.token_hex(4), "client_secret": "selftest-secret-" + secrets.token_urlsafe(18)}, fh)
    PY
    STUB_FLAGS=()
    if [ "${LAYA_AUTH_SELFTEST_STUB_MISMATCH:-0}" = "1" ]; then STUB_FLAGS=(--stub-mismatch); fi
    python3 -I "$HELPER" --stub-server "$SD" ${STUB_FLAGS[@]+"${STUB_FLAGS[@]}"} > "$W/stub.log" 2>&1 &
    STUB_PID=$!
    for _ in $(seq 1 100); do
        [ -s "$SD/port" ] && break
        sleep 0.1
    done
    if [ -s "$SD/port" ]; then
        PORT="$(cat "$SD/port")"
        python3 -I - "$SD/client.json" "$PORT" "$W/m2m-client.json" <<'PY'
    import json, os, sys
    c = json.load(open(sys.argv[1]))
    fd = os.open(sys.argv[3], os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as fh:
        json.dump({**c, "token_url": f"http://127.0.0.1:{int(sys.argv[2])}/oauth2/token"}, fh)
    PY
        set +e
        env LAYA_M2M_FILE="$W/m2m-client.json" just laya-probe-auth augury-dev "http://127.0.0.1:$PORT/mcp" --apr "$A" > "$W/chain.log" 2>&1
        prc=$?
        set -e
        touch "$SD/stop"
        wait "$STUB_PID" || true
        STUB_PID=""
        TOK="$(cat "$SD/token" 2> /dev/null || true)"
        SEC="$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))["client_secret"])' "$SD/client.json")"
        LINE="$(head -n 1 "$SD/mcp.log" 2> /dev/null || true)"
        LEAK=""
        if [ -z "$TOK" ] || grep -qF -- "$TOK" "$W/chain.log"; then LEAK="$LEAK token"; fi
        if grep -qF -- "$SEC" "$W/chain.log"; then LEAK="$LEAK secret"; fi
        if [ "$LINE" = "authorization_bearer_matches=true" ] && [ -z "$LEAK" ] && grep -q '^m2m-token ok: host 127.0.0.1 HTTP 200 ' "$W/chain.log"; then
            grep -m 1 '^m2m-token ok:' "$W/chain.log"
            echo "CHAIN bearer reached the wire ($LINE); token and secret absent from the recipe output"
        else
            fail "CHAIN: stub line '${LINE:-<none>}', leaked:${LEAK:- none}, recipe exit $prc (log $W/chain.log)"
            grep -v -F -e "${TOK:-<no token>}" -e "$SEC" "$W/chain.log" | tail -5 >&2 || true
        fi
    else
        fail "CHAIN: the stub did not write its port within 10 s (log $W/stub.log)"
    fi
    # A fixture client file for the cases below (never read by a real token endpoint: no case reaches one).
    CLIENT="$W/m2m-fixture.json"
    python3 -I - "$CLIENT" "$SRV" <<'PY'
    import json, os, secrets, sys
    fd = os.open(sys.argv[1], os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as fh:
        json.dump({"client_id": "fixture-client-" + secrets.token_hex(4), "client_secret": "fixture-secret-" + secrets.token_urlsafe(18),
                   "token_url": f"https://{sys.argv[2]}.eu-west.augury.pmcp.run/oauth2/token"}, fh)
    PY
    EPH="$SRV.eu-west.augury.pmcp.run"
    EPU="https://$EPH/mcp"
    # ---- REFUSAL: _laya-auth-refusal-check against a scripted curl (first on PATH for this table only) ----
    CS="$W/curlshim"
    mkdir -p "$CS"
    just _laya-auth-curl-shim "$CS/curl"
    RN=0
    rcase() {
        local kind="$1" name="$2" want="$3" pat="$4" list="$5" call="$6" url="$7" wait="$8" mode="${9:-}" log rc
        RN=$((RN + 1))
        log="$W/refusal-$RN.log"
        LAST_LOG="$log"
        printf '%s\n' $list > "$W/curl-list"
        : > "$W/curl-calls"
        set +e
        env PATH="$CS:$PATH" CURL_LIST="$W/curl-list" CURL_CALLS="$W/curl-calls" CURL_CALL="$call" CURL_INIT=200 CURL_PRM=200 \
            CURL_MODE="$mode" LAYA_AUTH_POLL_SLEEP=0 just _laya-auth-refusal-check "$url" "$wait" > "$log" 2>&1
        rc=$?
        set -e
        if [ "$rc" -eq "$want" ] && grep -Eq -- "$pat" "$log"; then
            echo "REFUSAL $kind: $name -> exit $rc, $(grep -Eo -m 1 -- "$pat.*" "$log" | cut -c 1-230)"
            return 0
        fi
        fail "REFUSAL $kind: $name: exit $rc (want $want), expected a line matching '$pat' (log $log)"
        tail -3 "$log" >&2 || true
        return 1
    }
    if rcase accept "tools/list 401, tools/call 401" 0 '^auth-refusal ok: tools/list 401, tools/call 401 after 1 poll\(s\)' "401" 401 "$EPU" 0; then
        grep -Eq 'first_401_utc [0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z; initialize 200 \(observed, not asserted\); oauth-protected-resource 200 \(observed\)$' "$LAST_LOG" \
            || fail "REFUSAL accept: the ok line lacks first_401_utc or the two observed codes"
        [ "$(grep -c '^POST initialize$' "$W/curl-calls" || true)" -eq 1 ] && [ "$(grep -c '^GET ' "$W/curl-calls" || true)" -eq 1 ] \
            || fail "REFUSAL accept: initialize and the oauth-protected-resource GET were not each sent once"
    fi
    if rcase accept "tools/list 200 then 401 within wait 30" 0 '^auth-refusal ok: tools/list 401, tools/call 401 after 2 poll\(s\)' "200 401" 401 "$EPU" 30; then
        # The order of the JSON-RPC requests, not only the verdict: no tools/call while tools/list was open.
        SEQ="$(grep -E '^POST tools/(list|call)$' "$W/curl-calls" | tr '\n' ',' || true)"
        if [ "$SEQ" = "POST tools/list,POST tools/list,POST tools/call," ]; then
            echo "REFUSAL accept: call count after polling -> tools/list, tools/list, then 1 tools/call (none while tools/list was open)"
        else
            fail "REFUSAL accept: call count after polling: the requests were '${SEQ:-none}', expected tools/list, tools/list, tools/call"
        fi
    fi
    if rcase refuse "tools/list 200 with wait 0" 1 '^REFUSED auth-open: unauthenticated tools/list answered HTTP 200' "200" 401 "$EPU" 0; then
        NC="$(grep -c '^POST tools/call$' "$W/curl-calls" || true)"
        if [ "$NC" -eq 0 ]; then
            echo "REFUSAL refuse: call count while tools/list is open -> 0 tools/call requests sent"
        else
            fail "REFUSAL refuse: call count: $NC tools/call request(s) sent while tools/list answered 200"
        fi
    fi
    rcase refuse "tools/list 401, tools/call 200" 1 '^REFUSED auth-open: tools/call' "401" 200 "$EPU" 0 || true
    rcase refuse "curl transport failure" 1 '^REFUSED auth-check: transport' "401" 401 "$EPU" 0 transport || true
    rcase refuse "an http URL" 2 '^REFUSED auth-check: config' "401" 401 "http://$EPH/mcp" 0 || true
    rcase refuse "a URL not ending /mcp" 2 '^REFUSED auth-check: config' "401" 401 "https://$EPH/health" 0 || true
    rcase refuse "a non-integer wait" 2 '^REFUSED auth-check: config' "401" 401 "$EPU" soon || true
    rcase refuse "a wait over 3600" 2 '^REFUSED auth-check: config' "401" 401 "$EPU" 3601 || true
    # ---- PROBE-AUTH: laya-probe-auth refuses before any probe runs ----
    PN=0
    pa_case() {
        local name="$1" pat="$2" log rc
        shift 2
        PN=$((PN + 1))
        log="$W/probe-auth-$PN.log"
        set +e
        "$@" > "$log" 2>&1
        rc=$?
        set -e
        if [ "$rc" -eq 2 ] && grep -Eq -- "$pat" "$log" && ! grep -Eq '^probe: |^\{' "$log"; then
            echo "PROBE-AUTH refuse: $name -> exit 2, $(grep -Eo -m 1 -- "$pat.*" "$log" | cut -c 1-170)"
        else
            fail "PROBE-AUTH refuse: $name: exit $rc, expected exit 2 with '$pat' and no probe output (log $log)"
            tail -3 "$log" >&2 || true
        fi
    }
    BFIX="fixture-bearer-$(python3 -I -c 'import secrets; print(secrets.token_hex(12))')"
    pa_case "LAYA_M2M_FILE unset" '^REFUSED probe-auth: LAYA_M2M_FILE is not set' env -u LAYA_M2M_FILE just laya-probe-auth augury-dev "$EPU" --apr "$A"
    pa_case "APRENDER_DECIDE_PROBE_TOKEN pre-set" '^REFUSED probe-auth: APRENDER_DECIDE_PROBE_TOKEN is already set' \
        env LAYA_M2M_FILE="$CLIENT" APRENDER_DECIDE_PROBE_TOKEN="$BFIX" just laya-probe-auth augury-dev "$EPU" --apr "$A"
    pa_case "the argv bearer flag" '^REFUSED probe-auth: a probe argument starts with --bearer' \
        env LAYA_M2M_FILE="$CLIENT" just laya-probe-auth augury-dev "$EPU" --apr "$A" --bearer "$BFIX"
    pa_case "a --bearer=<value> word" '^REFUSED probe-auth: a probe argument starts with --bearer' \
        env LAYA_M2M_FILE="$CLIENT" just laya-probe-auth augury-dev "$EPU" --apr "$A" "--bearer=$BFIX"
    if grep -qF -- "$BFIX" "$W"/probe-auth-*.log; then fail "PROBE-AUTH refuse: a refused bearer value appears in a recipe log"; fi
    pa_case "a second --url" '^REFUSED probe-auth: a second --url' env LAYA_M2M_FILE="$CLIENT" just laya-probe-auth augury-dev "$EPU" --apr "$A" --url "$EPU"
    pa_case "the us-east motivation URL with profile augury-dev" '^REFUSED target-endpoint: ' \
        env LAYA_M2M_FILE="$CLIENT" just laya-probe-auth augury-dev https://aprender-mcp-decide-motivation.us-east.true-mcp.com/mcp --apr "$A"
    cp "$CLIENT" "$W/m2m-644.json"
    chmod 644 "$W/m2m-644.json"
    pa_case "a mode-644 client file" '^REFUSED m2m-file-mode: ' env LAYA_M2M_FILE="$W/m2m-644.json" just laya-probe-auth augury-dev "http://127.0.0.1:9/mcp" --apr "$A"
    pa_case "LAYA_M2M_MIN_EXPIRES out of range" '^REFUSED probe-auth: config' env LAYA_M2M_FILE="$CLIENT" LAYA_M2M_MIN_EXPIRES=60 just laya-probe-auth augury-dev "$EPU" --apr "$A"
    # ---- the fixture deployment on augury-dev for SRV (restored on EXIT): a STATE and a DRY_RUN-rendered config ----
    mkdir -p "$AUG_DIR/deploy-$SRV.state"
    printf '[deployment]\nendpoint = "%s"\n' "$EPU" > "$AUG_DIR/deploy-$SRV.state/deployment.toml"
    just laya-deploy-config "$A" off "$SRV" dev augury-dev eu-west-1 > "$W/augury-config.log" 2>&1 || { fail "could not render the augury-dev fixture config (log $W/augury-config.log)"; }
    # ---- RE-EXEC (dry): DRY_RUN prints what the live run would mint and mints nothing ----
    XN=0
    rx_dry() {
        local name="$1" pat="$2" want="$3" log rc
        shift 3
        XN=$((XN + 1))
        log="$W/reexec-dry-$XN.log"
        set +e
        "$@" > "$log" 2>&1
        rc=$?
        set -e
        if { [ "$want" = any ] || [ "$rc" -eq "$want" ]; } && grep -Eq -- "$pat" "$log" && ! grep -q 'm2m-token ok\|REFUSED m2m-' "$log"; then
            echo "RE-EXEC $name -> exit $rc, $(grep -Eo -m 1 -- "$pat.*" "$log" | cut -c 1-170)"
        else
            fail "RE-EXEC $name: exit $rc, expected '$pat' and nothing minted (log $log)"
            tail -3 "$log" >&2 || true
        fi
    }
    rx_dry "laya-deploy-verify DRY_RUN" "^DRY-RUN: LAYA_M2M_FILE is set; the live run mints a token for $EPH before the first probe\$" 0 \
        env LAYA_M2M_FILE="$W/absent-client.json" just laya-deploy-verify "$A" "$SRV" augury-dev 2
    rx_dry "laya-deploy DRY_RUN" "^DRY-RUN: LAYA_M2M_FILE is set; the live run mints a token for $EPH before the first probe\$" any \
        env LAYA_M2M_FILE="$W/absent-client.json" just laya-deploy "$A" "$T" "$T/data" "$T/checkpoint" "$SRV" dev augury-dev
    rx_dry "laya-deploy-verify with a pre-set token" '^REFUSED probe-auth: APRENDER_DECIDE_PROBE_TOKEN is set' 2 \
        env APRENDER_DECIDE_PROBE_TOKEN="$BFIX" just laya-deploy-verify "$A" "$SRV" augury-dev 2
    rx_dry "laya-deploy with a pre-set token" '^REFUSED probe-auth: APRENDER_DECIDE_PROBE_TOKEN is set' 2 \
        env APRENDER_DECIDE_PROBE_TOKEN="$BFIX" just laya-deploy "$A" "$T" "$T/data" "$T/checkpoint" "$SRV" dev augury-dev
    # ---- RE-EXEC LIVE: both live branches EXECUTED, through a python3 shim (intercepts only the minting helper) and a fake aws ----
    REAL_PY="$(command -v python3)"
    PS="$W/pyshim"
    FA="$W/fakeaws"
    mkdir -p "$PS" "$FA"
    python3 -I -c 'import secrets; print("fixture-token-" + secrets.token_urlsafe(24))' > "$W/shim-token"
    chmod 600 "$W/shim-token"
    SHIM_TOKEN="$(cat "$W/shim-token")"
    SHIM_SHA="$(printf '%s' "$SHIM_TOKEN" | shasum -a 256 | awk '{print $1}')"
    cat > "$PS/python3" <<'SH'
    #!/usr/bin/env bash
    # Intercepts ONLY `python3 -I scripts/laya_deploy/m2m_token.py --file ...`: records that argv (it holds no secret),
    # then execs the words after `--` with a fixture token, as the helper would. Anything else is the real python3.
    if [ "${1:-}" = "-I" ] && [ "${2:-}" = "scripts/laya_deploy/m2m_token.py" ] && [ "${3:-}" = "--file" ]; then
        shift 2
        if [ -f "$SHIM_HELPER_LOG" ] && [ "$(wc -l < "$SHIM_HELPER_LOG" | tr -d ' ')" -ge 3 ]; then
            echo "python3 shim: a re-exec loop (3 helper invocations already)" >&2
            exit 99
        fi
        printf '%s\n' "$*" >> "$SHIM_HELPER_LOG"
        [ "${SHIM_NO_EXEC:-0}" = "1" ] && exit 0
        while [ $# -gt 0 ] && [ "$1" != "--" ]; do shift; done
        [ $# -gt 0 ] && shift
        APRENDER_DECIDE_PROBE_TOKEN="$(cat "$SHIM_TOKEN_FILE")"
        export APRENDER_DECIDE_PROBE_TOKEN LAYA_M2M_MINTED=1
        exec "$@"
    fi
    exec "$SHIM_REAL_PY" "$@"
    SH
    cat > "$FA/aws" <<'SH'
    #!/usr/bin/env bash
    # Answers sts with the augury-dev account and stops at the first lambda call; logs each argv with the sha256 of the
    # token it sees in APRENDER_DECIDE_PROBE_TOKEN (never the token) and the LAYA_M2M_MINTED it sees.
    t="${APRENDER_DECIDE_PROBE_TOKEN:-}"
    if [ -n "$t" ]; then h="$(printf '%s' "$t" | shasum -a 256 | awk '{print $1}')"; else h=none; fi
    printf '%s token_sha256=%s minted=%s\n' "$*" "$h" "${LAYA_M2M_MINTED:-unset}" >> "$FAKE_AWS_LOG"
    case "$1 $2" in
        "sts get-caller-identity") echo 783960389328 ;;
        "lambda "*) echo "fake aws: stopping at the first lambda call" >&2; exit 254 ;;
        *) echo "fake aws: unexpected call" >&2; exit 97 ;;
    esac
    SH
    chmod +x "$PS/python3" "$FA/aws"
    live() {
        local log="$1"
        shift
        : > "$W/helper-argv.log"
        : > "$W/fake-aws.log"
        set +e
        env -u DRY_RUN -u LAYA_PMCP_CONFIG PATH="$PS:$FA:$PATH" SHIM_REAL_PY="$REAL_PY" SHIM_HELPER_LOG="$W/helper-argv.log" \
            SHIM_TOKEN_FILE="$W/shim-token" FAKE_AWS_LOG="$W/fake-aws.log" LAYA_M2M_FILE="$CLIENT" "$@" > "$log" 2>&1
        LRC=$?
        set -e
    }
    live_check() {
        local name="$1" want_argv="$2" log="$3" probe_line="$4" n
        n="$(wc -l < "$W/helper-argv.log" | tr -d ' ')"
        if [ "$n" -ne 1 ]; then fail "RE-EXEC LIVE $name: $n helper invocation(s), expected exactly 1 (log $log)"; tail -3 "$log" >&2 || true; return; fi
        if [ "$(cat "$W/helper-argv.log")" != "$want_argv" ]; then fail "RE-EXEC LIVE $name: helper argv '$(cat "$W/helper-argv.log")' is not '$want_argv'"; return; fi
        if grep -qF -- "$SHIM_TOKEN" "$log"; then fail "RE-EXEC LIVE $name: the fixture token is in the recipe output"; return; fi
        if ! grep -Eq -- "$probe_line token_sha256=$SHIM_SHA minted=1\$" "$W/fake-aws.log"; then
            fail "RE-EXEC LIVE $name: the child's AWS call did not carry the fixture token and LAYA_M2M_MINTED=1 ($(tail -2 "$W/fake-aws.log" | tr '\n' ';'))"
            return
        fi
        echo "RE-EXEC LIVE $name: 1 helper invocation, argv preserved ($want_argv); the child's $(grep -Eo -m 1 -- "$probe_line" "$W/fake-aws.log" | cut -c 1-40) call carried the fixture token (sha256 ${SHIM_SHA:0:12}) and LAYA_M2M_MINTED=1; token absent from the output"
    }
    live "$W/reexec-live-verify.log" just laya-deploy-verify "$A" "$SRV" augury-dev 2
    live_check "laya-deploy-verify" "--file $CLIENT --expect-host $EPH --min-expires 1800 -- just laya-deploy-verify $A $SRV augury-dev 2" \
        "$W/reexec-live-verify.log" '^lambda get-function-configuration .*'
    live "$W/reexec-live-verify-600.log" env LAYA_M2M_MIN_EXPIRES=600 just laya-deploy-verify "$A" "$SRV" augury-dev 2
    live_check "laya-deploy-verify with LAYA_M2M_MIN_EXPIRES=600" "--file $CLIENT --expect-host $EPH --min-expires 600 -- just laya-deploy-verify $A $SRV augury-dev 2" \
        "$W/reexec-live-verify-600.log" '^lambda get-function-configuration .*'
    live "$W/reexec-live-deploy.log" just laya-deploy "$A" "$T" "$T/data" "$T/checkpoint" "$SRV" dev augury-dev
    live_check "laya-deploy" "--file $CLIENT --expect-host $EPH --min-expires 2400 -- just laya-deploy $A $T $T/data $T/checkpoint $SRV dev augury-dev" \
        "$W/reexec-live-deploy.log" '^sts get-caller-identity .*'
    grep -Eq '^REFUSED eligibility: .*SyntheticNotDeployable' "$W/reexec-live-deploy.log" \
        || fail "RE-EXEC LIVE laya-deploy: the child did not stop at REFUSED eligibility ... SyntheticNotDeployable"
    if grep -v '^sts get-caller-identity ' "$W/fake-aws.log" | grep -q .; then
        fail "RE-EXEC LIVE laya-deploy: a non-sts AWS call ran ($(grep -v '^sts ' "$W/fake-aws.log" | head -1 | cut -c 1-80))"
    else
        echo "RE-EXEC LIVE laya-deploy: the child stopped at $(grep -Eo -m 1 'REFUSED eligibility: .*SyntheticNotDeployable' "$W/reexec-live-deploy.log" | cut -c 1-90) before any non-sts AWS call"
    fi
    # ---- TOKEN HOST: the precedence, observed at laya-probe-auth's helper argv (the shim records it and runs nothing) ----
    COG="augury-fixture.auth.eu-west-1.amazoncognito.com"
    HN=0
    th_case() {
        local name="$1" want="$2" log rc
        shift 2
        HN=$((HN + 1))
        log="$W/token-host-$HN.log"
        : > "$W/helper-argv.log"
        set +e
        env PATH="$PS:$PATH" SHIM_REAL_PY="$REAL_PY" SHIM_HELPER_LOG="$W/helper-argv.log" SHIM_NO_EXEC=1 LAYA_M2M_FILE="$CLIENT" \
            "$@" just laya-probe-auth augury-dev "$EPU" --apr "$A" > "$log" 2>&1
        rc=$?
        set -e
        case "$want" in
            REFUSED*)
                if [ "$rc" -eq 2 ] && grep -qF -- "$want" "$log" && [ ! -s "$W/helper-argv.log" ]; then
                    echo "TOKEN HOST $name -> exit 2, $(grep -F -m 1 -- "$want" "$log" | cut -c 1-170)"
                else
                    fail "TOKEN HOST $name: exit $rc, expected '$want' and no helper run (log $log)"
                fi ;;
            *)
                if [ "$rc" -eq 0 ] && grep -q -- "--expect-host $want " "$W/helper-argv.log"; then
                    echo "TOKEN HOST $name -> the helper gets --expect-host $want"
                else
                    fail "TOKEN HOST $name: exit $rc, the helper argv '$(cat "$W/helper-argv.log")' lacks --expect-host $want (log $log)"
                fi ;;
        esac
    }
    th_case "neither variable nor note" "$EPH" env
    th_case "LAYA_M2M_TOKEN_HOST set" "$COG" env LAYA_M2M_TOKEN_HOST="$COG"
    printf '%s\n' "$COG" > "$AUG_DIR/m2m-token-host"
    th_case "the m2m-token-host note (variable unset)" "$COG" env -u LAYA_M2M_TOKEN_HOST
    th_case "variable and note differ" "REFUSED probe-auth: token-host conflict" env LAYA_M2M_TOKEN_HOST="other-fixture.auth.eu-west-1.amazoncognito.com"
    printf '%s\n' "evil.example.com" > "$AUG_DIR/m2m-token-host"
    th_case "a note outside pmcp.run / amazoncognito.com" "REFUSED probe-auth: the token host evil.example.com" env
    rm -f "$AUG_DIR/m2m-token-host"
    # ---- WIRING: the guarded re-exec in both recipes; no probe line anywhere carries the argv bearer ----
    just --dump --dump-format json > "$W/dump.json"
    set +e
    python3 -I - "$W/dump.json" > "$W/wiring.log" 2>&1 <<'PY'
    import json, re, sys
    recipes = json.load(open(sys.argv[1]))["recipes"]
    render = lambda line: "".join(f if isinstance(f, str) else "<interpolation>" for f in line)
    problems = []
    for n in ("laya-deploy-verify", "laya-deploy"):
        body = [render(l) for l in recipes[n]["body"]]
        ex = [i for i, l in enumerate(body) if re.match(r"\s*exec python3 -I scripts/laya_deploy/m2m_token\.py --file ", l)]
        if len(ex) != 1:
            problems.append(f"WIRING {n}: {len(ex)} exec line(s) of the minting helper, expected 1")
            continue
        e = ex[0]
        g = next((i for i in range(e - 1, -1, -1) if re.match(r"\s*if .*LAYA_M2M_FILE", body[i])), None)
        if g is None or '"${LAYA_M2M_MINTED:-}" != "1"' not in body[g]:
            problems.append(f"WIRING {n}: the helper exec (line {e}) is not under an `if` that requires LAYA_M2M_MINTED != 1 (guard line {g})")
            continue
        r = next((i for i in range(e) if "APRENDER_DECIDE_PROBE_TOKEN+x" in body[i] and '"${LAYA_M2M_MINTED:-}" != "1"' in body[i]), None)
        if r is None:
            problems.append(f"WIRING {n}: no refusal of a pre-set APRENDER_DECIDE_PROBE_TOKEN before the helper exec")
            continue
        print(f"WIRING {n}: pre-set-token refusal at line {r} < LAYA_M2M_MINTED-guarded helper exec at line {e} (guard line {g})")
    def probe_lines(body):
        return [l.strip() for l in body if not l.strip().startswith("#") and ('"$PROBE"' in l or "--example probe" in l)]
    # case table: the pattern must see a planted bearer and must not flag a clean line
    pv = '"$' + 'PROBE"'  # built, so this case table is not itself a probe line of this recipe
    if not any("--bearer" in l for l in probe_lines([f'    {pv} --url "$U" --bearer "$X"'])) or any("--bearer" in l for l in probe_lines([f'    {pv} --url "$U"', f'    # {pv} --bearer x'])):
        problems.append("WIRING probe argv: the case table failed (a planted --bearer is not caught, or a clean/comment line is)")
    count, names = 0, 0
    for n, rec in recipes.items():
        if not n.startswith(("laya", "_laya")):
            continue
        pl = probe_lines([render(l) for l in rec["body"]])
        if pl:
            names += 1
            count += len(pl)
        for l in pl:
            if "--bearer" in l:
                problems.append(f"WIRING probe argv: {n} runs the probe with the argv bearer: {l[:100]}")
    if count == 0:
        problems.append("WIRING probe argv: no probe line found at all (the scan is blind)")
    else:
        print(f"WIRING probe argv: {count} probe line(s) in {names} recipe(s), none carries --bearer; case table: a planted --bearer is caught")
    for p in problems:
        print("FAIL " + p)
    sys.exit(1 if problems else 0)
    PY
    wrc=$?
    set -e
    grep -v '^FAIL ' "$W/wiring.log" || true
    if [ "$wrc" -ne 0 ]; then
        while IFS= read -r l; do fail "${l#FAIL }"; done < <(grep '^FAIL ' "$W/wiring.log" || echo "FAIL WIRING: the check exited $wrc")
    fi
    CALLS="$(wc -l < "$REC" | tr -d ' ')"
    echo "AWS CALLS: $CALLS"
    [ "$CALLS" -eq 0 ] || fail "the aws recorder was called: $(head -3 "$REC" | tr '\n' ';')"
    if [ "$FAILS" -eq 0 ]; then
        echo "AUTH SELFTEST OK"
    else
        echo "AUTH SELFTEST FAILED ($FAILS failed checks)" >&2
        exit 1
    fi

# CONTAINMENT FIRST: throttle <server> to reserved concurrency 0 (every invocation is refused at
# once, warm instances included; undo with `aws lambda delete-function-concurrency`), verify it,
# then delete the inline weights policy. The bucket stays (idle-free). Removing the deployment and
# the object is the human's call: the commands are PRINTED, never run.
laya-teardown server env="dev" profile="ze-kasher-dev":
    #!/usr/bin/env bash
    set -euo pipefail
    SERVER="{{server}}"
    # `named`, not `deployable`: containment must be able to name ANY deployed function, the retired one included.
    just _laya-server-check "$SERVER" named
    ENV="{{ trim_start_match(env, "env=") }}"
    P="{{profile}}"
    case "$ENV" in
        dev|prod) ;;
        *) echo "ERROR: '$ENV' is not a known environment (expected dev or prod)" >&2; exit 2 ;;
    esac
    # The deployment triple (plan 13-04, D-06): containment on the wrong tenant is harmful, so an implicit
    # or contradictory target is REFUSED here too. contain() in laya-deploy calls this with exactly the
    # environment it exported, which passes (TEARDOWN REACHABLE in _laya-target-selftest).
    TLINE="$(just _laya-target-check "$P" "")"
    read -r T_TARGET T_REGION _ _ <<< "$TLINE"
    just _laya-account-check "$P" ""
    export PMCP_TARGET="$T_TARGET" AWS_PROFILE="$P" AWS_REGION="$T_REGION"
    unset PMCP_API_URL AWS_DEFAULT_REGION
    SD="$(just _laya-state-dir "$P")"
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
    echo "  NOT RUN -- the human's call (profile $P: pmcp.run target $T_TARGET, region $T_REGION):"
    echo "    remove the deployment (the decide config must be at the shared root while it runs):"
    echo "      PMCP_TARGET=$T_TARGET AWS_PROFILE=$P AWS_REGION=$T_REGION just _laya-crates-root-swap crates $SD/deploy-config-$SERVER.toml $SD/destroy-$SERVER.state cargo pmcp deploy destroy --manifest-path crates"
    echo "    remove the weights:"
    echo "      aws s3 rm --profile $P --region $T_REGION --recursive s3://aprender-decide-weights-${ACCOUNT}-${ENV}/decide/$SERVER/"
    echo "    or resume serving instead: aws lambda delete-function-concurrency --profile $P --region $T_REGION --function-name $SERVER"

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
    # An ambient target cannot steer an offline case (plan 13-04): every case names its profile.
    unset PMCP_TARGET PMCP_API_URL AWS_PROFILE AWS_REGION AWS_DEFAULT_REGION LAYA_ACCOUNT_CHECK_FORCE
    [ -f "$TSV" ] || { echo "LAYA GATES SELFTEST FAILED: the gate table $TSV does not exist" >&2; exit 1; }
    W="$(mktemp -d)"
    LOGD="$W/logs"
    mkdir -p "$LOGD"
    # ONE variable for the server id, passed explicitly to every call (D-19); LAYA_SELFTEST_SERVER overrides it.
    SRV="${LAYA_SELFTEST_SERVER:-aprender-mcp-decide-antisemite}"
    RETIRED_SERVER="aprender-mcp-decide"   # the retired id: it has no thin crate, so no deploy recipe may accept it
    CFG="models/decide/deploy-config-$SRV.toml"
    PROOF="models/decide/resolver-proof-$SRV.txt"
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
    # The fixture pmcp config (plan 13-04): the target checks read it, never ~/.pmcp/config.toml.
    just _laya-pmcp-fixture-config "$W/pmcp-config.toml"
    export LAYA_PMCP_CONFIG="$W/pmcp-config.toml"
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
    # FAKE_AWS_ACCOUNT (plan 13-04, row account-check) is what sts answers; the default is the fake's own.
    ACCT="${FAKE_AWS_ACCOUNT:-123456789012}"
    B="aprender-decide-weights-$ACCT-dev"
    S="${FAKE_SERVER:?FAKE_SERVER names the server whose role policy this fake returns}"
    case "$1 $2" in
        "lambda get-function") echo "arn:aws:iam::$ACCT:role/decide-fn-role" ;;
        "sts get-caller-identity") echo "$ACCT" ;;
        "iam list-role-policies")
            [ "$mode" = list-fail ] && { echo "An error occurred (AccessDenied) when calling the ListRolePolicies operation" >&2; exit 255; }
            echo '["pmcp-declared"]' ;;
        "iam get-role-policy") echo "{\"Statement\":[{\"Effect\":\"Allow\",\"Action\":\"s3:GetObject\",\"Resource\":\"arn:aws:s3:::$B/decide/$S/*\"}]}" ;;
        "iam list-attached-role-policies")
            [ "$mode" = attached-fail ] && { echo "An error occurred (Throttling) when calling the ListAttachedRolePolicies operation" >&2; exit 255; }
            echo '["arn:aws:iam::aws:policy/service-role/AWSLambdaBasicExecutionRole"]' ;;
        "iam get-policy") echo v1 ;;
        "iam get-policy-version") echo '{"Statement":[{"Effect":"Allow","Action":["logs:CreateLogStream","logs:PutLogEvents"],"Resource":"*"}]}' ;;
        "s3api head-bucket")
            [ "$mode" = bucket-absent ] && exit 254
            echo "fake aws: unexpected call: $*" >&2; exit 97 ;;
        "s3api create-bucket"|"s3api put-public-access-block"|"s3api put-bucket-encryption"|"s3api put-bucket-tagging")
            [ "$mode" = bucket-absent ] && exit 0
            echo "fake aws: unexpected call: $*" >&2; exit 97 ;;
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
    export FAKE_SERVER="$SRV"
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
        # The child EXPORTS its own eligibility output before it restores the historical log (plan 13-01):
        # the export is deleted first and read only when newer than this marker, so a restored
        # historical log can never satisfy row verify.
        local EXPORT="models/decide/selftest/cases/eligibility-export-$SRV.log" SRC
        rm -f "$EXPORT"
        touch "$W/armed-start"
        echo "  (running the default-armed laya-deploy-selftest once: laya-verify of the deployed artifact, multi-GB)"
        set +e
        env -u LAYA_ELIGIBLE_APR -u LAYA_ELIGIBLE_RUN -u LAYA_ELIGIBLE_DATA -u LAYA_ELIGIBLE_BASE -u LAYA_DEPLOY_SELFTEST_POSITIVE \
            just laya-deploy-selftest > "$W/armed-selftest.log" 2>&1
        ARMED_RC=$?
        set -e
        cp models/decide/selftest/cases/positive.log "$W/armed-positive.log" 2>/dev/null || : > "$W/armed-positive.log"
        SRC="$EXPORT"
        : > "$W/armed-eligibility.log"
        if [ ! -f "$SRC" ]; then
            echo "no export" > "$W/armed-eligibility.why"
        elif [ "$SRC" -nt "$W/armed-start" ]; then
            cp "$SRC" "$W/armed-eligibility.log"
            echo "this run's export" > "$W/armed-eligibility.why"
        else
            echo "export not from this run" > "$W/armed-eligibility.why"
        fi
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
    SCOPED="{\"Effect\":\"Allow\",\"Action\":\"s3:GetObject\",\"Resource\":\"arn:aws:s3:::dry-run-bucket/decide/$SRV/*\"}"
    LOGS='{"Effect":"Allow","Action":["logs:CreateLogStream","logs:PutLogEvents"],"Resource":"*"}'
    grant() { just _laya-grant-check "$1" dry-run-bucket "${2:-$SRV}" aprender-decide-weights-dev; }
    row_grant_check() {
        local inl="{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED]}}"
        local att='"kind":"attached","name":"arn:aws:iam::aws:policy/X","document":{"Statement":'
        must_fail attached-s3-any 'REFUSED grant: attached policy .* an attached managed policy reaches S3' \
            grant "[$inl,{$att[{\"Effect\":\"Allow\",\"Action\":\"s3:GetObject\",\"Resource\":\"arn:aws:s3:::*\"}]}}]"
        must_fail get-star 'REFUSED grant: .*a wildcard S3 action' \
            grant "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED,{\"Effect\":\"Allow\",\"Action\":\"s3:Get*\",\"Resource\":\"arn:aws:s3:::dry-run-bucket/decide/$SRV/*\"}]}}]"
        must_fail decide-prefix-star 'REFUSED grant: .*a wildcard S3 resource' \
            grant "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED,{\"Effect\":\"Allow\",\"Action\":\"s3:GetObject\",\"Resource\":\"arn:aws:s3:::dry-run-bucket/decide/*\"}]}}]"
        must_fail not-action 'REFUSED grant: .*NotAction/NotResource' \
            grant "[{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[$SCOPED,{\"Effect\":\"Allow\",\"NotAction\":\"iam:*\",\"Resource\":\"*\"}]}}]"
        must_pass stack-declared 'grant ok: checked 1 inline and 1 attached policies' grant "[$inl,{$att[$LOGS]}}]"
        # D-19, both directions: neither server's prefix grant satisfies the other server's check.
        local old="{\"name\":\"pmcp-declared\",\"document\":{\"Statement\":[{\"Effect\":\"Allow\",\"Action\":\"s3:GetObject\",\"Resource\":\"arn:aws:s3:::dry-run-bucket/decide/$RETIRED_SERVER/*\"}]}}"
        must_fail old-prefix-for-new 'REFUSED grant: .*(wildcard S3 resource broader than|an S3 grant other than|no inline policy grants)' grant "[$old]"
        must_fail new-prefix-for-old 'REFUSED grant: ' grant "[$inl]" "$RETIRED_SERVER"
    }
    row_grant_listing_failure() {
        must_fail list-fail 'ERROR: IAM read failed: listing the inline policies' env PATH="$FAKE:$PATH" FAKE_AWS_MODE=list-fail just laya-grant "$SRV"
        if grep -q 'REFUSED grant' "$(logof list-fail)"; then red "a failed listing was reported as a grant verdict"; fi
        must_fail attached-fail 'ERROR: IAM read failed: listing the attached managed policies' env PATH="$FAKE:$PATH" FAKE_AWS_MODE=attached-fail just laya-grant "$SRV"
        must_pass ok 'grant ok: checked 1 inline and 1 attached policies' env PATH="$FAKE:$PATH" FAKE_AWS_MODE=ok just laya-grant "$SRV"
    }
    row_teardown_classify() {
        must_fail access-denied 'ERROR: deleting the legacy policy aprender-decide-weights-dev from decide-fn-role failed' env PATH="$FAKE:$PATH" FAKE_AWS_MODE=teardown-denied just laya-teardown "$SRV"
        must_pass no-such-entity 'no legacy policy aprender-decide-weights-dev on decide-fn-role' env PATH="$FAKE:$PATH" FAKE_AWS_MODE=teardown-nosuch just laya-teardown "$SRV"
        # Containment must reach ANY deployed function, the retired one included (`named` mode, not `deployable`).
        must_pass retired-contained 'CONTAINED: aprender-mcp-decide reserved concurrency 0' env PATH="$FAKE:$PATH" FAKE_AWS_MODE=teardown-nosuch just laya-teardown "$RETIRED_SERVER"
    }
    row_sha256_helper() {
        tiny
        local H F NP
        H="$(shasum -a 256 "$W/tiny.apr" | awk '{print $1}')"
        F="$W/foreign-rtk"; mkdir -p "$F"
        printf '#!/bin/sh\necho "rtk 0.1.0 (Rust Type Kit): unknown subcommand $1"\nexit 0\n' > "$F/rtk"; chmod +x "$F/rtk"
        must_pass foreign-rtk "sha256   $H" env PATH="$F:$PATH" just laya-deploy-config "$W/tiny.apr" off "$SRV"
        [ "$RC" -eq 0 ] && [ "$(pinned)" = "$H" ] || red "with a foreign rtk first on PATH the pin is '$(pinned 2>/dev/null || echo none)', not shasum's $H"
        NP="$(nortk_path)"
        if env PATH="$NP" sh -c 'command -v rtk' > /dev/null 2>&1; then red "could not build a PATH without rtk"; fi
        must_pass rtk-absent "sha256   $H" env PATH="$NP" just laya-deploy-config "$W/tiny.apr" off "$SRV"
        [ "$RC" -eq 0 ] && [ "$(pinned)" = "$H" ] || red "with rtk absent the pin is '$(pinned 2>/dev/null || echo none)', not shasum's $H"
        set_pin APRENDER_DECIDE_SHA256 0000000000000000000000000000000000000000000000000000000000000000
        must_fail sha-pin 'REFUSED sha-pin:' env PATH="$F:$PATH" just laya-deploy "$W/tiny.apr" "$T" "$T/data" "$T/checkpoint" "$SRV"
    }
    row_resolver_proof_sed() {
        local d="$W/rp"; mkdir -p "$d"
        printf 'running 1 test\ntest deployment::builder::aprender_resolver_proof::aprender_decide_resolves_from_shared_crates_root ... RESOLVED root=crates server=%s -> crates/%s-lambda\nCONTROL root=crates/%s-lambda server=%s -> crates/aprender-mcp-chronos-lambda\nok\n' "$SRV" "$SRV" "$SRV" "$SRV" > "$d/prefixed.log"
        # The same log shape for the RETIRED id: its RESOLVED line must not be read as the new server's.
        printf 'running 1 test\ntest deployment::builder::aprender_resolver_proof::aprender_decide_resolves_from_shared_crates_root ... RESOLVED root=crates server=%s -> crates/%s-lambda\nok\n' "$RETIRED_SERVER" "$RETIRED_SERVER" > "$d/retired.log"
        printf 'running 1 test\ntest deployment::builder::aprender_resolver_proof::aprender_decide_resolves_from_shared_crates_root ... ok\n' > "$d/none.log"
        must_fail no-resolved 'REFUSED resolver-parse: no RESOLVED line' just _laya-resolver-parse "$d/none.log" "$SRV"
        must_fail retired-id-line 'REFUSED resolver-parse: no RESOLVED line' just _laya-resolver-parse "$d/retired.log" "$SRV"
        must_pass libtest-prefixed "^RESOLVED=crates/$SRV-lambda\$" just _laya-resolver-parse "$d/prefixed.log" "$SRV"
        grep -qx 'CONTROL=crates/aprender-mcp-chronos-lambda' "$(logof libtest-prefixed)" || red "the CONTROL value was not read"
    }
    row_resolver_proof() {
        local nogit="$W/not-a-checkout"; mkdir -p "$nogit"
        if git -C "$nogit" rev-parse --git-dir > /dev/null 2>&1; then red "environment: $nogit is inside a git checkout"; return; fi
        must_fail not-git 'is not a git checkout' just laya-resolver-proof "$nogit" 0000000 "$W/rp-scratch" "$SRV"
        must_fail bad-server 'REFUSED server: ' just laya-resolver-proof "$nogit" 0000000 "$W/rp-scratch" "Bad Server"
        local body; body="$(just --show laya-resolver-proof)"
        if printf '%s' "$body" | grep -qF 'PARSED="$(just _laya-resolver-parse "$LOG" "$SERVER")"' && ! printf '%s' "$body" | grep -qF "s/^RESOLVED root="; then
            echo "  must-pass wiring: RESOLVED/CONTROL come from _laya-resolver-parse (no column-0 sed)"
        else
            red "laya-resolver-proof does not read its tokens through _laya-resolver-parse"
        fi
        if [ "$FULL" = "1" ]; then
            local sdk commit
            sdk="${LAYA_GATES_SDK:-$HOME/Development/mcp/sdk/rust-mcp-sdk}"
            commit="$(sed -n 's/^sdk_commit=//p' "$BK/$PROOF")"
            must_pass real "RESOLVER PROOF: crates/$SRV-lambda " just laya-resolver-proof "$sdk" "$commit" "$W/rp-scratch" "$SRV"
        else
            full_only "the real resolver proof (cargo-pmcp test in an SDK archive)"
        fi
    }
    # A crafted pack record (Phase 15 D-08/D-09) for the synthetic fixture: gate_rec <path> <apr_sha256> <true|false>.
    gate_rec() {
        python3 -I -c 'import json, sys; p, sha, acc = sys.argv[1:4]; json.dump({"schema": "laya-train-pack-outcome-v1", "accepted": acc == "true", "deploy_eligible": acc == "true", "apr_sha256": sha, "pack_image_digest": "sha256:" + "5" * 64, "parity_contract_sha256": "7" * 64}, open(p, "w"))' "$@"
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
        if [ "$ARMED_RC" -eq 0 ] && grep -q "^DRY-RUN OK $SRV " "$W/armed-selftest.log" && grep -qx 'DEPLOY SELFTEST OK' "$W/armed-selftest.log"; then
            echo "  must-pass default-armed: exit 0, $(grep -m 1 '^DRY-RUN OK' "$W/armed-selftest.log" | cut -c 1-120), then the bare DEPLOY SELFTEST OK"
        else
            red "must-pass default-armed: exit $ARMED_RC without DRY-RUN OK and the bare OK line (log $W/armed-selftest.log)"
            tail -4 "$W/armed-selftest.log" >&2 || true
        fi
    }
    row_deploy_refusals() {
        tiny
        run setup just laya-deploy-config "$W/tiny.apr" off "$SRV"
        [ "$RC" -eq 0 ] || { red "laya-deploy-config on the synthetic fixture failed"; return; }
        set_pin APRENDER_DECIDE_S3_URI UNSET-run-just-laya-deploy-config
        must_fail placeholder 'REFUSED placeholder:' just laya-deploy "$W/tiny.apr" "$T" "$T/data" "$T/checkpoint" "$SRV"
        run regen just laya-deploy-config "$W/tiny.apr" off "$SRV"
        must_fail eligibility 'REFUSED eligibility: .*SyntheticNotDeployable' just laya-deploy "$W/tiny.apr" "$T" "$T/data" "$T/checkpoint" "$SRV"
        gate_rec "$W/rec-zeros.json" 0000000000000000000000000000000000000000000000000000000000000000 true
        gate_rec "$W/rec-unaccepted.json" "$GOLDEN" false
        must_fail record-sha-mismatch 'REFUSED eligibility: pack record apr_sha256 .* differs from the local file' \
            env LAYA_PACK_OUTCOME="$W/rec-zeros.json" just laya-deploy "$W/tiny.apr" "$T" "$T/data" "$T/checkpoint" "$SRV"
        must_fail record-not-accepted 'REFUSED eligibility: pack record accepted is not true' \
            env LAYA_PACK_OUTCOME="$W/rec-unaccepted.json" just laya-deploy "$W/tiny.apr" "$T" "$T/data" "$T/checkpoint" "$SRV"
        must_fail retired-id 'REFUSED server: ' just laya-deploy "$W/tiny.apr" "$T" "$T/data" "$T/checkpoint" "$RETIRED_SERVER"
        need_deployed || return 0
        armed_deploy_selftest
        if grep -q "^DRY-RUN OK $SRV " "$W/armed-positive.log"; then
            echo "  must-pass deployed: $(grep -m 1 '^DRY-RUN OK' "$W/armed-positive.log" | cut -c 1-120) (the default-armed selftest's positive dry run)"
        else
            red "must-pass deployed: the positive dry run printed no DRY-RUN OK (log $W/armed-positive.log)"
        fi
    }
    row_upload_eligibility() {
        tiny
        must_fail synthetic 'REFUSED eligibility: .*SyntheticNotDeployable' just laya-upload "$W/tiny.apr" "$T" "$T/data" "$T/checkpoint" "$SRV"
        gate_rec "$W/rec-zeros.json" 0000000000000000000000000000000000000000000000000000000000000000 true
        gate_rec "$W/rec-unaccepted.json" "$GOLDEN" false
        must_fail record-sha-mismatch 'REFUSED eligibility: pack record apr_sha256 .* differs from the local file' \
            env LAYA_PACK_OUTCOME="$W/rec-zeros.json" just laya-upload "$W/tiny.apr" "$T" "$T/data" "$T/checkpoint" "$SRV"
        must_fail record-not-accepted 'REFUSED eligibility: pack record accepted is not true' \
            env LAYA_PACK_OUTCOME="$W/rec-unaccepted.json" just laya-upload "$W/tiny.apr" "$T" "$T/data" "$T/checkpoint" "$SRV"
        must_fail retired-id 'REFUSED server: ' just laya-upload "$W/tiny.apr" "$T" "$T/data" "$T/checkpoint" "$RETIRED_SERVER"
        if [ "$FULL" = "1" ]; then
            need_deployed || return 0
            must_pass deployed '^DRY-RUN: would upload ' just laya-upload "$DEP_APR" "$DEP_RUN" "$DEP_DATA" "$BASE" "$SRV"
            # The real-bytes record path (Phase 15 D-08): smoke-stance-64 was accepted by its Graviton pack task; the
            # record is found by convention beside its `run` dir. Either local outcome is accepted (after plan 15-02 this
            # Mac may accept outright); the case prints which. The server's finding log is kept and restored.
            local SM="$MAIN/models/decide/smoke-stance-64" SMH="31ace8ea95ca0c750b6d007d0f4c68e4322d20363117f837673a936d6f696c85" FL="models/decide/cross-cpu-finding-$SRV.log" smp
            for smp in "$SM/smoke-stance-64.apr" "$SM/run/gate-report.json" "$SM/data" "$SM/logs/pack-outcome.json"; do
                [ -e "$smp" ] || { red "the smoke-stance-64 record case inputs are absent ($smp): this case cannot measure anything"; return 0; }
            done
            if [ -f "$FL" ]; then cp -p "$FL" "$W/finding.bk"; else rm -f "$W/finding.bk"; fi
            must_pass smoke-record '^DRY-RUN: would upload ' just laya-upload "$SM/smoke-stance-64.apr" "$SM/run" "$SM/data" "$BASE" "$SRV"
            if [ -f "$W/finding.bk" ]; then cp -p "$W/finding.bk" "$FL"; else rm -f "$FL"; fi
            if grep -Fq "eligible: ELIGIBLE pack-record sha256 $SMH" "$(logof smoke-record)"; then
                echo "  smoke-record: $(grep -Fm 1 'eligible: ELIGIBLE pack-record' "$(logof smoke-record)" | cut -c 1-200)"
            else
                red "smoke-record: no 'ELIGIBLE pack-record sha256 $SMH' line (log $(logof smoke-record))"
            fi
        else
            full_only "the DRY_RUN uploads of the deployed artifact and of smoke-stance-64 through its pack record (laya-verify, multi-GB)"
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
            red "must-pass deployed: no deploy_eligible true for $H in $W/armed-eligibility.log ($(cat "$W/armed-eligibility.why" 2> /dev/null || echo "no reason recorded"))"
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
    row_prefix_budget() {
        # Plan 10-01. Every case here asserts a fact that is a function of the task's prefix P or of an EXPLICIT
        # --max-texts, never the relation of P to the live contract door: plan 10-11 lowers that constant to the
        # owner's choice later in the phase, and this row must stay green at every door from 1 to 8.
        python3 -I -c 'import json, sys; w = sys.argv[1]; crit = {"not_antisemitic": "a", "antisemitic": "b"}; json.dump({"type": "choice", "instructions": " ".join(["the"] * 110), "criteria": crit}, open(w + "/pb-over.json", "w")); json.dump({"type": "choice", "instructions": "Is this tweet antisemitic?", "criteria": {"not_antisemitic": None, "antisemitic": None}}, open(w + "/pb-bare.json", "w")); crit["antisemitic"] = " ".join(["the"] * 60); json.dump({"type": "choice", "instructions": "Is this tweet antisemitic?", "criteria": crit}, open(w + "/pb-cut.json", "w"))' "$W"
        must_fail over-ceiling 'REFUSED prefix-budget' just laya-prepare-goldstandard --measure --task "$W/pb-over.json" --max-texts 8
        must_fail cut 'REFUSED prefix-cut' just laya-prepare-goldstandard --measure --task "$W/pb-cut.json"
        must_pass bare '"markers": ?2,.*"fits_door": ?true' just laya-prepare-goldstandard --measure --task "$W/pb-bare.json"
        must_pass report-only '"door_enforced": ?false,.*"max_texts_that_fit": ?[1-7][,}]' just laya-prepare-goldstandard --measure --task "$W/pb-over.json"
    }
    row_setfit_cell() {
        # Plan 10-05. The refusals are the default tier: they fire before the apr pin, any build or any
        # output dir. The cell itself trains on CPU for about 30 min, so it is FULL tier only; its selection
        # is written under $W (the fourth argument), never into the real data dir.
        must_fail no-data '^SETFIT CELL REFUSED select: no benchmark-manifest.json' just laya-setfit-cell "$W/setfit-no-data" "$W/setfit-out"
        mkdir -p "$W/setfit-stub"
        printf '{}\n' > "$W/setfit-stub/benchmark-manifest.json"
        must_fail no-config '^SETFIT CELL REFUSED config:' just laya-setfit-cell "$W/setfit-stub" "$W/setfit-out2" "$W/no-such-config.json"
        test ! -e "$W/setfit-out" || red "no-data: the refused cell created its output dir"
        test ! -e "$W/setfit-out2" || red "no-config: the refused cell created its output dir"
        if [ "$FULL" = "1" ]; then
            must_pass cell '^SETFIT CELL OK cell=s64e1b16 seed=13 ' just laya-setfit-cell data/decide/goldstandard-2024-setfit "$W/setfit-cell" scripts/laya_train/setfit_cell_e1b16_cpu.json "$W/setfit-sel"
        else
            full_only "the s64e1b16 cell (CPU SetFit training, about 30 min)"
        fi
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
        must_fail one-sample 'samples must be >= 2' just laya-deploy-verify "$DEP_APR" "$SRV" ze-kasher-dev 1
        must_fail missing-apr 'does not exist' just laya-deploy-verify "$W/no-such.apr" "$SRV"
        must_fail retired-id 'REFUSED server: ' just laya-deploy-verify "$DEP_APR" "$RETIRED_SERVER"
        must_pass dry-run '^DRY-RUN: stopped before the network' just laya-deploy-verify "$DEP_APR" "$SRV"
    }
    row_bootstrap_build() {
        local Z="$W/zigshim"; mkdir -p "$Z"
        printf '#!/bin/sh\necho "cargo-zigbuild shim: built nothing"\nexit 0\n' > "$Z/cargo-zigbuild"; chmod +x "$Z/cargo-zigbuild"
        must_fail stale 'ERROR: .*(is older than this build|does not exist after the build)' env PATH="$Z:$PATH" just laya-build-bootstrap "$SRV"
        must_pass real '^BOOTSTRAP aarch64 OK ' just laya-build-bootstrap "$SRV"
    }
    row_edge_health_url() {
        must_fail health-path 'REFUSED edge-health-url' just _laya-edge-health-url "https://$SRV.us-east.true-mcp.com/health"
        must_fail http 'REFUSED edge-health-url' just _laya-edge-health-url "http://$SRV.us-east.true-mcp.com/mcp"
        must_pass mcp "^https://$SRV.us-east.true-mcp.com/health\$" just _laya-edge-health-url "https://$SRV.us-east.true-mcp.com/mcp"
    }
    row_edge_health_check() {
        must_fail chronos "REFUSED edge-health: serverId is 'chronos-forecaster'" just _laya-edge-health-check '{"serverId":"chronos-forecaster"}' "$SRV"
        must_fail bootstrap-body 'REFUSED edge-health: serverId is None' just _laya-edge-health-check "{\"package\":\"$SRV-lambda\",\"server\":\"$SRV\"}" "$SRV"
        # The reverse-prefix case: the retired server's healthy edge body is not the new server's.
        must_fail retired-server "REFUSED edge-health: serverId is '$RETIRED_SERVER'" just _laya-edge-health-check "{\"status\":\"healthy\",\"serverId\":\"$RETIRED_SERVER\"}" "$SRV"
        must_pass decide "^edge-health ok: serverId $SRV\$" just _laya-edge-health-check "{\"status\":\"healthy\",\"serverId\":\"$SRV\"}" "$SRV"
    }
    row_edge_health_get() {
        # A scripted curl in $W (the Nth call answers line N of the script, the last line repeats):
        # a 404 that never clears and an HTTP 500 are contained; 404, 404, 200 is retried to success.
        local C="$W/curlshim" S="$W/curl-script" L="$W/curl-calls.log" O="$W/health-get.json"
        mkdir -p "$C"
        printf '#!/bin/sh\nout=""\nwhile [ $# -gt 0 ]; do case "$1" in -o) out="$2"; shift 2 ;; -w|--max-time) shift 2 ;; *) shift ;; esac; done\nn=$(( $(wc -l < "$CURL_CALLS" | tr -d " ") + 1 ))\ncode="$(sed -n "${n}p" "$CURL_SCRIPT")"\n[ -n "$code" ] || code="$(tail -n 1 "$CURL_SCRIPT")"\necho "$code" >> "$CURL_CALLS"\nprintf "{\\"serverId\\":\\"shim\\"}" > "$out"\nprintf "%%s" "$code"\n' > "$C/curl"
        chmod +x "$C/curl"
        printf '404\n' > "$S"; : > "$L"
        must_fail forever 'REFUSED edge-health-get: HTTP 404 after 3 attempt' env PATH="$C:$PATH" CURL_SCRIPT="$S" CURL_CALLS="$L" LAYA_EDGE_HEALTH_TRIES=3 LAYA_EDGE_HEALTH_SLEEP=0 just _laya-edge-health-get "https://$SRV.us-east.true-mcp.com/health" "$O"
        [ ! -s "$O" ] || red "forever: a refused GET left a body file behind"
        printf '500\n' > "$S"; : > "$L"
        must_fail http-500 'REFUSED edge-health-get: HTTP 500 after 1 attempt' env PATH="$C:$PATH" CURL_SCRIPT="$S" CURL_CALLS="$L" LAYA_EDGE_HEALTH_SLEEP=0 just _laya-edge-health-get "https://$SRV.us-east.true-mcp.com/health" "$O"
        [ "$(wc -l < "$L" | tr -d ' ')" -eq 1 ] || red "http-500: the 500 was retried"
        printf '404\n404\n200\n' > "$S"; : > "$L"
        must_pass retried '^edge-health-get ok: HTTP 200 after 3 attempt\(s\)' env PATH="$C:$PATH" CURL_SCRIPT="$S" CURL_CALLS="$L" LAYA_EDGE_HEALTH_SLEEP=0 just _laya-edge-health-get "https://$SRV.us-east.true-mcp.com/health" "$O"
    }
    row_server_check() {
        must_fail space 'REFUSED server: .*not a server id' just _laya-server-check "bad name" named
        must_fail digit-led 'REFUSED server: .*not a server id' just _laya-server-check 1-bad-id named
        must_fail upper-case-led 'REFUSED server: .*not a server id' just _laya-server-check Aprender-bad named
        must_fail too-long 'REFUSED server: .*is 65 characters' just _laya-server-check "$(printf 'a%.0s' $(seq 1 65))" named
        must_fail hyphen-ended 'REFUSED server: .*ends with a hyphen' just _laya-server-check aprender-mcp-decide-x- named
        must_fail no-thin-crate 'REFUSED server: .*no thin crate' just _laya-server-check "$SRV-nonexistent" deployable
        must_fail retired-deployable 'REFUSED server: .*declares no bootstrap bin' just _laya-server-check "$RETIRED_SERVER" deployable
        must_fail unknown-mode 'REFUSED server: unknown mode' just _laya-server-check "$SRV" sometimes
        must_pass thin-crate "^server ok: $SRV \\(deployable\\)\$" just _laya-server-check "$SRV" deployable
        must_pass retired-named "^server ok: $RETIRED_SERVER \\(named\\)\$" just _laya-server-check "$RETIRED_SERVER" named
        # Wiring: every recipe that reaches AWS checks the id before it does anything else (teardown by name only).
        local r body
        for r in laya-deploy-config laya-deploy laya-upload laya-grant laya-deploy-verify; do
            body="$(just --show "$r")"
            printf '%s' "$body" | grep -qF 'just _laya-server-check "$SERVER" deployable' || red "$r does not check the server id in deployable mode"
        done
        body="$(just --show laya-teardown)"
        printf '%s' "$body" | grep -qF 'just _laya-server-check "$SERVER" named' || red "laya-teardown does not check the server id in named mode"
        echo "  must-pass wiring: the five deploy-path recipes check the id (deployable) and laya-teardown names it (named)"
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
        printf '[server]\nname = "%s"\n[environment]\nAPRENDER_DECIDE_S3_URI = "s3://b/decide/%s/h.apr"\n[[iam.statements]]\neffect = "Allow"\nactions = ["s3:GetObject"]\nresources = ["arn:aws:s3:::b/decide/%s/*"]\n' "$SRV" "$SRV" "$SRV" > "$ok"
        local oldp="$W/iam-old-prefix.toml"
        sed "s#decide/$SRV/\*#decide/$RETIRED_SERVER/*#" "$ok" > "$oldp"
        sed 's/actions = \["s3:GetObject"\]/actions = ["s3:*"]/' "$ok" > "$star"
        { cat "$ok"; printf '[[iam.statements]]\neffect = "Allow"\nactions = ["s3:GetObject"]\nresources = ["arn:aws:s3:::other/x"]\n'; } > "$two"
        must_fail s3-star 'REFUSED iam: the statement' just _laya-iam-check "$star"
        must_fail two-statements 'REFUSED iam: expected exactly one' just _laya-iam-check "$two"
        must_fail old-prefix 'REFUSED iam: the statement' just _laya-iam-check "$oldp"
        must_pass scoped "^iam ok: Allow s3:GetObject arn:aws:s3:::b/decide/$SRV/\\*" just _laya-iam-check "$ok"
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
            must_fail no-bootstrap-line 'D  `bootstrap` is declared by 6 packages' \
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
    row_region_selftest() {
        grep -v '^region = "us-east-1"$' crates/aprender-mcp-decide-lambda/.pmcp/deploy.toml.template > "$W/region-zero.toml" || true
        must_fail zero-line-template '^REGION SELFTEST FAILED' env LAYA_REGION_SELFTEST_TEMPLATE="$W/region-zero.toml" just _laya-region-selftest
        must_pass default '^REGION SELFTEST OK$' just _laya-region-selftest
        grep -qx 'AWS CALLS: 0' "$(logof default)" || red "default: the region selftest did not report AWS CALLS: 0"
    }
    row_weights_bucket_region() {
        : > "$FAKE_AWS_LOG"
        must_fail malformed '^REFUSED region' env PATH="$FAKE:$PATH" FAKE_AWS_MODE=bucket-absent just laya-weights-bucket dev ze-kasher-dev EU-WEST-1
        [ ! -s "$FAKE_AWS_LOG" ] || red "malformed: the fake aws was called before the region refusal ($(head -n 1 "$FAKE_AWS_LOG" | cut -c 1-60))"
        : > "$FAKE_AWS_LOG"
        must_pass eu-west-1 '\(region eu-west-1\)$' env PATH="$FAKE:$PATH" FAKE_AWS_MODE=bucket-absent just laya-weights-bucket dev augury-dev eu-west-1
        grep -q '^s3api create-bucket .*--create-bucket-configuration LocationConstraint=eu-west-1' "$FAKE_AWS_LOG" \
            || red "eu-west-1: the fake aws recorded no create-bucket naming LocationConstraint=eu-west-1"
        : > "$FAKE_AWS_LOG"
    }
    row_prepare_motivation_args() {
        mkdir -p "$W/pm"
        must_fail spaced-csv 'REFUSED csv-missing' just laya-prepare-motivation --csv "$W/pm/no such dir/a b.csv" --out "$W/pm/o"
        if grep -q 'unrecognized arguments' "$(logof spaced-csv)"; then red "spaced-csv: the spaced path was word-split (unrecognized arguments)"; fi
        must_pass selftest '^PREPARE MOTIVATION SELFTEST OK' just laya-prepare-motivation --selftest
    }
    row_fetch() {
        # D-20 (plan 14-11): a fake aws serves a run whose gate-report.json carries one flipped byte; the real recipe refuses it.
        local FR="$W/fetchrun" FA="$W/fetchaws" TID="00000000-0000-4000-8000-0000000000f1" FD="$W/fetchdest"
        mkdir -p "$FA" "$FR"
        python3 -I - "$FR" "$TID" <<'PYF'
    import hashlib, json, os, sys
    root, tid = sys.argv[1:3]
    files = {"verdict.json": b'{"phase":"gate_failed"}\n', "outcome.json": b'{"exit_class":"gate_fail"}\n',
             "train.log": b"epoch 1\n", "run/gate-report.json": b'{"pass":false,"margin":0.01}\n',
             "data/task.json": b'{"task":"x"}\n'}
    entries = [{"path": p, "size": len(b), "sha256": hashlib.sha256(b).hexdigest()} for p, b in sorted(files.items())]
    man = {"schema": "laya-train-manifest-v1", "task_id": tid, "name": "fixture-run", "images": {}, "contracts": {}, "entries": entries, "unlisted": 0}
    files["manifest.json"] = json.dumps(man).encode()
    for p, b in files.items():
        fp = os.path.join(root, p)
        os.makedirs(os.path.dirname(fp), exist_ok=True)
        open(fp, "wb").write(b)
    # the served copy of gate-report.json has one flipped byte (the manifest keeps the original hash)
    g = os.path.join(root, "run", "gate-report.json")
    b = bytearray(open(g, "rb").read())
    b[2] ^= 1
    open(g + ".tampered", "wb").write(bytes(b))
    PYF
        cat > "$FA/aws" <<'SH'
    #!/usr/bin/env bash
    # fake `aws s3api get-object --profile P --bucket B --key K FILE`: serves $FETCH_RUN/<key minus runs/NAME/TASK/>
    printf '%s\n' "$*" >> "${FAKE_AWS_LOG:-/dev/null}"
    [ "$1 $2" = "s3api get-object" ] || { echo "fake aws: unexpected call: $*" >&2; exit 97; }
    key=""; out=""
    while [ $# -gt 0 ]; do
        case "$1" in
            --key) key="$2"; shift 2 ;;
            --profile|--bucket) shift 2 ;;
            s3api|get-object) shift ;;
            *) out="$1"; shift ;;
        esac
    done
    rel="${key#runs/fixture-run/*/}"
    src="$FETCH_RUN/$rel"
    [ -n "${FETCH_TAMPER:-}" ] && [ -f "$src.tampered" ] && src="$src.tampered"
    [ -f "$src" ] || { echo "An error occurred (NoSuchKey) when calling the GetObject operation" >&2; exit 254; }
    cp "$src" "$out"
    SH
        chmod +x "$FA/aws"
        must_fail tampered 'REFUSED fetch-sha256' env PATH="$FA:$PATH" FETCH_RUN="$FR" FETCH_TAMPER=1 \
            just laya-fetch fixture-run "$TID" fixture-bucket --dest "$FD"
        [ "$RC" -eq 2 ] || red "tampered: exit $RC, not 2"
        [ ! -e "$FD" ] || red "tampered: the destination exists after a refused fetch"
        ls -d "$W"/.fixture-run.fetch-* > /dev/null 2>&1 && red "tampered: a staging directory was left behind"
        must_pass untampered '^FETCH OK phase gate_failed apr none$' env PATH="$FA:$PATH" FETCH_RUN="$FR" just laya-fetch fixture-run "$TID" fixture-bucket --dest "$FD"
        [ -f "$FD/run/gate-report.json" ] && [ -f "$FD/verdict.json" ] && [ -f "$FD/logs/train.log" ] || red "untampered: the destination layout is incomplete"
        must_fail refetch-over 'REFUSED fetch-dest' env PATH="$FA:$PATH" FETCH_RUN="$FR" just laya-fetch fixture-run "$TID" fixture-bucket --dest "$FD"
        must_fail wrong-profile 'REFUSED fetch-profile' env PATH="$FA:$PATH" FETCH_RUN="$FR" just laya-fetch fixture-run "$TID" fixture-bucket --dest "$W/fetchdest2" --profile augury-dev
        must_pass selftest '^LAYA FETCH SELFTEST OK' just laya-fetch --selftest
    }
    row_drift() {
        # D-15 (plan 14-20): an unknown leg is refused before any build or measurement; the selftest proves the parser, the
        # pinned-binary leg on a fake pack_laya, the generated Graviton script against a tag-modelling fake aws (three tag
        # mutants refused), the static policy table, bashrs-gate and the collect scrub. The long legs never run in this row.
        must_fail unknown-leg '^REFUSED drift-leg: ' just laya-drift local --only no-such-leg --out "$W/drift-out"
        [ "$RC" -eq 2 ] || red "unknown-leg: exit $RC, not 2"
        [ ! -e "$W/drift-out" ] || red "unknown-leg: a refused run created its output directory"
        grep -q 'cargo build\|LEG ' "$(logof unknown-leg)" && red "unknown-leg: a refused run built or measured something"
        must_pass selftest '^DRIFT SELFTEST OK' just laya-drift --selftest
        grep -q '^ok   bashrs-gate passes the generated fixture script$' "$(logof selftest)" || red "selftest: the generated script was not linted"
    }
    row_failure_surface() {
        # Plan 14-16: the failure-surface enumeration is bound to the code it was derived from; a tampered copy (one
        # exit class flipped) is refused, the real one passes, and every planted mutation is refused for its own rule.
        local FST="$W/failure-surface-tampered.json"
        python3 -I - crates/aprender-mcp-laya-train-lambda/fixtures/failure-surface-v1.json "$FST" <<'PYS'
    import json, sys
    a = json.load(open(sys.argv[1]))
    m = a["exit_classes"]["job_exit_code"]
    m["refused"] = 0 if m["refused"] else 1
    json.dump(a, open(sys.argv[2], "w"))
    PYS
        must_fail tampered '^REFUSED surface-exit-class' just laya-failure-surface --check --artifact "$FST"
        [ "$RC" -eq 1 ] || red "tampered: exit $RC, not 1"
        must_pass check '^FAILURE SURFACE OK rows=' just laya-failure-surface --check
        must_pass selftest '^FAILURE SURFACE SELFTEST OK' just laya-failure-surface --selftest
    }
    row_trainer_synth() {
        # D-01 / D-12 (plan 14-08): an unknown env is refused before any build; the real synth and the offline template check pass.
        must_fail staging-refused 'REFUSED env' just laya-trainer-synth staging
        [ "$RC" -eq 2 ] || red "staging-refused: exit $RC, not 2"
        must_fail one-digest-refused 'REFUSED digest' just laya-trainer-synth dev "sha256:$(printf '1%.0s' $(seq 64))"
        [ "$RC" -eq 2 ] || red "one-digest-refused: exit $RC, not 2"
        must_pass dev '^TRAINER SYNTH OK env=dev$' just laya-trainer-synth dev
        grep -q '^TRAINER STACK CHECK OK' "$(logof dev)" || red "dev: the synth did not print TRAINER STACK CHECK OK"
        grep -q 'PLACEHOLDER DIGESTS (synth only)' "$(logof dev)" || red "dev: the placeholder digests were not announced"
        must_pass selftest '^STACK CHECK SELFTEST OK$' python3 -I scripts/laya_deploy/laya_trainer_stack_check.py --selftest
    }
    row_trainer_image() {
        # D-01 / D-02 (plan 14-09): a foreign profile or env is refused before any command; a dry run against placeholder state
        # (never the real models/laya-trainer) prints the commands and reaches no aws.
        local TS="$W/ops-state-image"
        python3 -I scripts/laya_deploy/laya_trainer_ops.py --write-placeholder-state "$TS" > /dev/null || { red "the placeholder state could not be written"; return; }
        must_fail foreign-profile '^REFUSED profile: ' just laya-trainer-image train dev augury-dev
        [ "$RC" -eq 2 ] || red "foreign-profile: exit $RC, not 2"
        must_fail staging-env '^REFUSED env: ' just laya-trainer-image train staging ze-kasher-dev
        [ "$RC" -eq 2 ] || red "staging-env: exit $RC, not 2"
        must_pass train-dry '^AWS CALLS: 0$' just laya-trainer-image train dev ze-kasher-dev --state-dir "$TS"
        grep -q '^+ aws codebuild start-build .*name=DOCKERFILE,value=scripts/laya_train/container/Dockerfile.train' "$(logof train-dry)" || red "train-dry: no start-build with the train Dockerfile"
        must_pass pack-dry '^AWS CALLS: 0$' just laya-trainer-image pack dev ze-kasher-dev --state-dir "$TS"
        grep -q '^+ aws codebuild start-build .*name=DOCKERFILE,value=deploy-extensions/container/pack-arm64/Dockerfile' "$(logof pack-dry)" || red "pack-dry: no start-build with the pack Dockerfile"
        [ "$(wc -l < "$REC" | tr -d ' ')" -eq 0 ] || red "the aws recorder was called by a dry run"
        must_pass selftest '^LAYA TRAINER OPS SELFTEST OK$' just laya-trainer-image --selftest
    }
    row_trainer_infra() {
        # D-01 / D-02 (plan 14-09): the stack recipes refuse a foreign profile and a deploy without built digests; a dry run prints
        # `cdk diff` before `cdk deploy --require-approval never` and reaches no aws.
        local TS="$W/ops-state-infra" TZ0="$W/ops-state-infra-zero"
        python3 -I scripts/laya_deploy/laya_trainer_ops.py --write-placeholder-state "$TS" > /dev/null || { red "the placeholder state could not be written"; return; }
        python3 -I scripts/laya_deploy/laya_trainer_ops.py --write-placeholder-state "$TZ0" > /dev/null || { red "the placeholder state could not be written"; return; }
        python3 -I - "$TZ0/images-dev.json" <<'PYZ'
    import json, sys
    d = json.load(open(sys.argv[1]))
    d["train"]["digest"] = "sha256:" + "0" * 64
    json.dump(d, open(sys.argv[1], "w"))
    PYZ
        must_fail foreign-profile '^REFUSED profile: ' just laya-trainer-infra diff dev augury-dev
        [ "$RC" -eq 2 ] || red "foreign-profile: exit $RC, not 2"
        must_fail placeholder-digest '^REFUSED infra: no built image digests' just laya-trainer-infra deploy dev ze-kasher-dev --state-dir "$TZ0"
        [ "$RC" -eq 2 ] || red "placeholder-digest: exit $RC, not 2"
        must_pass deploy-dry '^AWS CALLS: 0$' just laya-trainer-infra deploy dev ze-kasher-dev --state-dir "$TS"
        local DIFFN DEPN
        DIFFN="$(grep -n '^+ npx cdk diff .*stack=laya-trainer ' "$(logof deploy-dry)" | head -n 1 | cut -d: -f1)"
        DEPN="$(grep -n '^+ npx cdk deploy .*--require-approval never' "$(logof deploy-dry)" | head -n 1 | cut -d: -f1)"
        { [ -n "$DIFFN" ] && [ -n "$DEPN" ] && [ "$DIFFN" -lt "$DEPN" ]; } || red "deploy-dry: cdk diff (line '$DIFFN') does not precede cdk deploy (line '$DEPN')"
        must_pass outputs-images-dry '^AWS CALLS: 0$' just laya-trainer-infra outputs-images dev ze-kasher-dev --state-dir "$TS"
        must_pass outputs-dry '^AWS CALLS: 0$' just laya-trainer-infra outputs dev ze-kasher-dev --state-dir "$TS"
        [ "$(wc -l < "$REC" | tr -d ' ')" -eq 0 ] || red "the aws recorder was called by a dry run"
    }
    row_trainer_stage() {
        # D-03 / D-07 (plan 14-09): a wrong base tar, an extra dataset file and a bad prefix are refused before any command; dry runs
        # against placeholder state reach no aws.
        local TS="$W/ops-state-stage" DS="$W/ops-ds" DSX="$W/ops-ds-extra" TAR="$W/ops-base.tar" PIN="$W/ops-base-stage.json"
        python3 -I scripts/laya_deploy/laya_trainer_ops.py --write-placeholder-state "$TS" > /dev/null || { red "the placeholder state could not be written"; return; }
        mkdir -p "$DS" "$DSX"
        printf '{"task":"x"}\n' > "$DS/task.json"; printf '{"text":"a","label":0}\n' > "$DS/train.jsonl"; printf '{"text":"b","label":1}\n' > "$DS/eval.jsonl"
        cp "$DS"/*.jsonl "$DS/task.json" "$DSX/"; printf 'x\n' > "$DSX/notes.txt"
        printf 'a tiny stand-in for the base tar\n' > "$TAR"
        python3 -I - "$PIN" "$TAR" <<'PYS'
    import hashlib, json, sys
    pin, tar = sys.argv[1:3]
    d = json.load(open("scripts/laya_train/container/base-stage.json"))
    b = open(tar, "rb").read()
    d.update({"tar_sha256": hashlib.sha256(b).hexdigest(), "tar_bytes": len(b)})
    json.dump(d, open(pin, "w"))
    PYS
        must_fail foreign-profile '^REFUSED profile: ' just laya-trainer-stage base dev augury-dev
        [ "$RC" -eq 2 ] || red "foreign-profile: exit $RC, not 2"
        must_fail wrong-tar '^REFUSED stage: ' just laya-trainer-stage base dev ze-kasher-dev --tar "$TAR" --state-dir "$TS"
        [ "$RC" -eq 2 ] || red "wrong-tar: exit $RC, not 2"
        must_fail extra-file '^REFUSED stage: DIR holds' just laya-trainer-stage dataset "$DSX" p1 dev ze-kasher-dev --state-dir "$TS"
        [ "$RC" -eq 2 ] || red "extra-file: exit $RC, not 2"
        must_fail bad-prefix '^REFUSED stage: PREFIX' just laya-trainer-stage dataset "$DS" Upper dev ze-kasher-dev --state-dir "$TS"
        [ "$RC" -eq 2 ] || red "bad-prefix: exit $RC, not 2"
        must_pass base-dry '^AWS CALLS: 0$' just laya-trainer-stage base dev ze-kasher-dev --tar "$TAR" --stage-json "$PIN" --state-dir "$TS"
        grep -q '^+ aws s3 cp .* s3://.*/base/laya-55cf4c4e/laya-base.tar ' "$(logof base-dry)" || red "base-dry: no s3 cp to base/laya-55cf4c4e/laya-base.tar"
        must_pass dataset-dry '^AWS CALLS: 0$' just laya-trainer-stage dataset "$DS" p1 dev ze-kasher-dev --state-dir "$TS"
        grep -q '^+ aws s3 cp .*/train.jsonl s3://.*/datasets/p1/train.jsonl ' "$(logof dataset-dry)" || red "dataset-dry: no s3 cp of train.jsonl to datasets/p1/"
        [ "$(wc -l < "$REC" | tr -d ' ')" -eq 0 ] || red "the aws recorder was called by a dry run"
    }
    row_trainer_probe() {
        # D-13 / D-16 (plan 14-09): an unapproved live probe is refused before any call; dry runs print the one request and reach no aws.
        local TS="$W/ops-state-probe"
        python3 -I scripts/laya_deploy/laya_trainer_ops.py --write-placeholder-state "$TS" > /dev/null || { red "the placeholder state could not be written"; return; }
        must_fail foreign-profile '^REFUSED profile: ' just laya-trainer-probe cuda dev augury-dev
        [ "$RC" -eq 2 ] || red "foreign-profile: exit $RC, not 2"
        must_fail unknown-mode '^REFUSED usage: ' just laya-trainer-probe gpu dev ze-kasher-dev
        [ "$RC" -eq 2 ] || red "unknown-mode: exit $RC, not 2"
        must_fail unapproved-spend '^REFUSED spend: ' env -u LAYA_TRAINER_APPROVED_SPEND DRY_RUN=0 just laya-trainer-probe cuda dev ze-kasher-dev --state-dir "$TS"
        [ "$RC" -eq 2 ] || red "unapproved-spend: exit $RC, not 2"
        must_pass cuda-dry '^AWS CALLS: 0$' just laya-trainer-probe cuda dev ze-kasher-dev --state-dir "$TS"
        grep -q '"LAYA_STEP": "probe"' "$(logof cuda-dry)" || red "cuda-dry: the request does not carry LAYA_STEP probe"
        grep -q '"MaxRuntimeInSeconds": 1800' "$(logof cuda-dry)" || red "cuda-dry: the request does not carry MaxRuntimeInSeconds 1800"
        must_pass sigterm-dry '^AWS CALLS: 0$' just laya-trainer-probe sigterm dev ze-kasher-dev --state-dir "$TS"
        grep -q '"LAYA_STEP": "probe-sigterm"' "$(logof sigterm-dry)" || red "sigterm-dry: the request does not carry LAYA_STEP probe-sigterm"
        [ "$(wc -l < "$REC" | tr -d ' ')" -eq 0 ] || red "the aws recorder was called"
    }
    row_trainer_config() {
        # D-10 (plan 14-10): a template declaring IAM and a foreign profile are refused; a dry run renders the tracked template from
        # placeholder outputs (never the real models/laya-trainer) at mode 600 and reaches no aws.
        local TS="$W/ops-state-config" BADT="$W/ops-iam.template"
        python3 -I scripts/laya_deploy/laya_trainer_ops.py --write-placeholder-state "$TS" > /dev/null || { red "the placeholder state could not be written"; return; }
        cp crates/aprender-mcp-laya-train-lambda/.pmcp/deploy.toml.template "$BADT"
        printf '\n[[iam.statements]]\neffect = "Allow"\nactions = ["s3:GetObject"]\nresources = ["*"]\n' >> "$BADT"
        must_fail template-iam '^REFUSED config: the template declares IAM' just laya-trainer-config dev ze-kasher-dev --template "$BADT" --state-dir "$TS"
        [ "$RC" -eq 2 ] || red "template-iam: exit $RC, not 2"
        [ ! -e "$TS/deploy-config-dev.toml" ] || red "template-iam: a config was written from a template declaring IAM"
        must_fail foreign-profile '^REFUSED profile: ' just laya-trainer-config dev augury-dev --state-dir "$TS"
        [ "$RC" -eq 2 ] || red "foreign-profile: exit $RC, not 2"
        must_pass render '^CONFIG OK deploy-config-dev.toml$' just laya-trainer-config dev ze-kasher-dev --state-dir "$TS"
        grep -qx 'AWS CALLS: 0' "$(logof render)" || red "render: the dry run did not report AWS CALLS: 0"
        [ "$(stat -f %Lp "$TS/deploy-config-dev.toml" 2> /dev/null || stat -c %a "$TS/deploy-config-dev.toml")" = "600" ] || red "render: the config is not mode 600"
        if grep -q 'UNSET-run-just-laya-trainer-config' "$TS/deploy-config-dev.toml" || grep -v '^[[:space:]]*#' "$TS/deploy-config-dev.toml" | grep -q 'iam'; then
            red "render: a placeholder survived or the config names iam"
        fi
        grep -q '^APRENDER_LAYA_TRAIN_BUCKET = "aprender-laya-train-dev-' "$TS/deploy-config-dev.toml" || red "render: the bucket value was not filled"
        [ "$(wc -l < "$REC" | tr -d ' ')" -eq 0 ] || red "the aws recorder was called"
    }
    row_trainer_deploy() {
        # D-10 / Pitfall 9 (plan 14-10): a foreign profile, a missing config and an unapproved live deploy are refused before any command;
        # a dry run prints the swap and the cargo pmcp line, reaches no aws, and leaves crates/.pmcp and crates/deploy byte-identical.
        local TS="$W/ops-state-deploy" B0="$W/pmcp-before.txt" B1="$W/pmcp-after.txt" TSE="$W/ops-state-deploy-empty"
        python3 -I scripts/laya_deploy/laya_trainer_ops.py --write-placeholder-state "$TS" > /dev/null || { red "the placeholder state could not be written"; return; }
        python3 -I scripts/laya_deploy/laya_trainer_ops.py --write-placeholder-state "$TSE" > /dev/null || { red "the placeholder state could not be written"; return; }
        just laya-trainer-config dev ze-kasher-dev --state-dir "$TS" > /dev/null 2>&1 || { red "the config could not be rendered for the deploy cases"; return; }
        find crates/.pmcp crates/deploy -type f -print0 2> /dev/null | LC_ALL=C sort -z | xargs -0 shasum -a 256 > "$B0"
        must_fail foreign-profile '^REFUSED profile: ' just laya-trainer-deploy dev augury-dev --state-dir "$TS"
        [ "$RC" -eq 2 ] || red "foreign-profile: exit $RC, not 2"
        must_fail no-config '^REFUSED deploy: deploy-config-dev.toml is missing' just laya-trainer-deploy dev ze-kasher-dev --state-dir "$TSE"
        [ "$RC" -eq 2 ] || red "no-config: exit $RC, not 2"
        must_fail unapproved-live '^REFUSED approval: ' env -u LAYA_TRAINER_APPROVED_DEPLOY DRY_RUN=0 just laya-trainer-deploy dev ze-kasher-dev --state-dir "$TS"
        [ "$RC" -eq 2 ] || red "unapproved-live: exit $RC, not 2"
        must_pass dry '^AWS CALLS: 0$' just laya-trainer-deploy dev ze-kasher-dev --state-dir "$TS"
        grep -Eq '^\+ just _laya-crates-root-swap crates .*deploy-config-dev\.toml .*deploy-dev\.state cargo pmcp deploy --manifest-path crates --regenerate-stack --no-post-deploy-test --no-oauth --no-color$' "$(logof dry)" \
            || red "dry: the swap and cargo pmcp line were not printed"
        [ "$(wc -l < "$REC" | tr -d ' ')" -eq 0 ] || red "the aws recorder was called by a dry run"
        find crates/.pmcp crates/deploy -type f -print0 2> /dev/null | LC_ALL=C sort -z | xargs -0 shasum -a 256 > "$B1"
        cmp -s "$B0" "$B1" || red "crates/.pmcp or crates/deploy changed"
        must_pass selftest '^LAYA TRAINER OPS SELFTEST OK$' just laya-trainer-deploy --selftest
    }
    # The deployment triple (plan 13-04, D-06): the four pure helpers and their selftest.
    row_target_check() {
        must_fail wrong-region '^REFUSED target: ' just _laya-target-check augury-dev us-east-1
        [ "$RC" -eq 2 ] || red "wrong-region: exit $RC, not 2"
        must_fail stale-pmcp-target '^REFUSED target: PMCP_TARGET ' env PMCP_TARGET=dev just _laya-target-check augury-dev eu-west-1
        [ "$RC" -eq 2 ] || red "stale-pmcp-target: exit $RC, not 2"
        must_fail api-url-conflict '^REFUSED target: PMCP_API_URL ' env PMCP_API_URL=https://fixture1.execute-api.us-east-1.amazonaws.com just _laya-target-check augury-dev eu-west-1
        [ "$RC" -eq 2 ] || red "api-url-conflict: exit $RC, not 2"
        must_pass augury '^augury-dev eu-west-1 \.eu-west\.augury\.pmcp\.run 783960389328$' just _laya-target-check augury-dev eu-west-1
    }
    row_account_check() {
        : > "$FAKE_AWS_LOG"
        must_fail foreign-account '^REFUSED target-account: profile augury-dev resolves to an account ending 9666' \
            env PATH="$FAKE:$PATH" LAYA_ACCOUNT_CHECK_FORCE=1 FAKE_AWS_ACCOUNT=816069129666 just _laya-account-check augury-dev eu-west-1
        [ "$RC" -eq 2 ] || red "foreign-account: exit $RC, not 2"
        must_pass table-account '^target-account ok: profile augury-dev is account 783960389328$' \
            env PATH="$FAKE:$PATH" LAYA_ACCOUNT_CHECK_FORCE=1 FAKE_AWS_ACCOUNT=783960389328 just _laya-account-check augury-dev eu-west-1
        [ "$(grep -c '^sts get-caller-identity --profile augury-dev ' "$FAKE_AWS_LOG")" -eq 2 ] || red "the forced account check did not ask the fake sts once per case"
        : > "$FAKE_AWS_LOG"
    }
    row_state_dir() {
        must_fail dot-dot '^REFUSED state-dir: ' just _laya-state-dir ../x
        must_pass ze-kasher-dev '^models/decide$' just _laya-state-dir ze-kasher-dev
        must_pass augury-dev '^models/decide/augury-dev$' just _laya-state-dir augury-dev
    }
    row_endpoint_check() {
        must_fail us-east-for-augury '^REFUSED target-endpoint: ' just _laya-endpoint-check https://aprender-mcp-decide-motivation.us-east.true-mcp.com/mcp augury-dev
        must_pass eu-west '^aprender-mcp-decide-motivation$' just _laya-endpoint-check https://aprender-mcp-decide-motivation.eu-west.augury.pmcp.run/mcp augury-dev
    }
    row_target_selftest() {
        just _laya-pmcp-fixture-config "$W/target-bad.toml"
        sed 's/^aws_profile = "augury-dev"$/aws_profile = "ze-kasher-dev"/' "$W/target-bad.toml" > "$W/target-bad2.toml"
        cmp -s "$W/target-bad.toml" "$W/target-bad2.toml" && { red "could not plant ze-kasher-dev into the fixture's augury-dev target"; return; }
        must_fail augury-names-ze-kasher-dev '^TARGET SELFTEST FAILED' env LAYA_TARGET_SELFTEST_CONFIG="$W/target-bad2.toml" just _laya-target-selftest
        [ "$RC" -eq 1 ] || red "augury-names-ze-kasher-dev: exit $RC, not 1"
        must_pass fixture '^TARGET SELFTEST OK$' just _laya-target-selftest
        grep -qx 'AWS CALLS: 0' "$(logof fixture)" || red "fixture: the target selftest did not report AWS CALLS: 0"
    }
    # D-07 (plan 13-06): the unauthenticated refusal probe, the authenticated probe path and the auth selftest.
    row_auth_refusal_check() {
        local C="$W/authcurl" S="$W/authcurl-list" L="$W/authcurl-calls" U="https://$SRV.eu-west.augury.pmcp.run/mcp"
        mkdir -p "$C"
        just _laya-auth-curl-shim "$C/curl"
        printf '200\n' > "$S"; : > "$L"
        must_fail open-list '^REFUSED auth-open: unauthenticated tools/list answered HTTP 200' \
            env PATH="$C:$PATH" CURL_LIST="$S" CURL_CALLS="$L" CURL_CALL=401 CURL_INIT=200 CURL_PRM=200 LAYA_AUTH_POLL_SLEEP=0 just _laya-auth-refusal-check "$U" 0
        [ "$RC" -eq 1 ] || red "open-list: exit $RC, not 1"
        if grep -q '^POST tools/call$' "$L"; then red "open-list: tools/call was sent while tools/list answered 200"; fi
        printf '401\n' > "$S"; : > "$L"
        must_pass refused '^auth-refusal ok: tools/list 401, tools/call 401 after 1 poll\(s\)' \
            env PATH="$C:$PATH" CURL_LIST="$S" CURL_CALLS="$L" CURL_CALL=401 CURL_INIT=200 CURL_PRM=200 LAYA_AUTH_POLL_SLEEP=0 just _laya-auth-refusal-check "$U" 0
        # Plan 14-10: the optional third argument selects the tools/call payload. train_status sends the one task id that cannot exist;
        # with no third argument the payload is still classify (every decide caller is unchanged); an unknown tool is a config refusal.
        local BD="$W/authcurl-bodies" UT="https://aprender-mcp-laya-train.us-east.true-mcp.com/mcp"
        : > "$BD"; : > "$L"
        must_pass auth-tool '^auth-refusal ok: tools/list 401, tools/call 401 after 1 poll\(s\)' \
            env PATH="$C:$PATH" CURL_LIST="$S" CURL_CALLS="$L" CURL_BODIES="$BD" CURL_CALL=401 CURL_INIT=200 CURL_PRM=200 LAYA_AUTH_POLL_SLEEP=0 just _laya-auth-refusal-check "$UT" 0 train_status
        grep -q '"method":"tools/call","params":{"name":"train_status","arguments":{"task_id":"auth-refusal-probe"}}' "$BD" || red "auth-tool: the tools/call body is not the train_status payload"
        if grep -q 'classify' "$BD"; then red "auth-tool: a classify payload was sent for train_status"; fi
        : > "$BD"; : > "$L"
        must_pass auth-default '^auth-refusal ok: tools/list 401, tools/call 401 after 1 poll\(s\)' \
            env PATH="$C:$PATH" CURL_LIST="$S" CURL_CALLS="$L" CURL_BODIES="$BD" CURL_CALL=401 CURL_INIT=200 CURL_PRM=200 LAYA_AUTH_POLL_SLEEP=0 just _laya-auth-refusal-check "$U" 0
        grep -q '"params":{"name":"classify","arguments":{"texts":\["auth refusal probe"\]}}' "$BD" || red "auth-default: the default payload is no longer classify"
        if grep -q 'train_status' "$BD"; then red "auth-default: a train_status payload was sent with no third argument"; fi
        : > "$L"
        must_fail auth-tool-unknown '^REFUSED auth-check: config the tool' \
            env PATH="$C:$PATH" CURL_LIST="$S" CURL_CALLS="$L" CURL_CALL=401 CURL_INIT=200 CURL_PRM=200 LAYA_AUTH_POLL_SLEEP=0 just _laya-auth-refusal-check "$U" 0 bogus
        [ "$RC" -eq 2 ] || red "auth-tool-unknown: exit $RC, not 2"
        [ ! -s "$L" ] || red "auth-tool-unknown: a request was sent before the tool was refused"
    }
    row_trainer_grant() {
        # D-10 / T-14-71 (plan 14-10): the grant takes no URL, binds the endpoint to the function from the deploy state and its /health,
        # and runs the 401 check itself immediately before the IAM call. A scripted curl (first on PATH) answers every request; the aws
        # recorder must stay unused (a dry run prints the IAM commands and runs none).
        local TS="$W/ops-state-grant" TSE="$W/ops-state-grant-empty" TSB="$W/ops-state-grant-other" C="$W/grantcurl" S="$W/grantcurl-list" L="$W/grantcurl-calls" BD="$W/grantcurl-bodies"
        local EP="https://aprender-mcp-laya-train.us-east.true-mcp.com/mcp" HOK='{"status":"healthy","serverId":"aprender-mcp-laya-train"}'
        local HBAD='{"status":"healthy","serverId":"aprender-mcp-decide-motivation"}'
        mkdir -p "$C"
        just _laya-auth-curl-shim "$C/curl"
        local d
        for d in "$TS" "$TSE" "$TSB"; do
            python3 -I scripts/laya_deploy/laya_trainer_ops.py --write-placeholder-state "$d" > /dev/null || { red "the placeholder state could not be written"; return; }
        done
        python3 -I - "$TS/deploy-dev.json" "$EP" aprender-mcp-laya-train <<'PYG'
    import json, sys
    json.dump({"schema": "laya-trainer-deploy-v1", "env": "dev", "server_id": sys.argv[3], "package": "aprender-mcp-laya-train-lambda", "endpoint": sys.argv[2],
               "deployed_at": "2026-10-29T12:34:56Z"}, open(sys.argv[1], "w"))
    PYG
        cp "$TS/deploy-dev.json" "$TSB/deploy-dev.json"
        sed -i.bak 's#aprender-mcp-laya-train\.us-east#aprender-mcp-decide-motivation.us-east#' "$TSB/deploy-dev.json"; rm -f "$TSB/deploy-dev.json.bak"
        printf '401\n' > "$S"
        local CE=(env PATH="$C:$PATH" CURL_LIST="$S" CURL_CALLS="$L" CURL_BODIES="$BD" CURL_CALL=401 CURL_INIT=200 CURL_PRM=200 LAYA_AUTH_POLL_SLEEP=0)
        : > "$L"; : > "$BD"
        must_fail url-argument '^REFUSED usage: grant takes exactly ENV PROFILE: unknown argument' "${CE[@]}" CURL_HEALTH="$HOK" just laya-trainer-grant dev ze-kasher-dev "$EP" --state-dir "$TS"
        [ "$RC" -eq 2 ] || red "url-argument: exit $RC, not 2"
        [ ! -s "$L" ] || red "url-argument: a request was sent although the URL argument was refused"
        must_fail foreign-profile '^REFUSED profile: ' "${CE[@]}" CURL_HEALTH="$HOK" just laya-trainer-grant dev augury-dev --state-dir "$TS"
        [ "$RC" -eq 2 ] || red "foreign-profile: exit $RC, not 2"
        must_fail no-deploy-state '^REFUSED grant: endpoint not bound to aprender-mcp-laya-train' "${CE[@]}" CURL_HEALTH="$HOK" just laya-trainer-grant dev ze-kasher-dev --state-dir "$TSE"
        [ "$RC" -eq 2 ] || red "no-deploy-state: exit $RC, not 2"
        must_fail foreign-label '^REFUSED grant: endpoint not bound to aprender-mcp-laya-train' "${CE[@]}" CURL_HEALTH="$HOK" just laya-trainer-grant dev ze-kasher-dev --state-dir "$TSB"
        [ "$RC" -eq 2 ] || red "foreign-label: exit $RC, not 2"
        must_fail health-names-other '^REFUSED grant: endpoint not bound to aprender-mcp-laya-train' "${CE[@]}" CURL_HEALTH="$HBAD" just laya-trainer-grant dev ze-kasher-dev --state-dir "$TS"
        [ "$RC" -eq 2 ] || red "health-names-other: exit $RC, not 2"
        if grep -q '^POST tools/' "$L"; then red "an unbound endpoint reached the 401 check"; fi
        printf '200\n' > "$S"; : > "$L"
        must_fail oauth-open '^REFUSED grant: OAuth not confirmed \(tools/list answered 200\)' "${CE[@]}" CURL_HEALTH="$HOK" just laya-trainer-grant dev ze-kasher-dev --state-dir "$TS"
        [ "$RC" -eq 2 ] || red "oauth-open: exit $RC, not 2"
        if grep -q '^POST tools/call$' "$L"; then red "oauth-open: tools/call was sent while tools/list answered 200"; fi
        printf '401\n' > "$S"; : > "$L"
        must_fail unreachable '^REFUSED grant: endpoint unreachable \(transport error; retry\)' "${CE[@]}" CURL_MODE=transport CURL_HEALTH="$HOK" just laya-trainer-grant dev ze-kasher-dev --state-dir "$TS"
        [ "$RC" -eq 2 ] || red "unreachable: exit $RC, not 2"
        : > "$L"; : > "$BD"
        must_pass dry '^AWS CALLS: 0$' "${CE[@]}" CURL_HEALTH="$HOK" just laya-trainer-grant dev ze-kasher-dev --state-dir "$TS"
        local HN AN FN PN
        HN="$(grep -n '^+ curl ' "$(logof dry)" | head -n 1 | cut -d: -f1)"
        AN="$(grep -n "^+ just _laya-auth-refusal-check $EP 0 train_status\$" "$(logof dry)" | head -n 1 | cut -d: -f1)"
        FN="$(grep -n '^+ aws lambda get-function --function-name aprender-mcp-laya-train ' "$(logof dry)" | head -n 1 | cut -d: -f1)"
        PN="$(grep -n '^+ aws iam put-role-policy .*--policy-name aprender-laya-train-dev ' "$(logof dry)" | head -n 1 | cut -d: -f1)"
        { [ -n "$HN" ] && [ -n "$AN" ] && [ -n "$FN" ] && [ -n "$PN" ] && [ "$HN" -lt "$AN" ] && [ "$AN" -lt "$FN" ] && [ "$FN" -lt "$PN" ]; } \
            || red "dry: the health, auth-check, get-function and put-role-policy lines are missing or out of order ($HN $AN $FN $PN)"
        grep -q '"params":{"name":"train_status","arguments":{"task_id":"auth-refusal-probe"}}' "$BD" || red "dry: the 401 check did not send the train_status payload"
        [ "$(wc -l < "$REC" | tr -d ' ')" -eq 0 ] || red "the aws recorder was called"
    }
    row_trainer_call() {
        # D-10 / T-14-41 (plan 14-10): the recipe refuses before any run (no M2M file, a preset token, a foreign profile, another server's
        # endpoint); the client's selftest runs against a local HTTP fake; and one chain case sends the minted bearer, through the recipe and
        # m2m_token.py, to the loopback stub, with the token and secret absent from the recipe's output.
        local EP="https://aprender-mcp-laya-train.us-east.true-mcp.com/mcp" OTHER="https://aprender-mcp-decide-motivation.us-east.true-mcp.com/mcp"
        must_fail no-m2m-file '^REFUSED call: LAYA_M2M_FILE is not set' env -u LAYA_M2M_FILE just laya-trainer-call ze-kasher-dev "$EP" list-tools
        [ "$RC" -eq 2 ] || red "no-m2m-file: exit $RC, not 2"
        must_fail token-preset '^REFUSED call: APRENDER_DECIDE_PROBE_TOKEN is already set' env LAYA_M2M_FILE="$W/none.json" APRENDER_DECIDE_PROBE_TOKEN=preset just laya-trainer-call ze-kasher-dev "$EP" list-tools
        [ "$RC" -eq 2 ] || red "token-preset: exit $RC, not 2"
        must_fail foreign-profile '^REFUSED call: profile augury-dev is not allowed' env LAYA_M2M_FILE="$W/none.json" just laya-trainer-call augury-dev "$EP" list-tools
        [ "$RC" -eq 2 ] || red "foreign-profile: exit $RC, not 2"
        must_fail other-server '^REFUSED call: the endpoint names server aprender-mcp-decide-motivation' env LAYA_M2M_FILE="$W/none.json" just laya-trainer-call ze-kasher-dev "$OTHER" list-tools
        [ "$RC" -eq 2 ] || red "other-server: exit $RC, not 2"
        must_fail bearer-arg '^REFUSED call: an argument starts with --bearer' env LAYA_M2M_FILE="$W/none.json" just laya-trainer-call ze-kasher-dev "$EP" list-tools --bearer=x
        [ "$RC" -eq 2 ] || red "bearer-arg: exit $RC, not 2"
        must_pass selftest '^TRAIN CALL SELFTEST OK$' python3 -I scripts/laya_deploy/train_call.py --selftest
        # ---- chain: file -> m2m_token.py -> train_call.py -> the loopback stub (its /mcp answers 401 and logs whether the bearer matched)
        local SD="$W/call-stub" STUBPID=""
        mkdir -p "$SD"
        python3 -I - "$SD/client.json" <<'PYC'
    import json, os, secrets, sys
    fd = os.open(sys.argv[1], os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as fh:
        json.dump({"client_id": "selftest-client-" + secrets.token_hex(4), "client_secret": "selftest-secret-" + secrets.token_urlsafe(18)}, fh)
    PYC
        python3 -I scripts/laya_deploy/m2m_token.py --stub-server "$SD" > "$W/call-stub.log" 2>&1 &
        STUBPID=$!
        local _i
        for _i in $(seq 1 100); do
            [ -s "$SD/port" ] && break
            sleep 0.1
        done
        if [ -s "$SD/port" ]; then
            local PORT TOK SEC LINE
            PORT="$(cat "$SD/port")"
            python3 -I - "$SD/client.json" "$PORT" "$W/call-m2m-client.json" <<'PYD'
    import json, os, sys
    c = json.load(open(sys.argv[1]))
    fd = os.open(sys.argv[3], os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w") as fh:
        json.dump({**c, "token_url": f"http://127.0.0.1:{int(sys.argv[2])}/oauth2/token"}, fh)
    PYD
            run chain env -u LAYA_M2M_TOKEN_HOST -u APRENDER_DECIDE_PROBE_TOKEN LAYA_M2M_FILE="$W/call-m2m-client.json" just laya-trainer-call ze-kasher-dev "http://127.0.0.1:$PORT/mcp" list-tools
            touch "$SD/stop"
            wait "$STUBPID" || true
            TOK="$(cat "$SD/token" 2> /dev/null || true)"
            SEC="$(python3 -I -c 'import json, sys; print(json.load(open(sys.argv[1]))["client_secret"])' "$SD/client.json")"
            LINE="$(head -n 1 "$SD/mcp.log" 2> /dev/null || true)"
            if [ -z "$TOK" ] || grep -qF -- "$TOK" "$(logof chain)"; then red "chain: the token is absent from the stub or present in the recipe output"; fi
            if grep -qF -- "$SEC" "$(logof chain)"; then red "chain: the client secret is present in the recipe output"; fi
            [ "$LINE" = "authorization_bearer_matches=true" ] || red "chain: the stub saw '${LINE:-<none>}', not authorization_bearer_matches=true"
            grep -q '^m2m-token ok: host 127.0.0.1 HTTP 200 ' "$(logof chain)" || red "chain: no m2m-token ok line"
            grep -q '"ok":false,"error":{"http_status":401' "$(logof chain)" || red "chain: the client did not report the stub's 401 as one error line"
            [ "$RC" -eq 1 ] || red "chain: exit $RC, not 1 (a 401 is exit 1)"
            echo "  chain: the minted bearer reached the stub (authorization_bearer_matches=true); the 401 came back as one error line; token and secret absent from the output"
        else
            kill "$STUBPID" 2> /dev/null || true
            red "chain: the stub did not write its port within 10 s (log $W/call-stub.log)"
        fi
    }
    row_probe_auth() {
        must_fail no-m2m-file '^REFUSED probe-auth: LAYA_M2M_FILE is not set' \
            env -u LAYA_M2M_FILE just laya-probe-auth augury-dev "https://$SRV.eu-west.augury.pmcp.run/mcp" --apr "$W/none.apr"
        [ "$RC" -eq 2 ] || red "no-m2m-file: exit $RC, not 2"
        must_pass chain '^AUTH SELFTEST OK$' just _laya-auth-selftest
        grep -q '^CHAIN bearer reached the wire (authorization_bearer_matches=true)' "$(logof chain)" || red "chain: the auth selftest printed no CHAIN line"
    }
    row_auth_selftest() {
        must_fail stub-mismatch '^AUTH SELFTEST FAILED' env LAYA_AUTH_SELFTEST_STUB_MISMATCH=1 just _laya-auth-selftest
        [ "$RC" -eq 1 ] || red "stub-mismatch: exit $RC, not 1"
        must_pass fixture '^AUTH SELFTEST OK$' just _laya-auth-selftest
        grep -qx 'AWS CALLS: 0' "$(logof fixture)" || red "fixture: the auth selftest did not report AWS CALLS: 0"
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
    case "$COLD" in
        variant=*) refuse args "variant= must be the last positional parameter (got it as cold)" ;;
    esac
    case "$LONG" in
        variant=*) refuse args "variant= must be the last positional parameter (got it as long)" ;;
    esac
    case "$CONC" in
        variant=*) refuse args "variant= must be the last positional parameter (got it as conc)" ;;
    esac
    case "$BUDGET" in
        variant=*) refuse args "variant= must be the last positional parameter (got it as budget)" ;;
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
    [ "$(jget_selftest "$T/up2b/aws.json" name)" = "aprender-llm-endpoint-dev-2b" ] || x=0
    [ "$(jget_selftest "$T/up2b/aws.json" variant)" = "2b" ] || x=0
    [ "$(jget_selftest "$T/up2b/aws.json" suffix)" = "dev-2b" ] || x=0
    [ "$(jget_selftest "$T/up2b/aws.json" ecr_repo_name)" = "aprender-llm-endpoint-dev-2b" ] || x=0
    case "$(jget_selftest "$T/up2b/aws.json" bucket)" in
        *-dev-2b) ;;
        *) x=0 ;;
    esac
    grep -qE 'repository-names aprender-llm-endpoint-dev-2b( |$)' "$T/a-v3.log" || x=0
    grep -q 'variant=2b' "$T/a-v3.log" || x=0
    grep -qE 'aprender-llm-endpoint-dev( |$)' "$T/a-v3.log" && x=0
    row V3 "up variant=2b: dev-2b names in aws.json and in the calls, tagged variant=2b, no default-name call" "$T/c-v3.log" zero '^UP OK aprender-llm-endpoint-dev-2b' "$x"
    # V4: the 2b ids and a 2b pin pass the offline deploy ladder
    cp "$IDS/image-pushed.json" "$IDS/image-local.json" "$T/up2b/"
    sed -e 's/^GGUF_RUNG=.*/GGUF_RUNG=qwen35-2b-q4km/' -e 's/^GGUF_OBJECT=.*/GGUF_OBJECT=Qwen3.5-2B-Q4_K_M.gguf/' "$PIN" > "$T/pin-2b.env"
    : > "$T/a-v4.log"
    run_just "$T/c-v4.log" LLM_ENDPOINT_IDS_DIR="$T/up2b" LLM_ENDPOINT_PIN_FILE="$T/pin-2b.env" SHIM_MODE=refuse SHIM_LOG="$T/a-v4.log" DRY_RUN=1 \
        just llm-endpoint-deploy dev ze-kasher-dev variant=2b
    row V4 "deploy variant=2b DRY_RUN with the up2b ids and a 2b pin -> DRY-RUN OK aprender-llm-endpoint-dev-2b, zero aws calls" "$T/c-v4.log" zero \
        '^DRY-RUN OK aprender-llm-endpoint-dev-2b ' "$([ "$(lines "$T/a-v4.log")" -eq 0 ] && echo 1 || echo 0)"
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
    row V7d "up variant=2b over its own 2b record -> UP OK (the check accepts a matching record)" "$T/c-v7d.log" zero '^UP OK aprender-llm-endpoint-dev-2b' \
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
        */aprender-llm-endpoint-dev-2b) ;;
        *) x=0 ;;
    esac
    cmp -s "$IDS/image-pushed.json" "$T/v8-default-pushed.before" || x=0
    grep -qE '^push .*/aprender-llm-endpoint-dev-2b:' "$T/a-v8.log" || x=0
    row V8 "image 1 dev variant=2b -> pushed to the dev-2b repository, recorded in the 2b ids, default record untouched" "$T/c-v8.log" zero '^PUSHED .*/aprender-llm-endpoint-dev-2b:' "$x"
    # V9-V13: the remaining lifecycle recipes take the variant as their LAST positional (just binds in order,
    # so `dev ze-kasher-dev variant=2b`); teardown derives its function name from the library.
    # V9: teardown variant=2b removes only dev-2b-named resources
    rm -rf "${T:?}/td2b"
    mkdir -p "$T/td2b"
    cp "$T/up2b/aws.json" "$T/up2b/image-local.json" "$T/up2b/image-pushed.json" "$T/td2b/"
    printf '{"function":"aprender-llm-endpoint-dev-2b","name":"aprender-llm-endpoint-dev-2b","image_digest":"%s"}\n' "$DIGEST" > "$T/td2b/function.json"
    cp -R "$T/td2b" "$T/td2b-keep"
    : > "$T/a-v9.log"
    run_just "$T/c-v9.log" LLM_ENDPOINT_IDS_DIR="$T/td2b" SHIM_LOG="$T/a-v9.log" SHIM_CONCURRENCY=0 just llm-endpoint-teardown dev ze-kasher-dev variant=2b
    x=1
    grep -qE -- '--function-name aprender-llm-endpoint-dev-2b( |$)' "$T/a-v9.log" || x=0
    grep -qE -- '--role-name aprender-llm-endpoint-dev-2b( |$)' "$T/a-v9.log" || x=0
    grep -qE -- '--log-group-name /aws/lambda/aprender-llm-endpoint-dev-2b( |$)' "$T/a-v9.log" || x=0
    grep -qE -- "--bucket aprender-llm-endpoint-weights-$ACCT-dev-2b( |\$)" "$T/a-v9.log" || x=0
    grep -qE 'aprender-llm-endpoint-dev( |$)' "$T/a-v9.log" && x=0
    row V9 "teardown variant=2b: only dev-2b names reach the shim (function, role, log group, bucket), none of the default name" "$T/c-v9.log" zero '^TEARDOWN OK aprender-llm-endpoint-dev-2b' "$x"
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
    grep -qE 'iam create-role .*--role-name aprender-llm-endpoint-dev-2b-codebuild( |$)' "$T/a-v12.log" || x=0
    grep -qE 'iam create-role .*Key=variant,Value=2b' "$T/a-v12.log" || x=0
    grep -qE 'codebuild create-project ' "$T/a-v12.log" || x=0
    grep -qE 'codebuild start-build .*--project-name aprender-llm-endpoint-weights-dev-2b( |$)' "$T/a-v12.log" || x=0
    [ "$(jget_selftest "$T/w2b/weights.json" codebuild_role)" = "aprender-llm-endpoint-dev-2b-codebuild" ] || x=0
    [ "$(jget_selftest "$T/w2b/weights.json" codebuild_project)" = "aprender-llm-endpoint-weights-dev-2b" ] || x=0
    row V12 "weights codebuild variant=2b: role ...-dev-2b-codebuild (tagged variant=2b) and project ...-weights-dev-2b, both recorded" "$T/c-v12.log" zero '^WEIGHTS OK' "$x"
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
    write_weights "$W" aprender-llm-endpoint-weights-dev aprender-llm-endpoint-dev-2b
    : > "$T/a-v15.log"
    run_just "$T/c-v15.log" LLM_ENDPOINT_IDS_DIR="$W" SHIM_LOG="$T/a-v15.log" SHIM_CONCURRENCY=0 just llm-endpoint-teardown
    row V15 "teardown, default variant, a weights.json naming aprender-llm-endpoint-dev-2b (the 2b function's role) -> REFUSED ids, no delete call" "$T/c-v15.log" nonzero '^REFUSED ids:.*does not belong to' \
        "$(if grep -qE ' delete-' "$T/a-v15.log"; then echo 0; else echo 1; fi)"
    write_weights "$T/td2b-keep" aprender-llm-endpoint-weights-dev-2b aprender-llm-endpoint-dev-2b-codebuild
    : > "$T/a-v16.log"
    run_just "$T/c-v16.log" LLM_ENDPOINT_IDS_DIR="$T/td2b-keep" SHIM_LOG="$T/a-v16.log" SHIM_CONCURRENCY=0 just llm-endpoint-teardown dev ze-kasher-dev variant=2b
    x=1
    grep -qE 'codebuild delete-project .*--name aprender-llm-endpoint-weights-dev-2b( |$)' "$T/a-v16.log" || x=0
    grep -qE 'iam delete-role .*--role-name aprender-llm-endpoint-dev-2b-codebuild( |$)' "$T/a-v16.log" || x=0
    row V16 "teardown variant=2b with its own derived CodeBuild names -> TEARDOWN OK, deletes those" "$T/c-v16.log" zero '^TEARDOWN OK aprender-llm-endpoint-dev-2b' "$x"
    # V17: readback reads and records, it never writes, and it never records a value of an environment variable
    rm -rf "${T:?}/rb2b"
    mkdir -p "$T/rb2b"
    cp "$T/up2b/aws.json" "$T/rb2b/"
    python3 -I -c 'import json, sys; json.dump({"function": sys.argv[2], "name": sys.argv[2], "variant": "2b", "env": "dev"}, open(sys.argv[1], "w"))' "$T/rb2b/function.json" aprender-llm-endpoint-dev-2b
    : > "$T/a-v17.log"
    run_just "$T/c-v17.log" LLM_ENDPOINT_IDS_DIR="$T/rb2b" SHIM_LOG="$T/a-v17.log" SHIM_RB_NAME=aprender-llm-endpoint-dev-2b SHIM_URL_ABSENT=1 SHIM_CONCURRENCY=1 \
        APR_API_KEY_HASH="$HASH64" just llm-endpoint-readback dev ze-kasher-dev variant=2b
    x=1
    [ "$(find "$T/rb2b" -name 'readback-*.json' | wc -l | tr -d ' ')" -eq 1 ] || x=0
    RBF="$(find "$T/rb2b" -name 'readback-*.json' | head -1)"
    [ "$(jget_selftest "$RBF" function_url)" = absent ] || x=0
    [ "$(jget_selftest "$RBF" function_name)" = aprender-llm-endpoint-dev-2b ] || x=0
    grep -q "$HASH64" "$RBF" && x=0
    grep -q "$HASH64" "$T/c-v17.log" && x=0
    grep -q 'lambda get-function ' "$T/a-v17.log" || x=0
    grep -q 'lambda get-function-url-config ' "$T/a-v17.log" || x=0
    grep -qE '^[a-z0-9-]+ (create|update|put|delete|add|remove|tag)' "$T/a-v17.log" && x=0
    row V17 "readback variant=2b: one readback-*.json with function_url absent and no env value, read calls only (no write verb in the shim log)" "$T/c-v17.log" zero '^READBACK OK aprender-llm-endpoint-dev-2b ' "$x"
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
        '^DRY-RUN OK aprender-llm-endpoint-dev-2b .*gguf aaf42c8b7c3cab2bf3d69c355048d4a0ee9973d48f16c731c0520ee914699223' "$x"
    # V19: readback records only that a Function URL is present, never its host (the record gets committed)
    : > "$T/a-v19.log"
    rm -f "$T"/rb2b/readback-*.json
    run_just "$T/c-v19.log" LLM_ENDPOINT_IDS_DIR="$T/rb2b" SHIM_LOG="$T/a-v19.log" SHIM_RB_NAME=aprender-llm-endpoint-dev-2b SHIM_URL=https://secrethost9z.lambda-url.us-east-1.on.aws/ SHIM_CONCURRENCY=1 \
        just llm-endpoint-readback dev ze-kasher-dev variant=2b
    x=1
    RBF="$(find "$T/rb2b" -name 'readback-*.json' | head -1)"
    [ "$(jget_selftest "$RBF" function_url)" = present ] || x=0
    grep -q 'secrethost9z' "$RBF" && x=0
    grep -q 'secrethost9z' "$T/c-v19.log" && x=0
    row V19 "readback with a URL config: function_url is 'present', the host appears in neither the record nor stdout" "$T/c-v19.log" zero '^READBACK OK aprender-llm-endpoint-dev-2b ' "$x"
    # V20-V22: variant= in the wrong positional slot is refused, never silently bound to the default name
    : > "$T/a-v20.log"
    run_just "$T/c-v20.log" LLM_ENDPOINT_IDS_DIR="$IDS" SHIM_LOG="$T/a-v20.log" SHIM_CONCURRENCY=0 just llm-endpoint-teardown env=dev variant=iso
    row V20 "teardown env=dev variant=iso (variant lands in the profile slot) -> REFUSED args, nothing reached the shim" "$T/c-v20.log" nonzero '^REFUSED args: variant= must be the last positional' \
        "$([ "$(lines "$T/a-v20.log")" -eq 0 ] && echo 1 || echo 0)"
    : > "$T/a-v21.log"
    run_just "$T/c-v21.log" LLM_ENDPOINT_IDS_DIR="$T/rb2b" SHIM_LOG="$T/a-v21.log" APR_PROBE_KEY=k just llm-endpoint-measure dev ze-kasher-dev variant=2b
    row V21 "measure dev ze-kasher-dev variant=2b (variant lands in the label slot) -> REFUSED args, nothing reached the shim" "$T/c-v21.log" nonzero '^REFUSED args: variant= must be the last positional' \
        "$([ "$(lines "$T/a-v21.log")" -eq 0 ] && echo 1 || echo 0)"
    : > "$T/a-v24.log"
    run_just "$T/c-v24.log" LLM_ENDPOINT_IDS_DIR="$T/rb2b" SHIM_LOG="$T/a-v24.log" APR_PROBE_KEY=k just llm-endpoint-measure dev ze-kasher-dev smoke variant=2b
    row V24 "measure dev ze-kasher-dev smoke variant=2b (variant lands in the cold slot) -> REFUSED args, nothing reached the shim" "$T/c-v24.log" nonzero '^REFUSED args: variant= must be the last positional.*cold' \
        "$([ "$(lines "$T/a-v24.log")" -eq 0 ] && echo 1 || echo 0)"
    : > "$T/a-v25.log"
    run_just "$T/c-v25.log" LLM_ENDPOINT_IDS_DIR="$T/rb2b" SHIM_LOG="$T/a-v25.log" APR_PROBE_KEY=k just llm-endpoint-measure dev ze-kasher-dev smoke 1 variant=2b
    row V25 "measure dev ze-kasher-dev smoke 1 variant=2b (variant lands in the long slot) -> REFUSED args, nothing reached the shim" "$T/c-v25.log" nonzero '^REFUSED args: variant= must be the last positional.*long' \
        "$([ "$(lines "$T/a-v25.log")" -eq 0 ] && echo 1 || echo 0)"
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

# ---------------------------------------------------------------------------
# Phase 11: embed endpoint
#
# The Graviton gate (plan 11-01) and, later, the endpoint itself. Every recipe
# here either runs offline or refuses before it touches an AWS account; the
# owner-run measurement is `embed-probe-run`, and plan 11-04 drives it.
# ---------------------------------------------------------------------------

embed_probe_dir := "deploy/embed-endpoint/graviton-probe"
embed_probe_asset := "deploy-extensions/assets/embed-probe"
embed_probe_zip := "deploy-extensions/assets/embed-probe.zip"
embed_ids := "deploy/embed-endpoint/.ids"

# Build the Graviton probe as an arm64 Lambda asset, with its deterministic zip and record.
#
# Offline: no AWS contact, installs nothing. The asset is bootstrap plus the pinned
# encoder subset (never model.safetensors). The rename to `bootstrap` happens here
# only, as in build-trainer-asset. The probe's lock is a copy of the root Cargo.lock
# (gitignored), so a version the workspace does not link cannot appear silently.

embed-probe-asset:
    #!/usr/bin/env bash
    set -euo pipefail
    command -v cargo-zigbuild >/dev/null || {
        echo "ERROR: cargo-zigbuild is not installed (this recipe installs nothing)"
        echo "       install it yourself: cargo install cargo-zigbuild"
        exit 1
    }
    for f in full_model.apr tokenizer.json config.json modules.json full_manifest.json 1_Pooling/config.json; do
        test -f "{{minilm_dir}}/$f" || {
            echo "ERROR: no pinned encoder file {{minilm_dir}}/$f"
            echo "       set APRENDER_MINILM_DIR to the pinned all-MiniLM-L6-v2 directory"
            exit 1
        }
    done
    P="{{embed_probe_dir}}"
    test -f "$P/Cargo.lock" || cp Cargo.lock "$P/Cargo.lock"
    cargo zigbuild --release --manifest-path "$P/Cargo.toml" --target {{target}}.2.34
    TD=$(cargo metadata --manifest-path "$P/Cargo.toml" --no-deps --format-version 1 \
        | python3 -I -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')
    BIN="$TD/{{target}}/release/aprender-embed-graviton-probe"
    test -x "$BIN" || { echo "ERROR: no probe binary at $BIN"; exit 1; }
    A="{{embed_probe_asset}}"
    rm -rf "$A"
    mkdir -p "$A/model/1_Pooling" "{{embed_ids}}"
    cp "$BIN" "$A/bootstrap"
    chmod 0755 "$A/bootstrap"
    for f in full_model.apr tokenizer.json config.json modules.json full_manifest.json; do
        cp "{{minilm_dir}}/$f" "$A/model/$f"
    done
    cp "{{minilm_dir}}/1_Pooling/config.json" "$A/model/1_Pooling/config.json"
    python3 -I scripts/embed_endpoint/glibc_floor.py 34 "$A/bootstrap"
    file "$A/bootstrap" | grep -q 'ARM aarch64' || { echo "ERROR: bootstrap is not aarch64"; exit 1; }
    du -sh "$A" | awk '{print "  probe asset: " $1 " (Lambda unzipped limit 250 MB)"}'
    python3 -I scripts/embed_endpoint/package_zip.py build "$A" "{{embed_probe_zip}}" \
        --record "{{embed_ids}}/package-embed-probe.json"
    ls -lh "{{embed_probe_zip}}" | awk '{print "  probe package: " $5 " " $9}'

# The Graviton gate's owner-run recipes. Each one begins with the same refuse ladder:
# `refuse <check> <message>` exits 2 before any AWS call. The ladder refuses a bad
# environment, a mis-positioned argument, a memory outside 128 to 10240, a missing
# asset and a package zip that does not verify against its record. DRY_RUN=1 synthesizes
# and scans and nothing else; PREFLIGHT=1 adds the identity check and cdk diff.

# Validate the probe stack offline: synthesize it, scan the template, print its function.
embed-probe-synth env="dev" memory="10240":
    #!/usr/bin/env bash
    set -euo pipefail
    refuse() { echo "REFUSED $1: $2" >&2; exit 2; }
    ENV_NAME="{{ trim_start_match(env, "env=") }}"
    case "$ENV_NAME" in dev|prod) ;; *) refuse env "environment must be dev or prod, got '$ENV_NAME'" ;; esac
    MEM="{{ trim_start_match(memory, "memory=") }}"
    case "$MEM" in ''|*[!0-9]*) refuse memory "memory must be a whole number of MB, got '$MEM'" ;; esac
    if [ "$MEM" -lt 128 ] || [ "$MEM" -gt 10240 ]; then refuse memory "memory must be 128 to 10240 MB, got $MEM"; fi
    ASSET="${EMBED_PROBE_ASSET_DIR:-deploy-extensions/assets/embed-probe}"
    for f in bootstrap model/full_model.apr; do
        test -f "$ASSET/$f" || refuse asset "missing $ASSET/$f (build it: just embed-probe-asset)"
    done
    (cd deploy-extensions && npx cdk synth --context=env="$ENV_NAME" --context=stack=embed-probe --context=embedMemoryMb="$MEM" --quiet)
    T="deploy-extensions/cdk.out/aprender-embed-probe-$ENV_NAME.template.json"
    python3 -I scripts/embed_endpoint/ingress_scan.py "$T"
    python3 -I - "$T" <<'PYSYNTH'
    import json, sys
    t = json.load(open(sys.argv[1]))
    fns = [r["Properties"] for r in t["Resources"].values() if r["Type"] == "AWS::Lambda::Function"]
    p = fns[0]
    print(f"TEMPLATE function {p['FunctionName']} memory {p['MemorySize']} MB arch {p['Architectures'][0]} timeout {p['Timeout']} s")
    PYSYNTH
    echo "  synth OK for env=$ENV_NAME (no account contact)"

# Show what a probe deploy would change, against the real account (owner-run).
embed-probe-diff env="dev" profile="ze-kasher-dev" memory="10240":
    #!/usr/bin/env bash
    set -euo pipefail
    refuse() { echo "REFUSED $1: $2" >&2; exit 2; }
    ENV_NAME="{{ trim_start_match(env, "env=") }}"
    case "$ENV_NAME" in dev|prod) ;; *) refuse env "environment must be dev or prod, got '$ENV_NAME'" ;; esac
    PROFILE="{{profile}}"
    case "$PROFILE" in env=*|label=*|memory=*) refuse position "profile argument '$PROFILE' is mis-positioned" ;; esac
    MEM="{{ trim_start_match(memory, "memory=") }}"
    case "$MEM" in ''|*[!0-9]*) refuse memory "memory must be a whole number of MB, got '$MEM'" ;; esac
    if [ "$MEM" -lt 128 ] || [ "$MEM" -gt 10240 ]; then refuse memory "memory must be 128 to 10240 MB, got $MEM"; fi
    ASSET="${EMBED_PROBE_ASSET_DIR:-deploy-extensions/assets/embed-probe}"
    for f in bootstrap model/full_model.apr; do
        test -f "$ASSET/$f" || refuse asset "missing $ASSET/$f (build it: just embed-probe-asset)"
    done
    cd deploy-extensions && npx cdk diff --context=env="$ENV_NAME" --context=stack=embed-probe --context=embedMemoryMb="$MEM" --profile "$PROFILE"

# Deploy the probe stack, one function, no ingress. DRY_RUN=1 and PREFLIGHT=1 never touch
# the account except PREFLIGHT's identity check, which prints nothing about the account.
embed-probe-deploy env="dev" profile="ze-kasher-dev" memory="10240":
    #!/usr/bin/env bash
    set -euo pipefail
    refuse() { echo "REFUSED $1: $2" >&2; exit 2; }
    ENV_NAME="{{ trim_start_match(env, "env=") }}"
    case "$ENV_NAME" in dev|prod) ;; *) refuse env "environment must be dev or prod, got '$ENV_NAME'" ;; esac
    PROFILE="{{profile}}"
    case "$PROFILE" in env=*|label=*|memory=*) refuse position "profile argument '$PROFILE' is mis-positioned" ;; esac
    MEM="{{ trim_start_match(memory, "memory=") }}"
    case "$MEM" in ''|*[!0-9]*) refuse memory "memory must be a whole number of MB, got '$MEM'" ;; esac
    if [ "$MEM" -lt 128 ] || [ "$MEM" -gt 10240 ]; then refuse memory "memory must be 128 to 10240 MB, got $MEM"; fi
    ASSET="${EMBED_PROBE_ASSET_DIR:-deploy-extensions/assets/embed-probe}"
    ZIP="${EMBED_PROBE_ZIP:-deploy-extensions/assets/embed-probe.zip}"
    RECORD="${EMBED_PROBE_RECORD:-deploy/embed-endpoint/.ids/package-embed-probe.json}"
    for f in bootstrap model/full_model.apr; do
        test -f "$ASSET/$f" || refuse asset "missing $ASSET/$f (build it: just embed-probe-asset)"
    done
    test -f "$ZIP" && test -f "$RECORD" || refuse package "missing the package zip or its record (build it: just embed-probe-asset)"
    python3 -I scripts/embed_endpoint/package_zip.py verify "$ZIP" "$RECORD" >/dev/null 2>&1 \
        || refuse package "the zip does not match its record (rebuild: just embed-probe-asset)"
    if [ "${DRY_RUN:-0}" = "1" ] && [ "${PREFLIGHT:-0}" = "1" ]; then refuse mode "DRY_RUN and PREFLIGHT are exclusive"; fi
    CTX="--context=env=$ENV_NAME --context=stack=embed-probe --context=embedMemoryMb=$MEM"
    if [ "${DRY_RUN:-0}" = "1" ]; then
        (cd deploy-extensions && npx cdk synth $CTX --quiet)
        python3 -I scripts/embed_endpoint/ingress_scan.py "deploy-extensions/cdk.out/aprender-embed-probe-$ENV_NAME.template.json"
        echo "DRY-RUN OK aprender-embed-probe-$ENV_NAME (offline; nothing deployed; AWS CALLS 0)"
        exit 0
    fi
    if [ "${PREFLIGHT:-0}" = "1" ]; then
        (cd deploy-extensions && npx cdk synth $CTX --quiet)
        python3 -I scripts/embed_endpoint/ingress_scan.py "deploy-extensions/cdk.out/aprender-embed-probe-$ENV_NAME.template.json"
        aws sts get-caller-identity --profile "$PROFILE" --output json >/dev/null
        (cd deploy-extensions && npx cdk diff $CTX --profile "$PROFILE")
        echo "PREFLIGHT OK aprender-embed-probe-$ENV_NAME (identity checked; nothing deployed)"
        exit 0
    fi
    (cd deploy-extensions && npx cdk deploy $CTX --profile "$PROFILE" --require-approval never "aprender-embed-probe-$ENV_NAME")

# The owner-run measurement: refuses on the live account, runs the fixed sample plan
# (15 nominal invokes), and writes the RAW record under deploy/embed-endpoint/.ids/.
embed-probe-run env="dev" profile="ze-kasher-dev" label="baseline" memory="10240":
    #!/usr/bin/env bash
    set -euo pipefail
    refuse() { echo "REFUSED $1: $2" >&2; exit 2; }
    ENV_NAME="{{ trim_start_match(env, "env=") }}"
    case "$ENV_NAME" in dev|prod) ;; *) refuse env "environment must be dev or prod, got '$ENV_NAME'" ;; esac
    PROFILE="{{profile}}"
    case "$PROFILE" in env=*|label=*|memory=*) refuse position "profile argument '$PROFILE' is mis-positioned" ;; esac
    LABEL="{{label}}"
    case "$LABEL" in env=*|profile=*|memory=*) refuse position "label argument '$LABEL' is mis-positioned" ;; esac
    case "$LABEL" in *[!A-Za-z0-9_-]*|'') refuse label "label must be letters, digits, '-' or '_'" ;; esac
    MEM="{{ trim_start_match(memory, "memory=") }}"
    case "$MEM" in ''|*[!0-9]*) refuse memory "memory must be a whole number of MB, got '$MEM'" ;; esac
    if [ "$MEM" -lt 128 ] || [ "$MEM" -gt 10240 ]; then refuse memory "memory must be 128 to 10240 MB, got $MEM"; fi
    ASSET="${EMBED_PROBE_ASSET_DIR:-deploy-extensions/assets/embed-probe}"
    RECORD="${EMBED_PROBE_RECORD:-deploy/embed-endpoint/.ids/package-embed-probe.json}"
    ZIP="${EMBED_PROBE_ZIP:-deploy-extensions/assets/embed-probe.zip}"
    for f in bootstrap model/full_model.apr; do
        test -f "$ASSET/$f" || refuse asset "missing $ASSET/$f (build it: just embed-probe-asset)"
    done
    test -f "$ZIP" && test -f "$RECORD" || refuse package "missing the package zip or its record"
    python3 -I scripts/embed_endpoint/package_zip.py verify "$ZIP" "$RECORD" >/dev/null 2>&1 \
        || refuse package "the zip does not match its record (rebuild: just embed-probe-asset)"
    python3 -I scripts/embed_endpoint/graviton_run.py run --env "$ENV_NAME" --profile "$PROFILE" \
        --label "$LABEL" --memory "$MEM" --package-record "$RECORD" --out-dir deploy/embed-endpoint/.ids

# Tear the probe stack down (owner-run). The probe's log group is destroyed with it.
embed-probe-destroy env="dev" profile="ze-kasher-dev":
    #!/usr/bin/env bash
    set -euo pipefail
    refuse() { echo "REFUSED $1: $2" >&2; exit 2; }
    ENV_NAME="{{ trim_start_match(env, "env=") }}"
    case "$ENV_NAME" in dev|prod) ;; *) refuse env "environment must be dev or prod, got '$ENV_NAME'" ;; esac
    PROFILE="{{profile}}"
    case "$PROFILE" in env=*|label=*|memory=*) refuse position "profile argument '$PROFILE' is mis-positioned" ;; esac
    ASSET="${EMBED_PROBE_ASSET_DIR:-deploy-extensions/assets/embed-probe}"
    for f in bootstrap model/full_model.apr; do
        test -f "$ASSET/$f" || refuse asset "missing $ASSET/$f (the stack cannot be synthesized without it)"
    done
    (cd deploy-extensions && npx cdk destroy --context=env="$ENV_NAME" --context=stack=embed-probe --profile "$PROFILE" --force "aprender-embed-probe-$ENV_NAME")

# The offline gate. Scrubs AWS credentials, puts an aws shim on PATH that records and
# refuses every call, and an npx shim that passes through `cdk synth` (and the ts-node
# app runner cdk.json names, which cdk synth itself spawns) and records and refuses
# everything else. Then the refusal cases, the DRY_RUN path, the scanner, the package
# and normalizer selftests, and a negative control that proves the recorder can fire.
embed-gate-selftest:
    #!/usr/bin/env bash
    set -euo pipefail
    unset AWS_PROFILE AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN
    export AWS_EC2_METADATA_DISABLED=true
    REAL_NPX="$(command -v npx)"
    WORK="$(mktemp -d)"
    trap 'rm -rf "$WORK"' EXIT
    mkdir -p "$WORK/shim"
    cat > "$WORK/shim/aws" <<'SHIMAWS'
    #!/usr/bin/env bash
    echo "aws $*" >> "$EMBED_GATE_RECORDER"
    echo "REFUSED aws: the offline gate makes no AWS call" >&2
    exit 97
    SHIMAWS
    cat > "$WORK/shim/npx" <<'SHIMNPX'
    #!/usr/bin/env bash
    if [ "${1:-}" = "cdk" ] && [ "${2:-}" = "synth" ]; then exec "$EMBED_GATE_REAL_NPX" "$@"; fi
    if [ "${1:-}" = "ts-node" ]; then exec "$EMBED_GATE_REAL_NPX" "$@"; fi
    echo "npx $*" >> "$EMBED_GATE_NPX_REFUSED"
    echo "REFUSED npx: only cdk synth passes the offline gate" >&2
    exit 97
    SHIMNPX
    chmod +x "$WORK/shim/aws" "$WORK/shim/npx"
    export EMBED_GATE_RECORDER="$WORK/recorder" EMBED_GATE_NPX_REFUSED="$WORK/npx-refused" EMBED_GATE_REAL_NPX="$REAL_NPX"
    : > "$EMBED_GATE_RECORDER"; : > "$EMBED_GATE_NPX_REFUSED"
    export PATH="$WORK/shim:$PATH"

    CASES=0
    expect_refusal() {
        local name="$1" want="$2"; shift 2
        local out rc
        set +e; out="$("$@" 2>&1)"; rc=$?; set -e
        if [ "$rc" -eq 2 ] && printf '%s' "$out" | grep -q "REFUSED $want"; then
            echo "  refused as expected: $name"; CASES=$((CASES + 1))
        else
            echo "GATE FAILED: $name did not refuse (rc=$rc)"; printf '%s\n' "$out" | tail -n 4; exit 1
        fi
    }
    expect_refusal "unknown environment" env just embed-probe-deploy staging
    expect_refusal "mis-positioned profile" position just embed-probe-deploy dev env=dev
    expect_refusal "mis-positioned run label" position just embed-probe-run dev ze-kasher-dev profile=x 10240
    expect_refusal "memory out of range" memory just embed-probe-deploy dev ze-kasher-dev 999999
    expect_refusal "missing asset through the seam" asset env EMBED_PROBE_ASSET_DIR="$WORK/no-such-asset" just embed-probe-deploy dev
    mkdir -p "$WORK/tampered"
    cp deploy-extensions/assets/embed-probe.zip "$WORK/tampered/embed-probe.zip"
    printf 'x' >> "$WORK/tampered/embed-probe.zip"
    expect_refusal "zip that does not match its record" package env EMBED_PROBE_ZIP="$WORK/tampered/embed-probe.zip" just embed-probe-deploy dev
    python3 -I - deploy/embed-endpoint/.ids/package-embed-probe.json "$WORK/record-bad.json" <<'PYBADREC'
    import json, sys
    record = json.load(open(sys.argv[1])); record["apr_sha256"] = "0" * 64
    json.dump(record, open(sys.argv[2], "w"))
    PYBADREC
    expect_refusal "package record that does not match its zip" package env EMBED_PROBE_RECORD="$WORK/record-bad.json" just embed-probe-deploy dev
    expect_refusal "both mode flags" mode env DRY_RUN=1 PREFLIGHT=1 just embed-probe-deploy dev

    DRY_OUT="$(DRY_RUN=1 just embed-probe-deploy dev 2>&1)" || { echo "GATE FAILED: DRY_RUN did not succeed"; printf '%s\n' "$DRY_OUT" | tail -n 5; exit 1; }
    printf '%s' "$DRY_OUT" | grep -q 'DRY-RUN OK aprender-embed-probe-dev (offline; nothing deployed; AWS CALLS 0)' \
        || { echo "GATE FAILED: DRY_RUN did not print its offline line"; exit 1; }
    echo "  DRY_RUN synthesized and scanned the probe offline"; CASES=$((CASES + 1))

    python3 -I scripts/embed_endpoint/ingress_scan.py --selftest deploy-extensions/cdk.out/aprender-embed-probe-dev.template.json
    python3 -I scripts/embed_endpoint/package_zip.py selftest
    python3 -I scripts/embed_endpoint/graviton_gate.py selftest

    if [ -s "$EMBED_GATE_RECORDER" ] || [ -s "$EMBED_GATE_NPX_REFUSED" ]; then
        echo "GATE FAILED: a shim recorded a call the gate must not make"; cat "$EMBED_GATE_RECORDER" "$EMBED_GATE_NPX_REFUSED"; exit 1
    fi
    echo "AWS CALLS: 0 (aws shim recorder empty; npx passes through only cdk synth; credentials scrubbed)"

    aws sts get-caller-identity >/dev/null 2>&1 || true
    if [ ! -s "$EMBED_GATE_RECORDER" ]; then
        echo "GATE FAILED: negative control: the recorder did not fire on a deliberate call"; exit 1
    fi
    : > "$EMBED_GATE_RECORDER"
    echo "  negative control: the recorder fired once on a deliberate call, then was reset"
    echo "GATE SELFTEST OK ($CASES refusal and dry-run cases, plus the scanner, package and normalizer selftests)"
