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
# THE DEFAULTS ARE THE WORST SHAPE THE DOOR STILL ACCEPTS, deliberately.
# 800 points + a 200-step horizon x 50 holiday columns = 50 000 design feature cells,
# exactly `constants.fit_max_holiday_design_cost`, and the slowest of the three
# at-the-bound compositions measured on the aarch64 release host (1.692 s, vs 1.174 s
# many-rows and 0.106 s many-columns). That is the configuration a 2 s bar has to be
# asserted against, and 06-12 adds the bar here. The verifier's original 3000/181/84
# is now REFUSED at the door and can only be run against a build without the bound.
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
