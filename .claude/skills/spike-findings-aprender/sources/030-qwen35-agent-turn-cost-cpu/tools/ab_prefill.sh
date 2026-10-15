#!/bin/bash
# A/B the server's prefill path and the FP8 cache, 0.8B at 6 threads, alone on the host:
#   A  default                                   (session batched prefill, 64-token chunks, #4228)
#   B  APR_QWEN35_SESSION_PREFILL=per-token      (the 0.69.1 one-token-at-a-time path)
#   C  --no-fp8-cache                            (RSS lever: "saves ~1.5 GB RSS")
# Reads prefill tok/s from TTFT on short / tools2 / tools10 (reps=1) and RSS after each.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
PORT=8041
M="${MODEL:-0.8B}"; T="${THREADS:-6}"
run_variant() { # label, extra env (string or ""), extra args...
  local label="$1"; local extra_env="$2"; shift 2
  echo "=== $label ==="
  if [ -n "$extra_env" ]; then
    env RAYON_NUM_THREADS="$T" "$extra_env" ./bin/apr serve run "models/Qwen3.5-${M}-Q4_K_M.gguf" --port "$PORT" --host 127.0.0.1 --backend cpu "$@" > "results/serve-ab-$label.log" 2>&1 &
  else
    env RAYON_NUM_THREADS="$T" ./bin/apr serve run "models/Qwen3.5-${M}-Q4_K_M.gguf" --port "$PORT" --host 127.0.0.1 --backend cpu "$@" > "results/serve-ab-$label.log" 2>&1 &
  fi
  local pid=$!
  for _ in $(seq 1 120); do curl -sf -m 2 "http://127.0.0.1:$PORT/health" > /dev/null 2>&1 && break; sleep 1; done
  python3 -I tools/bench.py --base "http://127.0.0.1:$PORT/v1" --model "Qwen3.5-${M}-Q4_K_M" --label "ab-$label" --threads "$T" --pid "$pid" \
      --out "results/ab-$label.json" --md "results/ab-$label.md" --reps 1 --shapes short,tools2,tools10 --no-turn --no-conc \
      > "results/ab-$label.stdout" 2> "results/ab-$label.stderr"
  echo "$label rc=$? RSS(MB)=$(ps -o rss= -p "$pid" | awk '{printf "%.0f", $1/1024}')"
  grep -iE 'prefill|fp8|cache' "results/serve-ab-$label.log" | head -5
  kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null; sleep 2
}
run_variant "A-default" ""
run_variant "B-per-token" "APR_QWEN35_SESSION_PREFILL=per-token"
run_variant "C-no-fp8-cache" "" --no-fp8-cache
echo "AB DONE"
