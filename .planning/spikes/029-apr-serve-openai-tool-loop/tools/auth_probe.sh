#!/bin/bash
# Bearer-auth probe for `apr serve` started with APR_API_KEY. bash, not zsh: word splitting matters.
set -u
PORT="${1:-8032}"; KEY="${2:-spike-029-not-a-secret}"
B='{"model":"x","messages":[{"role":"user","content":"Hi"}],"max_tokens":4}'
echo "| request | HTTP | body |"; echo "|---|---|---|"
probe() { # $1 label, rest = curl header args
  local label="$1"; shift
  local code; code=$(curl -s -m 180 -o /tmp/auth-body.json -w '%{http_code}' -H 'Content-Type: application/json' "$@" -d "$B" "http://127.0.0.1:$PORT/v1/chat/completions")
  echo "| $label | $code | $(head -c 120 /tmp/auth-body.json | tr -d '\n') |"
}
probe "no Authorization header"
probe "Bearer wrong-key" -H "Authorization: Bearer wrong-key"
probe "Bearer <correct key>" -H "Authorization: Bearer $KEY"
echo "| GET /health, no header | $(curl -s -m 5 -o /dev/null -w '%{http_code}' "http://127.0.0.1:$PORT/health") | |"
echo "| GET /v1/models, no header | $(curl -s -m 5 -o /dev/null -w '%{http_code}' "http://127.0.0.1:$PORT/v1/models") | |"
