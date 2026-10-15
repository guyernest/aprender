#!/bin/bash
# Run the agent-turn cost matrix: one `apr serve` per (model, RAYON_NUM_THREADS), alone on the host.
# bash on purpose (the interactive shell here is zsh: no word splitting, no PIPESTATUS).
#
#   bash tools/run_matrix.sh                      # the default matrix
#   bash tools/run_matrix.sh "0.8B 6" "4B 14"     # a subset
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
PORT=8040
REPS="${REPS:-2}"
BUDGET="${BUDGET_S:-300}"
if [ "$#" -gt 0 ]; then CFGS=("$@"); else CFGS=("0.8B 6" "0.8B 14" "2B 6" "2B 14" "4B 6" "4B 14"); fi

for cfg in "${CFGS[@]}"; do
  set -- $cfg; m="$1"; t="$2"; label="${m}-t${t}"
  echo "=== $label ==="
  RAYON_NUM_THREADS="$t" ./bin/apr serve run "models/Qwen3.5-${m}-Q4_K_M.gguf" --port "$PORT" --host 127.0.0.1 --backend cpu \
      > "results/serve-$label.log" 2>&1 &
  pid=$!
  up=0
  for _ in $(seq 1 120); do
    if curl -sf -m 2 "http://127.0.0.1:$PORT/health" > /dev/null 2>&1; then up=1; break; fi
    sleep 1
  done
  if [ "$up" -ne 1 ]; then echo "$label: server did not come up"; kill "$pid" 2>/dev/null; continue; fi
  python3 -I tools/bench.py --base "http://127.0.0.1:$PORT/v1" --model "Qwen3.5-${m}-Q4_K_M" --label "$label" \
      --threads "$t" --pid "$pid" --out "results/bench-$label.json" --md "results/bench-$label.md" \
      --reps "$REPS" --budget-s "$BUDGET" > "results/bench-$label.stdout" 2> "results/bench-$label.stderr"
  rc=$?
  echo "$label bench rc=$rc  peak RSS (ps, MB): $(ps -o rss= -p "$pid" | awk '{printf "%.0f", $1/1024}')"
  kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
  sleep 2
done
echo "MATRIX DONE"
