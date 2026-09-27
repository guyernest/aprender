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
# re-derives what CI must check (plan 08-09), and whether the torch-free
# Python self-tests also run in CI is decided at plan 08-12's CI checkpoint.
# `--frozen` everywhere: the committed uv.lock (human-verified pins, plan 08-02
# Task 1) is what runs, never a fresh resolution.
# ---------------------------------------------------------------------------

# Regenerate the two tiny synthetic CI fixtures from Laya's / transformers' own code (byte-identical on re-run).
laya-fixtures:
    #!/usr/bin/env bash
    set -euo pipefail
    uv run --project scripts/laya_train --frozen python scripts/laya_train/metrics.py --selftest
    uv run --project scripts/laya_train --frozen python scripts/laya_train/fixtures.py

# TweetEval stance demo data (D-19): data/decide/tweet-stance-16/{task.json,train.jsonl,eval.jsonl}
# from the s16-seed13 selection, every shot verified against the manifest's exact_hash.
# Output is under the root-anchored, gitignored /data/ — tweet text is never committed.
laya-prepare-stance:
    #!/usr/bin/env bash
    set -euo pipefail
    for f in data/tweet-eval-stance/train.jsonl data/tweet-eval-stance/test.jsonl; do
        test -f "$f" || {
            echo "ERROR: $f is missing — fetch the local dataset first:" >&2
            echo "       apr data tweet-eval-stance --output data/tweet-eval-stance" >&2
            exit 1
        }
    done
    uv run --project scripts/laya_train --frozen python scripts/laya_train/prepare_stance.py

# Fine-tune Laya on <data> (task.json + train.jsonl + a REQUIRED eval.jsonl), calibrate and gate into
# the run dir <out>. Exit 0 = GATE PASS, 3 = GATE FAIL, 2 = input refused. Extra args pass through
# (--epochs E above 16 shots/class, --stopping early_stopping|fixed_epochs, --seeds N for a variance
# report over the first N contract variance_seeds -- only the declared seed ships --,
# --device mps|cuda|cpu). The default stopping rule is the contract's (early_stopping, 1.1.0).
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
# the torch-free self-tests (metrics, data refusals + split, gate decision + the two fail-closed demo
# vectors + T clamp + early stopping), then the torch lifecycle (both stopping rules, --seeds 3). Each
# step's status is checked directly; the recipe stops at the first failure. Prints LAYA TRAIN SELFTEST OK.
laya-train-selftest:
    #!/usr/bin/env bash
    set -euo pipefail
    for m in metrics data gate; do
        uv run --project scripts/laya_train --frozen python "scripts/laya_train/$m.py" --selftest
    done
    uv run --project scripts/laya_train --frozen python scripts/laya_train/lifecycle.py
    echo "LAYA TRAIN SELFTEST OK"

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

# Identity of a decide .apr (bounded/header/manifest rungs): sha256, recipe_id, base, variant, labels
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
    RESOLVED="$(sed -n 's/^RESOLVED root=crates server=aprender-mcp-decide -> //p' "$LOG" | head -n 1)"
    CONTROL="$(sed -n 's/^CONTROL root=[^ ]* server=aprender-mcp-decide -> //p' "$LOG" | head -n 1)"
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
    sha256() { if command -v rtk >/dev/null 2>&1; then rtk proxy shasum -a 256 "$1"; else shasum -a 256 "$1"; fi | awk '{print $1}'; }
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
    open(out, "w").write(text)
    print(f"  server   {server}\n  s3 uri   {uri}\n  sha256   {sha}\n  auth     {auth}")
    PY
    echo "  wrote $DIR/deploy.toml (gitignored) for env=$ENV"

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
# file, (6) the S3 object has the local size. DRY_RUN=1 stops after (6) with DRY-RUN OK. Live:
# touch -> DEPLOYING -> cargo pmcp deploy (crates root, swapped) -> compile-log, health-body and
# identity assertions; any identity failure runs `just laya-teardown` (containment).
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
    sha256() { if command -v rtk >/dev/null 2>&1; then rtk proxy shasum -a 256 "$1"; else shasum -a 256 "$1"; fi | awk '{print $1}'; }
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
        echo "IDENTITY FAILURE: $1 -- containing (reserved concurrency 0, grant removed)" >&2
        just laya-teardown "$SERVER" "$ENV" "$PROFILE" \
            || echo "ERROR: containment failed -- throttle it by hand: aws lambda put-function-concurrency --function-name $SERVER --reserved-concurrent-executions 0" >&2
        exit 1
    }
    echo "DEPLOYING $SERVER"
    set +e
    just _laya-crates-root-swap crates "$CFG" "$SNAP" \
        cargo pmcp deploy --manifest-path crates --regenerate-stack --no-color > "$LOG" 2>&1
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
    just laya-grant "$SERVER" "$ENV" "$PROFILE"
    HEALTH="$(curl -fsS --max-time 30 "$ENDPOINT")" || contain "GET $ENDPOINT failed"
    python3 -c 'import json, sys; b = json.loads(sys.argv[1]); sys.exit(0 if b.get("package") == "aprender-mcp-decide-lambda" and b.get("server") == sys.argv[2] else 1)' "$HEALTH" "$SERVER" \
        || contain "the health body does not name the decide server: $HEALTH"
    set +e
    cargo run --release -p aprender-mcp-decide-lambda --example probe -- \
        --url "$ENDPOINT" --apr "$APR" --expect-sha256 "$H" > "models/decide/deploy-probe-$SERVER.log" 2>&1
    prc=$?
    set -e
    [ "$prc" -eq 0 ] || contain "the identity probe failed (log: models/decide/deploy-probe-$SERVER.log)"
    echo "DEPLOYED $SERVER at $ENDPOINT: compile log, health body and live identity (sha256 $H) name the decide server"
    echo "  next: just laya-deploy-verify $APR $SERVER $PROFILE    (re-run just laya-grant after any pmcp.run redeploy)"

