#!/usr/bin/env bash
# Sends probe-remote.sh to the instance over SSM and prints its RESULT lines. Read-only for AWS
# state apart from the SSM command record. Usage: probe.sh [profile]
set -euo pipefail
PROFILE="${1:-ze-kasher-dev}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IDS="$HERE/../.ids/gpu-probe"
IID=$(python3 -I -c 'import json,sys;print(json.load(open(sys.argv[1]))["instance_id"])' "$IDS/instance.json")
python3 -I -c 'import json,sys;print(json.dumps({"commands":open(sys.argv[1]).read().splitlines()}))' "$HERE/probe-remote.sh" > "$IDS/ssm-params.json"
CID=$(aws ssm send-command --profile "$PROFILE" --region us-east-1 --instance-ids "$IID" \
  --document-name AWS-RunShellScript --timeout-seconds 1800 --parameters "file://$IDS/ssm-params.json" \
  --query Command.CommandId --output text)
echo "command $CID on $IID"
# `ssm wait` gives up after about 100 s and the probe takes minutes: poll until the status is terminal.
while :; do
  ST=$(aws ssm get-command-invocation --profile "$PROFILE" --region us-east-1 --command-id "$CID" --instance-id "$IID" \
    --query Status --output text 2>/dev/null || echo Pending)
  case "$ST" in Pending|InProgress|Delayed) sleep 10 ;; *) break ;; esac
done
aws ssm get-command-invocation --profile "$PROFILE" --region us-east-1 --command-id "$CID" --instance-id "$IID" \
  --query '[Status,StandardOutputContent,StandardErrorContent]' --output text | tee "$IDS/probe-$(date -u +%Y%m%dT%H%M%SZ).out"
