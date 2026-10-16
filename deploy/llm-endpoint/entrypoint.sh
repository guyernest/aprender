#!/usr/bin/env bash
# Fail-closed entrypoint for the internal OpenAI-compatible LLM endpoint (phase 9, D-13).
#
# Order, and why:
#   1. REFUSE unless APR_API_KEY_HASH is exactly 64 hex characters. `apr serve` called with no hash
#      starts OPEN with only a warning (crates/apr-cli/src/commands/serve/auth.rs from_env), and a
#      malformed hash is "ignoring (auth disabled)": both would expose the model unauthenticated.
#   2. REFUSE unless MODEL_SHA256 is 64 hex: the pull below is verified against it, never trusted.
#   3. Pull the GGUF named by MODEL_S3_URI into MODEL_LOCAL (unless already there), then REFUSE
#      unless its sha256 equals MODEL_SHA256 (a mismatched file is deleted so a retry re-pulls).
#   4. Print the host facts and a timeline (ms since process start) that the client reads back from
#      the logs, then exec `apr serve run` on 0.0.0.0:$PORT. LWA needs the server to BE the process.
#
# Refusal lines on stderr, greppable by the self-test and by the operator in CloudWatch:
#   "REFUSED auth" (exit 2), "REFUSED model-pin" (exit 2), "REFUSED model-sha" (exit 4).
#
# Same script on Lambda, Lambda Managed Instances and Fargate.
#
# Pin rule: the `apr` this script execs is the release binary the Dockerfile copied in from the
# sha256-verified tarball named by deploy/llm-endpoint/pin.env, never a PATH lookup and never a
# fork-build. scripts/check_apr_bin_pinned.sh scans workflows, the Makefile and scripts/*.sh only, so
# it does NOT scan this directory: the pin is enforced by the image build, and by APR_BIN below.
#
# Logging (D-14, T-09-14): host facts and the word `set` for the key hash, NEVER its value; apr is
# never started with a verbose flag, so request bodies never reach the log.
#
# `set -e` is deliberately absent: this script is EXECUTED (never sourced), reports each step and
# decides itself which failures are fatal.
set -uo pipefail
APR_BIN="${APR_BIN:-/usr/local/bin/apr}"
SHA256SUM="${SHA256SUM:-sha256sum}"
MODEL_LOCAL="${MODEL_LOCAL:-/tmp/model.gguf}"
PORT="${PORT:-8080}"
T0=$(date +%s%N)
ms() { echo $(( ($(date +%s%N) - T0) / 1000000 )); }
log() { echo "[entry] t=$(ms)ms $*"; }
# The refusal line keeps the `[entry]` prefix and carries the timeline at the end of the line.
refuse() { echo "[entry] REFUSED $1: $2 (t=$(ms)ms)" >&2; exit "${3:-2}"; }
size_of() { wc -c < "$1" | tr -d ' '; }

log "start pid=$$ apr=$("$APR_BIN" --version 2>&1 | head -1)"
log "host: nproc=$(nproc 2>/dev/null || echo unknown) threads=${RAYON_NUM_THREADS:-unset} cpu_part=$(awk -F: '/CPU part/{gsub(/ /,"",$2); print $2; exit}' /proc/cpuinfo 2>/dev/null) model_name=$(awk -F: '/model name/{print $2; exit}' /proc/cpuinfo 2>/dev/null | sed 's/^ //')"
log "mem: $(awk '/MemTotal/{printf "total_kb=%s", $2}' /proc/meminfo 2>/dev/null) cgroup_max=$(cat /sys/fs/cgroup/memory.max 2>/dev/null || cat /sys/fs/cgroup/memory/memory.limit_in_bytes 2>/dev/null || echo unknown)"
log "env: MODEL_S3_URI=${MODEL_S3_URI:-unset} PLATFORM=${PLATFORM:-unset} AWS_LAMBDA_FUNCTION_MEMORY_SIZE=${AWS_LAMBDA_FUNCTION_MEMORY_SIZE:-unset}"

# (1) auth: length by glob, alphabet by regex. The `?` glob accepts ANY 64 characters, so both are needed.
HASH="${APR_API_KEY_HASH:-}"
case "$HASH" in
  ????????????????????????????????????????????????????????????????) ;;
  *) refuse auth "APR_API_KEY_HASH must be 64 hex chars (apr serve would start OPEN with only a warning)" 2 ;;
esac
printf '%s' "$HASH" | grep -Eq '^[0-9a-fA-F]{64}$' \
  || refuse auth "APR_API_KEY_HASH must be 64 hex chars (apr serve would start OPEN with only a warning)" 2
log "APR_API_KEY_HASH=set"

# (2) the model pin
WANT="${MODEL_SHA256:-}"
printf '%s' "$WANT" | grep -Eq '^[0-9a-fA-F]{64}$' \
  || refuse model-pin "MODEL_SHA256 unset or malformed (want 64 hex chars)" 2
# sha256sum prints lowercase; accept an uppercase pin without deleting a good file over case.
WANT=$(printf '%s' "$WANT" | tr 'A-F' 'a-f')

# (3) pull, then verify; the verification runs whether the file was pulled or already present
if [ -s "$MODEL_LOCAL" ]; then
  log "model already present bytes=$(size_of "$MODEL_LOCAL")"
else
  if [ -z "${MODEL_S3_URI:-}" ]; then log "FATAL: MODEL_S3_URI unset"; exit 2; fi
  log "s3 pull start"
  if aws s3 cp "$MODEL_S3_URI" "$MODEL_LOCAL" --only-show-errors; then
    log "s3 pull done bytes=$(size_of "$MODEL_LOCAL")"
  else
    log "FATAL: s3 pull failed rc=$?"; exit 3
  fi
fi
GOT=$("$SHA256SUM" "$MODEL_LOCAL" | awk '{print $1}')
if [ "$GOT" != "$WANT" ]; then
  rm -f "$MODEL_LOCAL"
  refuse model-sha "$MODEL_LOCAL sha256 mismatch: got $GOT want $WANT" 4
fi
log "model sha256 OK bytes=$(size_of "$MODEL_LOCAL")"

log "apr serve start"
# apr prints "Model ready ..." then binds; the adapter's /health poll turns ready at that moment.
# APR_SERVE_EXTRA is deliberately unquoted: it carries zero or more extra flags that must word-split.
# shellcheck disable=SC2086
exec "$APR_BIN" serve run "$MODEL_LOCAL" --host 0.0.0.0 --port "$PORT" --backend cpu ${APR_SERVE_EXTRA:-}
