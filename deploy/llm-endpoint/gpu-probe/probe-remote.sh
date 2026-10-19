#!/bin/bash
# Runs ON the instance (sent by probe.sh over SSM). The server binds 127.0.0.1 only: the CUDA-path
# routers ignore APR_API_KEY_HASH (paiml/aprender#5478), so loopback is what keeps this private.
# Prints `RESULT key=value` lines; one block per model. Nothing here changes the instance's exposure.
set -uo pipefail
[ -e /var/lib/gpu-probe-REFUSED ] && { echo "RESULT refused=1"; cat /var/log/gpu-probe-setup.log; exit 2; }
[ -e /var/lib/gpu-probe-READY ] || { echo "RESULT ready=0"; tail -20 /var/log/gpu-probe-setup.log; exit 3; }
PORT=8081
now() { date +%s.%N; }
CHAT='{"model":"m","max_tokens":48,"temperature":0,"messages":[{"role":"user","content":"Reply with the single word: ready"}]}'
TOOL='{"model":"m","max_tokens":96,"temperature":0,"messages":[{"role":"user","content":"What is the weather in Paris?"}],"tools":[{"type":"function","function":{"name":"get_weather","description":"Get the current weather for a city","parameters":{"type":"object","properties":{"city":{"type":"string"}},"required":["city"]}}}]}'
post() { curl -s -o "$2" -w '%{http_code} %{time_total}' -H 'content-type: application/json' -d "$1" "http://127.0.0.1:$PORT/v1/chat/completions"; }
for NAME in w08 w2b; do
  LOG=/tmp/serve-$NAME.log
  T0=$(now)
  /usr/local/bin/apr serve run /opt/models/$NAME.gguf --host 127.0.0.1 --port $PORT --backend cuda >"$LOG" 2>&1 &
  PID=$!
  UP=0
  for _ in $(seq 1 600); do
    kill -0 $PID 2>/dev/null || break
    curl -s -o /dev/null "http://127.0.0.1:$PORT/health" && { UP=1; break; }
    sleep 0.5
  done
  echo "RESULT model=$NAME healthy=$UP start_s=$(awk -v a="$(now)" -v b="$T0" 'BEGIN{printf "%.1f", a-b}')"
  if [ "$UP" = 1 ]; then
    echo "RESULT model=$NAME chat_cold=$(post "$CHAT" /tmp/$NAME-chat.json)"
    echo "RESULT model=$NAME tool_cold=$(post "$TOOL" /tmp/$NAME-tool1.json)"
    echo "RESULT model=$NAME tool_warm=$(post "$TOOL" /tmp/$NAME-tool2.json)"
    echo "RESULT model=$NAME tool_calls_present=$(grep -c '"tool_calls"' /tmp/$NAME-tool1.json)"
    echo "RESULT model=$NAME gpu_mem=$(nvidia-smi --query-gpu=name,compute_cap,memory.used --format=csv,noheader)"
  fi
  # Prove the mechanism engaged, not the intent: the server's own device lines and the process on the GPU.
  echo "RESULT model=$NAME gpu_process=$(nvidia-smi --query-compute-apps=pid,used_memory --format=csv,noheader | tr '\n' ';')"
  echo "RESULT model=$NAME server_device_lines=$(grep -i -E 'cuda|gpu|backend|sm_' "$LOG" | head -5 | tr '\n' '|')"
  kill $PID 2>/dev/null; wait $PID 2>/dev/null
done
