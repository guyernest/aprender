#!/usr/bin/env bash
# Pull the GGUF named by MODEL_S3_URI into MODEL_LOCAL (unless already there), print the host facts
# and a timeline (ms since process start) that the client reads back from the logs, then exec
# `apr serve run` on 0.0.0.0:$PORT. Same script on Lambda, Lambda Managed Instances and Fargate.
#
# Pin rule: the `apr` this script execs is the release binary the Dockerfile copied in from the
# sha256-verified tarball named by deploy/llm-endpoint/pin.env, never a PATH lookup and never a
# fork-build. scripts/check_apr_bin_pinned.sh scans workflows, the Makefile and scripts/*.sh only, so
# it does NOT scan this directory: the pin is enforced by the image build, and by the APR_BIN line below.
#
# `set -e` is deliberately absent: this script is EXECUTED (never sourced), reports each step and
# decides itself which failures are fatal.
set -uo pipefail
APR_BIN="${APR_BIN:-/usr/local/bin/apr}"
SHA256SUM="${SHA256SUM:-sha256sum}"
T0=$(date +%s%N)
ms() { echo $(( ($(date +%s%N) - T0) / 1000000 )); }
log() { echo "[entry] t=$(ms)ms $*"; }

log "start pid=$$ apr=$("$APR_BIN" --version 2>&1 | head -1)"
log "host: nproc=$(nproc) threads=${RAYON_NUM_THREADS:-unset} cpu_part=$(awk -F: '/CPU part/{gsub(/ /,"",$2); print $2; exit}' /proc/cpuinfo 2>/dev/null) model_name=$(awk -F: '/model name/{print $2; exit}' /proc/cpuinfo 2>/dev/null | sed 's/^ //')"
log "mem: $(awk '/MemTotal/{printf "total_kb=%s", $2}' /proc/meminfo 2>/dev/null) cgroup_max=$(cat /sys/fs/cgroup/memory.max 2>/dev/null || cat /sys/fs/cgroup/memory/memory.limit_in_bytes 2>/dev/null || echo unknown)"
log "env: MODEL_S3_URI=${MODEL_S3_URI:-unset} PLATFORM=${PLATFORM:-unset} AWS_LAMBDA_FUNCTION_MEMORY_SIZE=${AWS_LAMBDA_FUNCTION_MEMORY_SIZE:-unset}"

if [ -s "$MODEL_LOCAL" ]; then
  log "model already present bytes=$(stat -c %s "$MODEL_LOCAL")"
else
  if [ -z "${MODEL_S3_URI:-}" ]; then log "FATAL: MODEL_S3_URI unset"; exit 2; fi
  log "s3 pull start"
  if aws s3 cp "$MODEL_S3_URI" "$MODEL_LOCAL" --only-show-errors; then
    log "s3 pull done bytes=$(stat -c %s "$MODEL_LOCAL")"
  else
    log "FATAL: s3 pull failed rc=$?"; exit 3
  fi
fi

log "apr serve start"
# apr prints "Model ready ..." then binds; the adapter's /health poll turns ready at that moment.
# APR_SERVE_EXTRA is deliberately unquoted: it carries zero or more extra flags that must word-split.
# shellcheck disable=SC2086
exec "$APR_BIN" serve run "$MODEL_LOCAL" --host 0.0.0.0 --port "$PORT" --backend cpu ${APR_SERVE_EXTRA:-}
