#!/usr/bin/env bash
# Read-only readiness check: instance state, setup markers over SSM, and one /health call from the host
# itself. Prints READY / NOT READY. Usage: check.sh [profile]
set -euo pipefail
PROFILE="${1:-ze-kasher-dev}"; HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; IDS="$HERE/../.ids/gpu-host"
IID=$(python3 -I -c 'import json,sys;print(json.load(open(sys.argv[1]))["instance_id"])' "$IDS/host.json")
IP=$(python3 -I -c 'import json,sys;d=json.load(open(sys.argv[1]));print("%s:%s"%(d["private_ip"],d["port"]))' "$IDS/host.json")
CID=$(aws ssm send-command --profile "$PROFILE" --region us-east-1 --instance-ids "$IID" --document-name AWS-RunShellScript \
  --parameters "commands=[\"ls /var/lib/gpu-host-* 2>&1; tail -5 /var/log/gpu-host-setup.log; curl -s -m 5 http://$IP/health\"]" --query Command.CommandId --output text)
for _ in $(seq 1 30); do
  ST=$(aws ssm get-command-invocation --profile "$PROFILE" --region us-east-1 --command-id "$CID" --instance-id "$IID" --query Status --output text 2>/dev/null || echo Pending)
  case "$ST" in Pending|InProgress|Delayed) sleep 3 ;; *) break ;; esac
done
OUT=$(aws ssm get-command-invocation --profile "$PROFILE" --region us-east-1 --command-id "$CID" --instance-id "$IID" --query StandardOutputContent --output text)
printf '%s\n' "$OUT"
case "$OUT" in *gpu-host-REFUSED*) echo "REFUSED: setup failed verification" ;; *gpu-host-READY*) echo "READY" ;; *) echo "NOT READY" ;; esac