# Prove every deploy refusal OFFLINE on the synthetic tiny artifact (the only one 08-09 writes),
# DRY_RUN=1 throughout, with `aws` shadowed by a recorder that must stay empty. Cases: placeholder,
# sha-pin, resolver-proof, deploy-eligibility, upload-eligibility (the last two from laya-verify:
# SyntheticNotDeployable); the crates-root swap restored byte-identical on success, forced
# failure, SIGTERM and an absent root; resolver proof; bootstrap build. The positive dry run is
# armed only by LAYA_ELIGIBLE_APR/_RUN/_DATA/_BASE naming an artifact laya-verify accepts.
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
    if just --summary | tr ' ' '\n' | grep -qx 'laya-upload'; then
        expect_refused 5 upload-eligibility 'REFUSED eligibility: .*SyntheticNotDeployable' \
            just laya-upload "$A" "$T" "$T/data" "$T/checkpoint"
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
    # Positive dry run: armed only by a real artifact that laya-verify itself must accept.
    if [ -n "${LAYA_ELIGIBLE_APR:-}${LAYA_ELIGIBLE_RUN:-}${LAYA_ELIGIBLE_DATA:-}${LAYA_ELIGIBLE_BASE:-}" ]; then
        if [ -n "${LAYA_ELIGIBLE_APR:-}" ] && [ -n "${LAYA_ELIGIBLE_RUN:-}" ] && [ -n "${LAYA_ELIGIBLE_DATA:-}" ] && [ -n "${LAYA_ELIGIBLE_BASE:-}" ]; then
            just laya-deploy-config "$LAYA_ELIGIBLE_APR" off > "$CASES/positive-config.log" 2>&1 || fail "positive: laya-deploy-config"
            set +e
            just laya-deploy "$LAYA_ELIGIBLE_APR" "$LAYA_ELIGIBLE_RUN" "$LAYA_ELIGIBLE_DATA" "$LAYA_ELIGIBLE_BASE" > "$CASES/positive.log" 2>&1
            rc=$?
            set -e
            if [ "$rc" -eq 0 ] && grep -q '^DRY-RUN OK' "$CASES/positive.log"; then
                grep '^DRY-RUN OK' "$CASES/positive.log"
            else
                fail "positive dry run: exit $rc without DRY-RUN OK (log: $CASES/positive.log)"
            fi
        else
            fail "positive dry run: set ALL of LAYA_ELIGIBLE_APR/_RUN/_DATA/_BASE, or none"
        fi
    else
        echo "SKIP positive dry run: no deploy-eligible artifact (laya-finetune-gate-v1 demo.outcome gate_fail; set LAYA_ELIGIBLE_APR/_RUN/_DATA/_BASE to arm)"
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
        echo "DEPLOY SELFTEST OK"
    else
        echo "DEPLOY SELFTEST FAILED ($FAILS failed checks)" >&2
        exit 1
    fi
